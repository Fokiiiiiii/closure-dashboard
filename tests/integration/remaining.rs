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
async fn theme_cookie_changes_rendered_theme() {
    let (config, _) = setup().await;
    let response = router(config.clone())
        .oneshot(request(&config, Method::POST, "/theme", "theme=dark"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(cookie, "theme=dark");
    let response = router(config)
        .oneshot(
            Request::builder()
                .uri("/login")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        String::from_utf8(
            to_bytes(response.into_body(), 65536)
                .await
                .unwrap()
                .to_vec()
        )
        .unwrap()
        .contains("data-theme=\"dark\"")
    );
}

#[tokio::test]
async fn error_and_public_theme_preserve_cookie() {
    let (config, _) = setup().await;
    let response = router(config.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/theme")
                .header(header::ORIGIN, "http://localhost")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("theme=dark"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .starts_with("theme=dark")
    );
    let response = router(config)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/lang")
                .header(header::COOKIE, "theme=dark")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = String::from_utf8(
        to_bytes(response.into_body(), 65536)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("data-theme=\"dark\""));
    assert!(body.contains("role=\"alert\""));
    assert!(!body.contains("style="));
}
