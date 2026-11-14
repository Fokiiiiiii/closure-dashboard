use super::{heading, shell};
use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

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

fn row(body: &mut String, label: &str, value: &str) {
    let _ = write!(body, r"<tr><th>{label}</th><td>{value}</td></tr>");
}

struct TicketListContext<'a> {
    id_html: String,
    list: &'a Value,
    directory: Option<&'a Value>,
    state: &'a str,
    bulk_keys: &'a HashMap<String, String>,
    can_bulk: bool,
}

// Keep the route-facing adapter stable; the implementation is split into view helpers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tickets(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    list: &Value,
    directory: Option<&Value>,
    state: &str,
    ticket_settings: bool,
    bulk_keys: &HashMap<String, String>,
) -> String {
    let context = TicketListContext {
        id_html: escape(id),
        list,
        directory,
        state,
        bulk_keys,
        can_bulk: state == "open" && !bulk_keys.is_empty(),
    };
    let mut body = ticket_list_header(l, &context, ticket_settings);
    render_ticket_list_rows(&mut body, l, &context);
    if context.can_bulk {
        render_ticket_bulk_actions(&mut body, l);
    }
    if context.list["tickets"].as_array().is_none_or(Vec::is_empty) {
        let _ = write!(
            body,
            r#"<p class="empty">{}</p>"#,
            l.text(keys::TICKETS_NONE)
        );
    }
    render_ticket_list_pagination(&mut body, l, &context);
    body.push_str("</section>");
    shell(l, headers, id, name, keys::TICKETS, &body, false)
}

fn ticket_list_header(
    l: Locale<'_>,
    context: &TicketListContext<'_>,
    ticket_settings: bool,
) -> String {
    let mut body = format!(
        r#"<header class="page-heading">{}<div class="head-actions">"#,
        heading(l, keys::TICKETS)
    );
    if ticket_settings {
        let _ = write!(
            body,
            r#"<a class="button secondary" href="/g/{}/tickets/settings/blacklist">{}</a>"#,
            context.id_html,
            l.text(keys::TICKET_SETTINGS_BLACKLIST)
        );
    }
    let _ = write!(
        body,
        r#"<a class="button secondary" href="/g/{}/tickets/analytics">{}</a>"#,
        context.id_html,
        l.text(keys::TICKET_ANALYTICS)
    );
    body.push_str("</div></header>");
    body.push_str(r#"<section class="card panel"><div class="head-actions">"#);
    for (tab, key) in [("open", keys::TICKET_OPEN), ("closed", keys::TICKET_CLOSED)] {
        let current = if tab == context.state {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<a class="button secondary" href="/g/{}/tickets?state={tab}"{current}>{}</a>"#,
            context.id_html,
            l.text(key)
        );
    }
    if context.can_bulk {
        let _ = write!(
            body,
            r#"<form method="post" action="/g/{}/tickets/actions"><input type="hidden" name="state" value="open">"#,
            context.id_html
        );
    }
    body.push_str(r#"</div><div class="table-scroll"><table><thead><tr>"#);
    if context.can_bulk {
        let _ = write!(body, "<th>{}</th>", l.text(keys::TICKET_BULK_SELECT));
    }
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
    body
}

fn render_ticket_list_rows(body: &mut String, l: Locale<'_>, context: &TicketListContext<'_>) {
    let tickets = context.list["tickets"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for ticket in tickets {
        let number = ticket_number(ticket);
        let channel_id = ticket["channel_id"].as_str().unwrap_or("");
        let ticket_link = if session::snowflake(channel_id) {
            format!(
                r#"<a href="/g/{}/tickets/{}?state={}">#{}</a>"#,
                context.id_html,
                escape(channel_id),
                context.state,
                group_digits(number)
            )
        } else {
            format!("#{}", group_digits(number))
        };
        let opener = id_text(ticket["opener"].as_str().unwrap_or(""));
        let panel = ticket["panel"]
            .as_str()
            .map_or_else(|| "—".to_owned(), escape);
        let created = ticket["created"].as_i64().map_or_else(
            || "—".to_owned(),
            |time| l.relative_time(time, session::now()),
        );
        let closed = ticket["closed"].as_i64().map_or_else(
            || "—".to_owned(),
            |time| l.relative_time(time, session::now()),
        );
        let channel = channel_name(context.directory, channel_id)
            .map_or_else(String::new, |name| {
                format!(r#"<span class="mono">{}</span>"#, escape(name))
            });
        let selector = context
            .bulk_keys
            .get(channel_id)
            .and_then(|key| {
                ticket["revision"].as_i64().map(|revision| {
                    let value = format!("{revision}:{key}:{number}");
                    format!(
                        r#"<td><input type="checkbox" name="ticket_{channel_id}" value="{}" aria-label="{}"></td>"#,
                        escape(&value),
                        escape(&l.text(keys::TICKET_BULK_SELECT))
                    )
                })
            })
            .unwrap_or_default();
        let _ = write!(
            body,
            r#"<tr>{selector}<td>{ticket_link} {channel}</td><td><span class="pill">{}</span></td><td>{opener}</td><td>{panel}</td><td>{created}</td><td>{closed}</td></tr>"#,
            state_label(l, ticket["state"].as_str().unwrap_or(""))
        );
    }
    body.push_str("</tbody></table></div>");
}

fn render_ticket_bulk_actions(body: &mut String, l: Locale<'_>) {
    let _ = write!(
        body,
        r#"<fieldset class="settings-card"><legend>{}</legend><label class="field"><span>{}</span><select name="action" required><option value="ticket.close">{}</option><option value="ticket.request_close">{}</option><option value="ticket.add_label">{}</option><option value="ticket.remove_label">{}</option></select></label><label class="field"><span>{}</span><textarea name="reason" maxlength="1000" rows="2"></textarea></label><label class="field"><span>{}</span><input name="label" type="text"></label><button class="button" type="submit">{}</button></fieldset></form>"#,
        l.text(keys::TICKET_BULK_ACTIONS),
        l.text(keys::TICKET_BULK_ACTION),
        l.text(keys::TICKET_ACTIONS_CLOSE),
        l.text(keys::TICKET_ACTIONS_REQUEST_CLOSE),
        l.text(keys::TICKET_ACTIONS_ADD_LABEL),
        l.text(keys::TICKET_ACTIONS_REMOVE_LABEL),
        l.text(keys::TICKET_ACTIONS_REASON),
        l.text(keys::TICKET_ACTIONS_LABEL),
        l.text(keys::TICKET_BULK_RUN)
    );
}

fn render_ticket_list_pagination(
    body: &mut String,
    l: Locale<'_>,
    context: &TicketListContext<'_>,
) {
    if let Some(before) = context.list["next_before"]
        .as_str()
        .filter(|before| cursor_valid(context.state, before))
    {
        let query = escape(
            &serde_urlencoded::to_string([("state", context.state), ("before", before)])
                .unwrap_or_default(),
        );
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="/g/{}/tickets?{query}">{}</a></p>"#,
            context.id_html,
            l.text(keys::TICKET_MORE)
        );
    }
}

pub(crate) fn ticket_bulk_results(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    state: &str,
    results: &[(u64, String, bool, Option<u16>)],
) -> String {
    let id_html = escape(id);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets?state={state}">{}</a></div></header>"#,
        l.text(keys::TICKET_BULK_RESULTS),
        l.text(keys::TICKET_BULK_BACK)
    );
    body.push_str(r#"<section class="card panel"><div class="table-scroll"><table><thead><tr>"#);
    for key in [
        keys::NUMBER,
        keys::TICKET_CHANNEL,
        keys::TICKET_BULK_RESULT,
        keys::TICKET_BULK_STATUS,
    ] {
        let _ = write!(body, "<th>{}</th>", l.text(key));
    }
    body.push_str("</tr></thead><tbody>");
    for (number, channel_id, succeeded, status) in results {
        let channel = escape(channel_id);
        let result = if *succeeded {
            l.text(keys::TICKET_BULK_SUCCEEDED)
        } else {
            l.text(keys::TICKET_BULK_FAILED)
        };
        let status = status.map_or_else(|| "—".to_owned(), |status| format!("HTTP {status}"));
        let _ = write!(
            body,
            r#"<tr><td>#{}</td><td><span class="mono">{channel}</span></td><td>{result}</td><td>{status}</td></tr>"#,
            group_digits(*number)
        );
    }
    body.push_str("</tbody></table></div></section>");
    super::shell(l, headers, id, name, keys::TICKETS, &body, false)
}

fn transcript_https_url(value: &str) -> bool {
    value.starts_with("https://") && !value.contains('\\') && !value.chars().any(char::is_control)
}

fn transcript_message_text(line: &str) -> &str {
    line.split_once(": ")
        .map_or(line, |(_, content)| content)
        .trim_end_matches(['\n', '\r'])
}

fn transcript_message_author(line: &str) -> Option<&str> {
    let header = line.split_once(": ").map_or(line, |(header, _)| header);
    let author = header.split_once("] ").map_or(header, |(_, author)| author);
    author.rsplit_once(" (").map(|(name, _)| name)
}

fn transcript_message_time(line: &str) -> Option<&str> {
    line.strip_prefix('[')?
        .split_once(']')
        .map(|(time, _)| time)
}

fn transcript_plain_segment(body: &mut String, text: &str) {
    if !text.is_empty() {
        let _ = write!(
            body,
            r#"<pre class="ticket-transcript">{}</pre>"#,
            escape(text)
        );
    }
}

fn render_transcript_page(body: &mut String, page: &Value, seen_messages: &mut HashSet<String>) {
    let text = page["text"].as_str().unwrap_or("");
    let Some(messages) = page["messages"]
        .as_array()
        .filter(|messages| !messages.is_empty())
    else {
        transcript_plain_segment(body, text);
        return;
    };
    let chars = text.chars().collect::<Vec<_>>();
    let mut ranges = Vec::with_capacity(messages.len());
    let mut last_end = 0_usize;
    for message in messages {
        let Some(start) = message["start"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
        else {
            transcript_plain_segment(body, text);
            return;
        };
        let Some(end) = message["end"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
        else {
            transcript_plain_segment(body, text);
            return;
        };
        if start < last_end || end <= start || end > chars.len() {
            transcript_plain_segment(body, text);
            return;
        }
        ranges.push((message, start, end));
        last_end = end;
    }

    let mut cursor = 0;
    for (message, start, end) in ranges {
        transcript_plain_segment(body, &chars[cursor..start].iter().collect::<String>());
        let line = chars[start..end].iter().collect::<String>();
        let message_id = message["message_id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| message["message_id"].as_u64().map(|id| id.to_string()));
        let continuation = message_id
            .as_ref()
            .is_some_and(|id| start == 0 && seen_messages.contains(id));
        if let Some(id) = message_id {
            seen_messages.insert(id);
        }
        render_transcript_message(body, message, &line, continuation);
        cursor = end;
    }
    transcript_plain_segment(body, &chars[cursor..].iter().collect::<String>());
}

fn render_transcript_message(body: &mut String, message: &Value, line: &str, continuation: bool) {
    let content = escape(transcript_message_text(line));
    let author = message["author_name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| transcript_message_author(line))
        .unwrap_or("—");
    let author_id = message["author_id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| message["author_id"].as_u64().map(|id| id.to_string()))
        .unwrap_or_default();
    let timestamp = message["created_at"]
        .as_str()
        .filter(|time| !time.is_empty())
        .or_else(|| transcript_message_time(line));
    if continuation {
        let _ = write!(
            body,
            r#"<div class="ticket-message-continuation">{content}</div>"#
        );
        return;
    }

    body.push_str(
        r#"<article class="card ticket-message"><header class="ticket-message-heading">"#,
    );
    let _ = write!(body, r"<strong>{}</strong>", escape(author));
    if !author_id.is_empty() {
        let _ = write!(body, " {}", id_text(&author_id));
    }
    if let Some(timestamp) = timestamp {
        let _ = write!(body, r"<time>{}</time>", escape(timestamp));
    }
    body.push_str("</header>");
    let _ = write!(
        body,
        r#"<div class="ticket-message-content">{content}</div>"#
    );
    render_message_mentions(body, message);
    render_message_attachments(body, message);
    render_message_embeds(body, message);
    render_message_components(body, message);
    body.push_str("</article>");
}

fn render_message_mentions(body: &mut String, message: &Value) {
    let Some(mentions) = message["mentions"]
        .as_array()
        .filter(|mentions| !mentions.is_empty())
    else {
        return;
    };
    body.push_str(r#"<div class="ticket-message-mentions">"#);
    for mention in mentions {
        let kind = mention["kind"].as_str().unwrap_or("user");
        let id = mention["id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| mention["id"].as_u64().map(|id| id.to_string()));
        let Some(id) = id else {
            continue;
        };
        let prefix = if kind == "channel" { "#" } else { "@" };
        let _ = write!(
            body,
            r#"<span class="pill">{prefix}{}</span> "#,
            escape(&id)
        );
    }
    body.push_str("</div>");
}

fn render_message_attachments(body: &mut String, message: &Value) {
    let Some(attachments) = message["attachments"]
        .as_array()
        .filter(|items| !items.is_empty())
    else {
        return;
    };
    body.push_str(r#"<div class="ticket-message-attachments">"#);
    for attachment in attachments {
        let Some(url) = attachment["url"]
            .as_str()
            .filter(|url| transcript_https_url(url))
        else {
            continue;
        };
        let filename = attachment["filename"].as_str().unwrap_or(url);
        let content_type = attachment["content_type"].as_str().unwrap_or("");
        if content_type.starts_with("image/") {
            let _ = write!(
                body,
                r#"<a class="ticket-attachment-image" href="{}" target="_blank" rel="noopener noreferrer"><img src="{}" alt="{}" loading="lazy"></a>"#,
                escape(url),
                escape(url),
                escape(filename)
            );
        } else {
            let _ = write!(
                body,
                r#"<p><a href="{}" target="_blank" rel="noopener noreferrer">{}</a></p>"#,
                escape(url),
                escape(filename)
            );
        }
    }
    body.push_str("</div>");
}

fn render_message_embeds(body: &mut String, message: &Value) {
    let Some(embeds) = message["embeds"]
        .as_array()
        .filter(|items| !items.is_empty())
    else {
        return;
    };
    for embed in embeds {
        body.push_str(r#"<aside class="card ticket-embed">"#);
        let color = embed["color"]
            .as_u64()
            .filter(|color| *color <= 0x00ff_ffff)
            .map(|color| format!("#{color:06x}"))
            .or_else(|| {
                embed["color"]
                    .as_str()
                    .filter(|color| valid_hex_color(color))
                    .map(str::to_owned)
            });
        if let Some(color) = color {
            let _ = write!(
                body,
                r#"<input class="label-swatch" type="color" value="{}" disabled aria-hidden="true">"#,
                escape(&color)
            );
        }
        if let Some(name) = embed["author"]["name"].as_str() {
            let _ = write!(
                body,
                r#"<p class="ticket-embed-author">{}</p>"#,
                escape(name)
            );
        }
        if let Some(title) = embed["title"].as_str() {
            if let Some(url) = embed["url"]
                .as_str()
                .filter(|url| transcript_https_url(url))
            {
                let _ = write!(
                    body,
                    r#"<h3><a href="{}" target="_blank" rel="noopener noreferrer">{}</a></h3>"#,
                    escape(url),
                    escape(title)
                );
            } else {
                let _ = write!(body, "<h3>{}</h3>", escape(title));
            }
        }
        if let Some(description) = embed["description"].as_str() {
            let _ = write!(body, "<p>{}</p>", escape(description));
        }
        if let Some(fields) = embed["fields"].as_array() {
            for field in fields {
                let name = field["name"].as_str().unwrap_or("");
                let value = field["value"].as_str().unwrap_or("");
                let _ = write!(
                    body,
                    r#"<div class="ticket-embed-field"><strong>{}</strong><p>{}</p></div>"#,
                    escape(name),
                    escape(value)
                );
            }
        }
        for key in ["image_url", "thumbnail_url"] {
            if let Some(url) = embed[key].as_str().filter(|url| transcript_https_url(url)) {
                let _ = write!(
                    body,
                    r#"<a class="ticket-embed-image" href="{}" target="_blank" rel="noopener noreferrer"><img src="{}" alt="" loading="lazy" referrerpolicy="no-referrer"></a>"#,
                    escape(url),
                    escape(url)
                );
            }
        }
        if let Some(footer) = embed["footer"]["text"].as_str() {
            let _ = write!(body, r"<footer>{}</footer>", escape(footer));
        }
        body.push_str("</aside>");
    }
}

fn render_message_components(body: &mut String, message: &Value) {
    let Some(components) = message["components"]
        .as_array()
        .filter(|items| !items.is_empty())
    else {
        return;
    };
    body.push_str(r#"<div class="ticket-message-components">"#);
    for component in components {
        render_message_component(body, component, 0);
    }
    body.push_str("</div>");
}

fn render_message_component(body: &mut String, component: &Value, depth: usize) {
    if depth > 4 {
        return;
    }
    if let Some(children) = component["components"].as_array() {
        for child in children {
            render_message_component(body, child, depth + 1);
        }
        return;
    }
    let label = component["label"].as_str().unwrap_or("");
    if !label.is_empty() {
        if let Some(url) = component["url"]
            .as_str()
            .filter(|url| transcript_https_url(url))
        {
            let _ = write!(
                body,
                r#"<a class="button secondary" href="{}" target="_blank" rel="noopener noreferrer">{}</a>"#,
                escape(url),
                escape(label)
            );
        } else {
            let _ = write!(
                body,
                r#"<button class="button secondary" type="button" disabled>{}</button>"#,
                escape(label)
            );
        }
    }
    if let Some(placeholder) = component["placeholder"].as_str() {
        let _ = write!(body, "<p>{}</p>", escape(placeholder));
    }
    if let Some(options) = component["options"].as_array()
        && !options.is_empty()
    {
        body.push_str("<ul>");
        for option in options {
            let label = option["label"].as_str().unwrap_or("");
            let description = option["description"].as_str();
            let _ = write!(body, "<li>{}", escape(label));
            if let Some(description) = description {
                let _ = write!(body, " — {}", escape(description));
            }
            body.push_str("</li>");
        }
        body.push_str("</ul>");
    }
}

fn valid_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn transcript_page_url(
    id: &str,
    channel_id: &str,
    state: &str,
    start: u32,
    return_to: &str,
) -> String {
    let start = start.to_string();
    let query = serde_urlencoded::to_string([
        ("state", state),
        ("start", start.as_str()),
        ("return_to", return_to),
    ])
    .unwrap_or_default();
    format!("/g/{id}/tickets/{channel_id}?{query}")
}

struct TicketPageContext<'a> {
    id: &'a str,
    channel_id: &'a str,
    ticket: &'a Value,
    directory: Option<&'a Value>,
    state: &'a str,
    back_url: &'a str,
    start: u32,
    action_state: Option<&'a Value>,
    action_etag: Option<&'a str>,
    action_tokens: Option<&'a super::ActionTokens>,
    action_updated: bool,
    ticket_settings: bool,
}

// Keep the route-facing adapter stable; page rendering uses the context below.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ticket(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    channel_id: &str,
    ticket: &Value,
    directory: Option<&Value>,
    state: &str,
    back_url: &str,
    start: u32,
    action_state: Option<&Value>,
    action_etag: Option<&str>,
    action_tokens: Option<&super::ActionTokens>,
    action_updated: bool,
    ticket_settings: bool,
) -> String {
    let context = TicketPageContext {
        id,
        channel_id,
        ticket,
        directory,
        state,
        back_url,
        start,
        action_state,
        action_etag,
        action_tokens,
        action_updated,
        ticket_settings,
    };
    let mut body = String::new();
    render_ticket_header(&mut body, l, &context);
    render_ticket_metadata(&mut body, l, &context);
    render_ticket_actions(&mut body, l, &context);
    render_ticket_transcript(&mut body, l, &context);
    shell(l, headers, id, name, keys::TICKETS, &body, true)
}

fn render_ticket_header(body: &mut String, l: Locale<'_>, context: &TicketPageContext<'_>) {
    let id_html = escape(context.id);
    let _ = write!(
        body,
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="{}">{}</a>"#,
        l.number(keys::TICKET_TITLE, ticket_number(context.ticket)),
        escape(context.back_url),
        l.text(keys::TICKET_BACK)
    );
    if context.ticket_settings {
        let _ = write!(
            body,
            r#"<a class="button secondary" href="/g/{id_html}/tickets/settings/blacklist">{}</a>"#,
            l.text(keys::TICKET_SETTINGS_BLACKLIST)
        );
    }
    body.push_str("</div></header>");
    if context.action_updated {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ACTIONS_UPDATED)
        );
    }
    if context.ticket["state"].as_str() != Some("closed")
        && (context.action_state.is_none()
            || context.action_etag.is_none()
            || context.action_tokens.is_none())
    {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ACTIONS_UNAVAILABLE)
        );
    }
}

fn render_ticket_metadata(body: &mut String, l: Locale<'_>, context: &TicketPageContext<'_>) {
    body.push_str(r#"<section class="card panel"><div class="table-scroll"><table><tbody>"#);
    row(
        body,
        &l.text(keys::TICKET_STATE),
        &state_label(l, context.ticket["state"].as_str().unwrap_or("")),
    );
    let channel = channel_name(context.directory, context.channel_id)
        .map_or_else(|| id_text(context.channel_id), escape);
    row(body, &l.text(keys::TICKET_CHANNEL), &channel);
    row(
        body,
        &l.text(keys::TICKET_OPENER),
        &id_text(context.ticket["opener"].as_str().unwrap_or("")),
    );
    if let Some(claimed_by) = context.ticket["claimed_by"].as_str() {
        row(body, &l.text(keys::TICKET_CLAIMED_BY), &id_text(claimed_by));
    }
    if let Some(panel) = context.ticket["panel"].as_str() {
        row(body, &l.text(keys::TICKET_PANEL), &escape(panel));
    }
    if let Some(created) = context.ticket["created"].as_i64() {
        row(
            body,
            &l.text(keys::TICKET_CREATED),
            &l.relative_time(created, session::now()),
        );
    }
    if let Some(closed) = context.ticket["closed"].as_i64() {
        row(
            body,
            &l.text(keys::TICKET_CLOSED_AT),
            &l.relative_time(closed, session::now()),
        );
    }
    if let Some(closer) = context.ticket["closer"].as_str() {
        row(body, &l.text(keys::TICKET_CLOSER), &id_text(closer));
    }
    if let Some(reason) = context.ticket["reason"].as_str() {
        row(body, &l.text(keys::TICKET_REASON), &escape(reason));
    }
    body.push_str("</tbody></table></div></section>");
}

fn render_ticket_actions(body: &mut String, l: Locale<'_>, context: &TicketPageContext<'_>) {
    if let (Some(action_state), Some(etag), Some(tokens)) = (
        context.action_state,
        context.action_etag,
        context.action_tokens,
    ) {
        body.push_str(&super::ticket_actions::actions(
            l,
            context.id,
            context.channel_id,
            action_state,
            etag,
            tokens,
        ));
    }
}

fn render_ticket_transcript(body: &mut String, l: Locale<'_>, context: &TicketPageContext<'_>) {
    let Some(transcript) = context.ticket["transcript"].as_object() else {
        return;
    };
    if transcript.get("truncated") == Some(&Value::Bool(true)) {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPT_TRUNCATED)
        );
    }
    let _ = write!(
        body,
        r#"<section class="card panel"><div class="panel-heading"><h2>{}</h2></div>"#,
        l.text(keys::TICKET_TRANSCRIPT)
    );
    if let Some(pages) = transcript.get("pages").and_then(Value::as_array) {
        if pages.is_empty() {
            transcript_plain_segment(
                body,
                transcript.get("text").and_then(Value::as_str).unwrap_or(""),
            );
        } else {
            let mut seen_messages = HashSet::new();
            for page in pages {
                render_transcript_page(body, page, &mut seen_messages);
            }
        }
    } else {
        transcript_plain_segment(
            body,
            transcript.get("text").and_then(Value::as_str).unwrap_or(""),
        );
    }
    body.push_str(r#"<div class="head-actions">"#);
    if context.start > 0 {
        let previous = context.start.saturating_sub(4);
        let previous_url = transcript_page_url(
            context.id,
            context.channel_id,
            context.state,
            previous,
            context.back_url,
        );
        let _ = write!(
            body,
            r#"<a class="button secondary" href="{}">{}</a>"#,
            escape(&previous_url),
            l.text(keys::TICKET_TRANSCRIPT_PREVIOUS)
        );
    }
    if let Some(next) = transcript
        .get("next_page")
        .and_then(Value::as_u64)
        .and_then(|next| u32::try_from(next).ok())
        .filter(|next| *next > context.start)
    {
        let next_url = transcript_page_url(
            context.id,
            context.channel_id,
            context.state,
            next,
            context.back_url,
        );
        let _ = write!(
            body,
            r#"<a class="button secondary" href="{}">{}</a>"#,
            escape(&next_url),
            l.text(keys::TICKET_TRANSCRIPT_NEXT)
        );
    }
    body.push_str("</div></section>");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::render_transcript_message;

    #[test]
    fn embed_images_do_not_send_the_referrer() {
        let message = json!({"embeds": [{"image_url": "https://images.example.test/a.png"}]});
        let mut body = String::new();
        render_transcript_message(&mut body, &message, "", false);
        assert!(
            body.contains(
                r#"<img src="https://images.example.test/a.png" alt="" loading="lazy" referrerpolicy="no-referrer">"#
            ),
            "embed images are fetched without the Referer header: {body}"
        );
    }
}
