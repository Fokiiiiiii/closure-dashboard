use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user, views,
};
use crate::session;
use serde_json::Value;

fn valid_cursor(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_optional_number(value: Option<&Value>, max: Option<f64>) -> bool {
    value.is_some_and(|value| {
        value.is_null()
            || value.as_f64().is_some_and(|number| {
                number.is_finite() && number >= 0.0 && max.is_none_or(|max| number <= max)
            })
    })
}

fn valid_state(value: &Value, days: u16) -> bool {
    let Some(state) = value.as_object() else {
        return false;
    };
    if state.get("period_days").and_then(Value::as_u64) != Some(u64::from(days))
        || state.get("period_start").and_then(Value::as_i64).is_none()
        || state.get("period_end").and_then(Value::as_i64).is_none()
        || state
            .get("period_start")
            .and_then(Value::as_i64)
            .zip(state.get("period_end").and_then(Value::as_i64))
            .is_none_or(|(start, end)| end < start)
        || ["closed_count", "open_count", "records_scanned"]
            .iter()
            .any(|key| state.get(*key).and_then(Value::as_u64).is_none())
        || ["complete", "data_complete", "has_unreadable_records"]
            .iter()
            .any(|key| state.get(*key).and_then(Value::as_bool).is_none())
    {
        return false;
    }

    let Some(duration) = state.get("duration").and_then(Value::as_object) else {
        return false;
    };
    if ["samples", "missing", "total_seconds"]
        .iter()
        .any(|key| duration.get(*key).and_then(Value::as_u64).is_none())
        || !valid_optional_number(duration.get("average_seconds"), None)
    {
        return false;
    }

    let Some(ratings) = state.get("ratings").and_then(Value::as_object) else {
        return false;
    };
    if ["count", "missing", "score_sum"]
        .iter()
        .any(|key| ratings.get(*key).and_then(Value::as_u64).is_none())
        || !valid_optional_number(ratings.get("average"), Some(5.0))
    {
        return false;
    }
    let Some(distribution) = ratings.get("distribution").and_then(Value::as_object) else {
        return false;
    };
    if (1..=5).any(|score| {
        distribution
            .get(&score.to_string())
            .and_then(Value::as_u64)
            .is_none()
    }) {
        return false;
    }

    state
        .get("next_cursor")
        .is_none_or(|cursor| cursor.is_null() || cursor.as_str().is_some_and(valid_cursor))
}

pub(super) async fn page(
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

    let days = match query.get("days").map(String::as_str) {
        None | Some("7") => 7,
        Some("30") => 30,
        Some("90") => 90,
        Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    let cursor = match query.get("cursor") {
        None => None,
        Some(value) if valid_cursor(value) => Some(value.as_str()),
        Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };

    let analytics = app.api.ticket_analytics_state(&id, days, cursor, &actor);
    let guilds = app.api.guilds();
    let (analytics, guilds) = tokio::join!(analytics, guilds);
    let data = match analytics {
        Ok((data, _etag)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(state) = data.get("state").filter(|state| valid_state(state, days)) else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    };
    let name = guilds.ok().map_or_else(
        || id.clone(),
        |(guilds, _)| guild_name(&guilds, &id).to_owned(),
    );

    Html(views::ticket_analytics_page(
        l, &headers, &id, &name, days, state,
    ))
    .into_response()
}
