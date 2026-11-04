use super::*;

pub(super) fn bot_data(
    fake: &mut Fake,
    path: &str,
    method: &Method,
    body: &[u8],
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let fixture = |source: &str| serde_json::from_str::<serde_json::Value>(source).unwrap();
    let data = if path == "/v1/schema" {
        let mut schema = fixture(include_str!("../fixtures/schema-basic.json"));
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["choice"] =
            json!({"type":"string","enum":["one","<script>"]});
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["role"] = json!({"type":"string","x-ui":{"widget":"role"}});
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["<script>"] =
            json!({"type":"string","description":"<script>description</script>"});
        schema
    } else if path.ends_with("/config") && *method == Method::PUT {
        fake.form_body = Some(serde_json::from_slice(body).unwrap());
        if let Some(status) = fake.form_put_status {
            return (status, [(header::ETAG, "\"1\"")], json!({"error":{"message":"bad","issues":[{"path":"modules.welcome.config.channel","message":"<script>bad</script>"}]}}).to_string());
        }
        json!({"revision":2})
    } else if path.ends_with("/config") {
        fixture(include_str!("../fixtures/config-basic.json"))
    } else if path.ends_with("/effective") {
        fixture(include_str!("../fixtures/api-eff.json"))
    } else if path.ends_with("/directory") {
        if fake.directory_failure {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::ETAG, "\"1\"")],
                "{}".into(),
            );
        }
        json!({"channels":[{"id":"11","name":"<script>","kind":"text"},{"id":"12","name":"voice","kind":"voice"}],"roles":[{"id":"13","name":"role","managed":false},{"id":"14","name":"managed","managed":true}]})
    } else if path.ends_with("/access/123") {
        json!({"allowed":true})
    } else if path == "/v1/guilds" {
        json!([{"id":"42","name":"Server"}])
    } else {
        json!({})
    };
    (StatusCode::OK, [(header::ETAG, "\"1\"")], data.to_string())
}

async fn response(
    config: &Config,
    method: Method,
    path: &str,
    body: &str,
) -> axum::response::Response {
    router(config.clone())
        .oneshot(request(config, method, path, body))
        .await
        .unwrap()
}

async fn html(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 131072)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn list_and_form_render_schema_and_directory() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let page = response(&config, Method::GET, "/g/42/modules", "").await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = html(page).await;
    for expected in [
        "aria-current=\"page\"",
        "aria-checked=\"true\"",
        "/modules/welcome",
        "設定項目はありません",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    let page =
        html(response(&config, Method::GET, "/g/42/modules/welcome?saved=1", "").await).await;
    for expected in [
        "role=\"status\"",
        "type=\"checkbox\"",
        "type=\"radio\"",
        "<select",
        "&lt;script&gt;",
        "aria-describedby",
        "name=\"etag\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    for forbidden in ["<script>", "style=", ">voice</option>", ">managed</option>"] {
        assert!(!page.contains(forbidden), "{forbidden}");
    }
    let page = html(response(&config, Method::GET, "/g/42/modules/automod", "").await).await;
    for expected in ["type=\"number\"", "min=\"1\"", "max=\"365\""] {
        assert!(page.contains(expected), "{expected}");
    }
    let page = html(response(&config, Method::GET, "/g/42/modules/channels", "").await).await;
    assert!(page.contains("このモジュールに設定項目はありません"));
    state.lock().unwrap().directory_failure = true;
    let page = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(page.contains("type=\"text\""));
    assert!(page.contains("pattern=\"^[0-9]{1,20}$\""));
}

#[tokio::test]
async fn save_and_error_paths() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/modules/welcome";
    let body = "etag=%221%22&f.channel=11&f.include_bots=false&f.choice=one&f.role=&f.%3Cscript%3E=%3Cscript%3E";
    let saved = response(&config, Method::POST, path, body).await;
    assert_eq!(saved.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        saved.headers()[header::LOCATION],
        "/g/42/modules/welcome?saved=1"
    );
    {
        let state = state.lock().unwrap();
        let body = state.form_body.as_ref().unwrap();
        assert_eq!(body["modules"]["welcome"]["config"]["channel"], "11");
        assert_eq!(
            body["modules"]["welcome"]["config"]["role"],
            serde_json::Value::Null
        );
        assert_eq!(body["language"], "ja");
        assert!(
            body["modules"]["welcome"]["config"]
                .get("include_bots")
                .is_none()
        );
        assert_eq!(
            body["modules"]["logs"]["config"]["channel"],
            "123456789012345678"
        );
        assert!(
            state
                .queries
                .iter()
                .any(|q| q == "/v1/guilds/42/config?view=basic")
        );
        assert!(
            state
                .calls
                .iter()
                .any(|c| c.0.ends_with("/config") && c.2 == "\"1\"" && c.3 == "123")
        );
    }
    state.lock().unwrap().form_put_status = Some(StatusCode::UNPROCESSABLE_ENTITY);
    let invalid = response(&config, Method::POST, path, body).await;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(invalid).await;
    for expected in [
        "aria-invalid=\"true\"",
        "role=\"alert\"",
        "&lt;script&gt;bad&lt;/script&gt;",
        "value=\"&lt;script&gt;\"",
        "value=\"11\" selected",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("<script>"));
    for status in [
        StatusCode::PRECONDITION_FAILED,
        StatusCode::FORBIDDEN,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        state.lock().unwrap().form_put_status = Some(status);
        assert_eq!(
            response(&config, Method::POST, path, body).await.status(),
            status
        );
    }
    state.lock().unwrap().form_put_status = None;
    let stale = response(&config, Method::POST, path, "etag=%222%22").await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert!(html(stale).await.contains("他の人が変更しました"));
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.unknown=x")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(&config, Method::GET, "/g/42/modules/unknown", "")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.channel=x")
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let anonymous = Request::builder().uri(path).body(Body::empty()).unwrap();
    assert_eq!(
        router(config.clone())
            .oneshot(anonymous)
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let no_origin = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(
        router(config).oneshot(no_origin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn toggle_returns_only_to_same_origin() {
    let (config, _) = setup().await;
    for (referer, expected) in [
        ("http://localhost/g/42/modules/mod", "/g/42/modules/mod"),
        ("https://evil.example/g/42", "/g/42/modules"),
        ("http://localhost//evil", "/g/42/modules"),
    ] {
        let mut req = request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        );
        req.headers_mut()
            .insert(header::REFERER, referer.parse().unwrap());
        let result = router(config.clone()).oneshot(req).await.unwrap();
        assert_eq!(result.status(), StatusCode::SEE_OTHER);
        assert_eq!(result.headers()[header::LOCATION], expected);
    }
}
