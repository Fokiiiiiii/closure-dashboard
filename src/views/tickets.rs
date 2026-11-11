use super::{heading, shell};
use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

fn ticket_number(ticket: &Value) -> u64 {
    ticket["number"].as_u64().unwrap_or(0)
}

fn state_label(l: Locale<'_>, state: &str) -> String {
    l.text(match state {
        "open" => keys::TICKET_OPEN,
        "closing" => keys::TICKET_CLOSING,
        "closed" => keys::TICKET_CLOSED,
        _ => return escape(state),
    })
}

fn id_text(id: &str) -> String {
    if id.is_empty() {
        "—".to_owned()
    } else {
        format!(r#"<span class="mono">{}</span>"#, escape(id))
    }
}

fn channel_name<'a>(directory: Option<&'a Value>, id: &str) -> Option<&'a str> {
    directory?["channels"]
        .as_array()?
        .iter()
        .find(|channel| channel["id"].as_str() == Some(id))?["name"]
        .as_str()
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

fn row(body: &mut String, label: String, value: String) {
    let _ = write!(body, r#"<tr><th>{label}</th><td>{value}</td></tr>"#);
}

pub(crate) fn tickets(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    list: &Value,
    directory: Option<&Value>,
    state: &str,
) -> String {
    let id_html = escape(id);
    let mut body = format!(
        r#"<header class="page-heading">{}</header>"#,
        heading(l, keys::TICKETS)
    );
    body.push_str(r#"<section class="card panel"><div class="head-actions">"#);
    for (tab, key) in [("open", keys::TICKET_OPEN), ("closed", keys::TICKET_CLOSED)] {
        let current = if tab == state {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<a class="button secondary" href="/g/{id_html}/tickets?state={tab}"{current}>{}</a>"#,
            l.text(key)
        );
    }
    body.push_str(r#"</div><div class="table-scroll"><table><thead><tr>"#);
    for key in [
        keys::NUMBER,
        keys::TICKET_STATE,
        keys::TICKET_OPENER,
        keys::TICKET_PANEL,
        keys::TICKET_CREATED,
        keys::TICKET_CLOSED_AT,
    ] {
        let _ = write!(body, "<th>{}</th>", l.text(key));
    }
    body.push_str("</tr></thead><tbody>");
    let tickets = list["tickets"].as_array().map_or(&[][..], Vec::as_slice);
    for ticket in tickets {
        let number = ticket_number(ticket);
        let channel_id = ticket["channel_id"].as_str().unwrap_or("");
        let ticket_link = if session::snowflake(channel_id) {
            format!(
                r#"<a href="/g/{id_html}/tickets/{}?state={state}">#{}</a>"#,
                escape(channel_id),
                group_digits(number)
            )
        } else {
            format!("#{}", group_digits(number))
        };
        let opener = id_text(ticket["opener"].as_str().unwrap_or(""));
        let panel = ticket["panel"]
            .as_str()
            .map_or_else(|| "—".to_owned(), escape);
        let created = ticket["created"]
            .as_i64()
            .map_or_else(|| "—".to_owned(), |time| l.relative_time(time, session::now()));
        let closed = ticket["closed"]
            .as_i64()
            .map_or_else(|| "—".to_owned(), |time| l.relative_time(time, session::now()));
        let channel = channel_name(directory, channel_id).map_or_else(String::new, |name| {
            format!(r#"<span class="mono">{}</span>"#, escape(name))
        });
        let _ = write!(
            body,
            r#"<tr><td>{ticket_link} {channel}</td><td><span class="pill">{}</span></td><td>{opener}</td><td>{panel}</td><td>{created}</td><td>{closed}</td></tr>"#,
            state_label(l, ticket["state"].as_str().unwrap_or(""))
        );
    }
    body.push_str("</tbody></table></div>");
    if tickets.is_empty() {
        let _ = write!(
            body,
            r#"<p class="empty">{}</p>"#,
            l.text(keys::TICKETS_NONE)
        );
    }
    if let Some(before) = list["next_before"]
        .as_str()
        .filter(|before| cursor_valid(state, before))
    {
        let query = escape(
            &serde_urlencoded::to_string([("state", state), ("before", before)])
                .unwrap_or_default(),
        );
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="/g/{id_html}/tickets?{query}">{}</a></p>"#,
            l.text(keys::TICKET_MORE)
        );
    }
    body.push_str("</section>");
    shell(l, headers, id, name, keys::TICKETS, &body, false)
}

pub(crate) fn ticket(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    channel_id: &str,
    ticket: &Value,
    directory: Option<&Value>,
    state: &str,
    start: u32,
) -> String {
    let number = ticket_number(ticket);
    let id_html = escape(id);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets?state={state}">{}</a></div></header>"#,
        l.number(keys::TICKET_TITLE, number),
        l.text(keys::TICKET_BACK)
    );
    body.push_str(r#"<section class="card panel"><div class="table-scroll"><table><tbody>"#);
    row(
        &mut body,
        l.text(keys::TICKET_STATE),
        state_label(l, ticket["state"].as_str().unwrap_or("")),
    );
    let channel = channel_name(directory, channel_id).map_or_else(
        || id_text(channel_id),
        |name| escape(name),
    );
    row(&mut body, l.text(keys::TICKET_CHANNEL), channel);
    row(
        &mut body,
        l.text(keys::TICKET_OPENER),
        id_text(ticket["opener"].as_str().unwrap_or("")),
    );
    if let Some(claimed_by) = ticket["claimed_by"].as_str() {
        row(
            &mut body,
            l.text(keys::TICKET_CLAIMED_BY),
            id_text(claimed_by),
        );
    }
    if let Some(panel) = ticket["panel"].as_str() {
        row(&mut body, l.text(keys::TICKET_PANEL), escape(panel));
    }
    if let Some(created) = ticket["created"].as_i64() {
        row(
            &mut body,
            l.text(keys::TICKET_CREATED),
            l.relative_time(created, session::now()),
        );
    }
    if let Some(closed) = ticket["closed"].as_i64() {
        row(
            &mut body,
            l.text(keys::TICKET_CLOSED_AT),
            l.relative_time(closed, session::now()),
        );
    }
    if let Some(closer) = ticket["closer"].as_str() {
        row(
            &mut body,
            l.text(keys::TICKET_CLOSER),
            id_text(closer),
        );
    }
    if let Some(reason) = ticket["reason"].as_str() {
        row(&mut body, l.text(keys::TICKET_REASON), escape(reason));
    }
    body.push_str("</tbody></table></div></section>");
    if let Some(transcript) = ticket["transcript"].as_object() {
        let mut text = String::new();
        if let Some(pages) = transcript.get("pages").and_then(Value::as_array) {
            for page in pages {
                if let Some(page_text) = page["text"].as_str() {
                    text.push_str(page_text);
                }
            }
        } else if let Some(page_text) = transcript.get("text").and_then(Value::as_str) {
            text.push_str(page_text);
        }
        if transcript.get("truncated") == Some(&Value::Bool(true)) {
            let _ = write!(
                body,
                r#"<p class="form-status" role="status">{}</p>"#,
                l.text(keys::TICKET_TRANSCRIPT_TRUNCATED)
            );
        }
        let _ = write!(
            body,
            r#"<section class="card panel"><div class="panel-heading"><h2>{}</h2></div><pre class="ticket-transcript">{}</pre><div class="head-actions">"#,
            l.text(keys::TICKET_TRANSCRIPT),
            escape(&text)
        );
        if start > 0 {
            let previous = start.saturating_sub(4);
            let query = escape(&format!("state={state}&start={previous}"));
            let _ = write!(
                body,
                r#"<a class="button secondary" href="/g/{id_html}/tickets/{}?{query}">{}</a>"#,
                escape(channel_id),
                l.text(keys::TICKET_TRANSCRIPT_PREVIOUS)
            );
        }
        if let Some(next) = transcript
            .get("next_page")
            .and_then(Value::as_u64)
            .and_then(|next| u32::try_from(next).ok())
            .filter(|next| *next > start)
        {
            let query = escape(&format!("state={state}&start={next}"));
            let _ = write!(
                body,
                r#"<a class="button secondary" href="/g/{id_html}/tickets/{}?{query}">{}</a>"#,
                escape(channel_id),
                l.text(keys::TICKET_TRANSCRIPT_NEXT)
            );
        }
        body.push_str("</div></section>");
    }
    shell(l, headers, id, name, keys::TICKETS, &body, true)
}
