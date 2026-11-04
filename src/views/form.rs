use super::{modules::toggle, shell};
use crate::{
    form::{self, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{collections::HashMap, fmt::Write};

pub(crate) struct FormPage<'a> {
    pub id: &'a str,
    pub guild: &'a str,
    pub module: &'a str,
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
fn section_title(l: Locale<'_>, key: &str) -> String {
    l.text(match key {
        "warn" => keys::SEC_WARN,
        "timeout" => keys::SEC_TIMEOUT,
        "voice" => keys::SEC_VOICE,
        "ban" => keys::SEC_BAN,
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
        Kind::String | Kind::Roles => {
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

#[allow(clippy::too_many_lines)]
pub(crate) fn form_page(l: Locale<'_>, headers: &HeaderMap, data: &FormPage<'_>) -> String {
    let FormPage {
        id,
        guild,
        module,
        schema,
        stored,
        effective,
        directory,
        etag,
        input,
        errors,
        saved,
    } = *data;
    let title = module;
    let enabled = effective["modules"][module]["enabled"]
        .as_bool()
        .unwrap_or(false);
    let mut body = format!(
        "<header class=\"page-heading\"><h1 class=\"with-icon\">{}{}</h1>{}</header>",
        super::modules::module_icon(module),
        escape(title),
        toggle(l, id, module, enabled)
    );
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
    let fields = form::fields(schema);
    let _ = write!(
        body,
        "<form method=\"post\" action=\"/g/{}/modules/{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\">",
        escape(id),
        escape(module),
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
            let fallback = stored["modules"][module]["config"]
                .get(&field.name)
                .filter(|v| !v.is_null())
                .or_else(|| effective["modules"][module]["config"].get(&field.name))
                .unwrap_or(&Value::Null);
            let fallback = match fallback {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                v => v.to_string(),
            };
            let value = input.get(&field.name).unwrap_or(&fallback);
            let field_errors = errors.get(&field.name);
            let when = form::dependency(module, &field.name)
                .map_or_else(String::new, |(name, value)| {
                    format!(" when\" data-when=\"{}={}", escape(name), escape(value))
                });
            let _ = write!(
                body,
                "<div class=\"settings-row{when}\"><div><label for=\"field-{index}\" title=\"{}\">{}</label>",
                escape(field.schema["description"].as_str().unwrap_or(&field.label)),
                escape(&super::first_sentence(&field.label))
            );
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
            let _ = write!(
                body,
                "</div><div class=\"settings-control\">{}</div></div>",
                control(l, field, index, value, directory, field_errors.is_some())
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
