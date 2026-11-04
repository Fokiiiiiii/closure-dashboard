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
        "<header class=\"page-heading\"><h1>{}</h1></header><ul class=\"module-grid\">",
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
            String::new()
        } else {
            format!(
                "<a class=\"button secondary module-action\" href=\"/g/{}/modules/{}\">{}</a>",
                escape(id),
                escape(&module),
                l.text(keys::SETTINGS)
            )
        };
        let _ = write!(
            body,
            "<li class=\"card module-tile {}\"><div class=\"module-tile-heading\"><span class=\"module-name\">{}</span>{}</div><p class=\"muted module-description\">{}</p>{action}</li>",
            if enabled { "enabled" } else { "disabled" },
            escape(title),
            toggle(l, id, &module, enabled),
            escape(description)
        );
    }
    body.push_str("</ul>");
    shell(l, headers, id, name, keys::MODULES, &body, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn tiles_render_states_actions_and_escaped_description() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let schema = json!({"properties":{"modules":{"properties":{
            "alpha":{"description": format!("<script>{}</script>", "long ".repeat(100)),
                "properties":{"config":{"properties":{"option":{"type":"string"}}}}},
            "beta":{"description":"No options"}
        }}}});
        let effective = json!({"modules":{"alpha":{"enabled":true},"beta":{"enabled":false}}});
        let page = modules_page(
            catalogs.locale("en"),
            &HeaderMap::new(),
            "42",
            "Guild",
            &schema,
            &effective,
        );
        assert!(page.contains("<ul class=\"module-grid\">"));
        let tiles: Vec<_> = page
            .split("<li class=\"card module-tile ")
            .skip(1)
            .collect();
        assert_eq!(tiles.len(), 2);
        for (tile, expected) in tiles.iter().zip([
            [
                "enabled",
                "alpha",
                "aria-checked=\"true\"",
                "alpha: Enabled",
                "module-description",
                "/modules/alpha",
                "&lt;script&gt;",
            ],
            [
                "disabled",
                "beta",
                "aria-checked=\"false\"",
                "beta: Disabled",
                "module-description",
                "role=\"switch\"",
                "No options",
            ],
        ]) {
            for value in expected {
                assert!(tile.contains(value), "{value}");
            }
        }
        assert!(!tiles[1].contains("href=\"/g/42/modules/beta\""));
        assert!(!page.contains("class=\"pill"));
        assert!(!page.contains("This module has no settings"));
        assert!(!page.contains("<script>"));
    }
}
