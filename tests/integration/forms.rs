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
        if fake.rules {
            schema["properties"]["modules"]["anyOf"][0]["properties"]["channels"]["properties"]["config"] = json!({"anyOf":[{"type":"object","properties":{"auto_delete":{
                "type":"array","maxItems":25,"items":{"type":"object","required":["channel","after"],
                "properties":{
                    "channel":{"type":"string","pattern":"^[1-9][0-9]{0,19}$"},
                    "after":{"type":"string","pattern":"^[0-9]+[smhdw]( *[0-9]+[smhdw])*$"},
                    "skip_pinned":{"type":"boolean","default":true}}}},
                "texts":{"description":"texts","x-ui":{"widget":"entries"}},
                "notes":{"type":"object","propertyNames":{"pattern":"^[a-z]{1,8}$"},"x-ui":{"widget":"entries-json"}},
                "raid":{"type":["object","null"],"x-ui":{"widget":"json"}},
                "keep":{"x-ui":{"widget":"flag-or-roles"}}}}]});
        }
        schema["properties"]["timezone"]["anyOf"][0]["description"] =
            json!("<script>description</script>");
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
            let path = if fake
                .form_body
                .as_ref()
                .is_some_and(|b| b.get("timezone").is_some() && b.get("modules").is_none())
            {
                "timezone"
            } else {
                "modules.welcome.config.channel"
            };
            return (status, [(header::ETAG, "\"1\"")], json!({"error":{"message":"bad","issues":[{"path":path,"message":"<script>bad</script>"}]}}).to_string());
        }
        json!({"revision":2})
    } else if path.ends_with("/config") {
        let mut config = fixture(include_str!("../fixtures/config-basic.json"));
        config["moderator_roles"] = json!([13, "99"]);
        config["modules"]["moderation"]["config"]["timeout_role"] = json!("13");
        config
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
        json!({"channels":[{"id":"11","name":"<script>","kind":"text"},{"id":"12","name":"voice","kind":"voice"}],"roles":[{"id":"42","name":"@everyone","managed":false},{"id":"13","name":"<script>role</script>","managed":false},{"id":"14","name":"managed","managed":true}]})
    } else if path.ends_with("/access/123") {
        json!({"allowed":true})
    } else if path == "/v1/guilds" {
        json!([{"id":"42","name":"Server"}])
    } else {
        json!({})
    };
    (StatusCode::OK, [(header::ETAG, "\"1\"")], data.to_string())
}

#[tokio::test]
async fn a_list_is_edited_row_by_row() {
    let (config, state) = setup().await;
    {
        let mut state = state.lock().unwrap();
        state.forms = true;
        state.rules = true;
    }
    let path = "/g/42/modules/channels";
    let page = html(response(&config, Method::GET, path, "").await).await;
    for expected in [
        "name=\"f.auto_delete.0.channel\"",
        "name=\"f.auto_delete.0.after\"",
        "name=\"f.auto_delete.0.skip_pinned\"",
        "<option value=\"11\">&lt;script&gt;</option>",
        "list-caption",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("f.auto_delete.0.x"));
    assert!(!page.contains("style="));
    let rule = "etag=%221%22&f.auto_delete.0.channel=11&f.auto_delete.0.after=1h+30m\
        &f.auto_delete.0.skip_pinned=false&f.auto_delete.0.skip_pinned=true";
    let result = response(&config, Method::POST, path, rule).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["channels"]["config"]["auto_delete"],
        json!([{"channel": "11", "after": "1h 30m", "skip_pinned": true}])
    );
    let result = response(&config, Method::POST, path, "etag=%221%22&f.keep=1%2C+22").await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["channels"]["config"]["keep"],
        json!(["1", "22"])
    );
    let page = html(response(&config, Method::GET, path, "").await).await;
    for expected in [
        "name=\"f.texts.0.k\"",
        "name=\"f.notes.0.v\"",
        "pattern=\"^[a-z]{1,8}$\"",
        "class=\"json\"",
        "name=\"f.raid\"",
        "name=\"f.keep\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    let entries = "etag=%221%22&f.texts.0.k=sdk-done&f.texts.0.v=Noted.\
        &f.notes.0.k=spam&f.notes.0.v=%7B%22on%22%3Atrue%7D&f.raid=%7B%22levels%22%3A%5B%5D%7D";
    let result = response(&config, Method::POST, path, entries).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    let saved = &sent["modules"]["channels"]["config"];
    assert_eq!(saved["texts"], json!({"sdk-done": "Noted."}));
    assert_eq!(saved["notes"], json!({"spam": {"on": true}}));
    assert_eq!(saved["raid"], json!({"levels": []}));
    let result = response(
        &config,
        Method::POST,
        path,
        "etag=%221%22&f.notes.0.k=spam&f.notes.0.v=%7B",
    )
    .await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(html(result).await.contains("spam: "));
    let result = response(
        &config,
        Method::POST,
        path,
        "etag=%221%22&f.auto_delete.0.channel=11&f.auto_delete.0.after=",
    )
    .await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(result).await;
    assert!(page.contains("role=\"alert\""));
    assert!(page.contains("<option value=\"11\" selected>"));
}

#[tokio::test]
async fn hidden_dependent_role_is_preserved() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/modules/moderation";
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(page.contains("data-when=\"timeout_method=role\""));
    assert!(page.contains("data-when=\"voice_mute_method=role\""));
    assert!(!page.contains("name=\"f.timeout_method\" value=\"\""));
    let result = response(&config, Method::POST, path,
        "etag=%221%22&f.timeout_method=discord&f.timeout_role=&f.voice_mute_method=discord&f.voice_mute_role=").await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let state = state.lock().unwrap();
    let config = &state.form_body.as_ref().unwrap()["modules"]["moderation"]["config"];
    assert_eq!(config["timeout_role"], "13");
    assert!(config.get("voice_mute_role").is_none());
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
    assert!(!page.contains("Automatic moderation: rules"));
    assert!(!page.contains("class=\"muted mono\""));
    let page = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(page.contains(">channel</label>"));
    assert!(!page.contains("class=\"muted mono\""));
    let page = html(response(&config, Method::GET, "/g/42/modules/channels", "").await).await;
    assert!(page.contains("このモジュールに設定項目はありません"));
    state.lock().unwrap().directory_failure = true;
    let page = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(page.contains("type=\"text\""));
    assert!(page.contains("pattern=\"^[0-9]{1,20}$\""));
}

#[tokio::test]
async fn pages_keep_logout_without_preference_controls_and_use_correct_width() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    for (path, narrow) in [
        ("/g/42", false),
        ("/g/42/modules", false),
        ("/g/42/history", false),
        ("/g/42/modules/welcome", true),
        ("/g/42/settings", true),
        ("/g/42/permissions", true),
    ] {
        let page = html(response(&config, Method::GET, path, "").await).await;
        assert!(page.contains("action=\"/logout\""), "{path}");
        assert!(
            page.contains(if narrow {
                "class=\"content narrow\""
            } else {
                "class=\"content\""
            }),
            "{path}"
        );
        for forbidden in [
            "breadcrumb",
            "⌄",
            "name=\"lang\"",
            "name=\"theme\"",
            "aria-label=\"言語\"",
            "aria-label=\"テーマ\"",
            "aria-label=\"Language\"",
            "aria-label=\"Theme\"",
        ] {
            assert!(!page.contains(forbidden), "{path}: {forbidden}");
        }
    }
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

#[tokio::test]
async fn server_settings_render_and_save() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/settings";
    let page = html(response(&config, Method::GET, path, "").await).await;
    for expected in [
        "サーバー設定",
        "name=\"f.language\"",
        "type=\"radio\"",
        "name=\"f.prefix\"",
        "name=\"f.timezone\"",
        ">自動</label>",
        ">日本語</label>",
        ">English</label>",
        "name=\"f.log_channel\"",
        "name=\"f.moderator_roles\"",
        "不明なロール 99",
        "&lt;script&gt;role&lt;/script&gt;",
        "&lt;script&gt;description&lt;/script&gt;",
        "aria-current=\"page\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    for forbidden in [
        "<script>",
        "style=",
        "value=\"42\"",
        ">managed</label>",
        "空で既定に戻ります",
        "class=\"muted mono\"",
        "name=\"f.preset\"",
        "name=\"f.log_webhook\"",
        "name=\"f.success_emoji\"",
        "name=\"f.error_emoji\"",
        "settings-heading",
        ">未設定</label>",
        ">auto</label>",
        ">ja</label>",
        ">en</label>",
    ] {
        assert!(!page.contains(forbidden), "{forbidden}");
    }
    assert_eq!(page.matches("name=\"f.language\"").count(), 3);
    assert!(page.contains("title=\"&lt;script&gt;description&lt;/script&gt;\""));
    let body = "etag=%221%22&f.language=ja&f.timezone=Asia%2FTokyo&f.prefix=%21&f.log_channel=11&roles_present=1&f.moderator_roles=13&f.moderator_roles=99";
    let result = response(&config, Method::POST, path, body).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    assert_eq!(result.headers()[header::LOCATION], "/g/42/settings?saved=1");
    {
        let state = state.lock().unwrap();
        let body = state.form_body.as_ref().unwrap();
        assert_eq!(*body, json!({"log_channel":"11"}));
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
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&roles_present=1&f.moderator_roles=13&f.moderator_roles=77"
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        state.lock().unwrap().form_body.as_ref().unwrap()["moderator_roles"],
        json!(["13", "77"])
    );
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&roles_present=1&f.prefix="
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        *state.lock().unwrap().form_body.as_ref().unwrap(),
        json!({"moderator_roles":null,"prefix":null})
    );
    state.lock().unwrap().directory_failure = true;
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(page.contains("name=\"f.moderator_roles\""));
    assert!(page.contains("value=\"13,99\""));
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&f.moderator_roles=13%2C77"
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        state.lock().unwrap().form_body.as_ref().unwrap()["moderator_roles"],
        json!(["13", "77"])
    );
}

#[tokio::test]
async fn server_settings_errors_and_security() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/settings";
    let body = "etag=%221%22&f.timezone=%3Cscript%3E";
    state.lock().unwrap().form_put_status = Some(StatusCode::UNPROCESSABLE_ENTITY);
    let result = response(&config, Method::POST, path, body).await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(result).await;
    for expected in [
        "aria-invalid=\"true\"",
        "&lt;script&gt;bad&lt;/script&gt;",
        "value=\"&lt;script&gt;\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    state.lock().unwrap().form_put_status = Some(StatusCode::FORBIDDEN);
    let forbidden = response(&config, Method::POST, path, body).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert!(html(forbidden).await.contains("アクセス権がありません"));
    state.lock().unwrap().form_put_status = Some(StatusCode::PRECONDITION_FAILED);
    assert!(
        html(response(&config, Method::POST, path, body).await)
            .await
            .contains("他の人が変更しました")
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%222%22")
            .await
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.unknown=x")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&f.moderator_roles=bad"
        )
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
