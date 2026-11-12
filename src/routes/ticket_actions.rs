use super::{
    App, HeaderMap, Html, IntoResponse, Path, Redirect, Response, State, StatusCode, api_error,
    error_page, guild_name, lang, user,
};
use crate::session;
use crate::views;
use axum::Form;
use serde_json::json;
use std::collections::HashMap;

struct BulkSelection {
    channel_id: String,
    revision: i64,
    key: String,
    number: u64,
}

fn parse_bulk_selection(channel_id: &str, value: &str) -> Option<BulkSelection> {
    let mut fields = value.split(':');
    let revision = fields
        .next()?
        .parse::<i64>()
        .ok()
        .filter(|revision| *revision > 0)?;
    let key = fields.next()?;
    let number = fields
        .next()?
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)?;
    if fields.next().is_some()
        || !session::snowflake(channel_id)
        || !(1..=64).contains(&key.len())
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    Some(BulkSelection {
        channel_id: channel_id.to_owned(),
        revision,
        key: key.to_owned(),
        number,
    })
}

pub(super) async fn perform(
    State(app): State<App>,
    Path((id, channel_id)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) || !session::snowflake(&channel_id) {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }
    let Some(action) = form.get("action").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    };
    let Some(etag) = form.get("etag").filter(|value| !value.trim().is_empty()) else {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    };
    let Some(key) = form.get("key").map(String::as_str).filter(|key| {
        (1..=64).contains(&key.len())
            && key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }) else {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    };

    let data = match action {
        "ticket.close" | "ticket.request_close" => {
            let reason = form
                .get("reason")
                .map(|reason| reason.trim())
                .filter(|reason| !reason.is_empty());
            json!({ "reason": reason })
        }
        "ticket.reply" => {
            let content = form.get("content").map_or("", |content| content.trim());
            if content.is_empty() {
                return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
            }
            json!({ "content": content })
        }
        "ticket.add_label" | "ticket.remove_label" => {
            let label = form.get("label").map_or("", |label| label.trim());
            if label.is_empty() {
                return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
            }
            json!({ "label": label })
        }
        _ => return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers),
    };

    match app
        .api
        .ticket_action(&id, "ticket", &channel_id, action, data, etag, &actor, key)
        .await
    {
        Ok(_) => {
            let state = if action == "ticket.close" {
                "closed"
            } else {
                "open"
            };
            Redirect::to(&format!(
                "/g/{id}/tickets/{channel_id}?state={state}&updated=1"
            ))
            .into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

const BULK_ACTION_LIMIT: usize = 50;
const BULK_ACTION_PARALLELISM: usize = 5;

pub(super) async fn bulk_perform(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) || form.get("state").map(String::as_str) != Some("open") {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }

    let Some(action) = form.get("action").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    };
    if !matches!(
        action,
        "ticket.close" | "ticket.request_close" | "ticket.add_label" | "ticket.remove_label"
    ) {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }
    let reason = form
        .get("reason")
        .map(|reason| reason.trim().to_owned())
        .filter(|reason| !reason.is_empty());
    if reason
        .as_ref()
        .is_some_and(|reason| reason.chars().count() > 1000)
    {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }
    let label = form
        .get("label")
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty());
    if matches!(action, "ticket.add_label" | "ticket.remove_label") && label.is_none() {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }
    let mut selected = Vec::with_capacity(BULK_ACTION_LIMIT);
    let mut seen = std::collections::HashSet::with_capacity(BULK_ACTION_LIMIT);
    for (field, value) in &form {
        if let Some(channel_id) = field.strip_prefix("ticket_") {
            let Some(ticket) = parse_bulk_selection(channel_id, value) else {
                return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
            };
            if !seen.insert(ticket.channel_id.clone()) {
                return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
            }
            selected.push(ticket);
        } else if !matches!(field.as_str(), "state" | "action" | "reason" | "label") {
            return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
        }
    }
    if selected.is_empty() {
        return error_page(
            StatusCode::BAD_REQUEST,
            l,
            super::keys::TICKET_BULK_SELECT_REQUIRED,
            &headers,
        );
    }
    if selected.len() > BULK_ACTION_LIMIT {
        return error_page(StatusCode::BAD_REQUEST, l, super::keys::INVALID, &headers);
    }

    let guilds = match app.api.guilds().await {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    let guild_label = guild_name(&guilds, &id);
    let mut results = selected
        .iter()
        .map(|ticket| (ticket.number, ticket.channel_id.clone(), false, None))
        .collect::<Vec<_>>();

    for (batch_number, batch) in selected.chunks(BULK_ACTION_PARALLELISM).enumerate() {
        let mut tasks = tokio::task::JoinSet::new();
        for (offset, ticket) in batch.iter().enumerate() {
            let result_index = batch_number * BULK_ACTION_PARALLELISM + offset;
            let api = app.api.clone();
            let guild = id.clone();
            let actor = actor.clone();
            let channel_id = ticket.channel_id.clone();
            let key = ticket.key.clone();
            let etag = format!(r#""{}""#, ticket.revision);
            let action = action.to_owned();
            let data = if matches!(action.as_str(), "ticket.close" | "ticket.request_close") {
                json!({ "reason": reason.clone() })
            } else {
                json!({ "label": label.clone().unwrap_or_default() })
            };
            tasks.spawn(async move {
                let result = api
                    .ticket_action(
                        &guild,
                        "ticket",
                        &channel_id,
                        &action,
                        data,
                        &etag,
                        &actor,
                        &key,
                    )
                    .await;
                (result_index, result)
            });
        }

        while let Some(joined) = tasks.join_next().await {
            let Ok((index, result)) = joined else {
                continue;
            };
            match result {
                Ok(_) => results[index].2 = true,
                Err(error) => results[index].3 = Some(error.status.as_u16()),
            }
        }
    }

    Html(views::ticket_bulk_results(
        l,
        &headers,
        &id,
        &guild_label,
        "open",
        &results,
    ))
    .into_response()
}
