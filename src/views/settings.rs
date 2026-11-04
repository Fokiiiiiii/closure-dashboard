use super::{form::control, shell};
use crate::{
    form::{self, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{collections::HashMap, fmt::Write};

pub(crate) struct SettingsPage<'a> {
    pub id: &'a str,
    pub guild: &'a str,
    pub schema: &'a Value,
    pub stored: &'a Value,
    pub effective: &'a Value,
    pub directory: Option<&'a Value>,
    pub etag: &'a str,
    pub input: &'a HashMap<String, String>,
    pub errors: &'a HashMap<String, Vec<String>>,
    pub saved: bool,
}

fn display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(display).collect::<Vec<_>>().join(","),
        Value::Object(o) if o.len() == 1 => o
            .iter()
            .next()
            .filter(|(_, v)| v.is_null())
            .map_or_else(|| value.to_string(), |(k, _)| k.clone()),
        v => v.to_string(),
    }
}

fn roles(
    l: Locale<'_>,
    id: &str,
    field: &Field,
    index: usize,
    value: &str,
    directory: Option<&Value>,
    invalid: bool,
) -> String {
    let Some(directory) = directory else {
        return control(l, field, index, value, None, invalid);
    };
    let mut out = format!(
        "<input type=\"hidden\" name=\"roles_present\" value=\"1\"><div class=\"role-list\" role=\"group\" aria-labelledby=\"field-{index}-label\"{}>",
        if invalid {
            format!(" aria-describedby=\"field-{index}-error\"")
        } else {
            String::new()
        }
    );
    let selected: Vec<&str> = value.split(',').filter(|v| !v.is_empty()).collect();
    let mut found = Vec::new();
    for role in directory["roles"].as_array().into_iter().flatten() {
        let Some(role_id) = role["id"].as_str() else {
            continue;
        };
        if role_id == id || role["managed"] == true || role["name"] == "@everyone" {
            continue;
        }
        let checked = selected.contains(&role_id);
        if checked {
            found.push(role_id);
        }
        let color = role["color"].as_u64().unwrap_or(0).min(0xFF_FFFF);
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"f.moderator_roles\" value=\"{}\"{}><svg aria-hidden=\"true\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><circle cx=\"6\" cy=\"6\" r=\"5\" fill=\"#{color:06x}\"/></svg>{}</label>",
            escape(role_id),
            if checked { " checked" } else { "" },
            escape(role["name"].as_str().unwrap_or(role_id))
        );
    }
    for role_id in selected.iter().filter(|role| !found.contains(role)) {
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"f.moderator_roles\" value=\"{}\" checked>{}</label>",
            escape(role_id),
            l.with_text(keys::UNKNOWN_ROLE, "id", role_id)
        );
    }
    out.push_str("</div>");
    out
}

#[allow(clippy::too_many_lines)]
pub(crate) fn settings_page(l: Locale<'_>, headers: &HeaderMap, data: &SettingsPage<'_>) -> String {
    let fields = form::settings(data.schema);
    let mut body = format!(
        "<header class=\"page-heading\"><h1>{}</h1></header>",
        l.text(keys::SERVER_SETTINGS)
    );
    if data.saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::SAVED)
        );
    }
    if let Some(messages) = data.errors.get("") {
        for message in messages {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(message)
            );
        }
    }
    let _ = write!(
        body,
        "<form method=\"post\" action=\"/g/{}/settings\"><input type=\"hidden\" name=\"etag\" value=\"{}\">",
        escape(data.id),
        escape(data.etag)
    );
    let names = &[
        ("language", keys::SETTINGS_LANGUAGE),
        ("log_channel", keys::SETTINGS_LOG_CHANNEL),
        ("moderator_roles", keys::SETTINGS_MODERATOR_ROLES),
        ("prefix", keys::SETTINGS_PREFIX),
        ("timezone", keys::SETTINGS_TIMEZONE),
    ];
    body.push_str("<section class=\"card settings-card\">");
    for (name, label) in names {
        let Some((index, field)) = fields.iter().enumerate().find(|(_, f)| f.name == *name) else {
            continue;
        };
        let fallback = data
            .stored
            .get(*name)
            .filter(|v| !v.is_null())
            .or_else(|| data.effective.get(*name))
            .map_or_else(String::new, display);
        let value = data.input.get(*name).unwrap_or(&fallback);
        let errors = data.errors.get(*name);
        let heading = if field.kind == Kind::Roles && data.directory.is_some() {
            format!("<span id=\"field-{index}-label\">{}</span>", l.text(label))
        } else {
            format!("<label for=\"field-{index}\">{}</label>", l.text(label))
        };
        let _ = write!(body, "<div class=\"settings-row\"><div>{heading}");
        if let Some(description) = field.schema["description"].as_str() {
            let _ = write!(
                body,
                "<p class=\"settings-help\" title=\"{}\">{}</p>",
                escape(description),
                escape(&super::first_sentence(description))
            );
        }
        if let Some(messages) = errors {
            let _ = write!(
                body,
                "<div id=\"field-{index}-error\" class=\"form-error\" role=\"alert\">"
            );
            for message in messages {
                let _ = write!(body, "<div>⚠ {}</div>", escape(message));
            }
            body.push_str("</div>");
        }
        let widget = if field.kind == Kind::Roles {
            roles(
                l,
                data.id,
                field,
                index,
                value,
                data.directory,
                errors.is_some(),
            )
        } else {
            control(l, field, index, value, data.directory, errors.is_some())
        };
        let _ = write!(
            body,
            "</div><div class=\"settings-control\">{widget}</div></div>"
        );
    }
    body.push_str("</section>");
    let _ = write!(
        body,
        "<div class=\"save-bar\"><a class=\"button ghost\" href=\"/g/{}/settings\">{}</a><button class=\"button primary\" type=\"submit\">{}</button></div></form>",
        escape(data.id),
        l.text(keys::DISCARD),
        l.text(keys::SAVE)
    );
    shell(
        l,
        headers,
        data.id,
        data.guild,
        keys::SERVER_SETTINGS,
        &body,
        true,
    )
}
