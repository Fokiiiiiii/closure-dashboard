use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

const DEFAULT_PAGE_SIZE: usize = 25;

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn options(
    l: &Locale<'_>,
    values: Option<&Value>,
    selected: &str,
    all_label_key: &'static str,
    option_label_key: &'static str,
    argument: &'static str,
) -> String {
    let mut html = format!(
        r#"<option value=""{}>{}</option>"#,
        if selected.is_empty() || selected == "all" {
            " selected"
        } else {
            ""
        },
        escape(&l.text(all_label_key))
    );
    if let Some(values) = values.and_then(Value::as_array) {
        for value in values
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| valid_token(value))
        {
            let selected = if selected == value { " selected" } else { "" };
            let value_html = escape(value);
            let label_html = escape(&l.with_text(option_label_key, argument, value));
            let _ = write!(
                html,
                r#"<option value="{value_html}"{selected}>{label_html}</option>"#
            );
        }
    }
    html
}

fn utc_timestamp(seconds: i64) -> String {
    if seconds < 0 {
        return "—".to_owned();
    }
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_era - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    year += i64::from(month <= 2);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn page_url(
    id: &str,
    user_id: &str,
    action_type: &str,
    resource_type: &str,
    outcome: &str,
    date_from: &str,
    date_to: &str,
    page: usize,
    page_size: usize,
) -> String {
    let mut parameters = vec![
        ("page".to_owned(), page.to_string()),
        ("page_size".to_owned(), page_size.to_string()),
    ];
    for (key, value) in [
        ("user_id", user_id),
        ("action_type", action_type),
        ("resource_type", resource_type),
        ("outcome", outcome),
        ("date_from", date_from),
        ("date_to", date_to),
    ] {
        if !value.is_empty() {
            parameters.push((key.to_owned(), value.to_owned()));
        }
    }
    let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
    format!("/g/{}/tickets/audit-log?{query}", escape(id))
}

pub(crate) fn page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    state: &Value,
    user_id: &str,
    action_type: &str,
    resource_type: &str,
    outcome: &str,
    date_from: &str,
    date_to: &str,
) -> String {
    let id_html = escape(id);
    let title = l.text(keys::TICKET_AUDIT_LOG);
    let back = l.text(keys::TICKETS);
    let mut body = String::new();
    let _ = write!(
        body,
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets">{}</a></div></header>"#,
        escape(&title),
        escape(&back)
    );
    let page_url = format!("/g/{id_html}/tickets/audit-log");
    let action_options = options(
        &l,
        state.get("action_options"),
        action_type,
        keys::TICKET_AUDIT_ALL_ACTIONS,
        keys::TICKET_AUDIT_ACTION_LABEL,
        "action",
    );
    let resource_options = options(
        &l,
        state.get("resource_options"),
        resource_type,
        keys::TICKET_AUDIT_ALL_RESOURCES,
        keys::TICKET_AUDIT_RESOURCE_LABEL,
        "resource",
    );
    let outcome_options = options(
        &l,
        state.get("outcome_options"),
        outcome,
        keys::TICKET_AUDIT_ALL_RESULTS,
        keys::TICKET_AUDIT_OUTCOME_LABEL,
        "outcome",
    );
    let _ = write!(
        body,
        r#"<section class="card settings-card"><form method="get" action="{page_url}" data-audit-filter><label class="field"><span>{}</span><input type="text" name="user_id" inputmode="numeric" pattern="[0-9]{{1,20}}" maxlength="20" value="{}"></label><label class="field"><span>{}</span><select name="action_type">{action_options}</select></label><label class="field"><span>{}</span><select name="resource_type">{resource_options}</select></label><label class="field"><span>{}</span><select name="outcome">{outcome_options}</select></label><label class="field"><span>{}</span><input type="date" name="date_from" value="{}"></label><label class="field"><span>{}</span><input type="date" name="date_to" value="{}"></label><button class="button secondary" type="submit">{}</button></form></section>"#,
        escape(&l.text(keys::TICKET_AUDIT_USER_ID)),
        escape(user_id),
        escape(&l.text(keys::TICKET_AUDIT_ACTION_TYPE)),
        escape(&l.text(keys::TICKET_AUDIT_RESOURCE_TYPE)),
        escape(&l.text(keys::TICKET_AUDIT_RESULT)),
        escape(&l.text(keys::TICKET_AUDIT_DATE_FROM)),
        escape(date_from),
        escape(&l.text(keys::TICKET_AUDIT_DATE_TO)),
        escape(date_to),
        escape(&l.text(keys::TICKET_AUDIT_APPLY_FILTERS))
    );

    let entries = state["entries"].as_array().map_or(&[][..], Vec::as_slice);
    let total = state["total"].as_u64().unwrap_or_default();
    let page = state["page"]
        .as_u64()
        .and_then(|page| usize::try_from(page).ok())
        .unwrap_or(1);
    let page_size = state["page_size"]
        .as_u64()
        .and_then(|size| usize::try_from(size).ok())
        .filter(|size| (1..=100).contains(size))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let truncated = state["truncated"].as_bool().unwrap_or(false);
    let has_more = state["has_more"].as_bool().unwrap_or(false);
    let first = if entries.is_empty() {
        0
    } else {
        (page - 1) * page_size + 1
    };
    let last = (page - 1) * page_size + entries.len();
    let first_text = first.to_string();
    let last_text = last.to_string();
    let total_text = total.to_string();
    let count_summary = if total == 0 {
        l.text(keys::TICKET_AUDIT_EMPTY)
    } else if truncated {
        l.with_texts(
            keys::TICKET_AUDIT_COUNT_TRUNCATED,
            &[
                ("first", &first_text),
                ("last", &last_text),
                ("total", &total_text),
            ],
        )
    } else {
        l.with_texts(
            keys::TICKET_AUDIT_COUNT,
            &[
                ("first", &first_text),
                ("last", &last_text),
                ("total", &total_text),
            ],
        )
    };
    let _ = write!(
        body,
        r#"<p class="form-status" role="status">{}</p>"#,
        escape(&count_summary)
    );
    if truncated {
        let _ = write!(
            body,
            r#"<p class="settings-help" role="status">{}</p>"#,
            escape(&l.text(keys::TICKET_AUDIT_INCOMPLETE))
        );
    }

    let _ = write!(
        body,
        r#"<section class="card settings-card"><div class="panel-heading"><h2>{}</h2></div><table><thead><tr><th scope="col">{}</th><th scope="col">{}</th><th scope="col">{}</th><th scope="col">{}</th><th scope="col">{}</th></tr></thead><tbody>"#,
        escape(&l.text(keys::TICKET_AUDIT_ENTRIES)),
        escape(&l.text(keys::TICKET_AUDIT_TIMESTAMP)),
        escape(&l.text(keys::TICKET_AUDIT_USER_ID)),
        escape(&l.text(keys::TICKET_AUDIT_ACTION)),
        escape(&l.text(keys::TICKET_AUDIT_RESOURCE)),
        escape(&l.text(keys::TICKET_AUDIT_OUTCOME))
    );
    for entry in entries {
        let timestamp = entry["timestamp"].as_i64().unwrap_or_default();
        let timestamp = utc_timestamp(timestamp);
        let actor_id = entry["actor_id"].as_str().unwrap_or("");
        let actor = if actor_id == "system" {
            l.text(keys::TICKET_AUDIT_SYSTEM)
        } else {
            actor_id.to_owned()
        };
        let action = entry["action"].as_str().unwrap_or("");
        let resource = entry["resource"].as_str().unwrap_or("");
        let outcome = entry["outcome"].as_str().unwrap_or("");
        let action_label = l.with_text(keys::TICKET_AUDIT_ACTION_LABEL, "action", action);
        let resource_label = l.with_text(keys::TICKET_AUDIT_RESOURCE_LABEL, "resource", resource);
        let outcome_label = l.with_text(keys::TICKET_AUDIT_OUTCOME_LABEL, "outcome", outcome);
        let _ = write!(
            body,
            r#"<tr><td><time datetime="{}">{}</time></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
            escape(&timestamp),
            escape(&timestamp),
            escape(&actor),
            escape(&action_label),
            escape(&resource_label),
            escape(&outcome_label)
        );
    }
    body.push_str("</tbody></table>");
    if entries.is_empty() {
        let _ = write!(
            body,
            r#"<p class="empty">{}</p>"#,
            escape(&l.text(keys::TICKET_AUDIT_EMPTY))
        );
    }
    let privacy = l.text(keys::TICKET_AUDIT_PRIVACY);
    let _ = write!(
        body,
        r#"</section><p class="settings-help">{}</p>"#,
        escape(&privacy)
    );

    if page > 1 || has_more {
        let _ = write!(
            body,
            r#"<nav class="pagination" aria-label="{}">"#,
            escape(&l.text(keys::TICKET_AUDIT_PAGES_LABEL))
        );
        if page > 1 {
            let previous = page_url(
                id,
                user_id,
                action_type,
                resource_type,
                outcome,
                date_from,
                date_to,
                page - 1,
                page_size,
            );
            let _ = write!(
                body,
                r#"<a class="button secondary" rel="prev" href="{}">{}</a>"#,
                escape(&previous),
                escape(&l.text(keys::TICKET_AUDIT_PREVIOUS))
            );
        }
        let page_text = page.to_string();
        let page_label = l.with_text(keys::TICKET_AUDIT_PAGE, "page", &page_text);
        let _ = write!(
            body,
            r#"<span aria-current="page">{}</span>"#,
            escape(&page_label)
        );
        if has_more {
            let next = page_url(
                id,
                user_id,
                action_type,
                resource_type,
                outcome,
                date_from,
                date_to,
                page + 1,
                page_size,
            );
            let _ = write!(
                body,
                r#"<a class="button secondary" rel="next" href="{}">{}</a>"#,
                escape(&next),
                escape(&l.text(keys::TICKET_AUDIT_NEXT))
            );
        }
        body.push_str("</nav>");
    }

    super::shell(l, headers, id, name, keys::TICKET_AUDIT_LOG, &body, true)
}
