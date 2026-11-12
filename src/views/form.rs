use super::{modules::toggle, shell};
use crate::{
    form::{self, Column, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

pub(crate) struct FormPage<'a> {
    pub id: &'a str,
    pub guild: &'a str,
    pub module: &'a str,
    pub part: &'a str,
    pub schema: &'a Value,
    pub stored: &'a Value,
    pub effective: &'a Value,
    pub directory: Option<&'a Value>,
    pub etag: &'a str,
    pub input: &'a HashMap<String, String>,
    pub errors: &'a HashMap<String, Vec<String>>,
    pub saved: bool,
}

fn option(output: &mut String, value: &str, label: &str, selected: bool) {
    let _ = write!(
        output,
        "<option value=\"{}\"{}>{}</option>",
        escape(value),
        if selected { " selected" } else { "" },
        escape(label)
    );
}

const COMMON_TIME_ZONES: &[&str] = &[
    "UTC",
    "Africa/Cairo",
    "Africa/Johannesburg",
    "Africa/Lagos",
    "Africa/Nairobi",
    "America/Anchorage",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/Mexico_City",
    "America/New_York",
    "America/Phoenix",
    "America/Sao_Paulo",
    "America/Toronto",
    "America/Vancouver",
    "Asia/Bangkok",
    "Asia/Dubai",
    "Asia/Hong_Kong",
    "Asia/Jakarta",
    "Asia/Jerusalem",
    "Asia/Karachi",
    "Asia/Kolkata",
    "Asia/Seoul",
    "Asia/Shanghai",
    "Asia/Singapore",
    "Asia/Taipei",
    "Asia/Tokyo",
    "Australia/Adelaide",
    "Australia/Brisbane",
    "Australia/Perth",
    "Australia/Sydney",
    "Europe/Amsterdam",
    "Europe/Berlin",
    "Europe/Helsinki",
    "Europe/Istanbul",
    "Europe/London",
    "Europe/Moscow",
    "Europe/Paris",
    "Europe/Rome",
    "Pacific/Auckland",
    "Pacific/Honolulu",
];

fn parse_time_value(value: &str) -> Option<u32> {
    if let Ok(minutes) = value.parse() {
        return Some(minutes);
    }
    let mut parts = value.split(':');
    let hour = parts.next()?;
    let minute = parts.next()?;
    let seconds = parts.next();
    if parts.next().is_some()
        || seconds.is_some_and(|seconds| seconds != "00")
        || hour.len() != 2
        || minute.len() != 2
        || seconds.is_some_and(|seconds| seconds.len() != 2)
        || !hour.bytes().all(|digit| digit.is_ascii_digit())
        || !minute.bytes().all(|digit| digit.is_ascii_digit())
    {
        return None;
    }
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(1440);
    }
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

fn time_minute_control(
    l: Locale<'_>,
    field: &Field,
    id: &str,
    value: &str,
    invalid: bool,
    labelled_by: Option<&str>,
) -> String {
    let raw_name = format!("f.{}", field.name);
    let name = escape(&raw_name);
    let max = field.schema["maximum"]
        .as_i64()
        .or_else(|| field.schema["x-ui"]["max"].as_i64())
        .unwrap_or(1439);
    let minutes = parse_time_value(value);
    let end_of_day = max == 1440 && minutes == Some(1440);
    let minute_input = minutes
        .filter(|minutes| *minutes < 1440 && i64::from(*minutes) <= max.max(0))
        .map_or_else(String::new, |minutes| {
            format!("{:02}:{:02}", minutes / 60, minutes % 60)
        });
    let invalid_time = !value.is_empty() && !end_of_day && minute_input.is_empty();
    let time_type = if invalid_time { "text" } else { "time" };
    let shown_value: &str = if invalid_time { value } else { &minute_input };
    let described = if invalid {
        format!(" aria-describedby=\"{id}-error\"")
    } else {
        String::new()
    };
    let aria_invalid = if invalid || invalid_time {
        " aria-invalid=\"true\""
    } else {
        ""
    };
    let labelled_by = labelled_by.map_or_else(String::new, |caption| {
        format!(" aria-labelledby=\"{}\"", escape(caption))
    });
    let time_constraints = if invalid_time {
        ""
    } else {
        " step=\"60\" max=\"23:59\""
    };
    let mut out = format!(
        "<div class=\"time-minute-control\" data-time-minute><input type=\"{time_type}\" id=\"{id}\" name=\"{name}\"{described}{aria_invalid}{labelled_by}{time_constraints} data-minute-input value=\"{}\">",
        escape(shown_value)
    );
    if max == 1440 {
        let end_of_day_name = escape(&format!("{raw_name}.end-of-day"));
        let checkbox_id = format!("{id}-end-of-day");
        let _ = write!(
            out,
            "<span class=\"time-minute-end\"><input type=\"checkbox\" id=\"{checkbox_id}\" name=\"{end_of_day_name}\" value=\"1\" data-end-of-day{}><label for=\"{checkbox_id}\">{}</label></span>",
            if end_of_day { " checked" } else { "" },
            l.text("hours-end-of-day")
        );
    }
    out.push_str("</div>");
    out
}

fn timezone_control(l: Locale<'_>, attrs: &str, id: &str, field: &Field, value: &str) -> String {
    let list_id = format!("{id}-time-zones");
    let placeholder_key = if field.name.ends_with(".timezone") {
        "hours-timezone-inherit"
    } else {
        "hours-timezone-placeholder"
    };
    let placeholder = l.text(placeholder_key);
    let pattern = field.schema["pattern"]
        .as_str()
        .map_or_else(String::new, |pattern| {
            format!(" pattern=\"{}\"", escape(pattern))
        });
    let maxlength = field.schema["maxLength"]
        .as_u64()
        .map_or_else(String::new, |max| format!(" maxlength=\"{max}\""));
    let mut out = format!(
        "<input type=\"text\" {attrs} list=\"{list_id}\" placeholder=\"{}\"{pattern}{maxlength} value=\"{}\"><datalist id=\"{list_id}\" data-time-zone-list>",
        escape(&placeholder),
        escape(value)
    );
    let mut seen = HashSet::new();
    for zone in COMMON_TIME_ZONES {
        if seen.insert(*zone) {
            option(&mut out, zone, zone, false);
        }
    }
    if !value.trim().is_empty() && seen.insert(value) {
        option(&mut out, value, value, false);
    }
    out.push_str("</datalist>");
    out
}

fn ticket_panel_control(
    l: Locale<'_>,
    field: &Field,
    id: &str,
    value: &str,
    panels: Option<&Value>,
    invalid: bool,
) -> String {
    let name = escape(&format!("f.{}", field.name));
    let described = if invalid {
        format!(" aria-describedby=\"{id}-error\" aria-invalid=\"true\"")
    } else {
        String::new()
    };
    let mut out = format!("<select id=\"{id}\" name=\"{name}\"{described}>");
    option(
        &mut out,
        "",
        &l.text("hours-panel-default"),
        value.is_empty(),
    );
    let mut found = value.is_empty();
    let mut seen = HashSet::new();
    for panel in panels.and_then(Value::as_array).into_iter().flatten() {
        let Some(panel_id) = panel["id"].as_str().filter(|id| !id.is_empty()) else {
            continue;
        };
        if !seen.insert(panel_id) {
            continue;
        }
        let selected = panel_id == value;
        found |= selected;
        let label = panel["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .or_else(|| {
                panel["label"]
                    .as_str()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or(panel_id);
        option(&mut out, panel_id, label, selected);
    }
    if !found {
        let label = l.with_text("hours-panel-unknown", "id", value);
        option(&mut out, value, &label, true);
    }
    out.push_str("</select>");
    out
}

/// The heading of one card of a big module's settings.
pub(super) fn section_title(l: Locale<'_>, key: &str) -> String {
    l.text(match key {
        "warn" => keys::SEC_WARN,
        "timeout" => keys::SEC_TIMEOUT,
        "voice" => keys::SEC_VOICE,
        "ban" => keys::SEC_BAN,
        "auto_delete" => keys::SEC_AUTO_DELETE,
        "auto_react" => keys::SEC_AUTO_REACT,
        "escalation" => keys::SEC_ESCALATION,
        "templates" => keys::SEC_TEMPLATES,
        "rules" => keys::SEC_RULES,
        "counters" => keys::SEC_COUNTERS,
        "antiraid" => keys::SEC_ANTIRAID,
        "messages" => keys::SEC_MESSAGES,
        "texts" => keys::SEC_TEXTS,
        "roles" => keys::SEC_ROLES,
        "behavior" => keys::SEC_BEHAVIOR,
        "destination" => keys::SEC_DESTINATION,
        "events" => keys::SEC_EVENTS,
        _ => keys::CAT_OTHER,
    })
}

/// The shown text of one choice: translated when it is a value the dashboard knows, else as sent.
fn enum_label(l: Locale<'_>, field: &Field, value: &str) -> String {
    if let Some(label) = field.schema["x-enum-labels"][value].as_str() {
        return l.find(label).unwrap_or_else(|| escape(label));
    }
    if field.name.rsplit('.').next() == Some("channel_name_scheme") {
        let key = match value {
            "id" => "opt-id",
            "username" => "opt-username",
            "id_username" => "opt-id_username",
            "nickname" => "opt-nickname",
            "id_padded" => "opt-id_padded",
            "custom" => "opt-custom",
            _ => return escape(value),
        };
        return l.text(key);
    }
    if field.name.contains(".access_rules.") {
        let key = match value {
            "allow" => "opt-allow",
            "deny" => "opt-deny",
            _ => return escape(value),
        };
        return l.text(key);
    }
    if field.name.starts_with("integrations.") {
        let key = match (field.name.rsplit('.').next(), value) {
            (Some("auth"), "none") => "config-tickets-integration-auth-none",
            (Some("auth"), "bearer") => "config-tickets-integration-auth-bearer",
            (Some("auth"), "api_key") => "config-tickets-integration-auth-api-key",
            (Some("method"), "get") => "config-tickets-integration-method-get",
            (Some("method"), "post") => "config-tickets-integration-method-post",
            _ => return escape(value),
        };
        return l.text(key);
    }
    if field.name.rsplit('.').next() == Some("weekday") {
        let key = match value {
            "monday" => "hours-weekday-monday",
            "tuesday" => "hours-weekday-tuesday",
            "wednesday" => "hours-weekday-wednesday",
            "thursday" => "hours-weekday-thursday",
            "friday" => "hours-weekday-friday",
            "saturday" => "hours-weekday-saturday",
            "sunday" => "hours-weekday-sunday",
            _ => return escape(value),
        };
        return l.text(key);
    }
    let server_language =
        field.name == "language" && field.schema["x-ui"]["server_settings"] == true;
    let key = match value {
        "auto" if server_language => keys::LANG_AUTO,
        "ja" if server_language => keys::LANG_JA,
        "en" if server_language => keys::LANG_EN,
        "timeout" => keys::OPT_TIMEOUT,
        "kick" => keys::OPT_KICK,
        "ban" => keys::OPT_BAN,
        "discord" => keys::OPT_DISCORD,
        "role" => keys::OPT_ROLE,
        "private" => keys::OPT_PRIVATE,
        "public" => keys::OPT_PUBLIC,
        _ => return escape(value),
    };
    l.text(key)
}

#[allow(clippy::too_many_lines)]
pub(super) fn control(
    l: Locale<'_>,
    field: &Field,
    index: impl std::fmt::Display,
    value: &str,
    directory: Option<&Value>,
    invalid: bool,
) -> String {
    let id = format!("field-{index}");
    let name = escape(&format!("f.{}", field.name));
    let described = if invalid {
        format!(" aria-describedby=\"{id}-error\"")
    } else {
        String::new()
    };
    let attrs = format!(
        "id=\"{id}\" name=\"{name}\"{described}{}",
        if invalid {
            " aria-invalid=\"true\""
        } else {
            ""
        }
    );
    let widget = field.schema["x-ui"]["widget"].as_str().unwrap_or("");
    if widget == "ticket-form" {
        let mut result = format!("<select {attrs}>");
        option(
            &mut result,
            "",
            &l.text("tickets-form-inline"),
            value.is_empty(),
        );
        let mut found = value.is_empty();
        let mut seen = HashSet::new();
        for definition in directory
            .and_then(|directory| directory["ticket_forms"].as_array())
            .into_iter()
            .flatten()
        {
            let Some(form_id) = definition["id"]
                .as_str()
                .filter(|form_id| !form_id.is_empty())
            else {
                continue;
            };
            if !seen.insert(form_id) {
                continue;
            }
            let selected = form_id == value;
            found |= selected;
            let title = definition["title"]
                .as_str()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or(form_id);
            option(&mut result, form_id, title, selected);
        }
        if !found {
            let label = l.with_text("tickets-form-unknown", "id", value);
            option(&mut result, value, &label, true);
        }
        result.push_str("</select>");
        return result;
    }
    if matches!(widget, "channel" | "role") {
        let list = directory.and_then(|directory| {
            if widget == "channel" {
                directory["channels"].as_array()
            } else {
                directory["roles"].as_array()
            }
        });
        let Some(list) = list else {
            let unavailable = if widget == "role" {
                l.text(keys::TICKET_ACCESS_ROLE_PICKER_UNAVAILABLE)
            } else {
                l.text(keys::UNAVAILABLE)
            };
            let displayed = if value.is_empty() {
                escape(&l.text(keys::UNSET))
            } else {
                escape(value)
            };
            return format!(
                "<input type=\"hidden\" name=\"{name}\" value=\"{}\"><span id=\"{id}\" class=\"mono\">{displayed}</span><span class=\"settings-help\">{}</span>",
                escape(value),
                escape(&unavailable)
            );
        };
        let mut result = format!("<select {attrs}>");
        option(&mut result, "", &l.text(keys::UNSET), value.is_empty());
        let mut found = value.is_empty();
        let kinds: Vec<&str> = field.schema["x-ui"]["channel_kinds"]
            .as_array()
            .map_or_else(
                || vec!["text"],
                |kinds| kinds.iter().filter_map(Value::as_str).collect(),
            );
        for item in list {
            if (widget == "channel"
                && !item["kind"]
                    .as_str()
                    .is_some_and(|kind| kinds.contains(&kind)))
                || (widget == "role" && item["managed"] == true)
            {
                continue;
            }
            let Some(id) = item["id"].as_str() else {
                continue;
            };
            let selected = id == value;
            found |= selected;
            option(
                &mut result,
                id,
                item["name"].as_str().unwrap_or(id),
                selected,
            );
        }
        if !found {
            option(&mut result, value, value, true);
        }
        result.push_str("</select>");
        return result;
    }
    if widget == "time-minute" {
        return time_minute_control(l, field, &id, value, invalid, None);
    }
    if field.name == "timezone" || field.name.ends_with(".timezone") {
        return timezone_control(l, &attrs, &id, field, value);
    }
    match &field.kind {
        Kind::Boolean => format!(
            "<input type=\"hidden\" name=\"{name}\" value=\"false\"><input type=\"checkbox\" class=\"switch\" {attrs} value=\"true\"{}>",
            if value == "true" { " checked" } else { "" }
        ),
        Kind::Integer => format!(
            "<input type=\"number\" {attrs} min=\"{}\" max=\"{}\" value=\"{}\">",
            field.schema["minimum"],
            field.schema["maximum"],
            escape(value)
        ),
        Kind::Enum(values) if (2..=3).contains(&values.len()) => {
            let mut result = String::from("<div class=\"form-segments\">");
            for (i, v) in values.iter().enumerate() {
                let radio_id = if i == 0 {
                    id.clone()
                } else {
                    format!("{id}-{i}")
                };
                let _ = write!(
                    result,
                    "<input type=\"radio\" id=\"{radio_id}\" name=\"{name}\" value=\"{}\"{described}{}{}><label for=\"{radio_id}\">{}</label>",
                    escape(v),
                    if v == value { " checked" } else { "" },
                    if invalid {
                        " aria-invalid=\"true\""
                    } else {
                        ""
                    },
                    enum_label(l, field, v)
                );
            }
            result.push_str("</div>");
            result
        }
        Kind::Enum(values) => {
            let mut result = format!("<select {attrs}>");
            for v in values {
                // The label comes back already escaped, so it must not go through `option` again.
                let _ = write!(
                    result,
                    "<option value=\"{}\"{}>{}</option>",
                    escape(v),
                    if v == value { " selected" } else { "" },
                    enum_label(l, field, v)
                );
            }
            result.push_str("</select>");
            result
        }
        // A long text, such as a greeting, gets room to be written in; the bot checks its shape on save.
        Kind::String
            if field.schema["maxLength"]
                .as_u64()
                .is_some_and(|max| max > 200) =>
        {
            let max = field.schema["maxLength"]
                .as_u64()
                .map_or_else(String::new, |v| format!(" maxlength=\"{v}\""));
            format!(
                "<textarea {attrs} rows=\"4\"{max}>{}</textarea>",
                escape(value)
            )
        }
        Kind::List(_)
        | Kind::NestedList(_)
        | Kind::Entries
        | Kind::TextList
        | Kind::FormQuestions
        | Kind::Structured
        | Kind::Message(_) => String::new(),
        Kind::String | Kind::Roles | Kind::Words | Kind::FlagOrRoles => {
            let pattern = field.schema["pattern"]
                .as_str()
                .map_or_else(String::new, |v| format!(" pattern=\"{}\"", escape(v)));
            let max = field.schema["maxLength"]
                .as_u64()
                .map_or_else(String::new, |v| format!(" maxlength=\"{v}\""));
            format!(
                "<input type=\"text\" {attrs} value=\"{}\"{pattern}{max}>",
                escape(value)
            )
        }
    }
}

/// The path of one page of a module.
pub(crate) fn part_path(id: &str, module: &str, part: &str) -> String {
    if part.is_empty() {
        format!("/g/{id}/modules/{module}")
    } else {
        format!("/g/{id}/modules/{module}/{part}")
    }
}

/// The tabs of a big module: one link per page of it. A module with one page has none.
pub(crate) fn part_tabs(
    l: Locale<'_>,
    id: &str,
    module: &str,
    current: &str,
    fields: &[Field],
) -> String {
    let parts = form::parts(module, fields);
    if parts.len() < 2 {
        return String::new();
    }
    let mut out = String::from("<nav class=\"tabs\">");
    for part in parts {
        let _ = write!(
            out,
            "<a href=\"{}\"{}>{}</a>",
            escape(&part_path(id, module, part)),
            if part == current {
                " aria-current=\"page\""
            } else {
                ""
            },
            super::modules::part_name(l, module, part)
        );
    }
    out.push_str("</nav>");
    out
}

/// The caption of a field: the dashboard's own when it has one, else the bot's description.
fn field_label(l: Locale<'_>, field: &Field) -> String {
    l.find(&format!("ed-f-{}", field.name.replace('.', "_")))
        .unwrap_or_else(|| escape(&super::first_sentence(&field.label)))
}

/// A message setting in the form: what it says now, and the way to the composer.
fn message_row(l: Locale<'_>, id: &str, module: &str, field: &Field, current: &Value) -> String {
    let excerpt = match current {
        Value::String(text) => text.clone(),
        Value::Object(_) => current["content"]
            .as_str()
            .or_else(|| current["embeds"][0]["title"].as_str())
            .or_else(|| current["embeds"][0]["description"].as_str())
            .unwrap_or("")
            .to_owned(),
        _ => String::new(),
    };
    let rich = current["embeds"].as_array().is_some_and(|e| !e.is_empty());
    let shown: String = excerpt.chars().take(80).collect();
    let summary = if shown.is_empty() && !rich {
        l.text(keys::CP_DEFAULT_MESSAGE)
    } else {
        format!("{}{}", if rich { "▣ " } else { "" }, escape(&shown))
    };
    format!(
        "<div class=\"cp-summary\"><span class=\"muted\">{summary}</span><a class=\"button secondary\" href=\"/g/{}/modules/{}/compose/{}\">{}</a></div>",
        escape(id),
        escape(module),
        escape(&field.name),
        l.text(keys::CP_EDIT_MESSAGE)
    )
}

/// The caption of one column of a list.
fn column_title(l: Locale<'_>, name: &str, panel: bool, schema: &Value) -> String {
    if let Some(title) = schema["title"].as_str() {
        return l.find(title).unwrap_or_else(|| escape(title));
    }
    let key = match name {
        "id" if panel => keys::COL_PANEL_ID,
        "name" if panel => "col-panel-name",
        "description" if panel => "col-panel-description",
        "button_style" if panel => "col-panel-button-style",
        "title" if panel => "tickets-title",
        "color" if panel => "tickets-color",
        "content" if panel => "tickets-content",
        "welcome_title" => "tickets-welcome-title",
        "welcome_title_url" => "tickets-welcome-title-url",
        "welcome_description" => "tickets-welcome-description",
        "welcome_color" => "tickets-welcome-color",
        "welcome_author_name" => "tickets-welcome-author-name",
        "welcome_author_url" => "tickets-welcome-author-url",
        "welcome_author_icon_url" => "tickets-welcome-author-icon-url",
        "welcome_thumbnail_url" => "tickets-welcome-thumbnail-url",
        "welcome_image_url" => "tickets-welcome-image-url",
        "welcome_footer_text" => "tickets-welcome-footer-text",
        "welcome_footer_icon_url" => "tickets-welcome-footer-icon-url",
        "welcome_timestamp" => "tickets-welcome-timestamp",
        "welcome_fields" if panel => "tickets-welcome-fields",
        "rating_enabled" => "tickets-rating-enabled",
        "close_reasons" => "tickets-close-reasons",
        "allow_custom_close_reasons" => "tickets-allow-custom-close-reasons",
        "channel_name_scheme" => "tickets-channel-name-scheme",
        "channel_name_custom" => "tickets-channel-name-custom",
        "open_cooldown_seconds" => "tickets-open-cooldown-seconds",
        "access_rules" if panel => "tickets-access-rules",
        "role_id" | "role" => keys::COL_ROLE,
        "name" => keys::COL_NAME,
        "value" => keys::COL_TEXT,
        "inline" => keys::CP_INLINE,
        "headers" => "config-tickets-integration-headers",
        "secrets" => "config-tickets-integration-secrets",
        "response_mappings" => "config-tickets-integration-response-mappings",
        "value_template" => "config-tickets-integration-header-value-template",
        "path" => "config-tickets-integration-mapping-path",
        "channel" => keys::COL_CHANNEL,
        "level" => keys::COL_LEVEL,
        "percent" => keys::COL_PERCENT,
        "after" => keys::COL_AFTER,
        "skip_pinned" => keys::COL_SKIP_PINNED,
        "emojis" => keys::COL_EMOJIS,
        "skip_bots" => keys::COL_SKIP_BOTS,
        "warns" => keys::COL_WARNS,
        "action" => keys::COL_ACTION,
        "duration" => keys::COL_DURATION,
        "id" => keys::COL_ID,
        "label" => keys::COL_LABEL,
        "reason" => keys::COL_REASON,
        "category" => keys::COL_CATEGORY,
        "panel" => "hours-panel",
        "weekday" => "hours-weekday",
        "start_minute" => "hours-start",
        "end_minute" => "hours-end",
        "timezone" => "hours-timezone-override",
        "staff_roles" => keys::COL_STAFF_ROLES,
        "log_channel" => keys::COL_LOG_CHANNEL,
        "close_after_days" => keys::COL_CLOSE_AFTER_DAYS,
        "waiting_after_hours" => "tickets-waiting-after-hours",
        "first_response_timeout_hours" => "tickets-first-response-timeout-hours",
        "last_message_timeout_hours" => "tickets-last-message-timeout-hours",
        "waiting_category" => "tickets-waiting-category",
        "staff_teams" => "tickets-staff-teams",
        "exit_survey" => "tickets-exit-survey",
        "form_id" if panel => "tickets-form-id",
        "questions" => keys::COL_QUESTIONS,
        _ => return escape(name),
    };
    l.text(key)
}

fn role_value_text(value: Option<&Value>) -> String {
    if value.and_then(Value::as_array).is_some_and(Vec::is_empty) {
        "-".into()
    } else {
        cell_text(value)
    }
}

fn role_ids(value: &str) -> Vec<&str> {
    if value.trim() == "-" {
        return Vec::new();
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect()
}

fn role_ids_are_unique(value: &str) -> bool {
    let mut seen = HashSet::new();
    role_ids(value).into_iter().all(|id| seen.insert(id))
}

struct RoleControl<'a> {
    field: &'a Field,
    control_index: &'a str,
    mode_name: &'a str,
    mode: &'a str,
    required: bool,
    value: &'a str,
    directory: Option<&'a Value>,
    picker: bool,
}

fn roles_controls(l: Locale<'_>, role: &RoleControl<'_>) -> String {
    let id = format!("field-{}", role.control_index);
    let default_id = format!("{id}-default");
    let set_id = format!("{id}-set");
    let name = escape(&format!("f.{}", role.field.name));
    let mode_name = escape(role.mode_name);
    let mut out = String::new();
    let _ = write!(
        out,
        "<div id=\"{id}-modes\" class=\"role-mode\" role=\"radiogroup\" aria-label=\"{}\">",
        l.text(keys::COL_STAFF_ROLES_MODE)
    );
    if !role.required {
        let _ = write!(
            out,
            "<label><input type=\"radio\" id=\"{default_id}\" name=\"{mode_name}\" value=\"default\"{}>{}</label>",
            if role.mode == "default" {
                " checked"
            } else {
                ""
            },
            l.text(keys::COL_STAFF_ROLES_DEFAULT)
        );
    }
    let _ = write!(
        out,
        "<label><input type=\"radio\" id=\"{set_id}\" name=\"{mode_name}\" value=\"set\"{}>{}</label></div>",
        if role.mode == "set" { " checked" } else { "" },
        l.text(keys::COL_STAFF_ROLES_SET)
    );
    if role.picker
        && let Some(roles) = role
            .directory
            .and_then(|directory| directory["roles"].as_array())
    {
        let options_id = format!("{id}-options");
        let values = role_ids(role.value);
        let _ = write!(
            out,
            "<span id=\"{options_id}\" class=\"sr-only\">{}</span><div class=\"role-list\" role=\"group\" aria-labelledby=\"{options_id}\">",
            l.text(keys::COL_STAFF_ROLES_SELECT)
        );
        let mut seen = HashSet::new();
        for (index, role_id) in values.iter().enumerate() {
            let item = roles.iter().find(|role| {
                role["id"].as_str() == Some(*role_id)
                    && role["managed"] != true
                    && role["name"] != "@everyone"
            });
            let label = item
                .and_then(|role| role["name"].as_str())
                .map_or_else(|| l.with_text(keys::UNKNOWN_ROLE, "id", role_id), escape);
            seen.insert((*role_id).to_owned());
            let _ = write!(
                out,
                "<label class=\"role-option\"><input type=\"checkbox\" id=\"{id}-role-{index}\" name=\"{name}\" value=\"{}\" checked>{label}</label>",
                escape(role_id)
            );
        }
        let mut available = HashSet::new();
        for role in roles {
            let Some(role_id) = role["id"].as_str() else {
                continue;
            };
            if role["managed"] == true
                || role["name"] == "@everyone"
                || seen.contains(role_id)
                || !available.insert(role_id)
            {
                continue;
            }
            let label = escape(role["name"].as_str().unwrap_or(role_id));
            let index = available.len() + values.len() - 1;
            let _ = write!(
                out,
                "<label class=\"role-option\"><input type=\"checkbox\" id=\"{id}-role-{index}\" name=\"{name}\" value=\"{}\">{label}</label>",
                escape(role_id)
            );
        }
        out.push_str("</div>");
        return out;
    }
    let _ = write!(
        out,
        "<label for=\"{id}\">{}</label>{}",
        l.text(keys::COL_STAFF_ROLES),
        control(l, role.field, role.control_index, role.value, None, false)
    );
    out
}

fn roles_cell(l: Locale<'_>, role: &RoleControl<'_>) -> String {
    let id = format!("field-{}", role.control_index);
    let caption_id = format!("{id}-caption");
    let mut out = String::new();
    let _ = write!(
        out,
        "<div class=\"list-cell list-roles\" role=\"group\" aria-labelledby=\"{caption_id}\"><span id=\"{caption_id}\" class=\"list-caption\">{}</span>",
        l.text(keys::COL_STAFF_ROLES_PICKER)
    );
    out.push_str(&roles_controls(l, role));
    out.push_str("</div>");
    out
}

fn roles_field_widget(
    l: Locale<'_>,
    role: &RoleControl<'_>,
    labelled_by: &str,
    invalid: bool,
) -> String {
    let described = if invalid {
        format!(" aria-describedby=\"field-{}-error\"", role.control_index)
    } else {
        String::new()
    };
    let mut out = format!(
        "<div class=\"role-list-control\" role=\"group\" aria-labelledby=\"{}\"{described}>",
        escape(labelled_by)
    );
    out.push_str(&roles_controls(l, role));
    out.push_str("</div>");
    out
}

/// One row per named entry, and one empty row to add the next. After a refused save the rows come
/// back as they were typed.
fn entries_editor(
    l: Locale<'_>,
    field: &Field,
    current: &Value,
    input: &HashMap<String, String>,
) -> String {
    let prefix = format!("{}.", field.name);
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    // (name, value, ticked for removal)
    let mut rows: Vec<(String, String, bool)> = if numbers.is_empty() {
        current
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, value)| {
                let text = match value {
                    Value::String(text) => text.clone(),
                    value => value.to_string(),
                };
                (name.clone(), text, false)
            })
            .collect()
    } else {
        numbers
            .iter()
            .map(|n| {
                let get = |part: &str| input.get(&format!("{prefix}{n}.{part}")).cloned();
                (
                    get("k").unwrap_or_default(),
                    get("v").unwrap_or_default(),
                    get("x").is_some_and(|v| v == "1"),
                )
            })
            .collect()
    };
    rows.retain(|(name, value, removed)| *removed || !name.is_empty() || !value.is_empty());
    let existing = rows.len();
    if (rows.len() as u64) < field.schema["maxProperties"].as_u64().unwrap_or(100) {
        rows.push((String::new(), String::new(), false));
    }
    let pattern = field.schema["propertyNames"]["pattern"]
        .as_str()
        .map_or_else(String::new, |p| format!(" pattern=\"{}\"", escape(p)));
    let (key_title, value_title) = entry_titles(l, &field.name);
    let mut out = String::from("<div class=\"list-editor\">");
    for (row, (name, value, removed)) in rows.iter().enumerate() {
        let cell = |part: &str| escape(&format!("f.{prefix}{row}.{part}"));
        let _ = write!(
            out,
            "<div class=\"list-row\"><label class=\"list-cell list-key\"><span class=\"list-caption\">{key_title}</span><input type=\"text\" name=\"{}\" value=\"{}\" maxlength=\"64\"{pattern}></label><label class=\"list-cell list-value\"><span class=\"list-caption\">{value_title}</span><textarea name=\"{}\" rows=\"2\" spellcheck=\"false\">{}</textarea></label>",
            cell("k"),
            escape(name),
            cell("v"),
            escape(value)
        );
        if row < existing {
            let _ = write!(
                out,
                "<label class=\"list-cell list-remove\"><span class=\"list-caption\">{}</span><input type=\"checkbox\" name=\"{}\" value=\"1\"{}></label>",
                l.text(keys::COL_REMOVE),
                cell("x"),
                if *removed { " checked" } else { "" }
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

/// The captions of the name and of the value of a field of named entries.
fn entry_titles(l: Locale<'_>, field: &str) -> (String, String) {
    let name = match field {
        "texts" => keys::COL_MESSAGE_ID,
        "rules" => keys::COL_RULE_NAME,
        "counters" => keys::COL_COUNTER_NAME,
        _ => keys::COL_NAME,
    };
    (l.text(name), l.text(keys::COL_TEXT))
}

/// The shown text of one cell of a stored row.
fn cell_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        // A number is shown: left empty, a saved number would come back as no number at all.
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

struct ListEditorData<'a> {
    field: &'a Field,
    columns: &'a [Column],
    index: usize,
    current: &'a Value,
    stored: Option<&'a Value>,
    input: &'a HashMap<String, String>,
    directory: Option<&'a Value>,
    panel_options: Option<&'a Value>,
}

struct ListRow {
    number: usize,
    values: Vec<String>,
    removed: bool,
}

fn list_rows(data: &ListEditorData<'_>) -> (usize, Vec<ListRow>) {
    let prefix = format!("{}.", data.field.name);
    let cell = |number: usize, column: &Column| {
        data.input.get(&format!("{prefix}{number}.{}", column.name))
    };
    let mut numbers: Vec<usize> = data
        .input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    // Keep submitted row numbers stable when the form is shown again after an error.
    let current_len = data.current.as_array().map_or(0, Vec::len);
    let has_input = !numbers.is_empty();
    let mut rows: Vec<ListRow> = if has_input {
        numbers
            .iter()
            .map(|&number| {
                let values = data
                    .columns
                    .iter()
                    .map(|column| cell(number, column).cloned().unwrap_or_default())
                    .collect();
                let removed = data
                    .input
                    .get(&format!("{prefix}{number}.x"))
                    .is_some_and(|value| value == "1");
                ListRow {
                    number,
                    values,
                    removed,
                }
            })
            .collect()
    } else {
        data.current
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(number, row)| {
                let values = data
                    .columns
                    .iter()
                    .map(|column| {
                        let value = row.get(&column.name);
                        if column.kind == Kind::Roles {
                            role_value_text(value)
                        } else {
                            cell_text(value)
                        }
                    })
                    .collect();
                ListRow {
                    number,
                    values,
                    removed: false,
                }
            })
            .collect()
    };
    let has_new_row = rows.iter().any(|row| row.number == current_len);
    let can_add = data.field.schema["maxItems"]
        .as_u64()
        .is_none_or(|max| (current_len as u64) < max);
    if can_add && !has_new_row {
        let values = data
            .columns
            .iter()
            .map(|column| match &column.kind {
                Kind::Boolean => column.schema["default"]
                    .as_bool()
                    .unwrap_or(false)
                    .to_string(),
                Kind::Enum(values) => values.first().cloned().unwrap_or_default(),
                _ => String::new(),
            })
            .collect();
        rows.push(ListRow {
            number: current_len,
            values,
            removed: false,
        });
    }
    (current_len, rows)
}

fn ticket_teams_control(
    l: Locale<'_>,
    field: &Field,
    control_index: &str,
    title: &str,
    value: &str,
    directory: Option<&Value>,
) -> String {
    let teams = directory
        .and_then(|directory| directory.get("ticket_staff_teams"))
        .and_then(Value::as_array);
    let name = escape(&format!("f.{}", field.name));
    let caption = format!("field-{control_index}-caption");
    let selected: HashSet<&str> = value
        .split(',')
        .map(str::trim)
        .filter(|team| !team.is_empty() && *team != "-")
        .collect();
    let mut found = HashSet::new();
    let mut out = format!(
        "<div class=\"list-cell list-team-picker\"><span id=\"{caption}\" class=\"list-caption\">{title}</span><div class=\"role-list\" role=\"group\" aria-labelledby=\"{caption}\"><input type=\"hidden\" name=\"{name}\" value=\"\">"
    );
    let default_selected = selected.contains("default");
    found.insert("default");
    let _ = write!(
        out,
        "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"default\"{}>{}</label>",
        if default_selected { " checked" } else { "" },
        l.text("ticket-team-default")
    );
    if let Some(teams) = teams {
        for team in teams {
            let Some(id) = team["id"].as_str().filter(|id| !id.is_empty()) else {
                continue;
            };
            if !found.insert(id) {
                continue;
            }
            let checked = selected.contains(id);
            let label = team["name"]
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(id);
            let _ = write!(
                out,
                "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\"{}>{}</label>",
                escape(id),
                if checked { " checked" } else { "" },
                escape(label)
            );
        }
    } else {
        let _ = write!(
            out,
            "<p class=\"settings-help\">{}</p>",
            l.text("ticket-team-picker-unavailable")
        );
    }
    for team in selected.iter().filter(|team| !found.contains(**team)) {
        let label = l.with_text("ticket-team-unknown", "id", team);
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\" checked>{label}</label>",
            escape(team)
        );
    }
    out.push_str("</div></div>");
    out
}

fn nested_role_unavailable(
    l: Locale<'_>,
    cell: &Field,
    control_index: &str,
    title: &str,
    value: &str,
) -> String {
    format!(
        "<div class=\"list-cell\"><span id=\"field-{control_index}-caption\" class=\"list-caption\">{title}</span><input type=\"hidden\" name=\"f.{}\" value=\"{}\"><span class=\"mono\">{}</span><p class=\"settings-help\">{}</p></div>",
        escape(&cell.name),
        escape(value),
        escape(value),
        l.text("ticket-access-role-picker-unavailable")
    )
}

fn list_column_description(l: Locale<'_>, schema: &Value) -> Option<String> {
    let description = schema["description"]
        .as_str()
        .filter(|value| !value.is_empty())?;
    let key_like = description
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let description = if key_like {
        l.text(description)
    } else {
        super::first_sentence(description)
    };
    Some(escape(&description))
}

fn uses_schema_descriptions(field: &str) -> bool {
    field == "panels"
        || field.starts_with("panels.")
        || field == "integrations"
        || field.starts_with("integrations.")
}

fn list_cell_control(
    l: Locale<'_>,
    data: &ListEditorData<'_>,
    cell: &Field,
    control_index: &str,
    column: &Column,
    text: &str,
) -> String {
    let title = match (data.field.name.as_str(), column.name.as_str()) {
        ("integrations", "id") => l.text("config-tickets-integration-id"),
        ("integrations", "url") => l.text("config-tickets-integration-url"),
        ("integrations", "auth") => l.text("config-tickets-integration-auth"),
        ("integrations", "method") => l.text("config-tickets-integration-method"),
        ("integrations", "headers") => l.text("config-tickets-integration-headers"),
        ("integrations", "secrets") => l.text("config-tickets-integration-secrets"),
        ("integrations", "response_mappings") => {
            l.text("config-tickets-integration-response-mappings")
        }
        ("integrations", "send_form_data") => l.text("config-tickets-integration-send-form-data"),
        _ if data.field.name.ends_with(".access_rules") && column.name == "role_id" => {
            l.text(keys::TICKETS_ACCESS_RULE_ROLE)
        }
        _ if data.field.name.ends_with(".access_rules") && column.name == "action" => {
            l.text(keys::TICKETS_ACCESS_RULE_ACTION)
        }
        _ => column_title(l, &column.name, data.field.name == "panels", &column.schema),
    };
    let mut out = String::new();
    if column.schema["x-ui"]["widget"] == "ticket-panel" {
        let id = format!("field-{control_index}");
        let panel_control = ticket_panel_control(l, cell, &id, text, data.panel_options, false);
        let _ = write!(
            out,
            "<label class=\"list-cell\"><span class=\"list-caption\">{title}</span>{panel_control}</label>"
        );
    } else if column.schema["x-ui"]["widget"] == "ticket-staff-teams" {
        let control = ticket_teams_control(l, cell, control_index, &title, text, data.directory);
        out.push_str(&control);
    } else if column.schema["x-ui"]["widget"] == "role"
        && data.field.name.ends_with(".access_rules")
        && data
            .directory
            .and_then(|directory| directory["roles"].as_array())
            .is_none()
    {
        out.push_str(&nested_role_unavailable(
            l,
            cell,
            control_index,
            &title,
            text,
        ));
    } else if column.schema["x-ui"]["widget"] == "time-minute" {
        let id = format!("field-{control_index}");
        let caption_id = format!("{id}-caption");
        let end_of_day = data
            .input
            .get(&format!("{}.end-of-day", cell.name))
            .is_some_and(|value| value == "1");
        let time_value: &str = if end_of_day { "1440" } else { text };
        let time_control = time_minute_control(l, cell, &id, time_value, false, Some(&caption_id));
        let _ = write!(
            out,
            "<div class=\"list-cell\"><span id=\"{caption_id}\" class=\"list-caption\">{title}</span>{time_control}</div>"
        );
    } else {
        let _ = write!(
            out,
            "<label class=\"list-cell\"><span class=\"list-caption\">{title}</span>{}</label>",
            control(l, cell, control_index, text, data.directory, false)
        );
    }
    if uses_schema_descriptions(&data.field.name)
        && let Some(help) = list_column_description(l, &column.schema)
    {
        let _ = write!(out, "<small class=\"settings-help\">{help}</small>");
    }
    out
}

fn nested_list_editor(
    l: Locale<'_>,
    data: &ListEditorData<'_>,
    cell: &Field,
    control_index: &str,
    column: &Column,
    title: &str,
) -> String {
    let Kind::NestedList(columns) = &column.kind else {
        return String::new();
    };
    let prefix = format!("{}.", data.field.name);
    let outer_row = cell
        .name
        .strip_prefix(&prefix)
        .and_then(|rest| rest.split('.').next())
        .and_then(|row| row.parse::<usize>().ok())
        .unwrap_or(0);
    let current = data
        .current
        .as_array()
        .and_then(|rows| rows.get(outer_row))
        .and_then(|row| row.get(&column.name))
        .unwrap_or(&Value::Null);
    let stored = data
        .stored
        .and_then(Value::as_array)
        .and_then(|rows| rows.get(outer_row))
        .and_then(|row| row.get(&column.name))
        .filter(|value| !value.is_null());
    let nested = Field {
        name: cell.name.clone(),
        label: column.name.clone(),
        kind: Kind::List(columns.clone()),
        schema: column.schema.clone(),
    };
    let column_index = control_index
        .rsplit('-')
        .next()
        .and_then(|index| index.parse::<usize>().ok())
        .unwrap_or(0);
    let index = data
        .index
        .saturating_mul(1_000_000)
        .saturating_add(outer_row.saturating_mul(1_000))
        .saturating_add(column_index);
    let editor = list_editor(
        l,
        &ListEditorData {
            field: &nested,
            columns,
            index,
            current,
            stored,
            input: data.input,
            directory: data.directory,
            panel_options: data.panel_options,
        },
    );
    let mut out = format!(
        "<fieldset class=\"list-cell list-nested-list\"><legend>{title}</legend><input type=\"hidden\" name=\"f.{}_present\" value=\"1\">",
        escape(&nested.name)
    );
    if let Some(help) = list_column_description(l, &column.schema) {
        let _ = write!(out, "<p class=\"settings-help\">{help}</p>");
    }
    out.push_str(&editor);
    out.push_str("</fieldset>");
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn config_list_editor(
    l: Locale<'_>,
    field: &Field,
    columns: &[Column],
    index: usize,
    current: &Value,
    stored: Option<&Value>,
    input: &HashMap<String, String>,
    directory: Option<&Value>,
) -> String {
    list_editor(
        l,
        &ListEditorData {
            field,
            columns,
            index,
            current,
            stored,
            input,
            directory,
            panel_options: None,
        },
    )
}

#[allow(clippy::too_many_lines)]
fn list_row(l: Locale<'_>, data: &ListEditorData<'_>, current_len: usize, row: &ListRow) -> String {
    let prefix = format!("{}.", data.field.name);
    let mut out = String::from("<div class=\"list-row\">");
    for (c, (column, text)) in data.columns.iter().zip(&row.values).enumerate() {
        let mut schema = column.schema.clone();
        if matches!(column.name.as_str(), "channel" | "role") {
            schema["x-ui"]["widget"] = json!(column.name);
        }
        let cell = Field {
            name: format!("{prefix}{}.{}", row.number, column.name),
            label: column.name.clone(),
            kind: column.kind.clone(),
            schema,
        };
        let control_index = format!("list-{}-{}-{c}", data.index, row.number);
        if column.kind == Kind::Roles && column.schema["x-ui"]["widget"] == "roles" {
            let current_cell = data
                .current
                .as_array()
                .and_then(|rows| rows.get(row.number))
                .and_then(|current| current.get(&column.name));
            let stored_cell = data
                .stored
                .and_then(Value::as_array)
                .and_then(|rows| rows.get(row.number))
                .and_then(|stored| stored.get(&column.name))
                .filter(|value| !value.is_null());
            let mode_name = format!("role_mode:{}", cell.name);
            let mode_key = format!("role-mode:{}", cell.name);
            let mode = data.input.get(&mode_key).map_or(
                if column.required || stored_cell.is_some() {
                    "set"
                } else {
                    "default"
                },
                String::as_str,
            );
            let text = if mode == "default" {
                role_value_text(current_cell)
            } else {
                data.input
                    .get(&cell.name)
                    .cloned()
                    .unwrap_or_else(|| role_value_text(current_cell))
            };
            let picker =
                form::roles_picker(&column.kind, &column.schema, current_cell, data.directory)
                    && role_ids_are_unique(&text);
            let role = RoleControl {
                field: &cell,
                control_index: &control_index,
                mode_name: &mode_name,
                mode,
                required: column.required,
                value: &text,
                directory: data.directory,
                picker,
            };
            out.push_str(&roles_cell(l, &role));
        } else if column.kind == form::Kind::TextList {
            let current = data
                .current
                .as_array()
                .and_then(|rows| rows.get(row.number))
                .and_then(|current| current.get(&column.name))
                .unwrap_or(&Value::Null);
            let editor = text_list_editor(l, &cell, &control_index, current, data.input, false);
            let _ = write!(
                out,
                "<div class=\"list-cell list-text-list\"><span class=\"list-caption\">{}</span>{editor}</div>",
                column_title(l, &column.name, data.field.name == "panels", &column.schema)
            );
            if uses_schema_descriptions(&data.field.name)
                && let Some(help) = list_column_description(l, &column.schema)
            {
                let _ = write!(out, "<p class=\"settings-help\">{help}</p>");
            }
        } else if column.kind == form::Kind::FormQuestions {
            let current = data
                .current
                .as_array()
                .and_then(|rows| rows.get(row.number))
                .and_then(|current| current.get(&column.name))
                .unwrap_or(&Value::Null);
            let editor =
                form_questions_editor(l, &cell, &control_index, current, data.input, false);
            let _ = write!(
                out,
                r#"<div class="list-cell list-form-questions"><span class="list-caption">{}</span>{editor}</div>"#,
                column_title(l, &column.name, data.field.name == "panels", &column.schema)
            );
            if uses_schema_descriptions(&data.field.name)
                && let Some(help) = list_column_description(l, &column.schema)
            {
                let _ = write!(out, "<p class=\"settings-help\">{help}</p>");
            }
        } else if matches!(&column.kind, Kind::NestedList(_)) {
            out.push_str(&nested_list_editor(
                l,
                data,
                &cell,
                &control_index,
                column,
                &column_title(l, &column.name, data.field.name == "panels", &column.schema),
            ));
        } else {
            out.push_str(&list_cell_control(
                l,
                data,
                &cell,
                &control_index,
                column,
                text,
            ));
        }
    }
    let has_content = data
        .columns
        .iter()
        .zip(&row.values)
        .any(|(column, text)| !form::is_preset(&column.kind) && !text.is_empty());
    if row.number < current_len || row.removed || has_content {
        let _ = write!(
            out,
            "<label class=\"list-cell list-remove\"><span class=\"list-caption\">{}</span><input type=\"checkbox\" name=\"{}\" value=\"1\"{}></label>",
            l.text(keys::COL_REMOVE),
            escape(&format!("f.{prefix}{}.x", row.number)),
            if row.removed { " checked" } else { "" }
        );
    }
    if data.field.schema["x-ui"]["widget"] == "access-rules" {
        let order_key = format!("{prefix}{}.order", row.number);
        let order = data
            .input
            .get(&order_key)
            .cloned()
            .unwrap_or_else(|| (row.number + 1).to_string());
        let maximum = current_len.saturating_add(1).max(1);
        let _ = write!(
            out,
            "<label class=\"list-cell list-rule-order\"><span class=\"list-caption\">{}</span><input type=\"number\" min=\"1\" max=\"{maximum}\" name=\"f.{order_key}\" value=\"{}\"></label>",
            l.text(keys::TICKETS_ACCESS_RULE_ORDER),
            escape(&order)
        );
    }
    out.push_str("</div>");
    out
}

/// One row per entry of a list, and one empty row to add the next entry. After a refused save the
/// rows come back as they were typed.
fn list_editor(l: Locale<'_>, data: &ListEditorData<'_>) -> String {
    let (current_len, rows) = list_rows(data);
    let mut out = String::from("<div class=\"list-editor\">");
    for row in &rows {
        out.push_str(&list_row(l, data, current_len, row));
    }
    out.push_str("</div>");
    out
}

fn text_list_editor(
    l: Locale<'_>,
    field: &Field,
    index: impl std::fmt::Display,
    current: &Value,
    input: &HashMap<String, String>,
    invalid: bool,
) -> String {
    let prefix = format!("{}.", field.name);
    let row_limit = form::text_list_row_limit(field);
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .filter(|number| *number < row_limit)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut rows: Vec<(String, bool)> = if numbers.is_empty() {
        current
            .as_array()
            .into_iter()
            .flatten()
            .map(|value| {
                (
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned),
                    false,
                )
            })
            .collect()
    } else {
        numbers
            .iter()
            .map(|&number| {
                let key = format!("{prefix}{number}");
                (
                    input.get(&key).cloned().unwrap_or_default(),
                    input
                        .get(&format!("{key}.x"))
                        .is_some_and(|value| value == "1"),
                )
            })
            .collect()
    };
    rows.retain(|(value, removed)| *removed || !value.is_empty());
    let existing = rows.len();
    let active = rows.iter().filter(|(_, removed)| !removed).count();
    let blanks = form::text_list_limit(field)
        .saturating_sub(active)
        .min(row_limit.saturating_sub(rows.len()));
    rows.extend((0..blanks).map(|_| (String::new(), false)));

    let field_id = format!("field-{index}");
    let mut out = String::from(r#"<div class="list-editor text-list-editor">"#);
    for (row, (value, removed)) in rows.iter().enumerate() {
        let id = if row == 0 {
            field_id.clone()
        } else {
            format!("{field_id}-{row}")
        };
        let described = if invalid {
            format!(r#" aria-describedby="{field_id}-error" aria-invalid="true""#)
        } else {
            String::new()
        };
        let max = field.schema["items"]["maxLength"]
            .as_u64()
            .map_or_else(String::new, |max| format!(r#" maxlength="{max}""#));
        let item = format!("{} {}", l.text(keys::TEXT_LIST_ITEM), row + 1);
        let _ = write!(
            out,
            r#"<div class="list-row"><label class="list-cell list-question"><span class="list-caption">{item}</span><textarea id="{}" name="{}" rows="3"{max}{described}>"#,
            escape(&id),
            escape(&format!("f.{}.{row}", field.name)),
        );
        out.push('\n');
        out.push_str(&escape(value));
        out.push_str("</textarea></label>");
        if row < existing {
            let _ = write!(
                out,
                r#"<label class="list-cell list-remove"><span class="list-caption">{}</span><input type="checkbox" name="{}" value="1"{}></label>"#,
                l.text(keys::COL_REMOVE),
                escape(&format!("f.{}.{row}.x", field.name)),
                if *removed { " checked" } else { "" }
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

fn form_question_value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn form_question_type_label(l: Locale<'_>, kind: &str) -> String {
    let key = match kind {
        "legacy" => "form-question-type-legacy",
        "text" => "form-question-type-text",
        "string_select" => "form-question-type-string-select",
        "user_select" => "form-question-type-user-select",
        "role_select" => "form-question-type-role-select",
        "mentionable_select" => "form-question-type-mentionable-select",
        "channel_select" => "form-question-type-channel-select",
        "radio_group" => "form-question-type-radio-group",
        "checkbox_group" => "form-question-type-checkbox-group",
        "display" => "form-question-type-display",
        _ => return kind.to_owned(),
    };
    l.text(key)
}

fn form_question_options_editor(
    l: Locale<'_>,
    field: &Field,
    question: usize,
    original: Option<&Value>,
    input: &HashMap<String, String>,
) -> String {
    let prefix = format!("{}.{}.options.", field.name, question);
    let max = field.schema["items"]["oneOf"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|schema| schema["type"] == "object")
        .and_then(|schema| schema["properties"]["options"]["maxItems"].as_u64())
        .and_then(|max| usize::try_from(max).ok())
        .unwrap_or(25);
    let existing = original
        .and_then(|value| value["options"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .filter(|number| *number < max.saturating_add(5))
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.is_empty() {
        numbers.extend(0..existing.len().saturating_add(2).max(2).min(max));
    }
    let mut out = format!(
        "<details><summary>{}</summary>",
        escape(&l.text("form-question-options"))
    );
    for number in numbers.into_iter().take(max) {
        let option_prefix = format!("{prefix}{number}");
        let original_option = existing.get(number);
        let value = |name: &str| {
            input
                .get(&format!("{option_prefix}.{name}"))
                .cloned()
                .unwrap_or_else(|| {
                    form_question_value_text(original_option.and_then(|option| option.get(name)))
                })
        };
        let label = value("label");
        let choice_value = value("value");
        let description = value("description");
        let removed = input
            .get(&format!("{option_prefix}.x"))
            .is_some_and(|value| value == "1");
        let active = !label.is_empty() || !choice_value.is_empty() || !description.is_empty();
        let row_id = format!("field-{}-question-{}-option-{number}", field.name, question);
        out.push_str(r#"<div class="list-row question-option-row">"#);
        for (property, caption, text) in [
            ("label", "form-question-option-label", label.as_str()),
            ("value", "form-question-option-value", choice_value.as_str()),
            (
                "description",
                "form-question-option-description",
                description.as_str(),
            ),
        ] {
            let id = escape(&format!("{row_id}-{property}"));
            let name = escape(&format!("f.{option_prefix}.{property}"));
            let _ = write!(
                out,
                r#"<label class="list-cell"><span class="list-caption">{}</span><input id="{}" name="{}" maxlength="100" value="{}"></label>"#,
                escape(&l.text(caption)),
                id,
                name,
                escape(text)
            );
        }
        if original_option.is_some() || active || removed {
            let name = escape(&format!("f.{option_prefix}.x"));
            let _ = write!(
                out,
                r#"<label class="list-cell list-remove"><span class="list-caption">{}</span><input type="checkbox" name="{}" value="1"{}></label>"#,
                l.text(keys::COL_REMOVE),
                name,
                if removed { " checked" } else { "" }
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</details>");
    out
}

#[allow(clippy::too_many_lines)]
fn form_questions_editor(
    l: Locale<'_>,
    field: &Field,
    index: impl std::fmt::Display,
    current: &Value,
    input: &HashMap<String, String>,
    invalid: bool,
) -> String {
    let prefix = format!("{}.", field.name);
    let max = field.schema["maxItems"].as_u64().unwrap_or(5).min(5) as usize;
    let row_limit = max.saturating_add(5);
    let current_rows = current.as_array().cloned().unwrap_or_default();
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .filter(|number| *number < row_limit)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.is_empty() {
        numbers.extend(0..current_rows.len().min(row_limit));
        if numbers.len() < max {
            numbers.push(numbers.len());
        }
        if numbers.is_empty() {
            numbers.push(0);
        }
    }
    let field_id = format!("field-{index}");
    let described = if invalid {
        format!(r#" aria-describedby="{field_id}-error" aria-invalid="true""#)
    } else {
        String::new()
    };
    let kinds = [
        "legacy",
        "text",
        "string_select",
        "user_select",
        "role_select",
        "mentionable_select",
        "channel_select",
        "radio_group",
        "checkbox_group",
        "display",
    ];
    let mut out = String::from(r#"<div class="list-editor form-question-editor">"#);
    for row in numbers.into_iter().take(row_limit) {
        let row_prefix = format!("{}.{row}", field.name);
        let original = current_rows.get(row);
        let input_value = |property: &str| {
            input
                .get(&format!("{row_prefix}.{property}"))
                .map(String::as_str)
        };
        let row_has_input = input
            .keys()
            .any(|name| name.starts_with(&format!("{row_prefix}.")));
        let original_kind = match original {
            Some(Value::String(_)) => "legacy",
            Some(Value::Object(question)) => question
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("text"),
            _ => "text",
        };
        let kind = input_value("type").unwrap_or(original_kind);
        let label_default = match original {
            Some(Value::String(value)) => value.as_str(),
            Some(Value::Object(question)) => {
                question.get("label").and_then(Value::as_str).unwrap_or("")
            }
            _ => "",
        };
        let label = input_value("label").unwrap_or(label_default);
        let description = input_value("description").unwrap_or_else(|| {
            original
                .and_then(|question| question.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("")
        });
        let content = input_value("content").unwrap_or_else(|| {
            original
                .and_then(|question| question.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
        });
        let required = input_value("required").map_or_else(
            || {
                if row_has_input {
                    false
                } else {
                    original
                        .and_then(|question| question.get("required"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true)
                }
            },
            |value| value == "true",
        );
        let style = input_value("style").unwrap_or_else(|| {
            original
                .and_then(|question| question.get("style"))
                .and_then(Value::as_str)
                .unwrap_or("paragraph")
        });
        let min_length = input_value("min_length").map_or_else(
            || form_question_value_text(original.and_then(|question| question.get("min_length"))),
            str::to_owned,
        );
        let max_length = input_value("max_length").map_or_else(
            || form_question_value_text(original.and_then(|question| question.get("max_length"))),
            str::to_owned,
        );
        let placeholder = input_value("placeholder").unwrap_or_else(|| {
            original
                .and_then(|question| question.get("placeholder"))
                .and_then(Value::as_str)
                .unwrap_or("")
        });
        let min_values = input_value("min_values").map_or_else(
            || form_question_value_text(original.and_then(|question| question.get("min_values"))),
            str::to_owned,
        );
        let max_values = input_value("max_values").map_or_else(
            || form_question_value_text(original.and_then(|question| question.get("max_values"))),
            str::to_owned,
        );
        let removed = input
            .get(&format!("{row_prefix}.x"))
            .is_some_and(|value| value == "1");
        let row_id = format!("{field_id}-question-{row}");
        let question_title = format!("{} {}", l.text(keys::TEXT_LIST_ITEM), row + 1);
        let _ = write!(
            out,
            r#"<div class="list-row form-question-row"><div class="list-cell"><span class="list-caption">{}</span>"#,
            escape(&question_title)
        );
        let type_id = escape(&format!("{row_id}-type"));
        let type_name = escape(&format!("f.{row_prefix}.type"));
        let type_label = escape(&l.text("form-question-type"));
        let _ = write!(
            out,
            r#"<select id="{type_id}" name="{type_name}" aria-label="{type_label}"{described}>"#
        );
        for value in kinds {
            option(
                &mut out,
                value,
                &form_question_type_label(l, value),
                kind == value,
            );
        }
        if !kinds.contains(&kind) {
            option(&mut out, kind, kind, true);
        }
        out.push_str("</select></div>");
        let label_id = escape(&format!("{row_id}-label"));
        let label_name = escape(&format!("f.{row_prefix}.label"));
        let _ = write!(
            out,
            r#"<label class="list-cell list-question"><span class="list-caption">{}</span><textarea id="{label_id}" name="{label_name}" rows="2" maxlength="45"{described}>"#,
            l.text("form-question-label")
        );
        out.push_str(&escape(label));
        out.push_str("</textarea></label>");
        let required_id = escape(&format!("{row_id}-required"));
        let required_name = escape(&format!("f.{row_prefix}.required"));
        let _ = write!(
            out,
            r#"<label class="list-cell"><span class="list-caption">{}</span><input type="checkbox" id="{required_id}" name="{required_name}" value="true"{}{described}></label>"#,
            l.text("form-question-required"),
            if required { " checked" } else { "" }
        );
        let _ = write!(
            out,
            r#"<details class="list-cell form-question-details"><summary>{}</summary><small class="muted">{}</small>"#,
            l.text("form-question-more-settings"),
            l.text("form-question-kind-hint")
        );
        let _ = write!(
            out,
            r#"<label class="list-cell"><span class="list-caption">{}</span><input type="text" name="f.{row_prefix}.description" maxlength="100" value="{}"></label>"#,
            l.text("form-question-description"),
            escape(description)
        );
        let _ = write!(
            out,
            r#"<label class="list-cell list-question"><span class="list-caption">{}</span><textarea name="f.{row_prefix}.content" rows="3" maxlength="4000">{}</textarea></label>"#,
            l.text("form-question-content"),
            escape(content)
        );
        let style_id = escape(&format!("{row_id}-style"));
        let _ = write!(
            out,
            r#"<label class="list-cell"><span class="list-caption">{}</span><select id="{style_id}" name="f.{row_prefix}.style">"#,
            l.text("form-question-style")
        );
        option(
            &mut out,
            "short",
            &l.text("form-question-style-short"),
            style == "short",
        );
        option(
            &mut out,
            "paragraph",
            &l.text("form-question-style-paragraph"),
            style == "paragraph",
        );
        out.push_str("</select></label>");
        for (property, caption, value, min, maximum) in [
            (
                "min_length",
                "form-question-min-length",
                min_length,
                "0",
                "4000",
            ),
            (
                "max_length",
                "form-question-max-length",
                max_length,
                "1",
                "4000",
            ),
            (
                "min_values",
                "form-question-min-values",
                min_values,
                "0",
                "25",
            ),
            (
                "max_values",
                "form-question-max-values",
                max_values,
                "1",
                "25",
            ),
        ] {
            let name = escape(&format!("f.{row_prefix}.{property}"));
            let _ = write!(
                out,
                r#"<label class="list-cell"><span class="list-caption">{}</span><input type="number" name="{name}" min="{min}" max="{maximum}" value="{}"></label>"#,
                l.text(caption),
                escape(&value)
            );
        }
        let placeholder_name = escape(&format!("f.{row_prefix}.placeholder"));
        let _ = write!(
            out,
            r#"<label class="list-cell"><span class="list-caption">{}</span><input type="text" name="{placeholder_name}" maxlength="100" value="{}"></label>"#,
            l.text("form-question-placeholder"),
            escape(placeholder)
        );
        out.push_str(&form_question_options_editor(
            l, field, row, original, input,
        ));
        out.push_str("</details>");
        if original.is_some() || row_has_input || !label.is_empty() || removed {
            let remove_name = escape(&format!("f.{row_prefix}.x"));
            let _ = write!(
                out,
                r#"<label class="list-cell list-remove"><span class="list-caption">{}</span><input type="checkbox" name="{remove_name}" value="1"{}></label>"#,
                l.text(keys::COL_REMOVE),
                if removed { " checked" } else { "" }
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

#[allow(clippy::too_many_lines)]
pub(crate) fn form_page(l: Locale<'_>, headers: &HeaderMap, data: &FormPage<'_>) -> String {
    let FormPage {
        id,
        guild,
        module,
        part,
        schema,
        stored,
        effective,
        directory,
        etag,
        input,
        errors,
        saved,
    } = *data;
    let enabled = effective["modules"][module]["enabled"]
        .as_bool()
        .unwrap_or(false);
    let mut body = format!(
        "<header class=\"page-heading\"><h1 class=\"with-icon\">{}{}</h1>{}</header>",
        super::modules::module_icon(module),
        super::modules::part_name(l, module, part),
        if form::feature(module, part).is_some() {
            super::modules::feature_toggle(
                l,
                id,
                module,
                part,
                form::feature_on(module, part, effective),
            )
        } else {
            toggle(l, id, module, enabled)
        }
    );
    let every = form::fields(schema);
    body.push_str(&part_tabs(l, id, module, part, &every));
    if saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::SAVED)
        );
    }
    if let Some(messages) = errors.get("") {
        for message in messages {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(message)
            );
        }
    }
    let fields: Vec<Field> = every
        .into_iter()
        .filter(|f| form::part_of(module, &f.name) == part)
        .collect();
    let _ = write!(
        body,
        "<form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\">",
        escape(&part_path(id, module, part)),
        escape(etag)
    );
    if fields.is_empty() {
        let _ = write!(
            body,
            "<section class=\"card settings-card\"><p class=\"empty\">{}</p></section>",
            l.text(keys::NO_SETTINGS)
        );
    }
    let mut sections: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let key = form::section(module, &field.name);
        match sections.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(index),
            None => sections.push((key, vec![index])),
        }
    }
    sections.sort_by_key(|(key, _)| form::section_rank(key));
    let headed = sections.len() > 1;
    for (key, members) in &sections {
        body.push_str("<section class=\"card settings-card\">");
        if headed {
            let _ = write!(
                body,
                "<h2 class=\"settings-section\">{}</h2>",
                section_title(l, key)
            );
        }
        for &index in members {
            let field = &fields[index];
            let stored_current = form::lookup(&stored["modules"][module]["config"], &field.name)
                .filter(|v| !v.is_null());
            let current = stored_current
                .or_else(|| form::lookup(&effective["modules"][module]["config"], &field.name))
                .unwrap_or(&Value::Null);
            let fallback = match (&field.kind, current) {
                (_, Value::String(s)) => s.clone(),
                (_, Value::Null) => String::new(),
                // An empty list that was set on purpose is shown as the dash that makes it.
                (Kind::Words | Kind::Roles, Value::Array(a)) if a.is_empty() => "-".into(),
                (_, Value::Array(a)) if a.iter().all(Value::is_string) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                (_, v) => v.to_string(),
            };
            // Where the shown value comes from: a setting of this server, or what it inherits.
            let inherited = form::lookup(&stored["modules"][module]["config"], &field.name)
                .is_none_or(Value::is_null)
                && !current.is_null()
                && !fallback.is_empty();
            let value = input.get(&field.name).unwrap_or(&fallback);
            let field_errors = errors.get(&field.name);
            let when = form::dependency(module, &field.name)
                .map_or_else(String::new, |(name, value)| {
                    format!(" when\" data-when=\"{}={}", escape(name), escape(value))
                });
            let wide = if matches!(
                field.kind,
                Kind::List(_)
                    | Kind::Entries
                    | Kind::TextList
                    | Kind::FormQuestions
                    | Kind::Message(_)
            ) {
                " wide"
            } else {
                ""
            };
            let role_field = field.kind == Kind::Roles && field.schema["x-ui"]["widget"] == "roles";
            let description = escape(field.schema["description"].as_str().unwrap_or(&field.label));
            let heading = if role_field {
                format!(
                    "<span id=\"field-{index}-label\" title=\"{description}\">{}</span>",
                    l.text(keys::COL_STAFF_ROLES_PICKER)
                )
            } else {
                format!(
                    "<label for=\"field-{index}\" title=\"{description}\">{}</label>",
                    field_label(l, field)
                )
            };
            let _ = write!(
                body,
                "<div class=\"settings-row{wide}{when}\"><div>{heading}"
            );
            if inherited {
                let note = match &field.kind {
                    Kind::TextList | Kind::FormQuestions => keys::INHERITED_TEXT_LIST,
                    Kind::Words | Kind::Roles => keys::INHERITED_LIST,
                    _ => keys::INHERITED,
                };
                let _ = write!(body, "<small class=\"muted\">{}</small>", l.text(note));
            }
            if let Some(messages) = field_errors {
                let _ = write!(
                    body,
                    "<div id=\"field-{index}-error\" class=\"form-error\" role=\"alert\">"
                );
                for message in messages {
                    let _ = write!(body, "<div>⚠ {}</div>", escape(message));
                }
                body.push_str("</div>");
            }
            let widget = if role_field {
                let mode_name = format!("role_mode:{}", field.name);
                let mode_key = format!("role-mode:{}", field.name);
                let mode = input.get(&mode_key).map_or(
                    if stored_current.is_some() {
                        "set"
                    } else {
                        "default"
                    },
                    String::as_str,
                );
                let value = if mode == "default" {
                    &fallback
                } else {
                    input.get(&field.name).unwrap_or(&fallback)
                };
                let picker =
                    form::roles_picker(&field.kind, &field.schema, Some(current), directory)
                        && role_ids_are_unique(value);
                let control_index = index.to_string();
                let labelled_by = format!("field-{index}-label");
                let role = RoleControl {
                    field,
                    control_index: &control_index,
                    mode_name: &mode_name,
                    mode,
                    required: false,
                    value,
                    directory,
                    picker,
                };
                roles_field_widget(l, &role, &labelled_by, field_errors.is_some())
            } else {
                match &field.kind {
                    Kind::List(columns) => list_editor(
                        l,
                        &ListEditorData {
                            field,
                            columns,
                            index,
                            current,
                            stored: stored_current,
                            input,
                            directory,
                            panel_options: form::lookup(
                                &effective["modules"][module]["config"],
                                "panels",
                            ),
                        },
                    ),
                    Kind::Entries => entries_editor(l, field, current, input),
                    Kind::TextList => {
                        text_list_editor(l, field, index, current, input, field_errors.is_some())
                    }
                    Kind::FormQuestions => form_questions_editor(
                        l,
                        field,
                        index,
                        current,
                        input,
                        field_errors.is_some(),
                    ),
                    Kind::Message(_) => message_row(l, id, module, field, current),
                    _ => control(l, field, index, value, directory, field_errors.is_some()),
                }
            };
            let _ = write!(
                body,
                "</div><div class=\"settings-control\">{widget}</div></div>"
            );
        }
        body.push_str("</section>");
    }
    let _ = write!(
        body,
        "<div class=\"save-bar\"><a class=\"button ghost\" href=\"/g/{}/modules/{}\">{}</a><button class=\"button primary\" type=\"submit\">{}</button></div></form>",
        escape(id),
        escape(module),
        l.text(keys::DISCARD),
        l.text(keys::SAVE)
    );
    shell(l, headers, id, guild, keys::MODULES, &body, true)
}

#[cfg(test)]
mod tests {
    use super::{ListEditorData, control, list_editor};
    use std::collections::HashMap;

    fn channel_field(x_ui: &serde_json::Value) -> crate::form::Field {
        let module = json!({"properties": {"config": {"properties": {"category": {"type": "string", "x-ui": x_ui.clone()}}}}});
        crate::form::fields(&module).remove(0)
    }

    fn channel_directory() -> serde_json::Value {
        json!({"channels": [
            {"id": "1", "name": "general", "kind": "text"},
            {"id": "2", "name": "Tickets", "kind": "category"}
        ]})
    }

    fn id_list(
        name: &str,
        max_items: Option<usize>,
    ) -> (crate::form::Field, Vec<crate::form::Column>) {
        let mut schema = json!({"type": "array"});
        if let Some(max_items) = max_items {
            schema["maxItems"] = json!(max_items);
        }
        let column = crate::form::Column {
            name: "id".into(),
            kind: crate::form::Kind::String,
            required: true,
            schema: json!({"type": "string"}),
        };
        (
            crate::form::Field {
                name: name.into(),
                label: name.into(),
                kind: crate::form::Kind::List(vec![column.clone()]),
                schema,
            },
            vec![column],
        )
    }

    fn rows_with_ids(count: usize) -> serde_json::Value {
        serde_json::Value::Array(
            (0..count)
                .map(|index| json!({"id": format!("panel-{index}")}))
                .collect(),
        )
    }

    fn role_panel_field() -> (crate::form::Field, Vec<crate::form::Column>) {
        let module = json!({"properties": {"config": {"properties": {"panels": {
            "type": "array",
            "maxItems": 25,
            "items": {"type": "object", "required": ["id"], "properties": {
                "id": {"type": "string"},
                "category": {"type": "string", "maxLength": 4},
                "staff_roles": {
                    "type": "array",
                    "items": {"type": "string"},
                    "x-ui": {"widget": "roles"}
                }
            }}
        }}}}});
        let field = crate::form::fields(&module).remove(0);
        let columns = match &field.kind {
            crate::form::Kind::List(columns) => columns.clone(),
            _ => panic!("not a list"),
        };
        (field, columns)
    }

    fn role_directory() -> serde_json::Value {
        json!({"roles": [
            {"id": "13", "name": "Support", "managed": false},
            {"id": "42", "name": "@everyone", "managed": false},
            {"id": "14", "name": "managed", "managed": true}
        ]})
    }

    fn ticket_hours_schema() -> serde_json::Value {
        json!({"properties": {"config": {"properties": {
            "timezone": {"type": "string"},
            "hours": {
                "type": "array",
                "maxItems": 50,
                "items": {"type": "object", "required": [
                    "panel", "weekday", "start_minute", "end_minute"
                ], "properties": {
                    "panel": {
                        "anyOf": [
                            {"type": "string", "pattern": "^[a-zA-Z0-9_-]{1,32}$"},
                            {"type": "null"}
                        ],
                        "x-ui": {"widget": "ticket-panel"}
                    },
                    "weekday": {
                        "type": "string",
                        "enum": ["monday", "tuesday", "wednesday", "thursday",
                            "friday", "saturday", "sunday"]
                    },
                    "start_minute": {
                        "type": "integer", "minimum": 0, "maximum": 1439,
                        "x-ui": {"widget": "time-minute", "max": 1439}
                    },
                    "end_minute": {
                        "type": "integer", "minimum": 0, "maximum": 1440,
                        "x-ui": {"widget": "time-minute", "max": 1440}
                    }
                }}
            },
            "panels": {
                "type": "array",
                "maxItems": 20,
                "items": {"type": "object", "required": ["id"], "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "staff_roles": {
                        "type": "array",
                        "items": {"type": "string"},
                        "x-ui": {"level": "basic", "widget": "roles"}
                    },
                    "questions": {
                        "type": "array",
                        "maxItems": 5,
                        "items": {"type": "string", "minLength": 1, "maxLength": 45},
                        "x-ui": {"widget": "text-list"}
                    },
                    "timezone": {"type": "string"}
                }}
            }
        }}}})
    }

    fn ticket_hours_field() -> (crate::form::Field, Vec<crate::form::Column>) {
        let field = crate::form::fields(&ticket_hours_schema())
            .into_iter()
            .find(|field| field.name == "hours")
            .expect("hours field");
        let columns = match &field.kind {
            crate::form::Kind::List(columns) => columns.clone(),
            _ => panic!("hours must be a list"),
        };
        (field, columns)
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn ticket_hours_and_panel_questions_roles_render_together() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let locale = catalogs.locale("en");
        let (hours, hours_columns) = ticket_hours_field();
        let fields = crate::form::fields(&ticket_hours_schema());
        let panels = fields.iter().find(|field| field.name == "panels").unwrap();
        let timezone = fields
            .iter()
            .find(|field| field.name == "timezone")
            .unwrap();
        let crate::form::Kind::List(panel_columns) = &panels.kind else {
            panic!("panels is not a list");
        };
        let current = serde_json::json!({
            "hours": [{
                "panel": "billing",
                "weekday": "monday",
                "start_minute": 510,
                "end_minute": 1440
            }],
            "panels": [{
                "id": "billing",
                "name": "Billing",
                "staff_roles": ["13"],
                "questions": ["Need help?"],
                "timezone": ""
            }]
        });
        let hours_values = current["hours"].clone();
        let panel_values = current["panels"].clone();
        let directory = role_directory();
        let input = HashMap::new();
        let hours_html = list_editor(
            locale,
            &ListEditorData {
                field: &hours,
                columns: &hours_columns,
                index: 0,
                current: &hours_values,
                stored: Some(&hours_values),
                input: &input,
                directory: None,
                panel_options: Some(&panel_values),
            },
        );
        let retry_input = HashMap::from([
            ("hours.0.panel".to_owned(), "billing".to_owned()),
            ("hours.0.weekday".to_owned(), "friday".to_owned()),
            ("hours.0.start_minute".to_owned(), "09:15".to_owned()),
            ("hours.0.end_minute".to_owned(), "18:00".to_owned()),
        ]);
        let retry_html = list_editor(
            locale,
            &ListEditorData {
                field: &hours,
                columns: &hours_columns,
                index: 0,
                current: &hours_values,
                stored: Some(&hours_values),
                input: &retry_input,
                directory: None,
                panel_options: Some(&panel_values),
            },
        );
        assert!(retry_html.contains("value=\"09:15\""), "{retry_html}");
        assert!(retry_html.contains("value=\"18:00\""), "{retry_html}");
        assert!(
            retry_html.contains("value=\"friday\" selected"),
            "{retry_html}"
        );

        let panels_html = list_editor(
            locale,
            &ListEditorData {
                field: panels,
                columns: panel_columns,
                index: 1,
                current: &panel_values,
                stored: Some(&panel_values),
                input: &input,
                directory: Some(&directory),
                panel_options: Some(&panel_values),
            },
        );
        let unknown_timezone = control(
            locale,
            timezone,
            "global-timezone",
            "Mars/Olympus_Mons",
            None,
            false,
        );
        let html = format!("{hours_html}{panels_html}{unknown_timezone}");

        assert!(html.contains("name=\"f.hours.0.panel\""), "{html}");
        assert!(html.contains("Billing"), "{html}");
        assert!(html.contains("name=\"f.hours.0.weekday\""), "{html}");
        assert!(html.contains("Monday"), "{html}");
        assert!(html.contains("name=\"f.hours.0.start_minute\""), "{html}");
        assert!(html.contains("value=\"08:30\""), "{html}");
        assert!(
            html.contains("name=\"f.hours.0.end_minute.end-of-day\""),
            "{html}"
        );
        assert!(html.contains("24:00 (midnight)"), "{html}");
        assert!(html.contains("name=\"f.panels.0.staff_roles\""), "{html}");
        assert!(html.contains("Support"), "{html}");
        assert!(html.contains("name=\"f.panels.0.questions.0\""), "{html}");
        assert!(html.contains("Need help?"), "{html}");
        assert!(
            html.contains("Leave blank to use the global time zone"),
            "{html}"
        );
        assert!(html.contains("value=\"Mars/Olympus_Mons\""), "{html}");
        assert!(
            html.contains("<option value=\"Mars/Olympus_Mons\">"),
            "{html}"
        );
    }

    #[test]
    fn a_channel_widget_offers_the_kinds_its_schema_names() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let category = channel_field(&json!({"widget": "channel", "channel_kinds": ["category"]}));
        let html = control(
            catalogs.locale("en"),
            &category,
            0,
            "",
            Some(&channel_directory()),
            false,
        );
        assert!(html.contains("value=\"2\""), "{html}");
        assert!(!html.contains("value=\"1\""), "{html}");
    }

    #[test]
    fn a_channel_widget_offers_text_channels_when_no_kind_is_named() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let text = channel_field(&json!({"widget": "channel"}));
        let html = control(
            catalogs.locale("en"),
            &text,
            0,
            "",
            Some(&channel_directory()),
            false,
        );
        assert!(html.contains("value=\"1\""), "{html}");
        assert!(!html.contains("value=\"2\""), "{html}");
    }
    use serde_json::json;

    use super::cell_text;

    #[test]
    fn a_stored_number_is_shown_so_that_saving_keeps_it() {
        assert_eq!(cell_text(Some(&json!(3))), "3");
        assert_eq!(cell_text(Some(&json!(200))), "200");
        assert_eq!(cell_text(Some(&json!("a"))), "a");
        assert_eq!(cell_text(Some(&json!(true))), "true");
        assert_eq!(cell_text(None), "");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn ticket_panel_list_columns_render_localized_html_captions() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let module = json!({"properties": {"config": {"properties": {"panels": {
            "type": "array",
            "maxItems": 1,
            "items": {"type": "object", "required": ["id"], "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"},
                "description": {"type": "string"},
                "button_style": {"type": "string", "enum": ["primary", "secondary"]},
                "category": {"type": "string"},
                "staff_roles": {"type": "array", "items": {"type": "string"}},
                "log_channel": {"type": "string"},
                "close_after_days": {"type": "integer", "minimum": 0, "maximum": 3650}
            }}
        }}}}});
        let field = crate::form::fields(&module).remove(0);
        let crate::form::Kind::List(columns) = &field.kind else {
            panic!("not a list");
        };
        let current = json!([{"id": "Panel_A1"}]);
        let input = HashMap::new();

        for (tag, captions) in [
            (
                "en",
                [
                    "Panel ID",
                    "Panel name",
                    "Description",
                    "Button style",
                    "Category",
                    "Staff role IDs (comma separated)",
                    "Log channel",
                    "Close after inactivity (days)",
                ],
            ),
            (
                "ja",
                [
                    "パネルID",
                    "パネル名",
                    "説明",
                    "ボタンのスタイル",
                    "カテゴリ",
                    "スタッフロールID（カンマ区切り）",
                    "ログチャンネル",
                    "無活動で閉じるまで（日数）",
                ],
            ),
        ] {
            let html = list_editor(
                catalogs.locale(tag),
                &ListEditorData {
                    field: &field,
                    columns,
                    index: 0,
                    current: &current,
                    stored: None,
                    input: &input,
                    directory: None,
                    panel_options: None,
                },
            );
            for caption in captions {
                assert!(
                    html.contains(&format!("<span class=\"list-caption\">{caption}</span>")),
                    "{tag}: {html}"
                );
            }
        }

        let id = columns.iter().find(|column| column.name == "id").unwrap();
        let mut non_panel = field.clone();
        non_panel.name = "other".into();
        let legacy_id = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &non_panel,
                columns: std::slice::from_ref(id),
                index: 0,
                current: &current,
                stored: None,
                input: &input,
                directory: None,
                panel_options: None,
            },
        );
        assert!(
            legacy_id.contains("<span class=\"list-caption\">名前（英小文字・数字・_ -）</span>"),
            "{legacy_id}"
        );

        let mut unknown = id.clone();
        unknown.name = "<unknown>".into();
        let escaped = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &non_panel,
                columns: std::slice::from_ref(&unknown),
                index: 0,
                current: &current,
                stored: None,
                input: &input,
                directory: None,
                panel_options: None,
            },
        );
        assert!(
            escaped.contains("<span class=\"list-caption\">&lt;unknown&gt;</span>"),
            "{escaped}"
        );
    }

    #[test]
    fn a_panel_role_group_keeps_unknown_ids_and_has_an_accessible_name() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = role_panel_field();
        let current = json!([{"id": "support", "category": "help", "staff_roles": ["13", "99"]}]);
        let input = HashMap::new();
        let directory = role_directory();

        let english = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &current,
                stored: Some(&current),
                input: &input,
                directory: Some(&directory),
                panel_options: None,
            },
        );
        let staff_roles_column = columns
            .iter()
            .position(|column| column.name == "staff_roles")
            .unwrap();
        let caption_id = format!("field-list-0-0-{staff_roles_column}-caption");
        assert!(english.contains(&format!(
            "class=\"list-cell list-roles\" role=\"group\" aria-labelledby=\"{caption_id}\""
        )));
        assert!(english.contains("role=\"radiogroup\""));
        assert!(english.contains(&format!(
            "<span id=\"{caption_id}\" class=\"list-caption\">Staff roles</span>"
        )));
        assert!(english.contains("name=\"role_mode:panels.0.staff_roles\" value=\"set\" checked"));
        assert!(english.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));
        assert!(english.contains("value=\"99\" checked>Unknown role 99"));

        let japanese = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &current,
                stored: Some(&current),
                input: &input,
                directory: Some(&directory),
                panel_options: None,
            },
        );
        assert!(japanese.contains(&format!(
            "<span id=\"{caption_id}\" class=\"list-caption\">スタッフロール</span>"
        )));
        assert!(japanese.contains("不明なロール 99"));
    }

    #[test]
    fn a_role_cell_preserves_unset_empty_duplicates_and_directory_fallback() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = role_panel_field();
        let input = HashMap::new();
        let directory = role_directory();
        let inherited = json!([{"id": "support", "category": "help", "staff_roles": ["13"]}]);

        let inherited_html = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &inherited,
                stored: None,
                input: &input,
                directory: Some(&directory),
                panel_options: None,
            },
        );
        assert!(inherited_html.contains("value=\"default\" checked"));
        assert!(inherited_html.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));

        let empty = json!([{"id": "support", "category": "help", "staff_roles": []}]);
        let empty_html = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &empty,
                stored: Some(&empty),
                input: &input,
                directory: Some(&directory),
                panel_options: None,
            },
        );
        assert!(empty_html.contains("value=\"set\" checked"));
        assert!(!empty_html.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));

        let duplicate = json!([{"id": "support", "category": "help", "staff_roles": ["13", "13"]}]);
        let duplicate_html = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &duplicate,
                stored: Some(&duplicate),
                input: &input,
                directory: Some(&directory),
                panel_options: None,
            },
        );
        assert!(duplicate_html.contains("name=\"f.panels.0.staff_roles\" value=\"13, 13\""));
        assert!(duplicate_html.contains("Staff role IDs (comma separated)"));
        let staff_roles_column = columns
            .iter()
            .position(|column| column.name == "staff_roles")
            .unwrap();
        let duplicate_role_option_id = format!("id=\"field-list-0-0-{staff_roles_column}-role-");
        let add_row_role_option_id = format!("id=\"field-list-0-1-{staff_roles_column}-role-");
        assert!(!duplicate_html.contains(&duplicate_role_option_id));
        assert!(duplicate_html.contains(&add_row_role_option_id));

        let missing_directory = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &inherited,
                stored: Some(&inherited),
                input: &input,
                directory: None,
                panel_options: None,
            },
        );
        assert!(missing_directory.contains("name=\"f.panels.0.staff_roles\" value=\"13\""));
        assert!(!missing_directory.contains("role-option"));
    }

    #[test]
    fn list_rows_keep_their_names_after_removal_and_a_cell_error() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = role_panel_field();
        let current = json!([{"id": "old", "category": "help", "staff_roles": ["13"]}]);
        let stored = Some(&current);
        let input = HashMap::from([
            ("panels.0.x".into(), "1".into()),
            ("panels.0.id".into(), "old".into()),
            ("panels.0.category".into(), "help".into()),
            ("panels.0.staff_roles".into(), "13".into()),
            ("panels.1.id".into(), "new".into()),
            ("panels.1.category".into(), "toolong".into()),
            ("panels.1.staff_roles".into(), "13,99".into()),
            ("role-mode:panels.0.staff_roles".into(), "set".into()),
            ("role-mode:panels.1.staff_roles".into(), "set".into()),
        ]);
        let html = list_editor(
            catalogs.locale("en"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &current,
                stored,
                input: &input,
                directory: Some(&role_directory()),
                panel_options: None,
            },
        );

        assert!(html.contains("name=\"role_mode:panels.1.staff_roles\" value=\"set\" checked"));
        assert!(html.contains("name=\"f.panels.1.staff_roles\" value=\"99\" checked"));
        assert!(html.contains("name=\"f.panels.1.category\""));
        assert!(html.contains("name=\"f.panels.1.x\""));
    }

    #[test]
    fn an_unbounded_list_keeps_an_add_row_after_25_and_26_items() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = id_list("panels", None);
        let input = HashMap::new();

        for count in [25, 26] {
            let current = rows_with_ids(count);
            let html = list_editor(
                catalogs.locale("ja"),
                &ListEditorData {
                    field: &field,
                    columns: &columns,
                    index: 0,
                    current: &current,
                    stored: None,
                    input: &input,
                    directory: None,
                    panel_options: None,
                },
            );
            assert!(
                html.contains(&format!("name=\"f.panels.{count}.id\"")),
                "{count}: {html}"
            );
        }
    }

    #[test]
    fn a_list_respects_its_explicit_max_items() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = id_list("panels", Some(25));
        let current = rows_with_ids(25);
        let input = HashMap::new();
        let html = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &current,
                stored: None,
                input: &input,
                directory: None,
                panel_options: None,
            },
        );

        assert!(html.contains("name=\"f.panels.24.id\""), "{html}");
        assert!(!html.contains("name=\"f.panels.25.id\""), "{html}");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn text_lists_render_without_role_columns_and_limit_error_rows() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let question = serde_json::json!({
            "type": "array",
            "maxItems": 5,
            "items": { "type": "string", "minLength": 1, "maxLength": 45 },
            "description": "questions",
            "x-ui": { "level": "basic", "widget": "text-list" }
        });
        let module = serde_json::json!({
            "properties": { "config": { "properties": {
                "questions": question.clone(),
                "panels": {
                    "type": "array",
                    "maxItems": 5,
                    "items": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {
                            "id": { "type": "string" },
                            "questions": question
                        }
                    }
                }
            }}}
        });
        let fields = crate::form::fields(&module);
        let root = fields
            .iter()
            .find(|field| field.name == "questions")
            .unwrap();
        let panels = fields.iter().find(|field| field.name == "panels").unwrap();
        let values = serde_json::json!(["Need, help?", "\nAdditional,\ndetails"]);
        for (tag, label) in [("en", "Question 1"), ("ja", "質問 1")] {
            let html = super::text_list_editor(
                catalogs.locale(tag),
                root,
                0,
                &values,
                &std::collections::HashMap::new(),
                false,
            );
            assert_eq!(html.matches("<textarea").count(), 5, "{tag}: {html}");
            assert!(
                html.contains(
                    r#"<textarea id="field-0" name="f.questions.0" rows="3" maxlength="45">"#
                ),
                "{html}"
            );
            assert!(html.contains(label), "{tag}: {html}");
            assert!(html.contains("Need, help?"), "{html}");
            assert!(html.contains("Additional,\ndetails"), "{html}");
            assert!(html.contains(">\n\nAdditional,"), "{html}");
            assert_eq!(html.matches("list-cell list-remove").count(), 2, "{html}");
        }

        let empty = super::text_list_editor(
            catalogs.locale("en"),
            root,
            0,
            &serde_json::json!([]),
            &std::collections::HashMap::new(),
            false,
        );
        assert_eq!(empty.matches("<textarea").count(), 5, "{empty}");
        assert!(!empty.contains("list-cell list-remove"), "{empty}");

        let crate::form::Kind::List(columns) = &panels.kind else {
            panic!("panels is not a list");
        };
        let panel_values = serde_json::json!([{
            "id": "billing",
            "questions": ["Panel, prompt"]
        }]);
        let panel_html = super::list_editor(
            catalogs.locale("en"),
            &super::ListEditorData {
                field: panels,
                columns,
                index: 1,
                current: &panel_values,
                stored: Some(&panel_values),
                input: &std::collections::HashMap::new(),
                directory: None,
                panel_options: None,
            },
        );
        assert!(
            panel_html.contains("name=\"f.panels.0.questions.0\""),
            "{panel_html}"
        );
        assert!(panel_html.contains("Panel, prompt"), "{panel_html}");
        assert!(panel_html.contains("Questions"), "{panel_html}");

        let excessive: std::collections::HashMap<_, _> = (0..10_000)
            .map(|index| (format!("questions.{index}"), "question".to_owned()))
            .collect();
        let limited = super::text_list_editor(
            catalogs.locale("en"),
            root,
            0,
            &serde_json::json!([]),
            &excessive,
            true,
        );
        assert_eq!(limited.matches("<textarea").count(), 10, "{limited}");
        assert!(limited.contains(r#"aria-invalid="true""#), "{limited}");
        assert!(!limited.contains(r#"name="f.questions.10""#), "{limited}");
    }

    #[test]
    fn list_control_ids_are_unique_across_long_and_adjacent_lists() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = id_list("panels", None);
        let input = HashMap::new();
        let current = rows_with_ids(101);
        let long_list = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &field,
                columns: &columns,
                index: 0,
                current: &current,
                stored: None,
                input: &input,
                directory: None,
                panel_options: None,
            },
        );
        let mut adjacent_field = field.clone();
        adjacent_field.name = "adjacent".into();
        let adjacent_current = rows_with_ids(1);
        let adjacent_list = list_editor(
            catalogs.locale("ja"),
            &ListEditorData {
                field: &adjacent_field,
                columns: &columns,
                index: 1,
                current: &adjacent_current,
                stored: None,
                input: &input,
                directory: None,
                panel_options: None,
            },
        );
        let html = format!("{long_list}{adjacent_list}");

        assert!(html.contains("name=\"f.panels.100.id\""), "{html}");
        assert!(html.contains("name=\"f.adjacent.0.id\""), "{html}");
        let mut ids: Vec<_> = html
            .split("id=\"")
            .skip(1)
            .map(|attribute| attribute.split('"').next().unwrap())
            .collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "{html}");
    }
}
