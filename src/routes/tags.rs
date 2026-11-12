use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user,
};
use crate::session;
use axum::Form;
use serde_json::{Value, json};

const MAX_ID_CHARS: usize = 16;
const MAX_BODY_CHARS: usize = 2_000;

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

    let tag_query = query
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let cursor = query
        .get("cursor")
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    let article_query = query
        .get("article_q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let article_cursor = query
        .get("article_cursor")
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    let edit_id = query
        .get("edit")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if tag_query.is_some_and(|value| !valid_query(value))
        || cursor.is_some_and(|value| !valid_tag_id(value))
        || article_query.is_some_and(|value| !valid_query(value))
        || article_cursor.is_some_and(|value| !valid_article_cursor(value))
        || edit_id.is_some_and(|value| !valid_tag_id(value))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let state_query = edit_id.or(tag_query);
    let state_cursor = if edit_id.is_some() { None } else { cursor };
    let article_request = async {
        if article_query.is_some() || article_cursor.is_some() {
            app.api
                .public_knowledge_state(&id, article_query, None, article_cursor)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    };
    let (tags, articles, guilds) = tokio::join!(
        app.api
            .ticket_tags_state(&id, state_query, state_cursor, &actor),
        article_request,
        app.api.guilds()
    );
    let data = match tags {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(mut state) = data
        .get("state")
        .filter(|state| state.is_object())
        .cloned()
        .filter(valid_tag_state)
    else {
        return api_error(&ApiError::unavailable(), l, &headers);
    };

    let article_state = match articles {
        Ok(Some((data, _))) => match data
            .get("state")
            .filter(|state| state.is_object())
            .cloned()
            .filter(valid_public_article_state)
        {
            Some(state) => state,
            None => return api_error(&ApiError::unavailable(), l, &headers),
        },
        Ok(None) => {
            let article_state = json!({
                "articles": state["articles"].clone(),
                "next_cursor": state["articles_next_cursor"].clone(),
            });
            if !valid_public_article_state(&article_state) {
                return api_error(&ApiError::unavailable(), l, &headers);
            }
            article_state
        }
        Err(error) => return api_error(&error, l, &headers),
    };
    state["articles"] = json!(article_options(&article_state));
    state["articles_next_cursor"] = article_state["next_cursor"].clone();
    let edit_tag = edit_id.and_then(|edit_id| {
        state["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tag| tag["id"].as_str() == Some(edit_id))
            .cloned()
    });
    if edit_id.is_some() && edit_tag.is_none() {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
    }

    let guilds = match guilds {
        Ok((guilds, _)) => guilds,
        Err(error) => return api_error(&error, l, &headers),
    };
    Html(views::tags_page(
        l,
        &headers,
        &id,
        guild_name(&guilds, &id),
        &state,
        edit_tag.as_ref(),
        tag_query.unwrap_or(""),
        cursor,
        article_query.unwrap_or(""),
        article_cursor,
        query.get("saved").is_some_and(|value| value == "1"),
        &app.nonces.fresh(),
    ))
    .into_response()
}

pub(super) async fn action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    if query.get("preview").is_some_and(|value| value == "1") {
        return preview(l, &headers, &form);
    }

    let action = form.get("action").map_or("", String::as_str);
    let Some(etag) = form
        .get("etag")
        .map(String::as_str)
        .filter(|value| valid_etag(value))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form
        .get("key")
        .map(String::as_str)
        .filter(|value| valid_key(value))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let tag_query = form
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let cursor = form
        .get("cursor")
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    let article_query = form
        .get("article_q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let article_cursor = form
        .get("article_cursor")
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    if tag_query.is_some_and(|value| !valid_query(value))
        || cursor.is_some_and(|value| !valid_tag_id(value))
        || article_query.is_some_and(|value| !valid_query(value))
        || article_cursor.is_some_and(|value| !valid_article_cursor(value))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let data = match action {
        "tag.create" | "tag.update" => {
            let raw_id = form.get("id").map_or("", String::as_str);
            let tag_id = if action == "tag.create" {
                normalize_tag_id(raw_id)
            } else {
                raw_id.trim().to_owned()
            };
            let body = form.get("body").map_or("", String::as_str);
            let article_id = form
                .get("article_id")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if !valid_tag_id(&tag_id)
                || !valid_body(body)
                || article_id.is_some_and(|value| !valid_knowledge_id(value))
            {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            let embed = match parse_embed(&form) {
                Ok(embed) => embed,
                Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
            };
            let custom_command_alias = match form.get("custom_command_alias").map(String::as_str) {
                None | Some("0" | "false" | "off") => false,
                Some("1" | "true" | "on") => true,
                Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
            };
            if body.trim().is_empty() && embed.is_none() && article_id.is_none() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({
                "id": tag_id,
                "body": body,
                "embed": embed,
                "article_id": article_id,
                "custom_command_alias": custom_command_alias,
            })
        }
        "tag.clone" => {
            let source_id = form.get("id").map_or("", String::as_str).trim();
            let new_id = normalize_tag_id(form.get("new_id").map_or("", String::as_str));
            if !valid_tag_id(source_id) || !valid_tag_id(&new_id) || source_id == new_id {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({ "id": source_id, "new_id": new_id })
        }
        "tag.delete" => {
            let source_id = form.get("id").map_or("", String::as_str).trim();
            if !valid_tag_id(source_id)
                || !form
                    .get("confirm_delete")
                    .is_some_and(|value| matches!(value.as_str(), "1" | "on" | "true"))
            {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            json!({ "id": source_id })
        }
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };

    match app
        .api
        .ticket_tag_action(&id, action, data, etag, &actor, key)
        .await
    {
        Ok(_) => {
            let mut parameters = vec![("saved", "1".to_owned())];
            if !matches!(action, "tag.create" | "tag.clone") {
                if let Some(cursor) = cursor {
                    parameters.push(("cursor", cursor.to_owned()));
                }
                if let Some(tag_query) = tag_query {
                    parameters.push(("q", tag_query.to_owned()));
                }
            }
            if let Some(article_query) = article_query {
                parameters.push(("article_q", article_query.to_owned()));
            }
            let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
            Redirect::to(&format!("/g/{id}/tickets/tags?{query}")).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

fn preview(
    l: crate::i18n::Locale<'_>,
    headers: &HeaderMap,
    form: &HashMap<String, String>,
) -> Response {
    let body = form.get("body").map_or("", String::as_str);
    if !valid_body(body) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let embed = match parse_embed(form) {
        Ok(embed) => embed,
        Err(()) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers),
    };
    Html(views::tags_preview(l, body, embed.as_ref())).into_response()
}

fn valid_tag_state(state: &Value) -> bool {
    state["tags"].as_array().is_some_and(|tags| {
        tags.iter().all(|tag| {
            let tag_type = tag["type"].as_str();
            valid_tag_id(tag["id"].as_str().unwrap_or(""))
                && matches!(tag_type, Some("Text" | "Embed" | "Embed + Text"))
                && tag["body"].as_str().is_some_and(valid_body)
                && (tag["embed"].is_null() || tag["embed"].is_object())
                && (tag["article_id"].is_null()
                    || valid_knowledge_id(tag["article_id"].as_str().unwrap_or("")))
                && (tag["article_title"].is_null()
                    || tag["article_title"].as_str().is_some_and(|title| {
                        !title.trim().is_empty()
                            && title.chars().count() <= 100
                            && !title.chars().any(char::is_control)
                    }))
                && tag
                    .get("custom_command_alias")
                    .map_or(true, Value::is_boolean)
                && tag
                    .get("alias_sync_pending")
                    .map_or(true, Value::is_boolean)
                && tag["revision"]
                    .as_i64()
                    .is_some_and(|revision| revision > 0)
        })
    }) && state
        .get("next_cursor")
        .is_some_and(|cursor| cursor.is_null() || cursor.as_str().is_some_and(valid_tag_id))
        && state.get("articles").is_some_and(Value::is_array)
        && state.get("articles_next_cursor").is_some_and(|cursor| {
            cursor.is_null() || cursor.as_str().is_some_and(valid_article_cursor)
        })
}

fn valid_public_article_state(state: &Value) -> bool {
    state["articles"].as_array().is_some()
        && state.get("next_cursor").is_some_and(|cursor| {
            cursor.is_null() || cursor.as_str().is_some_and(valid_article_cursor)
        })
        && state["articles"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|article| {
                valid_knowledge_id(article["id"].as_str().unwrap_or(""))
                    && article["title"].as_str().is_some_and(|title| {
                        !title.trim().is_empty()
                            && title.chars().count() <= 100
                            && !title.chars().any(char::is_control)
                    })
            })
}

fn article_options(state: &Value) -> Vec<Value> {
    state["articles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|article| {
            json!({
                "id": article["id"],
                "title": article["title"],
            })
        })
        .collect()
}

fn parse_embed(form: &HashMap<String, String>) -> Result<Option<Value>, ()> {
    let enabled = form
        .get("use_embed")
        .is_some_and(|value| matches!(value.as_str(), "1" | "on" | "true"));
    if !enabled {
        return Ok(None);
    }
    let text = form.get("embed").map_or("", String::as_str).trim();
    if text.is_empty() || text.chars().count() > 6_000 {
        return Err(());
    }
    let value: Value = serde_json::from_str(text).map_err(|_| ())?;
    value.as_object().map(|_| Some(value)).ok_or(())
}

fn valid_body(body: &str) -> bool {
    body.chars().count() <= MAX_BODY_CHARS
        && !body.chars().any(|character| {
            character == '\0' || character.is_control() && !matches!(character, '\n' | '\r' | '\t')
        })
}

fn normalize_tag_id(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(MAX_ID_CHARS)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn valid_tag_id(value: &str) -> bool {
    (1..=MAX_ID_CHARS).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_knowledge_id(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_query(value: &str) -> bool {
    value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

fn valid_article_cursor(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn valid_etag(value: &str) -> bool {
    value
        .trim()
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .is_some_and(|revision| {
            !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn valid_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::{HashMap, normalize_tag_id, parse_embed};

    #[test]
    fn tag_form_normalizes_ids_and_rejects_non_object_embeds() {
        assert_eq!(normalize_tag_id(" Refund! Policy "), "refundpolicy");

        let mut form = HashMap::from([
            ("use_embed".to_owned(), "1".to_owned()),
            (
                "embed".to_owned(),
                r##"{"title":"Refund policy","color":"#aabbcc"}"##.to_owned(),
            ),
        ]);
        assert!(parse_embed(&form).is_ok());

        form.insert("embed".to_owned(), "[]".to_owned());
        assert!(parse_embed(&form).is_err());
    }
}
