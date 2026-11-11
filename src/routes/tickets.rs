use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State, StatusCode,
    api_error, error_page, guild_name, keys, lang, user, views,
};
use crate::session;

const PAGE_SIZE: u8 = 50;

fn transcript_start(value: &str) -> Option<u32> {
    if value.is_empty()
        || value.len() > 10
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
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
        asked.push_str(&format!("&before={before}"));
    }
    let tickets = match app.api.tickets(&id, &asked, &actor).await {
        Ok((tickets, _)) => tickets,
        Err(error) => return api_error(&error, l, &headers),
    };
    let (guilds, directory) = tokio::join!(app.api.guilds(), app.api.directory(&id));
    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    let directory = directory.ok().map(|(directory, _)| directory);
    Html(views::tickets_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &tickets,
        directory.as_ref(),
        state,
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
    if !session::snowflake(&channel_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let state = match query.get("state").map(String::as_str) {
        Some("closed") => "closed",
        _ => "open",
    };
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
    let (guilds, directory) = tokio::join!(app.api.guilds(), app.api.directory(&id));
    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    let directory = directory.ok().map(|(directory, _)| directory);
    Html(views::ticket_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &channel_id,
        &ticket,
        directory.as_ref(),
        state,
        start.unwrap_or(0),
    ))
    .into_response()
}
