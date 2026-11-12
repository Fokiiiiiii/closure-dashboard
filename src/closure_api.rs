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
    /// Form token: the bot sends the form once, even across a dashboard restart.
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
        self.request_keyed(method, path, body, etag, actor, None, None)
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
    /// [`Self::request`] with an `Idempotency-Key`: the bot acts once, however often it arrives.
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
        locale: Option<&str>,
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
        if let Some(locale) = locale {
            builder = builder.header("x-closure-locale", locale);
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
    /// The cases a person may see. `query` is the encoded filter (`kind=ban&before=40`).
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
    pub async fn tickets(
        &self,
        guild: &str,
        query: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/tickets?{query}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket(
        &self,
        guild: &str,
        channel_id: &str,
        start: Option<u32>,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        let page = start.map_or_else(String::new, |start| format!("?start={start}&limit=4"));
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/tickets/{channel_id}{page}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_action_state(
        &self,
        guild: &str,
        scope_kind: &str,
        scope_id: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !valid_ticket_action_scope(scope_kind, scope_id) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid ticket action scope".into(),
                Vec::new(),
            ));
        }
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/ticket-actions/{scope_kind}/{scope_id}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_action_panels(
        &self,
        guild: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/ticket-actions/panels"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_history_state(
        &self,
        guild: &str,
        channel_id: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action_state(guild, "history", channel_id, actor)
            .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_history_action(
        &self,
        guild: &str,
        channel_id: &str,
        action: &str,
        data: Value,
        etag: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action(
            guild,
            "history",
            channel_id,
            action,
            data,
            etag,
            actor,
            idempotency_key,
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_labels_state(
        &self,
        guild: &str,
        actor: &str,
        cursor: Option<&str>,
    ) -> Result<(Value, Option<String>), ApiError> {
        if cursor.is_some_and(|cursor| {
            cursor.len() != 56
                || !cursor
                    .strip_prefix("ticket-label-definition:")
                    .is_some_and(|suffix| {
                        suffix.len() == 32
                            && suffix
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    })
        }) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid ticket label cursor".into(),
                Vec::new(),
            ));
        }
        let parameters = cursor
            .map(|cursor| vec![("cursor", cursor)])
            .unwrap_or_default();
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        let path = if query.is_empty() {
            format!("/v1/guilds/{guild}/ticket-actions/labels/labels")
        } else {
            format!("/v1/guilds/{guild}/ticket-actions/labels/labels?{query}")
        };
        self.request(Method::GET, &path, None, None, Some(actor))
            .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_label_action(
        &self,
        guild: &str,
        action: &str,
        data: Value,
        etag: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action(
            guild,
            "labels",
            "labels",
            action,
            data,
            etag,
            actor,
            idempotency_key,
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_tags_state(
        &self,
        guild: &str,
        query: Option<&str>,
        cursor: Option<&str>,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        let query = query.map(str::trim).filter(|query| !query.is_empty());
        if query
            .is_some_and(|query| query.chars().count() > 100 || query.chars().any(char::is_control))
            || cursor.is_some_and(|cursor| {
                cursor.is_empty()
                    || cursor.len() > 16
                    || !cursor.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'_'
                            || byte == b'-'
                    })
            })
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid ticket tag query".into(),
                Vec::new(),
            ));
        }
        let mut parameters = Vec::new();
        if let Some(query) = query {
            parameters.push(("q", query.to_owned()));
        }
        if let Some(cursor) = cursor {
            parameters.push(("cursor", cursor.to_owned()));
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        let path = if query.is_empty() {
            format!("/v1/guilds/{guild}/ticket-actions/tags/tags")
        } else {
            format!("/v1/guilds/{guild}/ticket-actions/tags/tags?{query}")
        };
        self.request(Method::GET, &path, None, None, Some(actor))
            .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_tag_action(
        &self,
        guild: &str,
        action: &str,
        data: Value,
        etag: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action(
            guild,
            "tags",
            "tags",
            action,
            data,
            etag,
            actor,
            idempotency_key,
        )
        .await
    }

    /// # Errors
    /// Returns actor-scoped account notification state.
    pub async fn notifications_state(
        &self,
        cursor: Option<&str>,
        actor: &str,
        locale: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !matches!(locale, "en" | "ja") {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid notification locale".into(),
                Vec::new(),
            ));
        }
        if cursor.is_some_and(|cursor| {
            cursor.is_empty()
                || cursor.len() > 512
                || !cursor.bytes().all(|byte| byte.is_ascii_graphic())
        }) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid notification cursor".into(),
                Vec::new(),
            ));
        }
        let path = if let Some(cursor) = cursor {
            let query = serde_urlencoded::to_string([("cursor", cursor)])
                .map_err(|_| ApiError::unavailable())?;
            format!("/v1/notifications?{query}")
        } else {
            "/v1/notifications".to_owned()
        };
        self.request_keyed(
            Method::GET,
            &path,
            None,
            None,
            Some(actor),
            None,
            Some(locale),
        )
        .await
    }

    /// # Errors
    /// Applies an actor-scoped notification action using its revision and idempotency key.
    pub async fn notification_action(
        &self,
        action: &str,
        data: Value,
        etag: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !matches!(
            action,
            "mark_read"
                | "mark_all_read"
                | "save_preferences"
                | "request_email_verification"
                | "verify_email"
                | "remove_email"
        ) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid notification action".into(),
                Vec::new(),
            ));
        }
        self.request_keyed(
            Method::POST,
            "/v1/notifications/actions",
            Some(serde_json::json!({ "action": action, "data": data })),
            Some(etag),
            Some(actor),
            Some(idempotency_key),
            None,
        )
        .await
    }

    pub async fn gallery_templates(
        &self,
        guild: &str,
        kind: Option<&str>,
        category: Option<&str>,
        query: Option<&str>,
        sort: &str,
        offset: u32,
        limit: u16,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if kind.is_some_and(|kind| !matches!(kind, "panel" | "tag" | "form"))
            || category.is_some_and(|category| {
                !matches!(
                    category,
                    "support"
                        | "moderation"
                        | "sales"
                        | "applications"
                        | "feedback"
                        | "general"
                        | "other"
                )
            })
            || query.is_some_and(|query| {
                query.chars().count() > 100 || query.chars().any(char::is_control)
            })
            || !matches!(sort, "popular" | "newest")
            || offset > 10_000
            || !(1..=50).contains(&limit)
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid gallery query".into(),
                Vec::new(),
            ));
        }
        let mut parameters = vec![
            ("sort".to_owned(), sort.to_owned()),
            ("offset".to_owned(), offset.to_string()),
            ("limit".to_owned(), limit.to_string()),
        ];
        for (key, value) in [("kind", kind), ("category", category), ("q", query)] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                parameters.push((key.to_owned(), value.to_owned()));
            }
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/ticket-gallery/templates?{query}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns a public approved gallery template.
    pub async fn gallery_template(
        &self,
        guild: &str,
        template_id: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !valid_gallery_template_id(template_id) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid gallery template ID".into(),
                Vec::new(),
            ));
        }
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/ticket-gallery/templates/{template_id}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Returns submissions owned by the verified actor.
    pub async fn gallery_submissions(
        &self,
        guild: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/ticket-gallery/submissions"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    /// # Errors
    /// Submits an explicitly consented gallery copy.
    pub async fn gallery_submit(
        &self,
        guild: &str,
        data: Value,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.request_keyed(
            Method::POST,
            &format!("/v1/guilds/{guild}/ticket-gallery/submissions"),
            Some(data),
            None,
            Some(actor),
            Some(idempotency_key),
            None,
        )
        .await
    }

    /// # Errors
    /// Updates or withdraws the actor's submission with its record revision.
    pub async fn gallery_submission_action(
        &self,
        guild: &str,
        submission_id: &str,
        data: Value,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !valid_gallery_template_id(submission_id) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid gallery submission ID".into(),
                Vec::new(),
            ));
        }
        self.request_keyed(
            Method::POST,
            &format!("/v1/guilds/{guild}/ticket-gallery/submissions/{submission_id}/actions"),
            Some(data),
            None,
            Some(actor),
            Some(idempotency_key),
            None,
        )
        .await
    }

    /// # Errors
    /// Records a successful import idempotently after the config change completes.
    pub async fn gallery_record_import(
        &self,
        guild: &str,
        template_id: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !valid_gallery_template_id(template_id) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid gallery template ID".into(),
                Vec::new(),
            ));
        }
        self.request_keyed(
            Method::POST,
            &format!("/v1/guilds/{guild}/ticket-gallery/templates/{template_id}/imports"),
            Some(serde_json::json!({})),
            None,
            Some(actor),
            Some(idempotency_key),
            None,
        )
        .await
    }

    pub async fn ticket_knowledge_state(
        &self,
        guild: &str,
        cursor: Option<&str>,
        query: Option<&str>,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if cursor.is_some_and(|cursor| {
            cursor.is_empty()
                || cursor.len() > 32
                || !cursor
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }) || query
            .is_some_and(|query| query.chars().count() > 100 || query.chars().any(char::is_control))
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid knowledge cursor or query".into(),
                Vec::new(),
            ));
        }
        let mut parameters = Vec::new();
        if let Some(cursor) = cursor {
            parameters.push(("cursor", cursor.to_owned()));
        }
        if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
            parameters.push(("q", query.to_owned()));
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        let path = if query.is_empty() {
            format!("/v1/guilds/{guild}/ticket-actions/knowledge/knowledge")
        } else {
            format!("/v1/guilds/{guild}/ticket-actions/knowledge/knowledge?{query}")
        };
        self.request(Method::GET, &path, None, None, Some(actor))
            .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn ticket_analytics_state(
        &self,
        guild: &str,
        days: u16,
        cursor: Option<&str>,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !matches!(days, 7 | 30 | 90)
            || cursor.is_some_and(|cursor| {
                cursor.is_empty()
                    || cursor.len() > 128
                    || !cursor
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            })
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid analytics period or cursor".into(),
                Vec::new(),
            ));
        }
        let mut parameters = vec![("days", days.to_string())];
        if let Some(cursor) = cursor {
            parameters.push(("cursor", cursor.to_owned()));
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        let path = format!("/v1/guilds/{guild}/ticket-actions/analytics/analytics?{query}");
        self.request(Method::GET, &path, None, None, Some(actor))
            .await
    }

    /// # Errors
    /// Returns configured-state metadata only; secret values are never returned.
    pub async fn ticket_integration_secrets(
        &self,
        guild: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action_state(guild, "integrations", "integrations", actor)
            .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error. The secret is sent only to the host API.
    pub async fn ticket_integration_secret_action(
        &self,
        guild: &str,
        action: &str,
        data: Value,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        self.ticket_action(
            guild,
            "integrations",
            "integrations",
            action,
            data,
            "\"0\"",
            actor,
            idempotency_key,
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    /// # Errors
    /// Returns a sanitized host API response for published Knowledge articles.
    pub async fn public_knowledge_state(
        &self,
        guild: &str,
        query: Option<&str>,
        category: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<(Value, Option<String>), ApiError> {
        let query = query.map(str::trim).filter(|query| !query.is_empty());
        let category = category
            .map(str::trim)
            .filter(|category| !category.is_empty());
        if query
            .is_some_and(|query| query.chars().count() > 100 || query.chars().any(char::is_control))
            || category.is_some_and(|category| {
                category.chars().count() > 100 || category.chars().any(char::is_control)
            })
            || cursor.is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > 256 || cursor.chars().any(char::is_control)
            })
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid public knowledge query".into(),
                Vec::new(),
            ));
        }
        let mut parameters = Vec::new();
        if let Some(query) = query {
            parameters.push(("q", query.to_owned()));
        }
        if let Some(category) = category {
            parameters.push(("category", category.to_owned()));
        }
        if let Some(cursor) = cursor {
            parameters.push(("cursor", cursor.to_owned()));
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        let path = if query.is_empty() {
            format!("/v1/guilds/{guild}/public/ticket-knowledge")
        } else {
            format!("/v1/guilds/{guild}/public/ticket-knowledge?{query}")
        };
        self.request(Method::GET, &path, None, None, None).await
    }

    /// # Errors
    /// Returns the same 404 for unknown, unpublished, or deleted articles.
    pub async fn public_knowledge_article(
        &self,
        guild: &str,
        slug: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !(1..=32).contains(&slug.len())
            || !slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid public knowledge slug".into(),
                Vec::new(),
            ));
        }
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/public/ticket-knowledge/{slug}"),
            None,
            None,
            None,
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized audit search result.
    async fn ticket_db_audit_events(
        &self,
        guild: &str,
        filters: &Value,
        limit: u32,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        let mut parameters = vec![("limit".to_owned(), limit.to_string())];
        for key in ["user_id", "action_type", "resource_type", "outcome"] {
            if let Some(value) = filters
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                parameters.push((key.to_owned(), value.to_owned()));
            }
        }
        for key in ["date_from", "date_to"] {
            if let Some(value) = filters.get(key).and_then(Value::as_i64) {
                parameters.push((key.to_owned(), value.to_string()));
            }
        }
        let query = serde_urlencoded::to_string(parameters).map_err(|_| ApiError::unavailable())?;
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/audit-events?{query}"),
            None,
            None,
            Some(actor),
        )
        .await
    }

    pub async fn ticket_audit_search(
        &self,
        guild: &str,
        data: Value,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        let page = data.get("page").and_then(Value::as_u64).unwrap_or(0);
        let page_size = data.get("page_size").and_then(Value::as_u64).unwrap_or(0);
        if !(1..=100).contains(&page) || !(1..=25).contains(&page_size) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid audit page".into(),
                Vec::new(),
            ));
        }
        let limit = page
            .checked_mul(page_size)
            .filter(|limit| *limit <= 2_500)
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid audit page".into(),
                    Vec::new(),
                )
            })?;
        let offset = (page - 1) * page_size;
        let mut plugin_data = data.clone();
        plugin_data["page"] = serde_json::json!(1);
        plugin_data["page_size"] = serde_json::json!(limit);

        let (plugin_response, etag) = self
            .ticket_action(
                guild,
                "guild",
                "guild",
                "audit.search",
                plugin_data,
                r#""0""#,
                actor,
                idempotency_key,
            )
            .await?;
        let plugin_state = plugin_response
            .get("result")
            .filter(|state| state.is_object())
            .ok_or_else(ApiError::unavailable)?;
        let plugin_entries = plugin_state["entries"]
            .as_array()
            .cloned()
            .ok_or_else(ApiError::unavailable)?;
        let plugin_total = plugin_state["total"]
            .as_u64()
            .ok_or_else(ApiError::unavailable)?;
        let (database_response, _) = self
            .ticket_db_audit_events(guild, &data, limit as u32, actor)
            .await?;
        let database_entries = database_response["entries"]
            .as_array()
            .cloned()
            .ok_or_else(ApiError::unavailable)?;
        let database_total = database_response["total"]
            .as_u64()
            .ok_or_else(ApiError::unavailable)?;
        let truncated = plugin_state["truncated"].as_bool().unwrap_or(false)
            || database_response["truncated"].as_bool().unwrap_or(false);

        let mut entries = plugin_entries;
        entries.extend(database_entries);
        entries.sort_by(|left, right| {
            right["timestamp"]
                .as_i64()
                .cmp(&left["timestamp"].as_i64())
                .then_with(|| left["actor_id"].as_str().cmp(&right["actor_id"].as_str()))
                .then_with(|| left["action"].as_str().cmp(&right["action"].as_str()))
        });
        let current_entries = entries
            .into_iter()
            .skip(offset as usize)
            .take(page_size as usize)
            .collect::<Vec<_>>();
        let total = plugin_total.saturating_add(database_total);
        let has_more = offset + (current_entries.len() as u64) < total || truncated;

        let mut action_options = plugin_state["action_options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !action_options
            .iter()
            .any(|value| value.as_str() == Some("settings_update"))
        {
            action_options.push(serde_json::json!("settings_update"));
        }
        let mut resource_options = plugin_state["resource_options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !resource_options
            .iter()
            .any(|value| value.as_str() == Some("settings"))
        {
            resource_options.push(serde_json::json!("settings"));
        }
        let mut outcome_options = plugin_state["outcome_options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !outcome_options
            .iter()
            .any(|value| value.as_str() == Some("success"))
        {
            outcome_options.push(serde_json::json!("success"));
        }

        Ok((
            serde_json::json!({
                "state": {
                    "entries": current_entries,
                    "total": total,
                    "page": page,
                    "page_size": page_size,
                    "has_more": has_more,
                    "truncated": truncated,
                    "action_options": action_options,
                    "resource_options": resource_options,
                    "outcome_options": outcome_options,
                }
            }),
            etag,
        ))
    }

    pub async fn ticket_action(
        &self,
        guild: &str,
        scope_kind: &str,
        scope_id: &str,
        action: &str,
        data: Value,
        etag: &str,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !valid_ticket_action_scope(scope_kind, scope_id)
            || !(1..=64).contains(&action.len())
            || !action.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'.'
                    || byte == b'_'
                    || byte == b'-'
            })
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid ticket action".into(),
                Vec::new(),
            ));
        }
        self.request_keyed(
            Method::POST,
            &format!("/v1/guilds/{guild}/ticket-actions/{scope_kind}/{scope_id}"),
            Some(serde_json::json!({ "action": action, "data": data })),
            Some(etag),
            Some(actor),
            Some(idempotency_key),
            None,
        )
        .await
    }

    /// # Errors
    /// Returns a sanitized API or transport error.
    pub async fn member_search(
        &self,
        guild: &str,
        scope_kind: &str,
        scope_id: &str,
        query: &str,
        actor: &str,
    ) -> Result<(Value, Option<String>), ApiError> {
        if !matches!(scope_kind, "guild" | "panel")
            || !valid_ticket_action_scope(scope_kind, scope_id)
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid member search scope".into(),
                Vec::new(),
            ));
        }
        let query = serde_urlencoded::to_string([
            ("scope_kind", scope_kind),
            ("scope_id", scope_id),
            ("q", query),
            ("limit", "25"),
        ])
        .map_err(|_| ApiError::unavailable())?;
        self.request(
            Method::GET,
            &format!("/v1/guilds/{guild}/members/search?{query}"),
            None,
            None,
            Some(actor),
        )
        .await
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
            None,
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
fn valid_gallery_template_id(id: &str) -> bool {
    id.parse::<i64>().is_ok_and(|id| id > 0)
}

fn valid_ticket_action_scope(scope_kind: &str, scope_id: &str) -> bool {
    match scope_kind {
        "guild" => scope_id == "guild",
        "knowledge" => scope_id == "knowledge",
        "analytics" => scope_id == "analytics",
        "integrations" => scope_id == "integrations",
        "tags" => scope_id == "tags",
        "labels" => scope_id == "labels",
        "gallery" => scope_id == "gallery",
        "history" | "ticket" => crate::session::snowflake(scope_id),
        "panel" => {
            (1..=32).contains(&scope_id.len())
                && scope_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        }
        _ => false,
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
