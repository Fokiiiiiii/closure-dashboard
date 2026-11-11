use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
};

fn count(value: &Value) -> String {
    value
        .as_u64()
        .map(group_digits)
        .unwrap_or_else(|| "—".to_owned())
}

fn average(value: &Value, unit: &str) -> String {
    value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
        .map(|number| format!("{number:.1} {unit}"))
        .unwrap_or_else(|| "—".to_owned())
}

fn utc_timestamp(seconds: i64) -> String {
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
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    year += i64::from(month <= 2);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn metric(body: &mut String, label: String, value: String) {
    let _ = write!(
        body,
        r#"<tr><th>{}</th><td>{}</td></tr>"#,
        label,
        escape(&value)
    );
}

fn valid_cursor(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn next_url(id: &str, days: u16, cursor: &str) -> String {
    format!(
        "/g/{}/tickets/analytics?days={days}&cursor={}",
        escape(id),
        escape(cursor)
    )
}

pub(crate) fn ticket_analytics_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    days: u16,
    state: &Value,
) -> String {
    let id_html = escape(id);
    let page_url = format!("/g/{id_html}/tickets/analytics");
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets">{}</a></div></header>"#,
        l.text(keys::TICKET_ANALYTICS_TITLE),
        l.text(keys::TICKET_ANALYTICS_BACK)
    );

    let _ = write!(
        body,
        r#"<section class="card panel"><form method="get" action="{page_url}"><label class="field"><span>{}</span><select name="days">"#,
        l.text(keys::TICKET_ANALYTICS_PERIOD)
    );
    for (period, key) in [
        (7_u16, keys::TICKET_ANALYTICS_7_DAYS),
        (30_u16, keys::TICKET_ANALYTICS_30_DAYS),
        (90_u16, keys::TICKET_ANALYTICS_90_DAYS),
    ] {
        let selected = if period == days { " selected" } else { "" };
        let _ = write!(
            body,
            r#"<option value="{period}"{selected}>{}</option>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><button class="button secondary" type="submit">{}</button></form></section>"#,
        l.text(keys::TICKET_ANALYTICS_SUBMIT)
    );
    let _ = write!(
        body,
        r#"<p class="form-status" role="status" data-analytics-loading hidden>{}</p><div id="ticket-analytics-results">"#,
        l.text(keys::TICKET_ANALYTICS_LOADING)
    );

    if let (Some(start), Some(end)) = (state["period_start"].as_i64(), state["period_end"].as_i64())
    {
        let range = l.with_texts(
            keys::TICKET_ANALYTICS_RANGE,
            &[
                ("start", &utc_timestamp(start)),
                ("end", &utc_timestamp(end)),
            ],
        );
        let _ = write!(body, r#"<p class="settings-help">{}</p>"#, range);
    }

    let _ = write!(
        body,
        r#"<p class="settings-help">{}</p><section class="card panel"><h2>{}</h2><table><tbody>"#,
        l.text(keys::TICKET_ANALYTICS_RETAINED_SCOPE),
        l.text(keys::TICKET_ANALYTICS_TITLE)
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_CLOSED),
        count(&state["closed_count"]),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_OPEN),
        count(&state["open_count"]),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_RECORDS_SCANNED),
        count(&state["records_scanned"]),
    );
    body.push_str("</tbody></table>");

    if state["complete"].as_bool() == Some(true) {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ANALYTICS_COMPLETE)
        );
    } else {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ANALYTICS_INCOMPLETE)
        );
    }
    if state["data_complete"].as_bool() == Some(false)
        || state["has_unreadable_records"].as_bool() == Some(true)
    {
        let _ = write!(
            body,
            r#"<p class="error" role="status">{}</p>"#,
            l.text(keys::TICKET_ANALYTICS_DATA_INCOMPLETE)
        );
    }
    let _ = write!(
        body,
        r#"<p class="settings-help">{}</p></section>"#,
        l.text(keys::TICKET_ANALYTICS_FILTERED_SCOPE)
    );

    let duration = &state["duration"];
    let _ = write!(
        body,
        r#"<section class="card panel"><h2>{}</h2><table><tbody>"#,
        l.text(keys::TICKET_ANALYTICS_DURATION)
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_DURATION_AVERAGE),
        average(&duration["average_seconds"], "s"),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_DURATION_SAMPLES),
        count(&duration["samples"]),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_DURATION_MISSING),
        count(&duration["missing"]),
    );
    body.push_str("</tbody></table></section>");

    let ratings = &state["ratings"];
    let _ = write!(
        body,
        r#"<section class="card panel"><h2>{}</h2><table><tbody>"#,
        l.text(keys::TICKET_ANALYTICS_RATINGS)
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_RATINGS_AVERAGE),
        average(&ratings["average"], "★"),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_RATINGS_COUNT),
        count(&ratings["count"]),
    );
    metric(
        &mut body,
        l.text(keys::TICKET_ANALYTICS_RATINGS_MISSING),
        count(&ratings["missing"]),
    );
    body.push_str("</tbody></table>");
    let _ = write!(
        body,
        r#"<h3>{}</h3><table><tbody>"#,
        l.text(keys::TICKET_ANALYTICS_RATING_DISTRIBUTION)
    );
    let distribution = &ratings["distribution"];
    for score in 1..=5 {
        let score_text = score.to_string();
        metric(
            &mut body,
            l.with_text(keys::TICKET_ANALYTICS_RATING_BUCKET, "score", &score_text),
            count(
                distribution
                    .get(score_text.as_str())
                    .unwrap_or(&Value::Null),
            ),
        );
    }
    body.push_str("</tbody></table>");
    if state["complete"].as_bool() == Some(true)
        && state["data_complete"].as_bool() == Some(true)
        && state["has_unreadable_records"].as_bool() == Some(false)
        && ratings["count"].as_u64() == Some(0)
    {
        let _ = write!(
            body,
            r#"<p class="settings-help">{}</p>"#,
            l.text(keys::TICKET_ANALYTICS_NO_RATINGS)
        );
    }
    body.push_str("</section>");

    if state["complete"].as_bool() != Some(true)
        && let Some(cursor) = state["next_cursor"]
            .as_str()
            .filter(|cursor| valid_cursor(cursor))
    {
        let _ = write!(
            body,
            r#"<p><a class="button secondary" data-analytics-continue href="{}">{}</a></p>"#,
            next_url(id, days, cursor),
            l.text(keys::TICKET_ANALYTICS_CONTINUE)
        );
    }

    body.push_str("</div>");
    super::shell(l, headers, id, name, keys::TICKETS, &body, true)
}
