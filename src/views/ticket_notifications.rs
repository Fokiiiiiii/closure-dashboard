use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

const GROUPS: [(i64, &str); 4] = [
    (0, keys::NOTIFICATIONS_TODAY),
    (1, keys::NOTIFICATIONS_YESTERDAY),
    (2, keys::NOTIFICATIONS_THIS_WEEK),
    (3, keys::NOTIFICATIONS_EARLIER),
];

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn group_for(timestamp: i64, now: i64) -> usize {
    let today = now.div_euclid(86_400);
    let day = timestamp.div_euclid(86_400);
    let age = today.saturating_sub(day);
    match age {
        i64::MIN..=0 => 0,
        1 => 1,
        2..=6 => 2,
        _ => 3,
    }
}

fn timestamp_text(item: &Value) -> String {
    let value = item["created_at_iso"].as_str().unwrap_or("");
    if value.is_empty() {
        return item["created_at"].as_i64().unwrap_or_default().to_string();
    }
    value.strip_suffix('Z').unwrap_or(value).replace('T', " ") + " UTC"
}

fn safe_href(value: &str) -> bool {
    let path = value.split(['?', '#']).next().unwrap_or("");
    value.starts_with("/g/")
        && !value.starts_with("//")
        && !value.contains("://")
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && !path.split('/').any(|segment| matches!(segment, "." | ".."))
}

pub(crate) fn ticket_notifications_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    state: &Value,
    etag: &str,
    cursor: Option<&str>,
    mark_all_key: &str,
    read_keys: &HashMap<String, String>,
) -> String {
    let mut content = format!(
        r#"<main class="center"><div class="center-column"><header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/">{}</a><a class="button secondary" href="/settings/notifications">{}</a></div></header>"#,
        l.text(keys::NOTIFICATIONS_TITLE),
        l.text(keys::SERVERS),
        l.text(keys::NOTIFICATION_SETTINGS_TITLE)
    );
    let unread_count = state["unread_count"].as_i64().unwrap_or(0).max(0);
    let _ = write!(
        content,
        r#"<p class="form-status" role="status">{}: {unread_count}</p>"#,
        l.text(keys::NOTIFICATIONS_UNREAD)
    );
    if unread_count > 0 {
        let _ = write!(
            content,
            r#"<form method="post" action="/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><button class="button secondary" type="submit" name="action" value="mark_all_read">{}</button></form>"#,
            escape(etag),
            escape(mark_all_key),
            l.text(keys::NOTIFICATIONS_MARK_ALL_READ)
        );
    }

    let items = state["notifications"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if items.is_empty() {
        let _ = write!(
            content,
            r#"<section class="card empty"><p>{}</p></section>"#,
            l.text(keys::NOTIFICATIONS_EMPTY)
        );
    } else {
        let now = now_unix();
        for (group, key) in GROUPS {
            let group_items = items
                .iter()
                .filter(|item| {
                    group_for(item["created_at"].as_i64().unwrap_or_default(), now)
                        == group as usize
                })
                .collect::<Vec<_>>();
            if group_items.is_empty() {
                continue;
            }
            let _ = write!(
                content,
                r#"<section class="notification-group"><h2>{}</h2>"#,
                l.text(key)
            );
            for item in group_items {
                let id = item["id"].as_i64().unwrap_or_default();
                if id <= 0 {
                    continue;
                }
                let id_text = id.to_string();
                let title = escape(
                    item["submission_title"]
                        .as_str()
                        .or_else(|| item["title"].as_str())
                        .unwrap_or("")
                        .trim(),
                );
                let raw_body = item["reason"]
                    .as_str()
                    .or_else(|| item["body"].as_str())
                    .unwrap_or("")
                    .trim();
                let event_type = item["event_type"].as_str().unwrap_or("");
                let status = match event_type {
                    "gallery.approved" => l.text(keys::NOTIFICATION_GALLERY_APPROVED),
                    "gallery.rejected" => l.text(keys::NOTIFICATION_GALLERY_REJECTED),
                    _ => String::new(),
                };
                let status_html = if status.is_empty() {
                    String::new()
                } else {
                    format!(r#"<span class="badge">{}</span>"#, escape(&status))
                };
                let body = if !raw_body.is_empty()
                    && matches!(event_type, "gallery.approved" | "gallery.rejected")
                {
                    format!(
                        "{}: {}",
                        l.text(keys::NOTIFICATION_GALLERY_REASON),
                        escape(raw_body)
                    )
                } else {
                    escape(raw_body)
                };
                let category = match item["category"].as_str().unwrap_or("") {
                    "gallery" => l.text(keys::NOTIFICATIONS_CATEGORY_GALLERY),
                    "integrations" => l.text(keys::NOTIFICATIONS_CATEGORY_INTEGRATIONS),
                    other => escape(other),
                };
                let time = escape(&timestamp_text(item));
                let is_unread = item["read_at"].is_null();
                if is_unread {
                    if let Some(key) = read_keys.get(&id_text) {
                        let _ = write!(
                            content,
                            r#"<article class="card notification-card unread"><form method="post" action="/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="notification_id" value="{id_text}"><input type="hidden" name="cursor" value="{}"><button class="notification-open" type="submit" name="action" value="mark_read_open"><span class="badge">{category}</span>{status_html}<span class="notification-title">{title}</span><span class="notification-body">{body}</span><time datetime="{}">{time}</time></button><button class="button ghost" type="submit" name="action" value="mark_read">{}</button></form></article>"#,
                            escape(etag),
                            escape(key),
                            escape(cursor.unwrap_or("")),
                            escape(item["created_at_iso"].as_str().unwrap_or("")),
                            l.text(keys::NOTIFICATIONS_MARK_READ)
                        );
                    }
                } else {
                    let href = item["href"]
                        .as_str()
                        .filter(|value| safe_href(value))
                        .unwrap_or("/notifications");
                    let _ = write!(
                        content,
                        r#"<article class="card notification-card"><a class="notification-open" href="{}"><span class="badge">{category}</span>{status_html}<span class="notification-title">{title}</span><span class="notification-body">{body}</span><time datetime="{}">{time}</time></a></article>"#,
                        escape(href),
                        escape(item["created_at_iso"].as_str().unwrap_or(""))
                    );
                }
            }
            content.push_str("</section>");
        }
    }
    if let Some(next) = state["next_cursor"].as_str().filter(|cursor| {
        !cursor.is_empty() && cursor.len() <= 19 && cursor.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        let _ = write!(
            content,
            r#"<nav class="pagination"><a class="button secondary" href="/notifications?cursor={}">{}</a></nav>"#,
            escape(next),
            l.text(keys::NOTIFICATIONS_OLDER)
        );
    }
    if cursor.is_some() {
        let _ = write!(
            content,
            r#"<a class="button ghost" href="/notifications">{}</a>"#,
            l.text(keys::NOTIFICATIONS_NEWEST)
        );
    }
    content.push_str("</div></main>");
    crate::views::page(l, headers, &content)
}

pub(crate) fn ticket_notification_settings_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    state: &Value,
    etag: &str,
    save_key: &str,
    request_email_key: &str,
    verify_email_key: &str,
    remove_email_key: &str,
) -> String {
    let preferences = &state["preferences"];
    let email = &state["email"];
    let email_available = email["available"] == Value::Bool(true);
    let email_verified = email["verified"] == Value::Bool(true);
    let address = email["address_masked"].as_str().unwrap_or("");
    let pending_address = email["pending_address_masked"].as_str().unwrap_or("");
    let pending = email["pending"] == Value::Bool(true);
    let mut content = format!(
        r#"<main class="center"><div class="center-column"><header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/notifications">{}</a><a class="button secondary" href="/">{}</a></div></header>"#,
        l.text(keys::NOTIFICATION_SETTINGS_TITLE),
        l.text(keys::NOTIFICATIONS_TITLE),
        l.text(keys::SERVERS)
    );

    let _ = write!(
        content,
        r#"<section class="card panel"><form method="post" action="/settings/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="save_preferences"><h2>{}</h2><label class="field checkbox"><input type="checkbox" name="in_app" value="true"{}><span>{}</span></label><label class="field checkbox"><input type="checkbox" name="discord_dm" value="true"{}><span>{}</span></label><small class="settings-help">{}</small>"#,
        escape(etag),
        escape(save_key),
        l.text(keys::NOTIFICATIONS_TITLE),
        checked(preferences["in_app"] == Value::Bool(true)),
        l.text(keys::NOTIFICATION_IN_APP),
        checked(preferences["discord_dm"] == Value::Bool(true)),
        l.text(keys::NOTIFICATION_DISCORD_DM),
        l.text(keys::NOTIFICATION_DISCORD_DM_HELP)
    );

    if !email_available {
        let _ = write!(
            content,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::NOTIFICATION_EMAIL_NOT_CONFIGURED)
        );
    } else if !email_verified {
        let _ = write!(
            content,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::NOTIFICATION_EMAIL_NEEDS_VERIFIED)
        );
    }

    let email_checked =
        email_available && email_verified && preferences["email"] == Value::Bool(true);
    let disabled = if email_available && email_verified {
        ""
    } else {
        " disabled"
    };
    let _ = write!(
        content,
        r#"<label class="field checkbox"><input type="checkbox" name="email" value="true"{}{}><span>{}</span></label><small class="settings-help">{}</small><button class="button primary" type="submit">{}</button></form></section>"#,
        checked(email_checked),
        disabled,
        l.text(keys::NOTIFICATION_EMAIL),
        l.text(keys::NOTIFICATION_EMAIL_HELP),
        l.text(keys::NOTIFICATION_SAVE)
    );

    if email_available {
        content.push_str(r#"<section class="card panel"><h2>"#);
        content.push_str(&l.text(keys::NOTIFICATION_EMAIL));
        content.push_str("</h2>");
        if email_verified && !address.is_empty() {
            let _ = write!(
                content,
                r#"<p class="form-status" role="status">{}: {}</p>"#,
                l.text(keys::NOTIFICATION_EMAIL_VERIFIED),
                escape(address)
            );
        } else if pending && !pending_address.is_empty() {
            let _ = write!(
                content,
                r#"<p class="form-status" role="status">{}: {}</p>"#,
                l.text(keys::NOTIFICATION_EMAIL_PENDING),
                escape(pending_address)
            );
        }
        let _ = write!(
            content,
            r#"<form method="post" action="/settings/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="request_email_verification"><label class="field"><span>{}</span><input type="email" name="email" maxlength="254" autocomplete="email" required></label><button class="button secondary" type="submit">{}</button></form>"#,
            escape(etag),
            escape(request_email_key),
            l.text(keys::NOTIFICATION_EMAIL_ADDRESS),
            l.text(keys::NOTIFICATION_EMAIL_SEND_CODE)
        );
        if pending {
            let _ = write!(
                content,
                r#"<form method="post" action="/settings/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="verify_email"><label class="field"><span>{}</span><input type="text" name="code" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{{6}}" minlength="6" maxlength="6" required></label><button class="button primary" type="submit">{}</button></form><form method="post" action="/settings/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="request_email_verification"><button class="button ghost" type="submit">{}</button></form>"#,
                escape(etag),
                escape(verify_email_key),
                l.text(keys::NOTIFICATION_EMAIL_CODE),
                l.text(keys::NOTIFICATION_EMAIL_VERIFY),
                escape(etag),
                escape(request_email_key),
                l.text(keys::NOTIFICATION_EMAIL_RESEND)
            );
        }
        if email_verified || pending {
            let _ = write!(
                content,
                r#"<form method="post" action="/settings/notifications/actions"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="remove_email"><label class="field checkbox"><input type="checkbox" name="confirm" value="true" required><span>{}</span></label><button class="button ghost" type="submit">{}</button></form>"#,
                escape(etag),
                escape(remove_email_key),
                l.text(keys::NOTIFICATION_EMAIL_CONFIRM_REMOVE),
                l.text(keys::NOTIFICATION_EMAIL_REMOVE)
            );
        }
        content.push_str("</section>");
    }

    content.push_str("</div></main>");
    crate::views::page(l, headers, &content)
}

fn checked(value: bool) -> &'static str {
    if value { " checked" } else { "" }
}
