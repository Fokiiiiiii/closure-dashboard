use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user, views,
};
use crate::session;

const PAGE_SIZE: u8 = 50;

fn transcript_start(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn cursor_valid(state: &str, value: &str) -> bool {
    match state {
        "open" => session::snowflake(value),
        "closed" => {
            !value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|byte| byte.is_ascii_digit())
                && value.parse::<u64>().is_ok_and(|number| number > 0)
        }
        _ => false,
    }
}

fn valid_transcript_return(id: &str, value: &str) -> bool {
    let prefix = format!("/g/{id}/transcripts");
    value
        .strip_prefix(&prefix)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('?'))
        && !value.contains("://")
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
}

pub(super) async fn list(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let state = match query.get("state").map(String::as_str) {
        Some("closed") => "closed",
        _ => "open",
    };
    let mut asked = format!("state={state}&limit={PAGE_SIZE}");
    if let Some(before) = query
        .get("before")
        .filter(|before| cursor_valid(state, before))
    {
        asked.push_str("&before=");
        asked.push_str(before);
    }
    let tickets = match app.api.tickets(&id, &asked, &actor).await {
        Ok((tickets, _)) => tickets,
        Err(error) => return api_error(&error, l, &headers),
    };
    let mut bulk_keys = HashMap::new();
    if state == "open" {
        for ticket in tickets["tickets"].as_array().into_iter().flatten() {
            let channel_id = ticket["channel_id"].as_str().unwrap_or("");
            if session::snowflake(channel_id)
                && ticket["revision"]
                    .as_i64()
                    .is_some_and(|revision| revision > 0)
            {
                bulk_keys.insert(channel_id.to_owned(), app.nonces.fresh());
            }
        }
    }
    let (guilds, directory, ticket_panels) = tokio::join!(
        app.api.guilds(),
        app.api.directory(&id),
        app.api.ticket_action_panels(&id, &actor),
    );
    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    let directory = directory.ok().map(|(directory, _)| directory);
    let ticket_settings = ticket_panels.is_ok_and(|(access, _)| {
        access["can_manage_guild"] == true
            || access["panels"]
                .as_array()
                .is_some_and(|panels| !panels.is_empty())
    });
    Html(views::tickets_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &tickets,
        directory.as_ref(),
        state,
        ticket_settings,
        &bulk_keys,
    ))
    .into_response()
}

pub(super) async fn one(
    State(app): State<App>,
    Path((id, channel_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) || !session::snowflake(&channel_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let state = match query.get("state").map(String::as_str) {
        Some("closed") => "closed",
        _ => "open",
    };
    let default_back_url = if state == "closed" {
        format!("/g/{id}/transcripts")
    } else {
        format!("/g/{id}/tickets?state={state}")
    };
    let back_url = query
        .get("return_to")
        .filter(|value| valid_transcript_return(&id, value))
        .cloned()
        .unwrap_or(default_back_url);
    let start = match query.get("start") {
        Some(value) => match transcript_start(value) {
            Some(start) => Some(start),
            None => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        },
        None => None,
    };
    let ticket = match app.api.ticket(&id, &channel_id, start, &actor).await {
        Ok((ticket, _)) => ticket,
        Err(error) => return api_error(&error, l, &headers),
    };
    let action_state = if ticket["state"].as_str() == Some("closed") {
        None
    } else {
        match app
            .api
            .ticket_action_state(&id, "ticket", &channel_id, &actor)
            .await
        {
            Ok((data, Some(etag))) if data["state"].is_object() => {
                Some((data["state"].clone(), etag))
            }
            _ => None,
        }
    };
    let action_tokens = action_state.as_ref().map(|_| views::ActionTokens {
        close: app.nonces.fresh(),
        request_close: app.nonces.fresh(),
        reply: app.nonces.fresh(),
        add_label: app.nonces.fresh(),
        remove_label: app.nonces.fresh(),
    });
    let (guilds, directory, ticket_panels) = tokio::join!(
        app.api.guilds(),
        app.api.directory(&id),
        app.api.ticket_action_panels(&id, &actor),
    );
    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    let directory = directory.ok().map(|(directory, _)| directory);
    let ticket_settings = ticket_panels.is_ok_and(|(access, _)| {
        access["can_manage_guild"] == true
            || access["panels"]
                .as_array()
                .is_some_and(|panels| !panels.is_empty())
    });
    Html(views::ticket_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &channel_id,
        &ticket,
        directory.as_ref(),
        state,
        &back_url,
        start.unwrap_or(0),
        action_state.as_ref().map(|(state, _)| state),
        action_state.as_ref().map(|(_, etag)| etag.as_str()),
        action_tokens.as_ref(),
        query.get("updated").is_some_and(|value| value == "1"),
        ticket_settings,
    ))
    .into_response()
}
