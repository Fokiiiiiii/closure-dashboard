use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{Method, StatusCode, header},
    response::IntoResponse,
    routing::any,
};
use closure_dashboard::{
    closure_api::{Api, ErrorKind},
    config::{Config, Secret},
    router, session,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[tokio::test]
async fn language_negotiation_and_removed_routes() {
    let (config, _) = setup().await;
    for path in ["/lang", "/theme"] {
        let response = router(config.clone())
            .oneshot(request(&config, Method::POST, path, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let req = Request::builder()
        .uri("/login")
        .header(header::COOKIE, "lang=ja; theme=dark")
        .header(header::ACCEPT_LANGUAGE, "en")
        .body(Body::empty())
        .unwrap();
    let response = router(config).oneshot(req).await.unwrap();
    assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "en");
    let body = String::from_utf8(
        to_bytes(response.into_body(), 65536)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("<html lang=\"en\" dir=\"ltr\" data-theme=\"auto\""));
    assert!(body.contains("Sign in with Discord"));
    assert!(!body.contains("name=\"lang\""));
    assert!(!body.contains("name=\"theme\""));
}

#[derive(Default)]
struct Fake {
    calls: Vec<(String, String, String, String)>,
    puts: usize,
    status: Option<StatusCode>,
    validation: bool,
    denied: bool,
    empty: bool,
    secret_body: bool,
    always_conflict: bool,
    put_forbidden: bool,
    invalid_schema: bool,
    stats_status: Option<StatusCode>,
    assigned: bool,
    queries: Vec<String>,
    forms: bool,
    directory_failure: bool,
    form_put_status: Option<StatusCode>,
    form_body: Option<serde_json::Value>,
}
async fn bot(State(state): State<Arc<Mutex<Fake>>>, request: Request) -> impl IntoResponse {
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    let method = request.method().clone();
    let auth = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let etag = request
        .headers()
        .get(header::IF_MATCH)
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let actor = request
        .headers()
        .get("x-closure-actor")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = to_bytes(request.into_body(), 65536).await.unwrap();
    let mut fake = state.lock().unwrap();
    fake.queries.push(format!("{path}?{query}"));
    fake.calls.push((path.clone(), auth, etag, actor));
    if let Some(status) = fake.status {
        return (status, [(header::ETAG, "\"1\"")], json!({"error":{"code":"secret-token", "message":"secret-token", "issues":[{"path":"x", "code":"x", "message":"secret-token"}]}}).to_string());
    }
    if fake.forms {
        return forms::bot_data(&mut fake, &path, &method, &body);
    }
    if path.ends_with("/stats")
        && let Some(status) = fake.stats_status
    {
        return (status, [(header::ETAG, "\"1\"")], "{}".into());
    }
    let value = if path.ends_with("/access/123") {
        json!({"allowed":!fake.denied})
    } else if path == "/v1/guilds" {
        if fake.secret_body {
            json!({"secret-token": "secret-token"})
        } else if fake.empty {
            json!([])
        } else {
            json!([{"id":"42","name":"<script>alert(1)</script>","icon":null}])
        }
    } else if path == "/v1/schema" {
        if fake.invalid_schema {
            json!({"properties":{"modules":{"properties":{"bad!":{"title":"bad"}}}}})
        } else {
            json!({"properties":{"modules":{"anyOf":[{"properties":{"mod":{"title":"<Mod>"}}}]}}})
        }
    } else if path.ends_with("/config") && method == Method::PUT {
        fake.puts += 1;
        if fake.put_forbidden {
            return (
                StatusCode::FORBIDDEN,
                [(header::ETAG, "\"1\"")],
                "{}".into(),
            );
        }
        if fake.validation {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                [(header::ETAG, "\"1\"")],
                json!({"error":{"message":"Invalid","issues":[{"message":"<bad>"}]}}).to_string(),
            );
        }
        if fake.puts == 1 || fake.always_conflict {
            return (
                StatusCode::PRECONDITION_FAILED,
                [(header::ETAG, "\"1\"")],
                "{}".to_owned(),
            );
        }
        let data: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(data["preset"], "latest");
        assert_eq!(data["modules"]["mod"]["enabled"], true);
        json!({"revision":3})
    } else if path.ends_with("/config") {
        json!({"preset":"latest","modules":{"mod":{"other":1}}})
    } else if path.ends_with("/effective") {
        if fake.assigned {
            json!({"moderator_roles":[{"id":"10","level":"moderator"}],"permissions":{"roles":{"11":50},"users":{"12":5},"keys":{"ban":3}}})
        } else {
            json!({"moderator_roles":[],"permissions":{}})
        }
    } else if path.ends_with("/directory") {
        json!({"roles":[{"id":"10","name":"<script>","color":1234},{"id":"11","name":"Admin","color":0}]})
    } else if path.ends_with("/stats") {
        json!({"members":{"total":1284,"joins":5,"leaves":2,"by_day":[{"date":"2026-11-03","joins":null,"leaves":null},{"date":"2026-11-04","joins":5,"leaves":2}]},"messages":{"retention_days":7,"total":123,"by_day":[{"date":"2026-11-03","count":null},{"date":"2026-11-04","count":123}]},"cases":{"total":3,"by_day":[{"date":"2026-11-04","kinds":{"warn":2,"<script>":1}}],"recent":[{"number":12,"kind":"<script>","target":"<script>","moderator":"<script>","reason":"<script>","created_at_unix":1793790842,"revoked":true}]}})
    } else if path.ends_with("/history") {
        json!([{"revision":2,"actor":"<script>","document":{"modules":{"channels":{"enabled":true}}},"created_at_unix":session::now()-7200},{"revision":1,"actor":"moderator","document":{"modules":{"channels":{"enabled":false}}},"created_at_unix":session::now()-7200}])
    } else {
        json!([])
    };
    (StatusCode::OK, [(header::ETAG, "\"1\"")], value.to_string())
}

#[path = "integration/forms.rs"]
mod forms;
async fn setup() -> (Config, Arc<Mutex<Fake>>) {
    let state = Arc::new(Mutex::new(Fake::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback(any(bot)).with_state(state.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        Config {
            api_url: format!("http://{address}"),
            admin_token: Secret("secret-token".into()),
            session_secret: Secret("01234567890123456789012345678901".into()),
            client_id: "client".into(),
            client_secret: Secret("secret".into()),
            redirect_uri: "http://localhost/auth/callback".into(),
            bind: "127.0.0.1:0".into(),
            dev_user: Some("123".into()),
            discord_api: format!("http://{address}"),
        },
        state,
    )
}
fn request(config: &Config, method: Method, path: &str, body: &str) -> Request {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::ORIGIN, "http://localhost")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(
            header::COOKIE,
            format!(
                "session={}",
                session::sign("123", &config.session_secret.0, session::now())
            ),
        )
        .body(Body::from(body.to_owned()))
        .unwrap()
}

macro_rules! api_get_test {
    ($name:ident, $method:ident ( $($arg:expr),* ), $path:expr) => {
        #[tokio::test]
        async fn $name() {
            let (config, state) = setup().await;
            Api::new(&config).$method($($arg),*).await.unwrap();
            let fake = state.lock().unwrap();
            assert_eq!(fake.calls.len(), 1);
            assert_eq!(fake.calls[0].0, $path);
            assert_eq!(fake.calls[0].1, "Bearer secret-token");
        }
    };
}
api_get_test!(api_guilds, guilds(), "/v1/guilds");
api_get_test!(api_access, access("42", "123"), "/v1/guilds/42/access/123");
api_get_test!(api_schema, schema("ja"), "/v1/schema");
api_get_test!(
    api_effective,
    effective("42"),
    "/v1/guilds/42/config/effective"
);
api_get_test!(api_history, history("42"), "/v1/guilds/42/config/history");
api_get_test!(api_config, config("42"), "/v1/guilds/42/config");
api_get_test!(api_stats, stats("42", 7), "/v1/guilds/42/stats");
api_get_test!(api_directory, directory("42"), "/v1/guilds/42/directory");

#[tokio::test]
async fn basic_api_uses_view_and_conditional_actor_headers() {
    let (config, state) = setup().await;
    let api = Api::new(&config);
    api.basic_schema("ja").await.unwrap();
    api.basic_config("42").await.unwrap();
    let error = api
        .put_basic_config("42", json!({}), "\"1\"", "123")
        .await
        .unwrap_err();
    assert_eq!(error.status, StatusCode::PRECONDITION_FAILED);
    let fake = state.lock().unwrap();
    assert_eq!(fake.queries[0], "/v1/schema?view=basic&locale=ja");
    assert_eq!(fake.queries[1], "/v1/guilds/42/config?view=basic");
    assert_eq!(fake.queries[2], "/v1/guilds/42/config?view=basic");
    assert_eq!(fake.calls[2].2, "\"1\"");
    assert_eq!(fake.calls[2].3, "123");
}

#[tokio::test]
async fn statistics_render_and_validate_periods() {
    let (config, state) = setup().await;
    for (query, days) in [
        ("", 7),
        ("?days=14", 14),
        ("?days=30", 30),
        ("?days=8", 7),
        ("?days=bad", 7),
    ] {
        let response = router(config.clone())
            .oneshot(request(&config, Method::GET, &format!("/g/42{query}"), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(response.into_body(), 131072)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        for expected in [
            "1,284",
            "+3",
            "123",
            "role=\"img\"",
            "<title",
            "<details>",
            "表で見る",
            "series-1",
            "series-6",
            "その他",
            "&lt;script&gt;",
            "title=\"&lt;script&gt;\"",
            "取り消し済み",
        ] {
            assert!(html.contains(expected), "{expected}");
        }
        for forbidden in [
            "<script>",
            "style=",
            "module-row",
            "Bot",
            "最後の変更",
            "日平均",
            "新しい順",
            "保存されます",
            "chart-note",
            "chart-empty",
        ] {
            assert!(!html.contains(forbidden), "{forbidden}");
        }
        assert!(html.contains(&format!("?days={days}\" aria-current=\"true\"")));
        assert!(
            state
                .lock()
                .unwrap()
                .queries
                .iter()
                .any(|q| q == &format!("/v1/guilds/42/stats?days={days}"))
        );
    }
}

#[tokio::test]
async fn statistics_failure_preserves_shell() {
    let (config, state) = setup().await;
    for status in [StatusCode::NOT_FOUND, StatusCode::SERVICE_UNAVAILABLE] {
        state.lock().unwrap().stats_status = Some(status);
        let response = router(config.clone())
            .oneshot(request(&config, Method::GET, "/g/42?days=14", ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(response.into_body(), 65536)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        for expected in [
            "統計を取得できません",
            "履歴",
            "権限",
            "?days=14",
            "aria-current=\"page\"",
        ] {
            assert!(html.contains(expected), "{expected}");
        }
    }
}

#[tokio::test]
async fn history_and_permissions_render() {
    let (config, state) = setup().await;
    let anonymous = router(config.clone())
        .oneshot(
            Request::builder()
                .uri("/g/42/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::SEE_OTHER);
    for (path, expected) in [
        (
            "/g/42/history",
            vec![
                "modules.channels.enabled",
                "false → true",
                "初期設定",
                "&lt;script&gt;",
            ],
        ),
        ("/g/42/permissions", vec!["ロールの割り当てはありません"]),
    ] {
        let response = router(config.clone())
            .oneshot(request(&config, Method::GET, path, ""))
            .await
            .unwrap();
        let html = String::from_utf8(
            to_bytes(response.into_body(), 65536)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        for value in expected {
            assert!(html.contains(value), "{value}");
        }
        assert!(!html.contains("<script>"));
        assert!(!html.contains("style="));
        assert!(!html.contains("<span class=\"pill\">"));
        assert!(!html.contains("assignment-cards"));
    }
    state.lock().unwrap().assigned = true;
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/g/42/permissions", ""))
        .await
        .unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), 65536)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    for expected in [
        "moderator_roles",
        "permissions.roles",
        "moderator",
        "50",
        "&lt;script&gt;",
        "ユーザー個別の割り当て",
    ] {
        assert!(html.contains(expected), "{expected}");
    }
}

#[tokio::test]
async fn api_put_config_sends_precondition_and_actor() {
    let (config, state) = setup().await;
    let error = Api::new(&config)
        .put_config("42", json!({}), "\"1\"", "123")
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Conflict);
    assert_eq!(
        state.lock().unwrap().calls[0],
        (
            "/v1/guilds/42/config".into(),
            "Bearer secret-token".into(),
            "\"1\"".into(),
            "123".into()
        )
    );
}
#[tokio::test]
async fn access_retry_actor_and_html() {
    let (config, state) = setup().await;
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/g/42", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("1,284"));
    assert!(!html.contains("&lt;Mod&gt;"));
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let fake = state.lock().unwrap();
    assert_eq!(fake.puts, 2);
    let puts: Vec<_> = fake.calls.iter().filter(|x| !x.2.is_empty()).collect();
    assert_eq!(puts.len(), 2);
    assert!(
        puts.iter()
            .all(|x| x.1 == "Bearer secret-token" && x.2 == "\"1\"" && x.3 == "123")
    );
    assert_eq!(
        fake.calls
            .iter()
            .filter(|x| x.0.ends_with("/access/123"))
            .count(),
        2
    );
}
#[tokio::test]
async fn error_classification_and_redaction() {
    let (config, state) = setup().await;
    let api = Api::new(&config);
    for (status, kind) in [
        (401, ErrorKind::Unauthorized),
        (403, ErrorKind::Forbidden),
        (404, ErrorKind::NotFound),
        (412, ErrorKind::Conflict),
        (422, ErrorKind::Validation),
        (428, ErrorKind::Precondition),
        (503, ErrorKind::Unavailable),
        (429, ErrorKind::RateLimited),
        (413, ErrorKind::TooLarge),
    ] {
        state.lock().unwrap().status = Some(StatusCode::from_u16(status).unwrap());
        let error = api.guilds().await.unwrap_err();
        assert_eq!(error.kind, kind);
        assert!(!format!("{error:?}").contains("secret-token"));
    }
    assert!(!format!("{config:?}").contains("secret-token"));
    state.lock().unwrap().status = None;
    state.lock().unwrap().secret_body = true;
    let (body, _) = api.guilds().await.unwrap();
    assert!(!body.to_string().contains("secret-token"));
}
#[tokio::test]
async fn csrf_state_body_and_validation() {
    let (config, state) = setup().await;
    let anonymous = Request::builder().uri("/").body(Body::empty()).unwrap();
    assert_eq!(
        router(config.clone())
            .oneshot(anonymous)
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let mut bad = request(&config, Method::POST, "/logout", "");
    bad.headers_mut()
        .insert(header::ORIGIN, "http://evil.test".parse().unwrap());
    assert_eq!(
        router(config.clone()).oneshot(bad).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let callback = Request::builder()
        .uri("/auth/callback?state=wrongxxx&code=x")
        .header(header::COOKIE, "oauth_state=expected")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router(config.clone())
            .oneshot(callback)
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(state.lock().unwrap().calls.is_empty());
    let invalid = request(
        &config,
        Method::POST,
        "/g/42/modules/bad%21/toggle",
        "enabled=true",
    );
    assert_eq!(
        router(config.clone())
            .oneshot(invalid)
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let large = request(
        &config,
        Method::POST,
        "/g/42/modules/mod/toggle",
        &format!("enabled={}", "a".repeat(65 * 1024)),
    );
    assert_eq!(
        router(config).oneshot(large).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}
#[tokio::test]
async fn access_errors_are_preserved() {
    let (config, state) = setup().await;
    for code in [403, 404, 503] {
        state.lock().unwrap().status = Some(StatusCode::from_u16(code).unwrap());
        let response = router(config.clone())
            .oneshot(request(&config, Method::GET, "/g/42", ""))
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), code);
    }
}

#[tokio::test]
async fn denied_access_and_validation_messages() {
    let (config, state) = setup().await;
    state.lock().unwrap().denied = true;
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/g/42", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    {
        let mut fake = state.lock().unwrap();
        fake.denied = false;
        fake.validation = true;
    }
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    assert!(
        String::from_utf8(body.to_vec())
            .unwrap()
            .contains("Invalid: &lt;bad&gt;")
    );
}

#[tokio::test]
async fn oauth_exchanges_code_and_discards_token() {
    let (mut config, _) = setup().await;
    let server = Router::new()
        .route(
            "/oauth2/token",
            axum::routing::post(|| async { json!({"access_token":"temporary-token"}).to_string() }),
        )
        .route(
            "/users/@me",
            axum::routing::get(|headers: axum::http::HeaderMap| async move {
                assert_eq!(headers[header::AUTHORIZATION], "Bearer temporary-token");
                json!({"id":"123"}).to_string()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.discord_api = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });
    let start = router(config.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/discord")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::FOUND);
    let location = start.headers()[header::LOCATION].to_str().unwrap();
    assert!(location.contains("scope=identify"));
    let state_cookie = start.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let state = state_cookie.split_once('=').unwrap().1;
    let req = Request::builder()
        .uri(format!("/auth/callback?state={state}&code=code"))
        .header(header::COOKIE, &state_cookie)
        .body(Body::empty())
        .unwrap();
    let response = router(config).oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(cookies.iter().any(|s| s.starts_with("session=123.")));
    assert!(cookies.iter().all(|s| !s.contains("temporary-token")));
}

#[tokio::test]
async fn development_route_is_disabled_in_release() {
    let (config, _) = setup().await;
    let response = router(config.clone())
        .oneshot(request(&config, Method::POST, "/auth/dev", ""))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        if cfg!(debug_assertions) {
            StatusCode::SEE_OTHER
        } else {
            StatusCode::NOT_FOUND
        }
    );
}

#[tokio::test]
async fn anonymous_routes_redirect_to_login() {
    let (config, _) = setup().await;
    for path in ["/", "/g/42", "/g/42/modules/mod/toggle"] {
        let response = router(config.clone())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/login");
    }
}

#[tokio::test]
async fn login_goes_straight_to_discord_unless_the_dev_login_is_on() {
    let (mut config, _) = setup().await;
    let anonymous = || {
        Request::builder()
            .uri("/login")
            .body(Body::empty())
            .unwrap()
    };
    let response = router(config.clone()).oneshot(anonymous()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let signed_in = request(&config, Method::GET, "/login", "");
    let response = router(config.clone()).oneshot(signed_in).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/");
    config.dev_user = None;
    let response = router(config.clone()).oneshot(anonymous()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/auth/discord");
}

#[tokio::test]
async fn referer_and_origin_must_match() {
    let (config, _) = setup().await;
    for (origin, referer) in [
        (Some("http://evil.test"), None),
        (None, Some("http://evil.test/")),
        (None, None),
    ] {
        let mut req = request(&config, Method::POST, "/logout", "");
        req.headers_mut().remove(header::ORIGIN);
        if let Some(value) = origin {
            req.headers_mut()
                .insert(header::ORIGIN, value.parse().unwrap());
        }
        if let Some(value) = referer {
            req.headers_mut()
                .insert(header::REFERER, value.parse().unwrap());
        }
        assert_eq!(
            router(config.clone()).oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn state_cookie_has_security_attributes() {
    let (mut config, _) = setup().await;
    config.redirect_uri = "https://example.test/auth/callback".into();
    let response = router(config)
        .oneshot(
            Request::builder()
                .uri("/auth/discord")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    for attribute in [
        "oauth_state=",
        "Path=/",
        "HttpOnly",
        "SameSite=Lax",
        "Max-Age=600",
        "Secure",
    ] {
        assert!(cookie.contains(attribute), "{attribute}");
    }
}

#[path = "integration/remaining.rs"]
mod remaining;
