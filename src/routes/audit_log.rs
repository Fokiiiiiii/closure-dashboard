use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user, views,
};
use crate::session;
use serde_json::{Value, json};

const PAGE_SIZE: usize = 25;
const MAX_PAGE: usize = 100;

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn date_bound(value: &str, inclusive_end: bool) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
    {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=month_days).contains(&day) {
        return None;
    }

    let shifted_year = year - i64::from(month <= 2);
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let start = days.checked_mul(86_400)?;
    if inclusive_end {
        start.checked_add(86_399)
    } else {
        Some(start)
    }
}

fn valid_entries(value: &Value) -> bool {
    value.as_array().is_some_and(|entries| {
        entries.iter().all(|entry| {
            entry["timestamp"]
                .as_i64()
                .is_some_and(|timestamp| timestamp >= 0)
                && entry["actor_id"].as_str().is_some_and(|id| {
                    (1..=20).contains(&id.len())
                        && id.bytes().all(|byte| byte.is_ascii_digit())
                        && id.parse::<u64>().is_ok_and(|id| id > 0)
                })
                && entry["action"].as_str().is_some_and(valid_token)
                && entry["resource"].as_str().is_some_and(valid_token)
                && entry["outcome"].as_str().is_some_and(valid_token)
        })
    })
}

fn valid_options(value: &Value) -> bool {
    value.as_array().is_some_and(|options| {
        !options.is_empty()
            && options
                .iter()
                .all(|value| value.as_str().is_some_and(valid_token))
    })
}

fn valid_state(value: &Value, page: usize) -> bool {
    value.as_object().is_some_and(|state| {
        state.get("entries").is_some_and(valid_entries)
            && state.get("total").and_then(Value::as_u64).is_some()
            && state.get("page").and_then(Value::as_u64) == Some(page as u64)
            && state.get("page_size").and_then(Value::as_u64) == Some(PAGE_SIZE as u64)
            && state.get("has_more").and_then(Value::as_bool).is_some()
            && state.get("truncated").and_then(Value::as_bool).is_some()
            && state.get("action_options").is_some_and(valid_options)
            && state.get("resource_options").is_some_and(valid_options)
            && state.get("outcome_options").is_some_and(valid_options)
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "Validate filters, query and render the audit page in one route."
)]
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

    let user_id = query.get("user_id").map_or("", String::as_str).trim();
    if !user_id.is_empty()
        && (user_id.len() > 20
            || !user_id.bytes().all(|byte| byte.is_ascii_digit())
            || !user_id.parse::<u64>().is_ok_and(|id| id > 0))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let action_type = query.get("action_type").map_or("", String::as_str).trim();
    let resource_type = query.get("resource_type").map_or("", String::as_str).trim();
    let outcome = query.get("outcome").map_or("", String::as_str).trim();
    if [action_type, resource_type, outcome]
        .iter()
        .any(|value| !value.is_empty() && *value != "all" && !valid_token(value))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let date_from = query.get("date_from").map_or("", String::as_str).trim();
    let date_to = query.get("date_to").map_or("", String::as_str).trim();
    let from = if date_from.is_empty() {
        None
    } else {
        let Some(timestamp) = date_bound(date_from, false) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        Some(timestamp)
    };
    let to = if date_to.is_empty() {
        None
    } else {
        let Some(timestamp) = date_bound(date_to, true) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        Some(timestamp)
    };
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }

    let page = match query.get("page") {
        Some(value) => match value.parse::<usize>() {
            Ok(page) if (1..=MAX_PAGE).contains(&page) => page,
            _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        },
        None => 1,
    };
    let filters = json!({
        "user_id": if user_id.is_empty() { None } else { Some(user_id) },
        "action_type": if action_type.is_empty() { None } else { Some(action_type) },
        "resource_type": if resource_type.is_empty() { None } else { Some(resource_type) },
        "outcome": if outcome.is_empty() { None } else { Some(outcome) },
        "date_from": from,
        "date_to": to,
        "page": page,
        "page_size": PAGE_SIZE,
    });
    let key = app.nonces.fresh();
    let audit = app.api.ticket_audit_search(&id, filters, &actor, &key);
    let guilds = app.api.guilds();
    let (audit, guilds) = tokio::join!(audit, guilds);
    let data = match audit {
        Ok((data, _etag)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(state) = data.get("state").filter(|state| valid_state(state, page)) else {
        return api_error(&crate::closure_api::ApiError::unavailable(), l, &headers);
    };
    let name = guilds.map_or_else(
        || id.clone(),
        |(guilds, _)| guild_name(&guilds, &id).to_owned(),
    );

    Html(views::audit_log_page(
        l,
        &headers,
        &id,
        &name,
        state,
        user_id,
        action_type,
        resource_type,
        outcome,
        date_from,
        date_to,
    ))
    .into_response()
}
