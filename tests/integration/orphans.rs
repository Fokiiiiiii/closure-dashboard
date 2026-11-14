use super::*;

const SETTINGS: &str = "/g/42/tickets/settings";
const DELETE: &str = "/g/42/tickets/settings/integrations/orphans/delete";
const GENERATION: &str = "0123456789abcdef0123456789abcdef";

fn orphan(integration_id: &str, secret_name: &str) -> serde_json::Value {
    json!({
        "integration_id": integration_id,
        "generation": GENERATION,
        "secret_name": secret_name,
        "updated_at_unix": 1_763_100_000,
    })
}

fn delete_body(integration_id: &str, secret_name: &str) -> String {
    format!("integration_id={integration_id}&generation={GENERATION}&secret_name={secret_name}")
}

#[tokio::test]
async fn owner_sees_orphan_secrets_and_deleting_one_removes_it() {
    let (config, state) = setup().await;
    {
        let mut fake = state.lock().unwrap();
        fake.ticket_integrations = true;
        fake.orphans = vec![orphan("crm", "api"), orphan("crm", "old")];
    }
    let (status, html) = get_html(&config, SETTINGS).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        html.matches("orphans/delete\"").count(),
        2,
        "each orphan has a delete form"
    );
    assert!(html.contains(r#"value="old""#));

    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            DELETE,
            &delete_body("crm", "api"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    {
        let fake = state.lock().unwrap();
        assert_eq!(fake.orphans.len(), 1);
        let (path, _, _, actor) = fake.calls.last().unwrap();
        assert_eq!(path, "/v1/guilds/42/integration-secrets/orphans/delete");
        assert_eq!(actor, "123", "the actor is sent to the API");
        let (_, posted) = fake.bodies.last().unwrap();
        let posted: serde_json::Value = serde_json::from_str(posted).unwrap();
        assert_eq!(
            posted,
            json!({"integration_id": "crm", "generation": GENERATION, "secret_name": "api"})
        );
    }
    let (_, html) = get_html(&config, SETTINGS).await;
    assert_eq!(
        html.matches("orphans/delete\"").count(),
        1,
        "the deleted orphan is no longer listed"
    );
}

#[tokio::test]
async fn members_who_may_not_manage_secrets_see_no_orphan_list() {
    let (config, state) = setup().await;
    {
        let mut fake = state.lock().unwrap();
        fake.ticket_integrations = true;
        fake.orphans = vec![orphan("crm", "api")];
        fake.orphan_list_status = Some(StatusCode::FORBIDDEN);
    }
    let (status, html) = get_html(&config, SETTINGS).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a refused list is not an error page"
    );
    assert!(!html.contains("orphans/delete"));
}

#[tokio::test]
async fn a_conflicting_delete_shows_the_error_on_the_settings_page() {
    let (config, state) = setup().await;
    {
        let mut fake = state.lock().unwrap();
        fake.ticket_integrations = true;
        fake.orphans = vec![orphan("crm", "api")];
        fake.orphan_delete_status = Some(StatusCode::CONFLICT);
    }
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            DELETE,
            &delete_body("crm", "api"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let html = String::from_utf8(
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        html.contains("form-error"),
        "the conflict is shown as an error"
    );
}

#[tokio::test]
async fn deleting_a_secret_that_is_not_an_orphan_is_not_found() {
    let (config, state) = setup().await;
    state.lock().unwrap().ticket_integrations = true;
    let response = router(config.clone())
        .oneshot(request(
            &config,
            Method::POST,
            DELETE,
            &delete_body("crm", "api"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn orphan_delete_without_the_origin_header_is_refused() {
    let (config, state) = setup().await;
    let mut req = request(&config, Method::POST, DELETE, &delete_body("crm", "api"));
    req.headers_mut().remove(header::ORIGIN);
    let response = router(config.clone()).oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .all(|(path, _, _, _)| !path.ends_with("/orphans/delete"))
    );
}

#[tokio::test]
async fn malformed_orphan_delete_is_rejected_before_the_api() {
    let (config, state) = setup().await;
    let body = format!("integration_id=CRM!&generation={GENERATION}&secret_name=api");
    let response = router(config.clone())
        .oneshot(request(&config, Method::POST, DELETE, &body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .all(|(path, _, _, _)| !path.ends_with("/orphans/delete"))
    );
}
