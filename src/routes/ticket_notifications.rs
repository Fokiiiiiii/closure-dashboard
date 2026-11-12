use super::{
    App, HeaderMap, Html, IntoResponse, Query, Redirect, Response, State, StatusCode, api_error,
    error_page, keys, lang, user, views,
};
use axum::Form;
use serde_json::{Value, json};
use std::collections::HashMap;

fn valid_cursor(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 19
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|id| id > 0)
}

fn valid_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn internal_submission_href(state: &Value, notification_id: &str) -> Option<String> {
    let notification = state["notifications"].as_array()?.iter().find(|item| {
        item["id"]
            .as_i64()
            .is_some_and(|id| id.to_string() == notification_id)
    })?;
    let guild = notification["source_guild_id"]
        .as_i64()
        .filter(|guild| *guild > 0)?;
    let href = notification["href"].as_str()?;
    let prefix = format!("/g/{guild}/tickets/gallery/submissions");
    let suffix = href.strip_prefix(&prefix)?;
    let within_route = suffix.is_empty()
        || suffix.starts_with('/')
        || suffix.starts_with('?')
        || suffix.starts_with('#');
    let path = href.split(['?', '#']).next().unwrap_or("");
    (within_route
        && href.starts_with("/g/")
        && !href.contains("://")
        && !href.contains('\\')
        && !href.chars().any(char::is_control)
        && !path.split('/').any(|segment| matches!(segment, "." | "..")))
    .then(|| href.to_owned())
}

fn checked(form: &HashMap<String, String>, name: &str) -> Option<bool> {
    match form.get(name).map(String::as_str) {
        None => Some(false),
        Some("true") => Some(true),
        _ => None,
    }
}

fn only_fields(form: &HashMap<String, String>, extra: &[&str]) -> bool {
    form.keys().all(|name| {
        matches!(
            name.as_str(),
            "action" | "etag" | "key" | "cursor" | "notification_id"
        ) || extra.contains(&name.as_str())
    })
}

pub(super) async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if query.keys().any(|key| key != "cursor") {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let cursor = match query.get("cursor") {
        Some(value) if valid_cursor(value) => Some(value.as_str()),
        Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        None => None,
    };
    let (state, etag) = match app.api.notifications_state(cursor, &actor, l.tag()).await {
        Ok(state) => state,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(etag) = etag else {
        return error_page(StatusCode::SERVICE_UNAVAILABLE, l, keys::INVALID, &headers);
    };
    let mut read_keys = HashMap::new();
    for item in state["notifications"].as_array().into_iter().flatten() {
        if item["read_at"].is_null()
            && let Some(id) = item["id"].as_i64().filter(|id| *id > 0)
        {
            read_keys.insert(id.to_string(), app.nonces.fresh());
        }
    }
    Html(views::ticket_notifications_page(
        l,
        &headers,
        &state,
        &etag,
        cursor,
        &app.nonces.fresh(),
        &read_keys,
    ))
    .into_response()
}

pub(super) async fn settings(State(app): State<App>, headers: HeaderMap) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let (state, etag) = match app.api.notifications_state(None, &actor, l.tag()).await {
        Ok(state) => state,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(etag) = etag else {
        return error_page(StatusCode::SERVICE_UNAVAILABLE, l, keys::INVALID, &headers);
    };
    Html(views::ticket_notification_settings_page(
        l,
        &headers,
        &state,
        &etag,
        &app.nonces.fresh(),
        &app.nonces.fresh(),
        &app.nonces.fresh(),
        &app.nonces.fresh(),
    ))
    .into_response()
}

pub(super) async fn action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    perform_action(app, headers, form, false).await
}

pub(super) async fn settings_action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    perform_action(app, headers, form, true).await
}

#[allow(
    clippy::too_many_lines,
    reason = "Validate and dispatch one notification action in order."
)]
async fn perform_action(
    app: App,
    headers: HeaderMap,
    form: HashMap<String, String>,
    settings_page: bool,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let action = form.get("action").map_or("", String::as_str);
    let etag = form.get("etag").map_or("", String::as_str);
    let key = form.get("key").map_or("", String::as_str);
    let cursor = form.get("cursor").map_or("", String::as_str);
    if etag.is_empty() || !valid_key(key) || (!cursor.is_empty() && !valid_cursor(cursor)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let data = match action {
        "mark_read" | "mark_read_open" => {
            if !only_fields(&form, &[]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let Some(id) = form
                .get("notification_id")
                .filter(|id| valid_cursor(id))
                .and_then(|id| id.parse::<i64>().ok())
            else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            json!({ "id": id })
        }
        "mark_all_read" => {
            if !only_fields(&form, &[]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({})
        }
        "save_preferences" => {
            if !settings_page || !only_fields(&form, &["in_app", "discord_dm", "email"]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let (Some(in_app), Some(discord_dm), Some(email)) = (
                checked(&form, "in_app"),
                checked(&form, "discord_dm"),
                checked(&form, "email"),
            ) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            json!({ "in_app": in_app, "discord_dm": discord_dm, "email": email })
        }
        "request_email_verification" => {
            if !settings_page || !only_fields(&form, &["email"]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let email = form
                .get("email")
                .map(|email| email.trim())
                .filter(|email| !email.is_empty());
            if let Some(email) = email {
                if email.chars().count() > 254 || email.chars().any(char::is_control) {
                    return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
                }
                json!({ "email": email })
            } else {
                json!({})
            }
        }
        "verify_email" => {
            if !settings_page || !only_fields(&form, &["code"]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let code = form.get("code").map_or("", |code| code.trim());
            if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({ "code": code })
        }
        "remove_email" => {
            if !settings_page || !only_fields(&form, &["confirm"]) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            if form.get("confirm").map(String::as_str) != Some("true") {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({ "confirm": true })
        }
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };

    if action == "mark_read_open" {
        let (state, fresh_etag) = match app
            .api
            .notifications_state((!cursor.is_empty()).then_some(cursor), &actor, l.tag())
            .await
        {
            Ok(state) => state,
            Err(error) => return api_error(&error, l, &headers),
        };
        if fresh_etag.as_deref() != Some(etag) {
            return error_page(StatusCode::PRECONDITION_FAILED, l, keys::INVALID, &headers);
        }
        let Some(href) = internal_submission_href(
            &state,
            form.get("notification_id").map_or("", String::as_str),
        ) else {
            return error_page(StatusCode::NOT_FOUND, l, keys::INVALID, &headers);
        };
        return match app
            .api
            .notification_action("mark_read", data, etag, &actor, key)
            .await
        {
            Ok(_) => Redirect::to(&href).into_response(),
            Err(error) => api_error(&error, l, &headers),
        };
    }

    match app
        .api
        .notification_action(action, data, etag, &actor, key)
        .await
    {
        Ok(_) => {
            if settings_page {
                Redirect::to("/settings/notifications").into_response()
            } else if cursor.is_empty() {
                Redirect::to("/notifications").into_response()
            } else {
                Redirect::to(&format!("/notifications?cursor={cursor}")).into_response()
            }
        }
        Err(error) => api_error(&error, l, &headers),
    }
}
