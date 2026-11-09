//! The pages of the cases of a server (see `views::cases_page`): a search, one case, a CSV, a note.

use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user, views,
};
use axum::http::header;
use serde_json::{Value, json};

/// How many days the counts on top of the list cover.
const SUMMARY_DAYS: u16 = 30;

fn digits(text: &str) -> bool {
    !text.is_empty() && text.len() <= 20 && text.bytes().all(|b| b.is_ascii_digit())
}

fn word(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The search a request asks for: as the form shows it, as the bot is asked for it, and whether
/// some value of it was not one.
struct Search {
    filter: views::CaseFilter,
    query: Vec<(&'static str, String)>,
    bad: bool,
}

fn search_of(query: &HashMap<String, String>) -> Search {
    let mut search = Search {
        filter: views::CaseFilter::default(),
        query: Vec::new(),
        bad: false,
    };
    let mut take = |name: &'static str, ok: &dyn Fn(&str) -> bool| -> String {
        let value = query.get(name).map_or("", |v| v.trim()).to_owned();
        if value.is_empty() {
            return String::new();
        }
        if ok(&value) {
            search.query.push((name, value.clone()));
            value
        } else {
            search.bad = true;
            String::new()
        }
    };
    let user = take("user", &digits);
    let moderator = take("moderator", &digits);
    let kind = take("kind", &word);
    let state = take("state", &|v| views::CASE_STATES.contains(&v));
    let source = take("source", &|v| views::CASE_SOURCES.contains(&v));
    let text = take("text", &|v| v.chars().count() <= 100);
    // The bot takes `hidden` as true or false, so the form's 1 (or on) is sent as true.
    let hidden = match query.get("hidden").map(|v| v.trim()) {
        Some("1" | "on" | "true") => {
            search.query.push(("hidden", "true".to_owned()));
            true
        }
        Some("") | None => false,
        Some(_) => {
            search.bad = true;
            false
        }
    };
    search.filter = views::CaseFilter {
        user,
        moderator,
        kind,
        state,
        source,
        text,
        hidden,
    };
    search
}

fn encode(pairs: &[(&str, String)]) -> String {
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

/// Whether the person may change the cases of the server (the right to edit the dashboard).
async fn may_edit(app: &App, id: &str, actor: &str) -> bool {
    app.api
        .access(id, actor)
        .await
        .is_ok_and(|(access, _)| access["allowed"] == true)
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
    let search = search_of(&query);
    // The page older than this case; a value that is not a number is the first page.
    let before = query
        .get("before")
        .filter(|before| before.parse::<i64>().is_ok());
    let mut asked = search.query.clone();
    let base = encode(&asked);
    if let Some(before) = before {
        asked.push(("before", before.clone()));
    }
    asked.push(("limit", "25".to_owned()));
    let asked = encode(&asked);
    let (guilds, list, summary, editable) = tokio::join!(
        app.api.guilds(),
        app.api.cases(&id, &asked, &actor),
        app.api.cases_summary(&id, SUMMARY_DAYS, &actor),
        may_edit(&app, &id, &actor)
    );
    let (guilds, list) = match (guilds, list) {
        (Ok((guilds, _)), Ok((list, _))) => (guilds, list),
        (Err(e), _) | (_, Err(e)) => return api_error(&e, l, &headers),
    };
    let banner = if search.bad {
        Some((false, l.text(keys::CASE_BAD_FILTER)))
    } else if query.get("noted").is_some_and(|v| v == "1") {
        Some((true, l.text(keys::CASE_NOTE_ADDED)))
    } else {
        None
    };
    let summary = summary.ok().map(|(summary, _)| summary);
    Html(views::cases_page(
        l,
        &headers,
        &views::CasesPage {
            id: &id,
            name: guild_name(&guilds, &id),
            filter: &search.filter,
            list: &list,
            summary: summary.as_ref(),
            days: SUMMARY_DAYS,
            may_edit: editable,
            query: &base,
            banner,
        },
    ))
    .into_response()
}

pub(super) async fn export(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let search = search_of(&query);
    match app.api.cases_csv(&id, &encode(&search.query), &actor).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"cases-{id}.csv\""),
                ),
                (header::CACHE_CONTROL, "no-store".to_owned()),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => api_error(&e, l, &headers),
    }
}

pub(super) async fn one(
    State(app): State<App>,
    Path((id, number)): Path<(String, i64)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let (guilds, case, editable) = tokio::join!(
        app.api.guilds(),
        app.api.case(&id, number, &actor),
        may_edit(&app, &id, &actor)
    );
    let (guilds, case) = match (guilds, case) {
        (Ok((guilds, _)), Ok((case, _))) => (guilds, case),
        (Err(e), _) | (_, Err(e)) => return api_error(&e, l, &headers),
    };
    let banner = query
        .get("updated")
        .is_some_and(|v| v == "1")
        .then(|| (true, l.text(keys::CASE_UPDATED)));
    Html(views::case_page(
        l,
        &headers,
        (&id, guild_name(&guilds, &id)),
        &case,
        editable,
        &banner,
    ))
    .into_response()
}

pub(super) async fn add_note(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let target = form.get("target").map_or("", |v| v.trim());
    let reason = form.get("reason").map_or("", |v| v.trim());
    if !digits(target) || reason.is_empty() || reason.chars().count() > 512 {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    match app.api.add_note(&id, target, reason, &actor).await {
        Ok((case, _)) => Redirect::to(&format!(
            "/g/{id}/cases/{}?updated=1",
            case["number"].as_i64().unwrap_or(0)
        ))
        .into_response(),
        Err(e) => api_error(&e, l, &headers),
    }
}

pub(super) async fn change(
    State(app): State<App>,
    Path((id, number)): Path<(String, i64)>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let change: Value = match form.get("op").map(String::as_str) {
        Some("reason") => {
            let reason = form.get("reason").map_or("", |v| v.trim());
            if reason.is_empty() || reason.chars().count() > 512 {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({ "reason": reason })
        }
        Some("hide") => json!({ "hidden": true }),
        Some("show") => json!({ "hidden": false }),
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    match app.api.change_case(&id, number, change, &actor).await {
        Ok(_) => Redirect::to(&format!("/g/{id}/cases/{number}?updated=1")).into_response(),
        Err(e) => api_error(&e, l, &headers),
    }
}
