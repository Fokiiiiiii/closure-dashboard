use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Response, State,
    StatusCode, api_error, error_page, keys, lang,
};
use crate::{session, views};

fn valid_cursor(cursor: &str) -> bool {
    cursor.len() == 16 && cursor.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_query(value: &str) -> bool {
    value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

fn valid_category(value: &str) -> bool {
    valid_query(value)
}

fn valid_slug(slug: &str) -> bool {
    (1..=32).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn not_found(l: crate::i18n::Locale<'_>, headers: &HeaderMap, guild: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(views::public_knowledge_not_found_page(l, headers, guild)),
    )
        .into_response()
}

pub(super) async fn list(
    State(app): State<App>,
    Path(guild): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    if !session::snowflake(&guild) {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
    }

    let search = query
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let category = query
        .get("category")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let cursor = query
        .get("cursor")
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    if search.is_some_and(|value| !valid_query(value))
        || category.is_some_and(|value| !valid_category(value))
        || cursor.is_some_and(|value| !valid_cursor(value))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let data = match app
        .api
        .public_knowledge_state(&guild, search, category, cursor)
        .await
    {
        Ok((data, _)) => data,
        Err(error) if error.status == StatusCode::NOT_FOUND => {
            return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
        }
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(state) = data
        .get("state")
        .filter(|state| state.is_object())
        .cloned()
        .filter(|state| {
            state["articles"].is_array()
                && state["categories"].is_array()
                && (state
                    .get("next_cursor")
                    .map_or(true, |cursor| cursor.is_null() || cursor.is_string()))
        })
    else {
        return api_error(&ApiError::unavailable(), l, &headers);
    };

    Html(views::public_knowledge_page(
        l,
        &headers,
        &guild,
        &state,
        search.unwrap_or(""),
        category.unwrap_or(""),
    ))
    .into_response()
}

pub(super) async fn article(
    State(app): State<App>,
    Path((guild, slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let l = lang(&headers, &app);
    if !session::snowflake(&guild) {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
    }
    if !valid_slug(&slug) {
        return not_found(l, &headers, &guild);
    }

    let data = match app.api.public_knowledge_article(&guild, &slug).await {
        Ok((data, _)) => data,
        Err(error) if error.status == StatusCode::NOT_FOUND => {
            return not_found(l, &headers, &guild);
        }
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(article) = data.get("article").filter(|article| article.is_object()) else {
        return not_found(l, &headers, &guild);
    };
    Html(views::public_knowledge_article_page(
        l, &headers, &guild, article,
    ))
    .into_response()
}
