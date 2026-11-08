use super::*;

#[tokio::test]
async fn guild_access_is_not_cached() {
    let (config, state) = setup().await;
    for _ in 0..2 {
        assert_eq!(
            router(config.clone())
                .oneshot(request(&config, Method::GET, "/g/42", ""))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| call.0.ends_with("/access/123"))
            .count(),
        2
    );
}

#[tokio::test]
async fn access_errors_show_localized_messages() {
    let (config, state) = setup().await;
    for (status, message) in [
        (403, "このサーバーへのアクセス権がありません"),
        (404, "このサーバーには参加していません"),
        (503, "Bot に接続できません"),
    ] {
        state.lock().unwrap().status = Some(StatusCode::from_u16(status).unwrap());
        let response = router(config.clone())
            .oneshot(request(&config, Method::GET, "/g/42", ""))
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(message));
        assert!(!body.contains("secret-token"));
    }
}

#[tokio::test]
async fn retry_stops_after_second_conflict() {
    let (config, state) = setup().await;
    state.lock().unwrap().always_conflict = true;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(state.lock().unwrap().puts, 2);
}

#[tokio::test]
async fn toggle_forbidden_is_shown() {
    let (config, state) = setup().await;
    state.lock().unwrap().put_forbidden = true;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    assert!(
        String::from_utf8(body.to_vec())
            .unwrap()
            .contains("このサーバーへのアクセス権がありません")
    );
}

#[tokio::test]
async fn invalid_guild_and_module_are_rejected() {
    let (config, state) = setup().await;
    state.lock().unwrap().invalid_schema = true;
    for path in [
        "/g/not-a-number",
        "/g/42/modules/bad%21/toggle",
        "/g/42/modules/missing/toggle",
    ] {
        let method = if path.contains("toggle") {
            Method::POST
        } else {
            Method::GET
        };
        assert_eq!(
            router(config.clone())
                .oneshot(request(&config, method, path, "enabled=true"))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn security_headers_cover_success_redirect_and_errors() {
    let (config, _) = setup().await;
    for path in ["/healthz", "/", "/auth/callback?state=wrong", "/missing"] {
        let response = router(config.clone())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        for name in [
            header::CACHE_CONTROL,
            header::X_CONTENT_TYPE_OPTIONS,
            header::REFERRER_POLICY,
            header::CONTENT_SECURITY_POLICY,
        ] {
            assert!(response.headers().contains_key(&name), "{path} {name}");
        }
    }
}

#[tokio::test]
async fn logout_clears_session_cookie() {
    let (config, _) = setup().await;
    let response = router(config.clone())
        .oneshot(request(&config, Method::POST, "/logout", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.starts_with("session=;"));
    assert!(cookie.contains("Max-Age=0"));
}

#[tokio::test]
async fn index_redirects_one_and_displays_empty() {
    let (config, state) = setup().await;
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/g/42");
    state.lock().unwrap().empty = true;
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        String::from_utf8(
            to_bytes(response.into_body(), 65536)
                .await
                .unwrap()
                .to_vec()
        )
        .unwrap()
        .contains("利用できるサーバーはありません")
    );
}

#[tokio::test]
async fn a_reload_of_the_first_page_does_not_ask_about_every_server_again() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(request(&config, Method::GET, "/", ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
    assert_eq!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| call.0.ends_with("/access/123"))
            .count(),
        1
    );
}

#[tokio::test]
async fn the_cases_are_listed_searched_and_paged_with_the_text_escaped() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let response = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/cases?kind=ban&user=555&hidden=1&before=20",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = String::from_utf8(
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(page.contains("#7"), "{page}");
    assert!(
        page.contains("&lt;b&gt;spam&lt;/b&gt;"),
        "the reason is escaped"
    );
    assert!(!page.contains("<b>spam</b>"));
    assert!(
        page.contains("/g/42/cases/note"),
        "a person who may edit can add a note"
    );
    assert!(page.contains("/g/42/cases/export?"), "and download the CSV");
    assert!(
        page.contains("before=7"),
        "the next page is asked after the last case shown"
    );
    let asked = state.lock().unwrap().queries.clone();
    let cases = asked
        .iter()
        .find(|query| query.contains("/cases?"))
        .expect("the bot was asked for the cases");
    for part in ["kind=ban", "user=555", "hidden=1", "before=20", "limit=25"] {
        assert!(cases.contains(part), "{part} in {cases}");
    }
}

#[tokio::test]
async fn a_value_that_is_not_one_is_not_sent_to_the_bot_and_is_said_so() {
    let (config, state) = setup().await;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/cases?user=abc&state=gone",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = String::from_utf8(
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(page.contains("role=\"alert\""), "{page}");
    let asked = state.lock().unwrap().queries.clone();
    let cases = asked
        .iter()
        .find(|query| query.contains("/cases?"))
        .expect("the bot was asked for the cases");
    assert!(!cases.contains("abc") && !cases.contains("gone"), "{cases}");
}

#[tokio::test]
async fn a_case_is_shown_and_its_reason_and_hiding_are_changed_through_the_bot() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let one = app
        .clone()
        .oneshot(request(&config, Method::GET, "/g/42/cases/7?updated=1", ""))
        .await
        .unwrap();
    assert_eq!(one.status(), StatusCode::OK);
    let page =
        String::from_utf8(to_bytes(one.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(page.contains("name=\"op\" value=\"reason\""), "{page}");
    assert!(page.contains("name=\"op\" value=\"hide\""));
    assert!(page.contains("role=\"status\""), "the update is told");

    let hidden = app
        .clone()
        .oneshot(request(&config, Method::POST, "/g/42/cases/7", "op=hide"))
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        hidden.headers()[header::LOCATION],
        "/g/42/cases/7?updated=1"
    );
    let reason = app
        .clone()
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/cases/7",
            "op=reason&reason=new+text",
        ))
        .await
        .unwrap();
    assert_eq!(reason.status(), StatusCode::SEE_OTHER);
    let bad = app
        .oneshot(request(&config, Method::POST, "/g/42/cases/7", "op=delete"))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    let bodies = state.lock().unwrap().bodies.clone();
    assert!(
        bodies.iter().any(|(_, body)| body == r#"{"hidden":true}"#),
        "{bodies:?}"
    );
    assert!(
        bodies
            .iter()
            .any(|(_, body)| body == r#"{"reason":"new text"}"#),
        "{bodies:?}"
    );
}

#[tokio::test]
async fn a_note_is_recorded_and_the_csv_is_downloaded() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let note = app
        .clone()
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/cases/note",
            "target=555&reason=asked+twice",
        ))
        .await
        .unwrap();
    assert_eq!(note.status(), StatusCode::SEE_OTHER);
    assert!(
        note.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .starts_with("/g/42/cases/")
    );
    let missing = app
        .clone()
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/cases/note",
            "target=x&reason=a",
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert!(
        state
            .lock()
            .unwrap()
            .bodies
            .iter()
            .any(|(_, body)| body.contains(r#""target":"555""#) && body.contains("asked twice"))
    );

    let csv = app
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/cases/export?kind=ban",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(csv.status(), StatusCode::OK);
    assert!(
        csv.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    assert!(
        csv.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("cases-42.csv")
    );
    let body = to_bytes(csv.into_body(), 1 << 20).await.unwrap();
    assert!(String::from_utf8_lossy(&body).starts_with("number,created_at"));
}
