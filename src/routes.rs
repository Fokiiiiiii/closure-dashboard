use crate::{
    closure_api::{Api, ApiError},
    config::Config,
    html::escape,
    i18n::{Catalogs, Locale, REGISTRY, keys},
    session,
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
    catalogs: std::sync::Arc<Catalogs>,
}

/// Creates the dashboard router.
///
/// # Panics
/// Panics if the embedded locale registry is invalid.
pub fn router(config: Config) -> Router {
    let app = App {
        api: Api::new(&config),
        config,
        catalogs: std::sync::Arc::new(
            Catalogs::new(REGISTRY).unwrap_or_else(|e| panic!("invalid embedded catalogs: {e}")),
        ),
    };
    Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/auth/discord", get(discord))
        .route("/auth/callback", get(callback))
        .route("/auth/dev", post(dev))
        .route("/logout", post(logout))
        .route("/theme", post(theme))
        .route("/lang", post(set_language))
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
fn lang<'a>(headers: &HeaderMap, app: &'a App) -> Locale<'a> {
    app.catalogs.negotiate(
        cookie(headers, "lang"),
        headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
    )
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
    let language = lang(request.headers(), &app);
    let public =
        matches!(path, "/login" | "/app.css" | "/healthz" | "/lang") || path.starts_with("/auth/");
    let mut response = if request.method() == Method::POST && !csrf(request.headers(), &app) {
        error_page(StatusCode::FORBIDDEN, language, keys::DENIED)
    } else if !public && user(request.headers(), &app).is_none() {
        Redirect::to("/login").into_response()
    } else if let Some(id) = path.strip_prefix("/g/").and_then(|p| p.split('/').next()) {
        if !session::snowflake(id) {
            error_page(StatusCode::BAD_REQUEST, language, keys::INVALID)
        } else if let Some(actor) = user(request.headers(), &app) {
            match app.api.access(id, &actor).await {
                Ok((access, _)) if access["allowed"] == true => run(next, request).await,
                Ok(_) => error_page(StatusCode::FORBIDDEN, language, keys::DENIED),
                Err(e) => api_error(&e, language),
            }
        } else {
            Redirect::to("/login").into_response()
        }
    } else {
        run(next, request).await
    };
    if response.status().is_client_error()
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/plain"))
    {
        response = error_page(response.status(), language, keys::INVALID);
    }
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_LANGUAGE,
        HeaderValue::from_str(language.tag()).unwrap_or_else(|_| HeaderValue::from_static("ja")),
    );
    h.insert(
        header::VARY,
        HeaderValue::from_static("Accept-Language, Cookie"),
    );
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
fn language_form(language: Locale<'_>) -> String {
    let mut options = String::new();
    for definition in language.catalogs().definitions() {
        let selected = if definition.tag == language.tag() {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            options,
            "<option value=\"{}\"{selected}>{}</option>",
            escape(definition.tag),
            escape(definition.name)
        );
    }
    format!(
        "<form method=\"post\" action=\"/lang\"><label>{}<select name=\"lang\">{options}</select></label><button>{}</button></form>",
        language.text(keys::LANGUAGE),
        language.text(keys::APPLY)
    )
}
fn page(language: Locale<'_>, headers: &HeaderMap, content: &str) -> Response {
    let theme = cookie(headers, "theme")
        .filter(|x| matches!(*x, "light" | "dark"))
        .unwrap_or("auto");
    Html(format!("<!doctype html><html lang=\"{}\" dir=\"{}\" data-theme=\"{theme}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"/app.css\"></head><body><header>{}</header>{content}</body></html>", escape(language.tag()), escape(language.dir()), language.text(keys::TITLE), language_form(language))).into_response()
}
fn error_page(status: StatusCode, language: Locale<'_>, key: &str) -> Response {
    (
        status,
        page(
            language,
            &HeaderMap::new(),
            &format!(
                "<main><p role=\"alert\">{}</p><a href=\"/\">{}</a></main>",
                language.text(key),
                language.text(keys::SERVERS)
            ),
        ),
    )
        .into_response()
}
fn api_error(error: &ApiError, language: Locale<'_>) -> Response {
    let key = match error.status {
        StatusCode::FORBIDDEN => keys::DENIED,
        StatusCode::NOT_FOUND => keys::NOTFOUND,
        StatusCode::SERVICE_UNAVAILABLE => keys::UNAVAILABLE,
        StatusCode::PRECONDITION_FAILED => keys::CONFLICT,
        _ => keys::INVALID,
    };
    error_page(error.status, language, key)
}
async fn login(State(app): State<App>, headers: HeaderMap) -> Response {
    let l = lang(&headers, &app);
    let mut content = format!(
        "<main><h1>{}</h1><a href=\"/auth/discord\">{}</a>",
        l.text(keys::TITLE),
        l.text(keys::LOGIN)
    );
    if cfg!(debug_assertions) && app.config.dev_user.is_some() {
        let _ = write!(
            content,
            "<form method=\"post\" action=\"/auth/dev\"><button>{}</button></form>",
            l.text(keys::DEV)
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
        return error_page(
            StatusCode::INTERNAL_SERVER_ERROR,
            lang(&headers, &app),
            keys::OAUTH,
        );
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
    let l = lang(&headers, &app);
    let state = query.get("state").map_or("", String::as_str);
    let expected = cookie(&headers, "oauth_state").unwrap_or("");
    let failure = || {
        set_cookie(
            error_page(StatusCode::BAD_REQUEST, l, keys::OAUTH),
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
    error_page(StatusCode::NOT_FOUND, lang(&headers, &app), keys::NOTFOUND)
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
        return error_page(StatusCode::BAD_REQUEST, lang(&headers, &app), keys::INVALID);
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
async fn set_language(
    State(app): State<App>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let Some(value) = form
        .get("lang")
        .filter(|v| app.catalogs.definitions().any(|d| d.tag == v.as_str()))
    else {
        return error_page(StatusCode::BAD_REQUEST, lang(&headers, &app), keys::INVALID);
    };
    let dest = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<axum::http::Uri>().ok())
        .filter(|u| {
            u.scheme_str()
                .zip(u.authority())
                .is_some_and(|(s, a)| Some(format!("{s}://{a}")) == own_origin(&app))
        })
        .and_then(|u| u.path_and_query().map(|p| p.as_str().to_owned()))
        .filter(|p| p.starts_with('/') && !p.starts_with("//") && !p.contains('\\'))
        .unwrap_or_else(|| "/".into());
    set_cookie(
        Redirect::to(&dest).into_response(),
        &cookie_value(&app, "lang", value, 31_536_000),
    )
}
async fn index(State(app): State<App>, headers: HeaderMap) -> Response {
    let l = lang(&headers, &app);
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
    let mut content = format!("<main><h1>{}</h1><ul>", l.text(keys::SERVERS));
    for (id, name) in &allowed {
        let _ = write!(content, "<li><a href=\"/g/{id}\">{}</a></li>", escape(name));
    }
    content.push_str("</ul>");
    if allowed.is_empty() {
        content.push_str(&l.text(keys::EMPTY));
    }
    let _ = write!(
        content,
        "<form method=\"post\" action=\"/logout\"><button>{}</button></form></main>",
        l.text(keys::LOGOUT)
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
fn theme_options(headers: &HeaderMap, language: Locale<'_>) -> String {
    let current_theme = cookie(headers, "theme").unwrap_or("auto");
    let mut options = String::new();
    for (value, key) in [
        ("auto", keys::AUTO),
        ("light", keys::LIGHT),
        ("dark", keys::DARK),
    ] {
        let selected = if value == current_theme {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            options,
            "<option value=\"{value}\"{selected}>{}</option>",
            language.text(key)
        );
    }
    options
}
async fn guild(State(app): State<App>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let l = lang(&headers, &app);
    let (guilds, effective, history, schema) = tokio::join!(
        app.api.guilds(),
        app.api.effective(&id),
        app.api.history(&id),
        app.api.schema(l.bot_language())
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
        .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
    let options = theme_options(&headers, l);
    let mut content = format!(
        "<div class=\"shell\"><aside><a href=\"/\">{}</a><nav><a href=\"/g/{id}\">{}</a></nav><form method=\"post\" action=\"/logout\"><button>{}</button></form><form method=\"post\" action=\"/theme\"><label>{}<select name=\"theme\">{options}</select></label><button>{}</button></form></aside><div class=\"workspace\"><div class=\"breadcrumb\"><a href=\"/\">{}</a> / {} / {}</div><main><h1>{}</h1><section class=\"stats\"><div class=\"card\">{}<strong>{}</strong></div><div class=\"card\">{}<strong>{active} / {}</strong></div><div class=\"card\">{}<strong>{last}</strong></div></section><section class=\"card\"><h2>{}</h2><table><tbody>",
        escape(name),
        l.text(keys::OVERVIEW),
        l.text(keys::LOGOUT),
        l.text(keys::THEME),
        l.text(keys::APPLY),
        l.text(keys::SERVERS),
        escape(name),
        l.text(keys::OVERVIEW),
        l.text(keys::OVERVIEW),
        l.text(keys::BOT),
        l.text(keys::CONNECTED),
        l.text(keys::ACTIVE),
        modules.len(),
        l.text(keys::LAST),
        l.text(keys::MODULES)
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
            l.text(if enabled {
                keys::ENABLED
            } else {
                keys::DISABLED
            }),
            escape(&module),
            !enabled,
            escape(title)
        );
    }
    let _ = write!(
        content,
        "</tbody></table></section><section class=\"card\"><h2>{}</h2><ul>",
        l.text(keys::RECENT)
    );
    if history_items.is_empty() {
        content.push_str(&l.text(keys::NONE));
    }
    for item in history_items.iter().take(4) {
        let _ = write!(
            content,
            "<li>{} · {}</li>",
            escape(item["actor"].as_str().unwrap_or("—")),
            item["created_at_unix"]
                .as_i64()
                .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()))
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
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let Some(enabled) = form.get("enabled").and_then(|s| match s.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID);
    };
    if !valid_module(&module) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID);
    }
    let schema = match app.api.schema(l.bot_language()).await {
        Ok((v, _)) => v,
        Err(e) => return api_error(&e, l),
    };
    if !module_names(&schema).contains_key(&module) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID);
    }
    for attempt in 0..2 {
        let (mut config, etag) = match app.api.config(&id).await {
            Ok(v) => v,
            Err(e) => return api_error(&e, l),
        };
        let Some(etag) = etag else {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID);
        };
        if !config.is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID);
        }
        if config["modules"].is_null() {
            config["modules"] = json!({});
        }
        if !config["modules"].is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID);
        }
        if config["modules"][&module].is_null() {
            config["modules"][&module] = json!({});
        }
        if !config["modules"][&module].is_object() {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID);
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
                return (
                    e.status,
                    page(
                        l,
                        &headers,
                        &format!("<main><p role=\"alert\">{text}</p></main>"),
                    ),
                )
                    .into_response();
            }
            Err(e) => return api_error(&e, l),
        }
    }
    error_page(StatusCode::PRECONDITION_FAILED, l, keys::CONFLICT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Definition;

    #[tokio::test]
    async fn rtl_page_uses_locale_direction() {
        let catalogs = Catalogs::new(&[
            REGISTRY[0],
            Definition {
                tag: "ar",
                name: "العربية",
                dir: "rtl",
                source: "title = عربي",
            },
        ])
        .unwrap();
        let response = page(catalogs.locale("ar"), &HeaderMap::new(), "");
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("<html lang=\"ar\" dir=\"rtl\""));
    }
}
