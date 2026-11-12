use std::collections::HashMap;

use axum::{Form, extract::Query};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    App, HeaderMap, Html, IntoResponse, Path, Redirect, Response, State, StatusCode, api_error,
    error_page, guild_name, keys, lang, user, views,
};
use crate::session;

const PAGE_SIZE: u8 = 50;
const MAX_FILTER_LABELS: usize = 100;
const MAX_PREVIOUS_PAGES: usize = 100;
const MAX_RETURN_QUERY_BYTES: usize = 262_144;
const MAX_LABELS_JSON_BYTES: usize = 131_072;

#[derive(Default, Deserialize)]
#[serde(default)]
struct TranscriptQuery {
    ticket_id: Option<String>,
    username: Option<String>,
    user_id: Option<String>,
    closed_by: Option<String>,
    panel: Option<String>,
    rating: Option<String>,
    claimed_by: Option<String>,
    close_reason: Option<String>,
    label: Vec<String>,
    column: Vec<String>,
    before: Option<String>,
    previous: Vec<String>,
    label_cursor: Option<String>,
    manage_labels: Option<String>,
    updated: Option<String>,
    conflict: Option<String>,
}

struct ParsedQuery {
    filters: views::TranscriptFilters,
    before: Option<String>,
    previous: Vec<String>,
    label_cursor: Option<String>,
    manage_labels: bool,
    updated: bool,
    conflict: bool,
}

pub(super) async fn page(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TranscriptQuery>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let parsed = match parse_query(query) {
        Ok(query) => query,
        Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    let asked = list_query(&parsed.filters, parsed.before.as_deref());
    let (tickets, labels, panel_state, guilds) = tokio::join!(
        app.api.tickets(&id, &asked, &actor),
        app.api
            .ticket_labels_state(&id, &actor, parsed.label_cursor.as_deref()),
        app.api.ticket_action_panels(&id, &actor),
        app.api.guilds(),
    );
    let tickets_value = match tickets {
        Ok((value, _)) => value,
        Err(error) => return api_error(&error, l, &headers),
    };
    let records = match valid_ticket_list(&tickets_value) {
        Some(records) => records,
        None => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    let (total, total_complete, username_truncated) = match (
        tickets_value["total"].as_u64(),
        tickets_value["total_complete"].as_bool(),
        tickets_value["username_truncated"].as_bool(),
    ) {
        (Some(total), Some(complete), Some(truncated)) => (total, complete, truncated),
        _ => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    let next_before = match tickets_value.get("next_before") {
        Some(Value::Null) => None,
        Some(Value::String(cursor)) if valid_closed_cursor(cursor) => Some(cursor.clone()),
        _ => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    let (labels_value, labels_etag) = match labels {
        Ok(value) => value,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(label_state) = labels_value.get("state") else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    };
    let Some(labels) = label_state["labels"].as_array() else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    };
    if !labels.iter().all(valid_label_definition) {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    }
    let next_label_cursor = match label_state.get("next_cursor") {
        Some(Value::Null) => None,
        Some(Value::String(cursor)) if valid_label_cursor(cursor) => Some(cursor.clone()),
        _ => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    let (syncing, backfill_incomplete, can_manage_labels) = match (
        label_state["syncing"].as_bool(),
        label_state["backfill_incomplete"].as_bool(),
        label_state["can_manage_labels"].as_bool(),
    ) {
        (Some(syncing), Some(incomplete), Some(can_manage)) => (syncing, incomplete, can_manage),
        _ => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    let filtered_label_etag = labels_etag.filter(|etag| valid_etag(etag));
    let panels = panel_state
        .ok()
        .and_then(|(state, _)| state["panels"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(valid_panel_option)
        .collect::<Vec<_>>();
    let name = match guilds {
        Ok((guilds, _)) => guild_name(&guilds, &id).to_owned(),
        Err(_) => id.clone(),
    };

    let mut row_tokens = HashMap::new();
    for record in &records {
        if let Some(channel_id) = record["channel_id"]
            .as_str()
            .filter(|channel| session::snowflake(channel))
        {
            if record["revision"]
                .as_i64()
                .is_some_and(|revision| revision > 0)
            {
                row_tokens.insert(
                    channel_id.to_owned(),
                    views::TranscriptActionTokens {
                        reason: app.nonces.fresh(),
                        add_label: app.nonces.fresh(),
                        remove_label: app.nonces.fresh(),
                    },
                );
            }
        }
    }
    let mut label_tokens = views::TranscriptLabelTokens::default();
    if can_manage_labels && filtered_label_etag.is_some() {
        label_tokens.create = app.nonces.fresh();
        for label in labels {
            if let Some(label_name) = label["name"].as_str() {
                label_tokens
                    .delete
                    .insert(label_name.to_owned(), app.nonces.fresh());
            }
        }
    }
    let return_query = query_string(&parsed, false, false);
    let page = views::TranscriptPageData {
        records,
        labels: labels.clone(),
        panels,
        filters: parsed.filters,
        total,
        total_complete,
        username_truncated,
        before: parsed.before,
        previous: parsed.previous,
        next_before,
        label_cursor: parsed.label_cursor,
        next_label_cursor,
        label_etag: filtered_label_etag,
        syncing,
        backfill_incomplete,
        can_manage_labels,
        manage_open: parsed.manage_labels,
        updated: parsed.updated,
        conflict: parsed.conflict,
        return_query,
        row_tokens,
        label_tokens,
    };
    Html(views::transcripts_page(l, &headers, &id, &name, &page)).into_response()
}

pub(super) async fn action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || !only_fields(
            &form,
            &[
                "channel_id",
                "action",
                "intent",
                "etag",
                "key",
                "reason",
                "labels_json",
                "label",
                "return_query",
            ],
        )
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(channel_id) = form
        .get("channel_id")
        .filter(|channel| session::snowflake(channel))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(etag) = form.get("etag").filter(|etag| valid_etag(etag)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form.get("key").filter(|key| valid_idempotency_key(key)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if form.get("action").map(String::as_str) != Some("ticket.history.update") {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(intent) = form.get("intent").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(labels_json) = form
        .get("labels_json")
        .filter(|value| value.len() <= MAX_LABELS_JSON_BYTES)
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let mut labels: Vec<String> = match serde_json::from_str(labels_json) {
        Ok(labels) => labels,
        Err(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    if !labels.iter().all(|label| valid_label_name(label)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let mut reason = form.get("reason").map(|reason| reason.trim().to_owned());
    if reason
        .as_ref()
        .is_some_and(|reason| reason.chars().count() > 1000 || reason.contains('\0'))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    if reason.as_deref().is_some_and(str::is_empty) {
        reason = None;
    }

    match intent {
        "reason" => {
            if form.get("reason").is_none() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        }
        "label_add" => {
            let Some(label) = form.get("label").filter(|label| valid_label_name(label)) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            labels.push(label.trim().to_owned());
        }
        "label_remove" => {
            let Some(label) = form.get("label").filter(|label| valid_label_name(label)) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            if let Some(index) = labels.iter().position(|value| value == label) {
                labels.remove(index);
            } else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        }
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    }

    let return_query = safe_return_query(form.get("return_query").map(String::as_str));
    match app
        .api
        .ticket_history_action(
            &id,
            channel_id,
            "ticket.history.update",
            json!({"reason": reason, "labels": labels}),
            etag,
            &actor,
            key,
        )
        .await
    {
        Ok(_) => Redirect::to(&with_result(&id, &return_query, "updated")).into_response(),
        Err(error)
            if error.status == StatusCode::PRECONDITION_FAILED
                || error.status == StatusCode::CONFLICT =>
        {
            Redirect::to(&with_result(&id, &return_query, "conflict")).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

pub(super) async fn label_action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || !only_fields(
            &form,
            &["action", "name", "color", "etag", "key", "return_query"],
        )
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(action) = form
        .get("action")
        .map(String::as_str)
        .filter(|action| matches!(*action, "ticket.label.create" | "ticket.label.delete"))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(etag) = form.get("etag").filter(|etag| valid_etag(etag)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form.get("key").filter(|key| valid_idempotency_key(key)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(name) = form.get("name").filter(|name| valid_label_name(name)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let data = if action == "ticket.label.create" {
        let Some(color) = form.get("color").filter(|color| valid_color(color)) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        json!({"name": name.trim(), "color": color})
    } else {
        json!({"name": name.trim()})
    };
    let mut return_query = safe_return_query(form.get("return_query").map(String::as_str));
    if action == "ticket.label.create" {
        return_query.label_cursor = None;
    }
    return_query.manage_labels = true;

    match app
        .api
        .ticket_label_action(&id, action, data, etag, &actor, key)
        .await
    {
        Ok(_) => {
            let query = query_string(&return_query, true, false);
            Redirect::to(&page_url(&id, &query)).into_response()
        }
        Err(error)
            if error.status == StatusCode::PRECONDITION_FAILED
                || error.status == StatusCode::CONFLICT =>
        {
            let query = query_string(&return_query, true, true);
            Redirect::to(&page_url(&id, &query)).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

fn valid_ticket_list(value: &Value) -> Option<Vec<Value>> {
    let records = value.get("tickets")?.as_array()?;
    for record in records {
        let channel = record["channel_id"].as_str()?;
        let number = record["number"].as_u64()?;
        let revision = record["revision"].as_i64()?;
        if !session::snowflake(channel)
            || number == 0
            || revision <= 0
            || record["closed"].as_i64().is_none()
            || record["has_transcript"].as_bool().is_none()
            || record["labels"]
                .as_array()?
                .iter()
                .any(|label| label.as_str().is_none())
            || !optional_snowflake(&record["opener"])
            || !optional_snowflake(&record["closer"])
            || !optional_snowflake(&record["claimed_by"])
        {
            return None;
        }
    }
    Some(records.clone())
}

fn optional_snowflake(value: &Value) -> bool {
    value.is_null()
        || value
            .as_str()
            .is_some_and(|value| session::snowflake(value) || value == "0")
}

fn valid_panel_option(value: &Value) -> bool {
    let Some(id) = value["id"].as_str() else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        && value["name"]
            .as_str()
            .is_none_or(|name| name.chars().count() <= 100 && !name.chars().any(char::is_control))
}

fn valid_label_definition(value: &Value) -> bool {
    let Some(name) = value["name"].as_str() else {
        return false;
    };
    let Some(color) = value["color"].as_str() else {
        return false;
    };
    valid_label_name(name) && valid_color(color)
}

fn valid_label_cursor(value: &str) -> bool {
    const PREFIX: &str = "ticket-label-definition:";
    value.len() == PREFIX.len() + 32
        && value.starts_with(PREFIX)
        && value[PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_closed_cursor(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|number| number > 0)
}

fn valid_ticket_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|number| number > 0)
}

fn valid_label_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_etag(value: &str) -> bool {
    (1..=128).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn valid_idempotency_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn only_fields(form: &HashMap<String, String>, allowed: &[&str]) -> bool {
    form.keys().all(|key| allowed.contains(&key.as_str()))
}

fn clean_text(value: Option<String>, max_chars: usize) -> Result<Option<String>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        return Err(());
    }
    Ok(Some(value.to_owned()))
}

fn parse_query(query: TranscriptQuery) -> Result<ParsedQuery, ()> {
    let ticket_id = clean_text(query.ticket_id, 20)?;
    if ticket_id
        .as_ref()
        .is_some_and(|value| !valid_ticket_id(value))
    {
        return Err(());
    }
    let username = clean_text(query.username, 100)?;
    let user_id = clean_text(query.user_id, 20)?;
    let closed_by = clean_text(query.closed_by, 20)?;
    let claimed_by = clean_text(query.claimed_by, 20)?;
    if [
        user_id.as_deref(),
        closed_by.as_deref(),
        claimed_by.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| !session::snowflake(value))
    {
        return Err(());
    }
    let panel = clean_text(query.panel, 100)?;
    let rating = clean_text(query.rating, 1)?;
    if rating
        .as_deref()
        .is_some_and(|value| !matches!(value, "1" | "2" | "3" | "4" | "5"))
    {
        return Err(());
    }
    let close_reason = clean_text(query.close_reason, 1000)?;
    if query.label.len() > MAX_FILTER_LABELS
        || query.label.iter().any(|label| !valid_label_name(label))
    {
        return Err(());
    }
    let mut labels = Vec::with_capacity(query.label.len());
    for label in query.label {
        let label = label.trim().to_owned();
        if !labels
            .iter()
            .any(|selected: &String| selected.eq_ignore_ascii_case(&label))
        {
            labels.push(label);
        }
    }
    if query.column.len() > views::TRANSCRIPT_COLUMNS.len()
        || query.column.iter().any(|column| {
            !views::TRANSCRIPT_COLUMNS
                .iter()
                .any(|(allowed, _)| *allowed == column)
        })
    {
        return Err(());
    }
    let mut columns = Vec::with_capacity(query.column.len());
    for column in query.column {
        if !columns.contains(&column) {
            columns.push(column);
        }
    }
    if columns.is_empty() {
        columns = views::TRANSCRIPT_COLUMNS
            .iter()
            .map(|(column, _)| (*column).to_owned())
            .collect();
    }
    let before = query.before.map(|value| value.trim().to_owned());
    if before
        .as_deref()
        .is_some_and(|value| !valid_closed_cursor(value))
    {
        return Err(());
    }
    let previous = query.previous;
    if previous.len() > MAX_PREVIOUS_PAGES
        || previous
            .iter()
            .any(|cursor| !cursor.is_empty() && !valid_closed_cursor(cursor))
    {
        return Err(());
    }
    let label_cursor = query.label_cursor.map(|value| value.trim().to_owned());
    if label_cursor
        .as_deref()
        .is_some_and(|cursor| !valid_label_cursor(cursor))
    {
        return Err(());
    }
    let manage_labels = query.manage_labels.as_deref() == Some("1");
    let updated = query.updated.as_deref() == Some("1");
    let conflict = query.conflict.as_deref() == Some("1");
    Ok(ParsedQuery {
        filters: views::TranscriptFilters {
            ticket_id,
            username,
            user_id,
            closed_by,
            panel,
            rating,
            claimed_by,
            close_reason,
            labels,
            columns,
        },
        before,
        previous,
        label_cursor,
        manage_labels,
        updated,
        conflict,
    })
}

fn list_query(filters: &views::TranscriptFilters, before: Option<&str>) -> String {
    let mut pairs = vec![
        ("state".to_owned(), "closed".to_owned()),
        ("limit".to_owned(), PAGE_SIZE.to_string()),
        ("include_total".to_owned(), "true".to_owned()),
    ];
    add_filter(&mut pairs, "ticket_id", filters.ticket_id.as_deref());
    add_filter(&mut pairs, "username", filters.username.as_deref());
    add_filter(&mut pairs, "user_id", filters.user_id.as_deref());
    add_filter(&mut pairs, "closed_by", filters.closed_by.as_deref());
    add_filter(&mut pairs, "claimed_by", filters.claimed_by.as_deref());
    add_filter(&mut pairs, "panel", filters.panel.as_deref());
    add_filter(&mut pairs, "rating", filters.rating.as_deref());
    add_filter(&mut pairs, "close_reason", filters.close_reason.as_deref());
    for label in &filters.labels {
        pairs.push(("label".to_owned(), label.clone()));
    }
    add_filter(&mut pairs, "before", before);
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

fn query_string(query: &ParsedQuery, manage: bool, conflict: bool) -> String {
    let mut pairs = Vec::new();
    add_filter(&mut pairs, "ticket_id", query.filters.ticket_id.as_deref());
    add_filter(&mut pairs, "username", query.filters.username.as_deref());
    add_filter(&mut pairs, "user_id", query.filters.user_id.as_deref());
    add_filter(&mut pairs, "closed_by", query.filters.closed_by.as_deref());
    add_filter(
        &mut pairs,
        "claimed_by",
        query.filters.claimed_by.as_deref(),
    );
    add_filter(&mut pairs, "panel", query.filters.panel.as_deref());
    add_filter(&mut pairs, "rating", query.filters.rating.as_deref());
    add_filter(
        &mut pairs,
        "close_reason",
        query.filters.close_reason.as_deref(),
    );
    for label in &query.filters.labels {
        pairs.push(("label".to_owned(), label.clone()));
    }
    for column in &query.filters.columns {
        pairs.push(("column".to_owned(), column.clone()));
    }
    add_filter(&mut pairs, "before", query.before.as_deref());
    for cursor in &query.previous {
        pairs.push(("previous".to_owned(), cursor.clone()));
    }
    add_filter(&mut pairs, "label_cursor", query.label_cursor.as_deref());
    if manage || query.manage_labels {
        pairs.push(("manage_labels".to_owned(), "1".to_owned()));
    }
    if conflict || query.conflict {
        pairs.push(("conflict".to_owned(), "1".to_owned()));
    }
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

fn page_url(id: &str, query: &str) -> String {
    if query.is_empty() {
        format!("/g/{id}/transcripts")
    } else {
        format!("/g/{id}/transcripts?{query}")
    }
}

fn safe_return_query(value: Option<&str>) -> ParsedQuery {
    value
        .filter(|value| value.len() <= MAX_RETURN_QUERY_BYTES)
        .and_then(|value| serde_urlencoded::from_str::<TranscriptQuery>(value).ok())
        .and_then(|query| parse_query(query).ok())
        .map(|mut query| {
            query.updated = false;
            query.conflict = false;
            query
        })
        .unwrap_or_else(|| ParsedQuery {
            filters: views::TranscriptFilters::default(),
            before: None,
            previous: Vec::new(),
            label_cursor: None,
            manage_labels: false,
            updated: false,
            conflict: false,
        })
}

fn with_result(id: &str, return_query: &ParsedQuery, result: &str) -> String {
    let mut query = query_string(return_query, return_query.manage_labels, false);
    let field = if result == "conflict" {
        "conflict"
    } else {
        "updated"
    };
    if !query.is_empty() {
        query.push('&');
    }
    query.push_str(field);
    query.push_str("=1");
    page_url(id, &query)
}

fn add_filter(pairs: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        pairs.push((key.to_owned(), value.to_owned()));
    }
}
