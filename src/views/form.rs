use super::{modules::toggle, shell};
use crate::{
    form::{self, Column, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use std::{collections::HashMap, fmt::Write};

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
    index: usize,
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
        for item in list.as_array().into_iter().flatten() {
            if (widget == "channel" && item["kind"] != "text")
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
fn column_title(l: Locale<'_>, name: &str) -> String {
    l.text(match name {
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
        _ => return escape(name),
    })
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
    // (the text of each column, whether the row is ticked for removal)
    let mut rows: Vec<(Vec<String>, bool)> = if numbers.is_empty() {
        current
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                let texts = columns
                    .iter()
                    .map(|c| cell_text(row.get(&c.name)))
                    .collect();
                (texts, false)
            })
            .collect()
    } else {
        numbers
            .iter()
            .map(|&n| {
                let texts = columns
                    .iter()
                    .map(|c| cell(n, c).cloned().unwrap_or_default())
                    .collect();
                let removed = input
                    .get(&format!("{prefix}{n}.x"))
                    .is_some_and(|v| v == "1");
                (texts, removed)
            })
            .collect()
    };
    rows.retain(|(texts, removed)| {
        *removed
            || columns
                .iter()
                .zip(texts)
                .any(|(c, text)| !form::is_preset(&c.kind) && !text.is_empty())
    });
    let existing = rows.len();
    if (rows.len() as u64) < field.schema["maxItems"].as_u64().unwrap_or(25) {
        let blank = columns
            .iter()
            .map(|c| match &c.kind {
                Kind::Boolean => c.schema["default"].as_bool().unwrap_or(false).to_string(),
                Kind::Enum(values) => values.first().cloned().unwrap_or_default(),
                _ => String::new(),
            })
            .collect();
        rows.push((blank, false));
    }
    let mut out = String::from("<div class=\"list-editor\">");
    for (row, (texts, removed)) in rows.iter().enumerate() {
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
            let _ = write!(
                out,
                "<label class=\"list-cell\"><span class=\"list-caption\">{}</span>{}</label>",
                column_title(l, &column.name),
                control(
                    l,
                    &cell,
                    100_000 + index * 1000 + row * 10 + c,
                    text,
                    directory,
                    false
                )
            );
        }
        if row < existing {
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
            let current = form::lookup(&stored["modules"][module]["config"], &field.name)
                .filter(|v| !v.is_null())
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
            let inherited = form::lookup(&stored["modules"][module]["config"], &field.name)
                .is_none_or(Value::is_null)
                && matches!(field.kind, Kind::Words | Kind::Roles)
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
            let _ = write!(
                body,
                "<div class=\"settings-row{wide}{when}\"><div><label for=\"field-{index}\" title=\"{}\">{}</label>",
                escape(field.schema["description"].as_str().unwrap_or(&field.label)),
                field_label(l, field)
            );
            if inherited {
                let _ = write!(
                    body,
                    "<small class=\"muted\">{}</small>",
                    l.text(keys::INHERITED)
                );
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
            let widget = match &field.kind {
                Kind::List(columns) => {
                    list_editor(l, field, columns, index, current, input, directory)
                }
                Kind::Entries => entries_editor(l, field, current, input),
                Kind::Message(_) => message_row(l, id, module, field, current),
                _ => control(l, field, index, value, directory, field_errors.is_some()),
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
}
