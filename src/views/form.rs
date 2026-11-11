use super::{modules::toggle, shell};
use crate::{
    form::{self, Column, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use std::{collections::{HashMap, HashSet}, fmt::Write};

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
    if let Some(directory) = directory
        && matches!(widget, "channel" | "role")
    {
        let mut result = format!("<select {attrs}>");
        option(&mut result, "", &l.text(keys::UNSET), value.is_empty());
        let list = if widget == "channel" {
            &directory["channels"]
        } else {
            &directory["roles"]
        };
        let mut found = value.is_empty();
        let kinds: Vec<&str> = field.schema["x-ui"]["channel_kinds"]
            .as_array()
            .map_or_else(
                || vec!["text"],
                |kinds| kinds.iter().filter_map(Value::as_str).collect(),
            );
        for item in list.as_array().into_iter().flatten() {
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
        Kind::List(_) | Kind::Entries | Kind::Structured | Kind::Message(_) => String::new(),
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
fn column_title(l: Locale<'_>, name: &str, panel: bool) -> String {
    l.text(match name {
        "id" if panel => keys::COL_PANEL_ID,
        "channel" => keys::COL_CHANNEL,
        "role" => keys::COL_ROLE,
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
        "staff_roles" => keys::COL_STAFF_ROLES,
        "log_channel" => keys::COL_LOG_CHANNEL,
        "close_after_days" => keys::COL_CLOSE_AFTER_DAYS,
        _ => return escape(name),
    })
}

fn role_value_text(value: Option<&Value>) -> String {
    if value
        .and_then(Value::as_array)
        .is_some_and(|values| values.is_empty())
    {
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

fn roles_controls(
    l: Locale<'_>,
    field: &Field,
    control_index: &str,
    mode_name: &str,
    mode: &str,
    required: bool,
    value: &str,
    directory: Option<&Value>,
    picker: bool,
) -> String {
    let id = format!("field-{control_index}");
    let default_id = format!("{id}-default");
    let set_id = format!("{id}-set");
    let name = escape(&format!("f.{}", field.name));
    let mode_name = escape(mode_name);
    let mut out = String::new();
    let _ = write!(
        out,
        "<div id=\"{id}-modes\" class=\"role-mode\" role=\"radiogroup\" aria-label=\"{}\">",
        l.text(keys::COL_STAFF_ROLES_MODE)
    );
    if !required {
        let _ = write!(
            out,
            "<label><input type=\"radio\" id=\"{default_id}\" name=\"{mode_name}\" value=\"default\"{}>{}</label>",
            if mode == "default" { " checked" } else { "" },
            l.text(keys::COL_STAFF_ROLES_DEFAULT)
        );
    }
    let _ = write!(
        out,
        "<label><input type=\"radio\" id=\"{set_id}\" name=\"{mode_name}\" value=\"set\"{}>{}</label></div>",
        if mode == "set" { " checked" } else { "" },
        l.text(keys::COL_STAFF_ROLES_SET)
    );
    if picker {
        if let Some(roles) = directory.and_then(|directory| directory["roles"].as_array()) {
            let options_id = format!("{id}-options");
            let values = role_ids(value);
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
                    .map_or_else(
                        || l.with_text(keys::UNKNOWN_ROLE, "id", role_id),
                        |name| escape(name),
                    );
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
    }
    let _ = write!(
        out,
        "<label for=\"{id}\">{}</label>{}",
        l.text(keys::COL_STAFF_ROLES),
        control(l, field, control_index, value, None, false)
    );
    out
}

fn roles_cell(
    l: Locale<'_>,
    field: &Field,
    control_index: &str,
    mode_name: &str,
    mode: &str,
    required: bool,
    value: &str,
    directory: Option<&Value>,
    picker: bool,
) -> String {
    let id = format!("field-{control_index}");
    let caption_id = format!("{id}-caption");
    let mut out = String::new();
    let _ = write!(
        out,
        "<div class=\"list-cell list-roles\" role=\"group\" aria-labelledby=\"{caption_id}\"><span id=\"{caption_id}\" class=\"list-caption\">{}</span>",
        l.text(keys::COL_STAFF_ROLES_PICKER)
    );
    out.push_str(&roles_controls(
        l,
        field,
        control_index,
        mode_name,
        mode,
        required,
        value,
        directory,
        picker,
    ));
    out.push_str("</div>");
    out
}

fn roles_field_widget(
    l: Locale<'_>,
    field: &Field,
    control_index: &str,
    mode_name: &str,
    mode: &str,
    value: &str,
    directory: Option<&Value>,
    picker: bool,
    labelled_by: &str,
    invalid: bool,
) -> String {
    let described = if invalid {
        format!(" aria-describedby=\"field-{control_index}-error\"")
    } else {
        String::new()
    };
    let mut out = format!(
        "<div class=\"role-list-control\" role=\"group\" aria-labelledby=\"{}\"{described}>",
        escape(labelled_by)
    );
    out.push_str(&roles_controls(
        l,
        field,
        control_index,
        mode_name,
        mode,
        false,
        value,
        directory,
        picker,
    ));
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

/// One row per entry of a list, and one empty row to add the next entry. After a refused save the
/// rows come back as they were typed.
fn list_editor(
    l: Locale<'_>,
    field: &Field,
    columns: &[Column],
    index: usize,
    current: &Value,
    stored: Option<&Value>,
    input: &HashMap<String, String>,
    directory: Option<&Value>,
) -> String {
    let prefix = format!("{}.", field.name);
    let cell =
        |number: usize, column: &Column| input.get(&format!("{prefix}{number}.{}", column.name));
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    // Keep submitted row numbers stable when the form is shown again after an error.
    let current_len = current.as_array().map_or(0, Vec::len);
    let has_input = !numbers.is_empty();
    let mut rows: Vec<(usize, Vec<String>, bool)> = if has_input {
        numbers
            .iter()
            .map(|&number| {
                let texts = columns
                    .iter()
                    .map(|column| cell(number, column).cloned().unwrap_or_default())
                    .collect();
                let removed = input
                    .get(&format!("{prefix}{number}.x"))
                    .is_some_and(|value| value == "1");
                (number, texts, removed)
            })
            .collect()
    } else {
        current
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(number, row)| {
                let texts = columns
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
                (number, texts, false)
            })
            .collect()
    };
    let has_new_row = rows.iter().any(|(number, _, _)| *number == current_len);
    let can_add = field.schema["maxItems"]
        .as_u64()
        .is_none_or(|max| (current_len as u64) < max);
    if can_add && !has_new_row {
        let blank = columns
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
        rows.push((current_len, blank, false));
    }
    let mut out = String::from("<div class=\"list-editor\">");
    for (row, texts, removed) in &rows {
        out.push_str("<div class=\"list-row\">");
        for (c, (column, text)) in columns.iter().zip(texts).enumerate() {
            let mut schema = column.schema.clone();
            if matches!(column.name.as_str(), "channel" | "role") {
                schema["x-ui"]["widget"] = json!(column.name);
            }
            let cell = Field {
                name: format!("{prefix}{row}.{}", column.name),
                label: column.name.clone(),
                kind: column.kind.clone(),
                schema,
            };
            if column.kind == Kind::Roles && column.schema["x-ui"]["widget"] == "roles" {
                let current_cell = current
                    .as_array()
                    .and_then(|rows| rows.get(*row))
                    .and_then(|current| current.get(&column.name));
                let stored_cell = stored
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.get(*row))
                    .and_then(|stored| stored.get(&column.name))
                    .filter(|value| !value.is_null());
                let mode_name = format!("role_mode:{}", cell.name);
                let mode_key = format!("role-mode:{}", cell.name);
                let mode = input
                    .get(&mode_key)
                    .map_or(
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
                    input
                        .get(&cell.name)
                        .cloned()
                        .unwrap_or_else(|| role_value_text(current_cell))
                };
                let picker = form::roles_picker(&column.kind, &column.schema, current_cell, directory)
                    && role_ids_are_unique(&text);
                out.push_str(&roles_cell(
                    l,
                    &cell,
                    &format!("list-{index}-{row}-{c}"),
                    &mode_name,
                    mode,
                    column.required,
                    &text,
                    directory,
                    picker,
                ));
            } else {
                let _ = write!(
                    out,
                    "<label class=\"list-cell\"><span class=\"list-caption\">{}</span>{}</label>",
                    column_title(l, &column.name, field.name == "panels"),
                    control(
                        l,
                        &cell,
                        format!("list-{index}-{row}-{c}"),
                        text,
                        directory,
                        false
                    )
                );
            }
        }
        let has_content = columns
            .iter()
            .zip(texts)
            .any(|(column, text)| !form::is_preset(&column.kind) && !text.is_empty());
        if *row < current_len || *removed || has_content {
            let _ = write!(
                out,
                "<label class=\"list-cell list-remove\"><span class=\"list-caption\">{}</span><input type=\"checkbox\" name=\"{}\" value=\"1\"{}></label>",
                l.text(keys::COL_REMOVE),
                escape(&format!("f.{prefix}{row}.x")),
                if *removed { " checked" } else { "" }
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
            let stored_current =
                form::lookup(&stored["modules"][module]["config"], &field.name)
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
            let wide = if matches!(field.kind, Kind::List(_) | Kind::Entries | Kind::Message(_)) {
                " wide"
            } else {
                ""
            };
            let role_field =
                field.kind == Kind::Roles && field.schema["x-ui"]["widget"] == "roles";
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
                let note = if matches!(field.kind, Kind::Words | Kind::Roles) {
                    keys::INHERITED_LIST
                } else {
                    keys::INHERITED
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
                    if stored_current.is_some() { "set" } else { "default" },
                    String::as_str,
                );
                let value = if mode == "default" {
                    &fallback
                } else {
                    input.get(&field.name).unwrap_or(&fallback)
                };
                let picker = form::roles_picker(&field.kind, &field.schema, Some(current), directory)
                    && role_ids_are_unique(value);
                roles_field_widget(
                    l,
                    field,
                    &index.to_string(),
                    &mode_name,
                    mode,
                    value,
                    directory,
                    picker,
                    &format!("field-{index}-label"),
                    field_errors.is_some(),
                )
            } else {
                match &field.kind {
                    Kind::List(columns) => list_editor(
                        l,
                        field,
                        columns,
                        index,
                        current,
                        stored_current,
                        input,
                        directory,
                    ),
                    Kind::Entries => entries_editor(l, field, current, input),
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
    use super::{control, list_editor};
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
        let crate::form::Kind::List(columns) = &field.kind else {
            panic!("not a list");
        };
        (field, columns.clone())
    }

    fn role_directory() -> serde_json::Value {
        json!({"roles": [
            {"id": "13", "name": "Support", "managed": false},
            {"id": "42", "name": "@everyone", "managed": false},
            {"id": "14", "name": "managed", "managed": true}
        ]})
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
    fn ticket_panel_list_columns_render_localized_html_captions() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let module = json!({"properties": {"config": {"properties": {"panels": {
            "type": "array",
            "maxItems": 1,
            "items": {"type": "object", "required": ["id"], "properties": {
                "id": {"type": "string"},
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
                    "カテゴリ",
                    "スタッフロールID（カンマ区切り）",
                    "ログチャンネル",
                    "無活動で閉じるまで（日数）",
                ],
            ),
        ] {
            let html = list_editor(
                catalogs.locale(tag),
                &field,
                columns,
                0,
                &current,
                None,
                &input,
                None,
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
            &non_panel,
            std::slice::from_ref(id),
            0,
            &current,
            None,
            &input,
            None,
        );
        assert!(
            legacy_id.contains("<span class=\"list-caption\">名前（英小文字・数字・_ -）</span>"),
            "{legacy_id}"
        );

        let mut unknown = id.clone();
        unknown.name = "<unknown>".into();
        let escaped = list_editor(
            catalogs.locale("ja"),
            &non_panel,
            std::slice::from_ref(&unknown),
            0,
            &current,
            None,
            &input,
            None,
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
            &field,
            &columns,
            0,
            &current,
            Some(&current),
            &input,
            Some(&directory),
        );
        assert!(english.contains("class=\"list-roles\" role=\"group\" aria-labelledby="));
        assert!(english.contains("role=\"radiogroup\""));
        assert!(english.contains("<span class=\"list-caption\">Staff roles</span>"));
        assert!(english.contains("name=\"role_mode:panels.0.staff_roles\" value=\"set\" checked"));
        assert!(english.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));
        assert!(english.contains("value=\"99\" checked>Unknown role 99"));

        let japanese = list_editor(
            catalogs.locale("ja"),
            &field,
            &columns,
            0,
            &current,
            Some(&current),
            &input,
            Some(&directory),
        );
        assert!(japanese.contains("<span class=\"list-caption\">スタッフロール</span>"));
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
            &field,
            &columns,
            0,
            &inherited,
            None,
            &input,
            Some(&directory),
        );
        assert!(inherited_html.contains("value=\"default\" checked"));
        assert!(inherited_html.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));

        let empty = json!([{"id": "support", "category": "help", "staff_roles": []}]);
        let empty_html = list_editor(
            catalogs.locale("en"),
            &field,
            &columns,
            0,
            &empty,
            Some(&empty),
            &input,
            Some(&directory),
        );
        assert!(empty_html.contains("value=\"set\" checked"));
        assert!(!empty_html.contains("name=\"f.panels.0.staff_roles\" value=\"13\" checked"));

        let duplicate = json!([{"id": "support", "category": "help", "staff_roles": ["13", "13"]}]);
        let duplicate_html = list_editor(
            catalogs.locale("en"),
            &field,
            &columns,
            0,
            &duplicate,
            Some(&duplicate),
            &input,
            Some(&directory),
        );
        assert!(duplicate_html.contains("name=\"f.panels.0.staff_roles\" value=\"13, 13\""));
        assert!(duplicate_html.contains("Staff role IDs (comma separated)"));
        assert!(!duplicate_html.contains("role-option"));

        let missing_directory = list_editor(
            catalogs.locale("en"),
            &field,
            &columns,
            0,
            &inherited,
            Some(&inherited),
            &input,
            None,
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
            &field,
            &columns,
            0,
            &current,
            stored,
            &input,
            Some(&role_directory()),
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
                &field,
                &columns,
                0,
                &current,
                None,
                &input,
                None,
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
            &field,
            &columns,
            0,
            &current,
            None,
            &input,
            None,
        );

        assert!(html.contains("name=\"f.panels.24.id\""), "{html}");
        assert!(!html.contains("name=\"f.panels.25.id\""), "{html}");
    }

    #[test]
    fn list_control_ids_are_unique_across_long_and_adjacent_lists() {
        let catalogs = crate::i18n::Catalogs::new(crate::i18n::REGISTRY).unwrap();
        let (field, columns) = id_list("panels", None);
        let input = HashMap::new();
        let current = rows_with_ids(101);
        let long_list = list_editor(
            catalogs.locale("ja"),
            &field,
            &columns,
            0,
            &current,
            None,
            &input,
            None,
        );
        let mut adjacent_field = field.clone();
        adjacent_field.name = "adjacent".into();
        let adjacent_current = rows_with_ids(1);
        let adjacent_list = list_editor(
            catalogs.locale("ja"),
            &adjacent_field,
            &columns,
            1,
            &adjacent_current,
            None,
            &input,
            None,
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
