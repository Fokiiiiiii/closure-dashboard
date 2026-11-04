use super::shell;
use crate::{
    form,
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

pub(super) fn toggle(l: Locale<'_>, id: &str, module: &str, enabled: bool) -> String {
    format!(
        "<form method=\"post\" action=\"/g/{}/{}/toggle\"><button class=\"switch\" type=\"submit\" name=\"enabled\" value=\"{}\" role=\"switch\" aria-checked=\"{enabled}\" aria-label=\"{}: {}\"></button></form>",
        escape(id),
        format_args!("modules/{}", escape(module)),
        !enabled,
        escape(module),
        l.text(if enabled {
            keys::ENABLED
        } else {
            keys::DISABLED
        })
    )
}

pub(crate) fn modules_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    schema: &Value,
    effective: &Value,
) -> String {
    let mut body = format!(
        "<header class=\"page-heading\"><h1>{}</h1></header><section class=\"card panel\"><ul class=\"rows\">",
        l.text(keys::MODULES)
    );
    for (module, schema) in form::modules(schema) {
        if !crate::routes::valid_module(&module) {
            continue;
        }
        let enabled = effective["modules"][&module]["enabled"]
            .as_bool()
            .unwrap_or(false);
        let description = schema["description"].as_str().unwrap_or("");
        let title = module.as_str();
        let action = if form::fields(&schema).is_empty() {
            format!("<span class=\"muted\">{}</span>", l.text(keys::NO_SETTINGS))
        } else {
            format!(
                "<a class=\"button secondary\" href=\"/g/{}/modules/{}\">{}</a>",
                escape(id),
                escape(&module),
                l.text(keys::SETTINGS)
            )
        };
        let _ = write!(
            body,
            "<li class=\"module-row module-list-row\"><div class=\"module-name\">{}<small class=\"muted module-description\">{}</small></div><span class=\"pill {}\">{}</span>{} {action}</li>",
            escape(title),
            escape(description),
            if enabled { "enabled" } else { "disabled" },
            l.text(if enabled {
                keys::ENABLED
            } else {
                keys::DISABLED
            }),
            toggle(l, id, &module, enabled)
        );
    }
    body.push_str("</ul></section>");
    shell(l, headers, id, name, keys::MODULES, &body)
}
