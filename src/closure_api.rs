use crate::config::Config;
use axum::{
    body::{Body, Bytes},
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde_json::Value;
use std::time::Duration;

/// A message to post: where, what, which message of the bot to replace, and when.
#[derive(Debug)]
pub struct Post<'a> {
    pub channel: &'a str,
    pub message: &'a Value,
    pub edit: Option<&'a str>,
    /// Unix seconds; `None` sends now.
    pub send_at: Option<i64>,
    /// The token of the form: the same form is sent once, even when it is posted again after a
    /// restart of the dashboard.
    pub key: Option<&'a str>,
}

#[derive(Clone)]
pub struct Api {
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Body>,
    base: String,
    token: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Validation,
    Precondition,
    Unavailable,
    RateLimited,
    TooLarge,
    Other,
}
#[derive(Debug, Clone)]
pub struct ApiIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub kind: ErrorKind,
    pub code: String,
    pub message: String,
    pub issues: Vec<ApiIssue>,
}
impl ApiError {
    fn new(status: StatusCode, message: String, issues: Vec<ApiIssue>) -> Self {
        let kind = match status.as_u16() {
            401 => ErrorKind::Unauthorized,
            403 => ErrorKind::Forbidden,
            404 => ErrorKind::NotFound,
            412 => ErrorKind::Conflict,
            422 => ErrorKind::Validation,
            428 => ErrorKind::Precondition,
            503 => ErrorKind::Unavailable,
            429 => ErrorKind::RateLimited,
            413 => ErrorKind::TooLarge,
            _ => ErrorKind::Other,
        };
        Self {
            status,
            kind,
            code: String::new(),
            message,
            issues,
        }
    }
    fn unavailable() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, String::new(), Vec::new())
    }
}
impl Api {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(Some(Duration::from_secs(3)));
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .wrap_connector(http);
        Self {
            client: Client::builder(TokioExecutor::new()).build(connector),
            base: config.api_url.trim_end_matches('/').into(),
            token: config.admin_token.0.clone(),
        }
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        etag: Option<&str>,
        actor: Option<&str>,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request_keyed(method, path, body, etag, actor, None)
            .await
    }
    /// Sends a request and reads the whole answer, within ten seconds.
    async fn transport(
        &self,
        request: Request<Body>,
    ) -> Result<(StatusCode, Option<String>, Bytes), ApiError> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let response = self
                .client
                .request(request)
                .await
                .map_err(|_| ApiError::unavailable())?;
            let status = response.status();
            let etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.replace(&self.token, "[redacted]"));
            let bytes = response
                .into_body()
                .collect()
                .await
                .map_err(|_| ApiError::unavailable())?
                .to_bytes();
            Ok::<_, ApiError>((status, etag, bytes))
        })
        .await
        .map_err(|_| ApiError::unavailable())?
    }
    /// [`Self::request`] with an `Idempotency-Key`: the bot does what the request asks once, whatever
    /// the number of times it comes.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    async fn request_keyed(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        etag: Option<&str>,
        actor: Option<&str>,
        key: Option<&str>,
    ) -> Result<(Value, Option<String>), ApiError> {
        let mut builder = Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.base))
            .header("accept", "application/json")
            .header("authorization", format!("Bearer {}", self.token));
        if let Some(e) = etag {
            builder = builder.header("if-match", e);
        }
        if let Some(a) = actor {
            builder = builder.header("x-closure-actor", a);
        }
        if let Some(key) = key {
            builder = builder.header("idempotency-key", key);
        }
        let request_body = if let Some(data) = body {
            builder = builder.header("content-type", "application/json");
            Body::from(data.to_string())
        } else {
            Body::empty()
        };
        let request = builder
            .body(request_body)
            .map_err(|_| ApiError::unavailable())?;
        let (status, etag, bytes) = self.transport(request).await?;
        let mut data: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) if !status.is_success() => {
                return Err(ApiError::new(status, String::new(), vec![]));
            }
            Err(_) => {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    String::new(),
                    vec![],
                ));
            }
        };
        redact(&mut data, &self.token);
        if !status.is_success() {
            let error = &data["error"];
            let message = error["message"].as_str().unwrap_or("").to_owned();
            let issues = error["issues"].as_array().map_or_else(Vec::new, |a| {
                a.iter()
                    .map(|x| ApiIssue {
                        path: x["path"].as_str().unwrap_or("").to_owned(),
                        code: x["code"].as_str().unwrap_or("").to_owned(),
                        message: x["message"].as_str().unwrap_or("").to_owned(),
                    })
                    .collect()
            });
            let mut result = ApiError::new(status, message, issues);
            error["code"]
                .as_str()
                .unwrap_or("http_error")
                .clone_into(&mut result.code);
            return Err(result);
        }
        Ok((data, etag))
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn guilds(&self) -> Result<(Value, Option<String>), ApiError> {
        self.request(Method::GET, "/v1/guilds", None, None, None)
            .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn access(
        &self,
        guild: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/access/{actor}"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn schema(&self, locale: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/schema?locale={locale}"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn basic_schema(&self, locale: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/schema?view=basic&locale={locale}"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn effective(&self, guild: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/config/effective"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn history(
        &self,
        guild: &str,
        before: Option<i64>,
    ) -> Result<(Value, Option<String>), ApiError> {
        // One more than a page: the revision before the last of it is what its change is read against.
        let before = before.map_or_else(String::new, |before| format!("&before={before}"));
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/config/history?limit=51{before}"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn stats(&self, guild: &str, days: u8) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/stats?days={days}"),
            None,
            None,
            None,
        )
        .await
    }
    /// The cases of a server a person may see, as the bot lists them. `query` is the encoded
    /// filter (`kind=ban&before=40`).
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn cases(
        &self,
        guild: &str,
        query: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/cases?{query}"),
            None,
            None,
            Some(actor),
        )
        .await
    }
    /// One case.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn case(
        &self,
        guild: &str,
        number: i64,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/cases/{number}"),
            None,
            None,
            Some(actor),
        )
        .await
    }
    /// The counts of the cases of the last `days` days.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn cases_summary(
        &self,
        guild: &str,
        days: u16,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/cases/summary?days={days}"),
            None,
            None,
            Some(actor),
        )
        .await
    }
    /// Records a note against a member in the name of `actor`.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn add_note(
        &self,
        guild: &str,
        target: &str,
        reason: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::POST,
            &format!("/v1/guilds/{guild}/cases"),
            Some(serde_json::json!({ "target": target, "reason": reason })),
            None,
            Some(actor),
        )
        .await
    }
    /// Changes the reason of a case, or hides or shows it.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn change_case(
        &self,
        guild: &str,
        number: i64,
        change: Value,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::PATCH,
            &format!("/v1/guilds/{guild}/cases/{number}"),
            Some(change),
            None,
            Some(actor),
        )
        .await
    }
    /// The CSV of the cases a filter finds.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn cases_csv(
        &self,
        guild: &str,
        query: &str,
        actor: &str,
    ) -> Result<Bytes, ApiError> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "{}/v1/guilds/{guild}/cases/export?{query}",
                self.base
            ))
            .header("authorization", format!("Bearer {}", self.token))
            .header("x-closure-actor", actor)
            .body(Body::empty())
            .map_err(|_| ApiError::unavailable())?;
        let (status, _, bytes) = self.transport(request).await?;
        if status.is_success() {
            Ok(bytes)
        } else {
            Err(ApiError::new(status, String::new(), vec![]))
        }
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn directory(&self, guild: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/directory"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn config(&self, guild: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/config"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn basic_config(&self, guild: &str) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/config?view=basic"),
            None,
            None,
            None,
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn put_basic_config(
        &self,
        guild: &str,
        value: Value,
        etag: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::PUT,
            &format!("/v1/guilds/{guild}/config?view=basic"),
            Some(value),
            Some(etag),
            Some(actor),
        )
        .await
    }
    /// Has the bot send a message to a channel of the server.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn send_message(
        &self,
        guild: &str,
        post: &Post<'_>,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        let mut body = serde_json::json!({"channel_id": post.channel, "message": post.message});
        if let Some(edit) = post.edit {
            body["edit"] = serde_json::json!(edit);
        }
        if let Some(at) = post.send_at {
            body["send_at"] = serde_json::json!(at);
        }
        self.request_keyed(
            Method::POST,
            &format!("/v1/guilds/{guild}/messages"),
            Some(body),
            None,
            Some(actor),
            post.key,
        )
        .await
    }
    /// The posts of the server that wait for their time.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn scheduled_posts(
        &self,
        guild: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/scheduled-posts"),
            None,
            None,
            Some(actor),
        )
        .await
    }
    /// Takes a waiting post back.
    ///
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn cancel_post(
        &self,
        guild: &str,
        key: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::DELETE,
            &format!("/v1/guilds/{guild}/scheduled-posts/{key}"),
            None,
            None,
            Some(actor),
        )
        .await
    }
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn put_config(
        &self,
        guild: &str,
        value: Value,
        etag: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::PUT,
            &format!("/v1/guilds/{guild}/config"),
            Some(value),
            Some(etag),
            Some(actor),
        )
        .await
    }
}
fn redact(value: &mut Value, secret: &str) {
    match value {
        Value::String(s) => *s = s.replace(secret, "[redacted]"),
        Value::Array(a) => {
            for v in a {
                redact(v, secret);
            }
        }
        Value::Object(o) => {
            let old = std::mem::take(o);
            for (key, mut v) in old {
                redact(&mut v, secret);
                o.insert(key.replace(secret, "[redacted]"), v);
            }
        }
        _ => {}
    }
}
