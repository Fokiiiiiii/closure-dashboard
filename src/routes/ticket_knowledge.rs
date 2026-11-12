use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user,
};
use crate::session;
use crate::views;
use axum::Form;
use serde_json::{Value, json};

fn valid_cursor(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_knowledge_id(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_title(value: &str) -> bool {
    let title = value.trim();
    !title.is_empty() && title.chars().count() <= 100 && !title.contains('\0')
}

fn valid_body(value: &str) -> bool {
    let body = value.trim();
    !body.is_empty() && body.chars().count() <= 4096 && !body.contains('\0')
}

fn parse_terms(form: &HashMap<String, String>, key: &str) -> Result<Option<Vec<String>>, ()> {
    let Some(text) = form.get(key) else {
        return Ok(None);
    };
    let mut values = Vec::new();
    for value in text
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if value.chars().count() > 100 || value.chars().any(char::is_control) {
            return Err(());
        }
        values.push(value.to_owned());
    }
    Ok(Some(values))
}

fn parse_custom_embed(form: &HashMap<String, String>) -> Result<Option<Value>, ()> {
    let Some(text) = form.get("custom_embed") else {
        return Ok(None);
    };
    let text = text.trim();
    if text.is_empty() {
        return Ok(Some(Value::Null));
    }
    if text.chars().count() > 6000 {
        return Err(());
    }
    let value: Value = serde_json::from_str(text).map_err(|_| ())?;
    if !value.is_object() {
        return Err(());
    }
    Ok(Some(value))
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
    if !session::snowflake(&id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let cursor = query
        .get("cursor")
        .map(String::as_str)
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty());
    if cursor.is_some_and(|cursor| !valid_cursor(cursor)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let search = query
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    if search.is_some_and(|q| q.chars().count() > 100 || q.chars().any(char::is_control)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let knowledge = app.api.ticket_knowledge_state(&id, cursor, search, &actor);
    let guilds = app.api.guilds();
    let (knowledge, guilds) = tokio::join!(knowledge, guilds);
    let data = match knowledge {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let state = data["state"].as_object().map(|_| data["state"].clone());
    let Some(state) = state.filter(|state| state["articles"].is_array()) else {
        return api_error(&ApiError::unavailable(), l, &headers);
    };
    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    Html(views::ticket_knowledge_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &state,
        search.unwrap_or(""),
        cursor,
        query.get("saved").is_some_and(|value| value == "1"),
        &app.nonces.fresh(),
    ))
    .into_response()
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
    if !session::snowflake(&id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let action = form.get("action").map_or("", String::as_str);
    let article_id = form.get("id").map_or("", String::as_str).trim();
    let Some(etag) = form.get("etag").filter(|etag| !etag.trim().is_empty()) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form
        .get("key")
        .map(String::as_str)
        .filter(|key| valid_key(key))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let cursor = form
        .get("cursor")
        .map(String::as_str)
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty());
    if cursor.is_some_and(|cursor| !valid_cursor(cursor)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let search = form
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    if search.is_some_and(|q| q.chars().count() > 100 || q.chars().any(char::is_control)) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    if !valid_knowledge_id(article_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let data = match action {
        "knowledge.create" | "knowledge.update" => {
            let title = form.get("title").map_or("", |title| title.trim());
            let body = form.get("body").map_or("", |body| body.trim());
            if !valid_title(title) || !valid_body(body) {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let categories = match parse_terms(&form, "categories") {
                Ok(values) => values,
                Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
            };
            let keywords = match parse_terms(&form, "keywords") {
                Ok(values) => values,
                Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
            };
            let custom_embed = match parse_custom_embed(&form) {
                Ok(value) => value,
                Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
            };
            let mut data = json!({ "id": article_id, "title": title, "body": body });
            if let Some(categories) = categories {
                data["categories"] = json!(categories);
            }
            if let Some(keywords) = keywords {
                data["keywords"] = json!(keywords);
            }
            if form.contains_key("published_present") {
                let published = form
                    .get("published")
                    .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "on"));
                data["published"] = json!(published);
            }
            if let Some(custom_embed) = custom_embed {
                data["custom_embed"] = custom_embed;
            }
            data
        }
        "knowledge.delete" => json!({ "id": article_id }),
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    match app
        .api
        .ticket_action(
            &id,
            "knowledge",
            "knowledge",
            action,
            data,
            etag,
            &actor,
            key,
        )
        .await
    {
        Ok(_) => {
            let mut parameters = vec![("saved", "1".to_owned())];
            if action != "knowledge.create" {
                if let Some(cursor) = cursor {
                    parameters.push(("cursor", cursor.to_owned()));
                }
                if let Some(search) = search {
                    parameters.push(("q", search.to_owned()));
                }
            }
            let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
            Redirect::to(&format!("/g/{id}/tickets/settings/knowledge?{query}")).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}
