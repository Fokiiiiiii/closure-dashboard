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
    for part in [
        "kind=ban",
        "user=555",
        "hidden=true",
        "before=20",
        "limit=25",
    ] {
        assert!(cases.contains(part), "{part} in {cases}");
    }
}

#[tokio::test]
async fn the_cases_csv_asks_the_bot_for_hidden_as_true() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let response = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/cases/export?hidden=1",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let asked = state.lock().unwrap().queries.clone();
    let export = asked
        .iter()
        .find(|query| query.contains("/cases/export?"))
        .expect("the bot was asked for the CSV");
    assert!(export.ends_with("hidden=true"), "{export}");
}

#[tokio::test]
async fn a_moderator_who_may_not_edit_reads_the_cases_and_nothing_else() {
    let (config, state) = setup().await;
    state.lock().unwrap().viewer = true;
    let app = router(config.clone());
    for path in ["/g/42/cases", "/g/42/cases/export", "/g/42/cases/7"] {
        let response = app
            .clone()
            .oneshot(request(&config, Method::GET, path, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
    for (method, path) in [(Method::POST, "/g/42/cases/note"), (Method::GET, "/g/42")] {
        let response = app
            .clone()
            .oneshot(request(&config, method, path, "target=555&reason=x"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn a_moderator_who_may_not_edit_is_sent_to_the_cases_of_the_server() {
    let (config, state) = setup().await;
    state.lock().unwrap().viewer = true;
    let app = router(config.clone());
    let response = app
        .oneshot(request(&config, Method::GET, "/", ""))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .and_then(|value| value.to_str().ok());
    assert_eq!(location, Some("/g/42/cases"));
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

#[tokio::test]
async fn ticket_lists_and_details_use_actor_checked_api_data() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let open = app
        .clone()
        .oneshot(request(&config, Method::GET, "/g/42/tickets", ""))
        .await
        .unwrap();
    assert_eq!(open.status(), StatusCode::OK);
    let page =
        String::from_utf8(to_bytes(open.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(page.contains("#3"));
    assert!(page.contains("state=closed"));
    assert!(page.contains("before=555000000000000003"));

    let closed = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets?state=closed&before=4",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::OK);
    let page = String::from_utf8(
        to_bytes(closed.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(page.contains("#4"));
    assert!(page.contains("state=closed&amp;before=4"));

    let detail = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/555000000000000003?state=open",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let page = String::from_utf8(
        to_bytes(detail.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(page.contains("&lt;script&gt;message&lt;/script&gt;"));
    assert!(!page.contains("<script>message</script>"));
    assert!(page.contains("start=4"));

    let invalid = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/555000000000000003?state=open&start=4294967296",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let next = app
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/555000000000000003?state=open&start=4",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(next.status(), StatusCode::OK);

    let state = state.lock().unwrap();
    assert!(
        state
            .queries
            .iter()
            .any(|query| query == "/v1/guilds/42/tickets?state=open&limit=50")
    );
    assert!(
        state
            .queries
            .iter()
            .any(|query| query == "/v1/guilds/42/tickets?state=closed&limit=50&before=4")
    );
    assert!(
        state
            .queries
            .iter()
            .any(|query| query == "/v1/guilds/42/tickets/555000000000000003?start=4&limit=4")
    );
    assert!(
        !state
            .queries
            .iter()
            .any(|query| query.contains("start=4294967296"))
    );
    assert!(
        state
            .calls
            .iter()
            .any(|call| { call.0 == "/v1/guilds/42/tickets" && call.3 == "123" }),
        "the actor is sent to the ticket API"
    );
}

#[tokio::test]
async fn transcript_next_link_href_reaches_the_next_page_when_followed() {
    let (config, state) = setup().await;
    let app = router(config.clone());
    let detail = app
        .clone()
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/555000000000000003?state=open",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let page = String::from_utf8(
        to_bytes(detail.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let href = page
        .split("href=\"")
        .skip(1)
        .filter_map(|part| part.split('"').next())
        .find(|href| href.contains("start=4"))
        .expect("the transcript renders a next page link");
    // One HTML decoding pass gives the URL the browser requests.
    let href = href.replace("&amp;", "&");
    let next = app
        .oneshot(request(&config, Method::GET, &href, ""))
        .await
        .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    assert!(
        state
            .lock()
            .unwrap()
            .queries
            .iter()
            .any(|query| query == "/v1/guilds/42/tickets/555000000000000003?start=4&limit=4"),
        "the followed next page link requests the next transcript page"
    );
}

#[tokio::test]
async fn transcript_filters_accept_repeated_label_and_previous_keys() {
    let (config, state) = setup().await;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/transcripts?label=alpha&label=beta&previous=5&previous=3",
            "",
        ))
        .await
        .unwrap();
    assert_ne!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "repeated scalar keys are the filter and next-page URLs the page itself generates"
    );
    let state = state.lock().unwrap();
    assert!(
        state
            .queries
            .iter()
            .any(|query| query.starts_with("/v1/guilds/42/tickets?")
                && query.contains("alpha")
                && query.contains("beta")),
        "both repeated label filters reach the ticket API"
    );
}

#[tokio::test]
async fn integration_editor_reads_and_saves_the_advanced_document() {
    let (config, state) = setup().await;
    state.lock().unwrap().ticket_integrations = true;
    let app = router(config.clone());
    let page = app
        .clone()
        .oneshot(request(&config, Method::GET, "/g/42/tickets/settings", ""))
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html =
        String::from_utf8(to_bytes(page.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(
        html.contains(r#"name="intent" value="integrations""#),
        "the integration editor comes from the advanced schema, which the basic view does not have"
    );
    let etag = html
        .split(r#"name="etag" value=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the page carries the revision")
        .replace("&quot;", "\"");

    let body = format!(
        "intent=integrations&etag=%22{}%22&integrations_present=1",
        etag.trim_matches('"')
    );
    let _ = app
        .oneshot(request(
            &config,
            Method::POST,
            "/g/42/tickets/settings",
            &body,
        ))
        .await
        .unwrap();

    let state = state.lock().unwrap();
    let (_, document) = state
        .bodies
        .iter()
        .find(|(path, _)| path == "/v1/guilds/42/config")
        .expect("the integration save writes the advanced document, not the basic one");
    let document: serde_json::Value = serde_json::from_str(document).unwrap();
    assert!(
        document["modules"]["tickets"]["config"]["integrations"].is_array(),
        "integrations live at modules.tickets.config.integrations"
    );
    assert!(
        document.get("integrations").is_none(),
        "no root integrations key"
    );
}

fn verification_form_fields(html: &str) -> Option<(String, String)> {
    let code_at = html.find(r#"name="code""#)?;
    let before = &html[..code_at];
    let key_marker = r#"name="key" value=""#;
    let key_at = before.rfind(key_marker)? + key_marker.len();
    let key = before[key_at..].split('"').next()?.to_owned();
    let etag_marker = r#"name="etag" value=""#;
    let etag_at = before.rfind(etag_marker)? + etag_marker.len();
    let etag = before[etag_at..].split('"').next()?.replace("&quot;", "\"");
    Some((etag, key))
}

fn form_encode_etag(etag: &str) -> String {
    etag.replace('"', "%22")
}

#[tokio::test]
async fn wrong_verification_code_keeps_a_form_with_the_current_revision() {
    let (config, _state) = setup().await;
    let app = router(config.clone());
    let page = app
        .clone()
        .oneshot(request(&config, Method::GET, "/settings/notifications", ""))
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html =
        String::from_utf8(to_bytes(page.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    let (etag, key) =
        verification_form_fields(&html).expect("the settings page shows the verification form");

    let wrong = format!(
        "action=verify_email&etag={}&key={key}&code=000000",
        form_encode_etag(&etag)
    );
    let failed = app
        .clone()
        .oneshot(request(
            &config,
            Method::POST,
            "/settings/notifications/actions",
            &wrong,
        ))
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let html = String::from_utf8(
        to_bytes(failed.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let (fresh, key) = verification_form_fields(&html)
        .expect("the error page after a wrong code shows the verification form again");
    assert_eq!(
        fresh, "\"2\"",
        "the error page must carry the revision the wrong attempt left behind"
    );

    let right = format!(
        "action=verify_email&etag={}&key={key}&code=123456",
        form_encode_etag(&fresh)
    );
    let saved = app
        .oneshot(request(
            &config,
            Method::POST,
            "/settings/notifications/actions",
            &right,
        ))
        .await
        .unwrap();
    assert!(
        saved.status().is_redirection(),
        "the correct code is accepted with the fresh revision, not refused with 412 (got {})",
        saved.status()
    );
}

#[tokio::test]
async fn tag_edit_finds_an_entry_past_the_first_page_of_a_name_search() {
    let (config, state) = setup().await;
    state.lock().unwrap().ticket_tag_count = 30;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/tags?edit=tag-029",
            "",
        ))
        .await
        .unwrap();
    let status = response.status();
    let html = String::from_utf8(
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "the edited tag is on the second page of the list"
    );
    assert!(html.contains("entry 29"), "the edit form shows the tag");
}

#[tokio::test]
async fn tag_edit_stops_scanning_after_the_page_limit() {
    let (config, state) = setup().await;
    state.lock().unwrap().ticket_tag_count = 25 * 21 + 1;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::GET,
            "/g/42/tickets/tags?edit=tag-525",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ticket_views_reach_the_api_acl_and_keep_guild_scope() {
    let (config, state) = setup().await;
    {
        let mut fake = state.lock().unwrap();
        fake.viewer = true;
        fake.ticket_forbidden = true;
    }
    let response = router(config.clone())
        .oneshot(request(&config, Method::GET, "/g/43/tickets", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| { call.0 == "/v1/guilds/43/tickets" && call.3 == "123" }),
        "the selected guild and actor reach the ticket API authorization check"
    );
}
