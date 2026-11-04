use crate::{
    closure_api::{Api, ApiError},
    config::Config,
    html::escape,
    messages, session,
};
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use serde_json::{Value, json};
use std::{collections::HashMap, fmt::Write, io::Read, time::Duration};
use subtle::ConstantTimeEq;

#[derive(Clone)]
struct App {
    config: Config,
    api: Api,
}

pub fn router(config: Config) -> Router {
    let app = App {
        api: Api::new(&config),
        config,
    };
    Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/auth/discord", get(discord))
        .route("/auth/callback", get(callback))
        .route("/auth/dev", post(dev))
        .route("/logout", post(logout))
        .route("/theme", post(theme))
        .route("/g/{id}", get(guild))
        .route("/g/{id}/modules/{module}/toggle", post(toggle))
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../assets/app.css"),
                )
            }),
        )
        .layer(DefaultBodyLimit::max(65536))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(65536))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
fn lang(headers: &HeaderMap) -> &'static str {
    let header = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mut best = (0.0_f32, "ja");
    for part in header.split(',') {
        let mut p = part.trim().split(';');
        let tag = p.next().unwrap_or("").to_ascii_lowercase();
        let q = p
            .find_map(|x| {
                x.trim()
                    .strip_prefix("q=")
                    .and_then(|v| v.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        let l = if tag == "en" || tag.starts_with("en-") {
            "en"
        } else if tag == "ja" || tag.starts_with("ja-") {
            "ja"
        } else {
            continue;
        };
        if q > best.0 {
            best = (q, l);
        }
    }
    best.1
}
fn cookie<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|s| s.trim().split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
}
fn user(headers: &HeaderMap, app: &App) -> Option<String> {
    session::verify(
        cookie(headers, "session")?,
        &app.config.session_secret.0,
        session::now(),
    )
}
fn cookie_value(app: &App, key: &str, value: &str, age: u64) -> String {
    format!(
        "{key}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{}",
        if app.config.redirect_uri.starts_with("https://") {
            "; Secure"
        } else {
            ""
        }
    )
}
fn set_cookie(mut response: Response, value: &str) -> Response {
    if let Ok(h) = HeaderValue::from_str(value) {
        response.headers_mut().append(header::SET_COOKIE, h);
    }
    response
}
fn own_origin(app: &App) -> Option<String> {
    let uri: axum::http::Uri = app.config.redirect_uri.parse().ok()?;
    Some(format!("{}://{}", uri.scheme_str()?, uri.authority()?))
}
fn csrf(headers: &HeaderMap, app: &App) -> bool {
    let Some(origin) = own_origin(app) else {
        return false;
    };
    if let Some(value) = headers.get(header::ORIGIN) {
        return value.to_str().is_ok_and(|v| v == origin);
    }
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<axum::http::Uri>().ok())
        .is_some_and(|u| {
            u.scheme_str()
                .zip(u.authority())
                .is_some_and(|(s, a)| format!("{s}://{a}") == origin)
        })
}
async fn guard(State(app): State<App>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let language = lang(request.headers());
    let public = matches!(path, "/login" | "/app.css" | "/healthz") || path.starts_with("/auth/");
    let mut response = if request.method() == Method::POST && !csrf(request.headers(), &app) {
        error_page(StatusCode::FORBIDDEN, language, "denied")
    } else if !public && user(request.headers(), &app).is_none() {
        Redirect::to("/login").into_response()
    } else if let Some(id) = path.strip_prefix("/g/").and_then(|p| p.split('/').next()) {
        if !session::snowflake(id) {
            error_page(StatusCode::BAD_REQUEST, language, "invalid")
        } else if let Some(actor) = user(request.headers(), &app) {
            match app.api.access(id, &actor).await {
                Ok((access, _)) if access["allowed"] == true => run(next, request).await,
                Ok(_) => error_page(StatusCode::FORBIDDEN, language, "denied"),
                Err(e) => api_error(&e, language),
            }
        } else {
            Redirect::to("/login").into_response()
        }
    } else {
        run(next, request).await
    };
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; style-src 'self'; img-src https://cdn.discordapp.com data:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"));
    response
}
async fn run(next: Next, request: Request) -> Response {
    tokio::time::timeout(Duration::from_secs(15), next.run(request))
        .await
        .unwrap_or_else(|_| StatusCode::GATEWAY_TIMEOUT.into_response())
}
fn page(language: &str, headers: &HeaderMap, content: &str) -> Response {
    let theme = cookie(headers, "theme")
        .filter(|x| matches!(*x, "light" | "dark"))
        .unwrap_or("auto");
    Html(format!("<!doctype html><html lang=\"{language}\" data-theme=\"{theme}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"/app.css\"></head><body>{content}</body></html>", messages::get(language,"title"))).into_response()
}
fn error_page(status: StatusCode, language: &str, key: &str) -> Response {
    (status, Html(format!("<!doctype html><html lang=\"{language}\"><meta charset=\"utf-8\"><link rel=\"stylesheet\" href=\"/app.css\"><main><p role=\"alert\">{}</p><a href=\"/\">{}</a></main></html>", messages::get(language,key), messages::get(language,"servers")))).into_response()
}
fn api_error(error: &ApiError, language: &str) -> Response {
    let key = match error.status {
        StatusCode::FORBIDDEN => "denied",
        StatusCode::NOT_FOUND => "notfound",
        StatusCode::SERVICE_UNAVAILABLE => "unavailable",
        StatusCode::PRECONDITION_FAILED => "conflict",
        _ => "invalid",
    };
    error_page(error.status, language, key)
}
async fn login(State(app): State<App>, headers: HeaderMap) -> Response {
    let l = lang(&headers);
    let mut content = format!(
        "<main><h1>{}</h1><a href=\"/auth/discord\">{}</a>",
        messages::get(l, "title"),
        messages::get(l, "login")
    );
    if cfg!(debug_assertions) && app.config.dev_user.is_some() {
        let _ = write!(
            content,
            "<form method=\"post\" action=\"/auth/dev\"><button>{}</button></form>",
            messages::get(l, "dev")
        );
    }
    content.push_str("</main>");
    page(l, &headers, &content)
}
async fn discord(State(app): State<App>, headers: HeaderMap) -> Response {
    let mut bytes = [0_u8; 32];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_err()
    {
        return error_page(StatusCode::INTERNAL_SERVER_ERROR, lang(&headers), "oauth");
    }
    let state = URL_SAFE_NO_PAD.encode(bytes);
    let query = serde_urlencoded::to_string([
        ("client_id", app.config.client_id.as_str()),
        ("redirect_uri", app.config.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", "identify"),
        ("state", state.as_str()),
    ])
    .unwrap_or_default();
    let response = (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            format!("https://discord.com/oauth2/authorize?{query}"),
        )],
    )
        .into_response();
    set_cookie(response, &cookie_value(&app, "oauth_state", &state, 600))
}
async fn callback(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers);
    let state = query.get("state").map_or("", String::as_str);
    let expected = cookie(&headers, "oauth_state").unwrap_or("");
    let failure = || {
        set_cookie(
            error_page(StatusCode::BAD_REQUEST, l, "oauth"),
            &cookie_value(&app, "oauth_state", "", 0),
        )
    };
    if expected.is_empty()
        || state.len() != expected.len()
        || !bool::from(state.as_bytes().ct_eq(expected.as_bytes()))
    {
        return failure();
    }
    let Some(code) = query.get("code") else {
        return failure();
    };
    let form = serde_urlencoded::to_string([
        ("client_id", app.config.client_id.as_str()),
        ("client_secret", app.config.client_secret.0.as_str()),
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", app.config.redirect_uri.as_str()),
    ])
    .unwrap_or_default();
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    let client: Client<_, Body> = Client::builder(TokioExecutor::new()).build(connector);
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let req = axum::http::Request::builder()
            .method(Method::POST)
            .uri(format!("{}/oauth2/token", app.config.discord_api))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form))
            .ok()?;
        let res = client.request(req).await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let json: Value =
            serde_json::from_slice(&res.into_body().collect().await.ok()?.to_bytes()).ok()?;
        let token = json["access_token"].as_str()?;
        let req = axum::http::Request::builder()
            .uri(format!("{}/users/@me", app.config.discord_api))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .ok()?;
        let res = client.request(req).await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let json: Value =
            serde_json::from_slice(&res.into_body().collect().await.ok()?.to_bytes()).ok()?;
        json["id"]
            .as_str()
            .filter(|id| session::snowflake(id))
            .map(str::to_owned)
    })
    .await
    .ok()
    .flatten();
    let Some(id) = result else { return failure() };
    let response = set_cookie(
        Redirect::to("/").into_response(),
        &cookie_value(&app, "oauth_state", "", 0),
    );
    set_cookie(
        response,
        &cookie_value(
            &app,
            "session",
            &session::sign(&id, &app.config.session_secret.0, session::now()),
            session::AGE,
        ),
    )
}
async fn dev(State(app): State<App>, headers: HeaderMap) -> Response {
    if cfg!(debug_assertions)
        && let Some(id) = &app.config.dev_user
    {
        return set_cookie(
            Redirect::to("/").into_response(),
            &cookie_value(
                &app,
                "session",
                &session::sign(id, &app.config.session_secret.0, session::now()),
                session::AGE,
            ),
        );
    }
    error_page(StatusCode::NOT_FOUND, lang(&headers), "notfound")
}
async fn logout(State(app): State<App>) -> Response {
    set_cookie(
        Redirect::to("/login").into_response(),
        &cookie_value(&app, "session", "", 0),
    )
}
async fn theme(
    State(app): State<App>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let Some(value) = form
        .get("theme")
        .filter(|v| matches!(v.as_str(), "light" | "dark" | "auto"))
    else {
        return error_page(StatusCode::BAD_REQUEST, lang(&headers), "invalid");
    };
    let dest = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<axum::http::Uri>().ok())
        .and_then(|u| u.path_and_query().map(|p| p.as_str().to_owned()))
        .filter(|p| p.starts_with('/') && !p.starts_with("//"))
        .unwrap_or_else(|| "/".into());
    set_cookie(
        Redirect::to(&dest).into_response(),
        &cookie_value(&app, "theme", value, 31_536_000),
    )
}
async fn index(State(app): State<App>, headers: HeaderMap) -> Response {
    let l = lang(&headers);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let (guilds, _) = match app.api.guilds().await {
        Ok(v) => v,
        Err(e) => return api_error(&e, l),
    };
    let mut allowed = Vec::new();
    for guild in guilds.as_array().into_iter().flatten() {
        let Some(id) = guild["id"].as_str().filter(|s| session::snowflake(s)) else {
            continue;
        };
        match app.api.access(id, &actor).await {
            Ok((access, _)) if access["allowed"] == true => allowed.push((
                id.to_owned(),
                guild["name"].as_str().unwrap_or(id).to_owned(),
            )),
            Ok(_) => {}
            Err(e) => return api_error(&e, l),
        }
    }
    if allowed.len() == 1 {
        return Redirect::to(&format!("/g/{}", allowed[0].0)).into_response();
    }
    let mut content = format!("<main><h1>{}</h1><ul>", messages::get(l, "servers"));
    for (id, name) in &allowed {
        let _ = write!(content, "<li><a href=\"/g/{id}\">{}</a></li>", escape(name));
    }
    content.push_str("</ul>");
    if allowed.is_empty() {
        content.push_str(messages::get(l, "empty"));
    }
    let _ = write!(
        content,
        "<form method=\"post\" action=\"/logout\"><button>{}</button></form></main>",
        messages::get(l, "logout")
    );
    page(l, &headers, &content)
}
fn module_names(schema: &Value) -> serde_json::Map<String, Value> {
    let modules = &schema["properties"]["modules"];
    modules["properties"]
        .as_object()
        .or_else(|| {
            modules["anyOf"]
                .as_array()
                .and_then(|a| a.iter().find_map(|v| v["properties"].as_object()))
        })
        .cloned()
        .unwrap_or_default()
}
fn theme_options(headers: &HeaderMap, language: &str) -> String {
    let current_theme = cookie(headers, "theme").unwrap_or("auto");
    let mut options = String::new();
    for value in ["auto", "light", "dark"] {
        let selected = if value == current_theme {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            options,
            "<option value=\"{value}\"{selected}>{}</option>",
            messages::get(language, value)
        );
    }
    options
}
async fn guild(State(app): State<App>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let l = lang(&headers);
    let (guilds, effective, history, schema) = tokio::join!(
        app.api.guilds(),
        app.api.effective(&id),
        app.api.history(&id),
        app.api.schema(l)
    );
    let (guilds, effective, history, schema) = match (guilds, effective, history, schema) {
        (Ok((guilds, _)), Ok((effective, _)), Ok((history, _)), Ok((schema, _))) => {
            (guilds, effective, history, schema)
        }
        (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => {
            return api_error(&e, l);
        }
    };
    let name = guilds
        .as_array()
        .and_then(|a| a.iter().find(|g| g["id"] == id))
        .and_then(|g| g["name"].as_str())
        .unwrap_or(&id);
    let modules = module_names(&schema);
    let active = modules
        .keys()
        .filter(|k| effective["modules"][*k]["enabled"] == true)
        .count();
    let history_items = history.as_array().cloned().unwrap_or_default();
    let last = history_items
        .first()
        .and_then(|v| v["created_at_unix"].as_i64())
        .map_or_else(
            || "—".into(),
            |v| messages::relative_time(l, v, session::now()),
        );
    let options = theme_options(&headers, l);
    let mut content = format!(
        "<div class=\"shell\"><aside><a href=\"/\">{}</a><nav><a href=\"/g/{id}\">{}</a></nav><form method=\"post\" action=\"/logout\"><button>{}</button></form><form method=\"post\" action=\"/theme\"><label>{}<select name=\"theme\">{options}</select></label><button>{}</button></form></aside><div class=\"workspace\"><div class=\"breadcrumb\"><a href=\"/\">{}</a> / {} / {}</div><main><h1>{}</h1><section class=\"stats\"><div class=\"card\">{}<strong>{}</strong></div><div class=\"card\">{}<strong>{active} / {}</strong></div><div class=\"card\">{}<strong>{last}</strong></div></section><section class=\"card\"><h2>{}</h2><table><tbody>",
        escape(name),
        messages::get(l, "overview"),
        messages::get(l, "logout"),
        messages::get(l, "theme"),
        messages::get(l, "apply"),
        messages::get(l, "servers"),
        escape(name),
        messages::get(l, "overview"),
        messages::get(l, "overview"),
        messages::get(l, "bot"),
        messages::get(l, "connected"),
        messages::get(l, "active"),
        modules.len(),
        messages::get(l, "last"),
        messages::get(l, "modules")
    );
    for (module, def) in modules {
        if !valid_module(&module) {
            continue;
        }
        let title = def["title"].as_str().unwrap_or(&module);
        let enabled = effective["modules"][&module]["enabled"] == true;
        let _ = write!(
            content,
            "<tr><td>{}</td><td><span class=\"badge\">{}</span></td><td><form method=\"post\" action=\"/g/{id}/modules/{}/toggle\"><input type=\"hidden\" name=\"enabled\" value=\"{}\"><button role=\"switch\" aria-checked=\"{enabled}\" aria-label=\"{}\">◉</button></form></td></tr>",
            escape(title),
            messages::get(l, if enabled { "enabled" } else { "disabled" }),
            escape(&module),
            !enabled,
            escape(title)
        );
    }
    let _ = write!(
        content,
        "</tbody></table></section><section class=\"card\"><h2>{}</h2><ul>",
        messages::get(l, "recent")
    );
    if history_items.is_empty() {
        content.push_str(messages::get(l, "none"));
    }
    for item in history_items.iter().take(4) {
        let _ = write!(
            content,
            "<li>{} · {}</li>",
            escape(item["actor"].as_str().unwrap_or("—")),
            item["created_at_unix"].as_i64().map_or_else(
                || "—".into(),
                |v| messages::relative_time(l, v, session::now())
            )
        );
    }
    content.push_str("</ul></section></main></div></div>");
    page(l, &headers, &content)
}
fn valid_module(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
async fn toggle(
    State(app): State<App>,
    Path((id, module)): Path<(String, String)>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let Some(enabled) = form.get("enabled").and_then(|s| match s.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }) else {
        return error_page(StatusCode::BAD_REQUEST, l, "invalid");
    };
    if !valid_module(&module) {
        return error_page(StatusCode::BAD_REQUEST, l, "invalid");
    }
    let schema = match app.api.schema(l).await {
        Ok((v, _)) => v,
        Err(e) => return api_error(&e, l),
    };
    if !module_names(&schema).contains_key(&module) {
        return error_page(StatusCode::BAD_REQUEST, l, "invalid");
    }
    for attempt in 0..2 {
        let (mut config, etag) = match app.api.config(&id).await {
            Ok(v) => v,
            Err(e) => return api_error(&e, l),
        };
        let Some(etag) = etag else {
            return error_page(StatusCode::BAD_GATEWAY, l, "invalid");
        };
        if !config.is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, "invalid");
        }
        if config["modules"].is_null() {
            config["modules"] = json!({});
        }
        if !config["modules"].is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, "invalid");
        }
        if config["modules"][&module].is_null() {
            config["modules"][&module] = json!({});
        }
        if !config["modules"][&module].is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, "invalid");
        }
        config["modules"][&module]["enabled"] = json!(enabled);
        match app.api.put_config(&id, config, &etag, &actor).await {
            Ok(_) => return Redirect::to(&format!("/g/{id}")).into_response(),
            Err(e) if e.status == StatusCode::PRECONDITION_FAILED && attempt == 0 => {}
            Err(e) if e.status == StatusCode::UNPROCESSABLE_ENTITY => {
                let text = std::iter::once(e.message)
                    .chain(e.issues.into_iter().map(|i| i.message))
                    .map(|s| escape(&s))
                    .collect::<Vec<_>>()
                    .join(": ");
                return (e.status, Html(format!("<p role=\"alert\">{text}</p>"))).into_response();
            }
            Err(e) => return api_error(&e, l),
        }
    }
    error_page(StatusCode::PRECONDITION_FAILED, l, "conflict")
}
