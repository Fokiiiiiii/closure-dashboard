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

/// A small line icon that says what a module is for; a plain box for one the dashboard does not know.
pub(super) fn module_icon(module: &str) -> String {
    let path = match module {
        "automod" => "M13 2L4 14h7l-1 8 9-12h-7z",
        "channels" => "M5 9h14M5 15h14M10 4L8 20M16 4l-2 16",
        "dashboard" => "M4 4h7v7H4zM13 4h7v4h-7zM13 11h7v9h-7zM4 14h7v6H4z",
        "diagnostics" => "M3 12h4l3-8 4 16 3-8h4",
        "logs" => "M7 3h8l4 4v14H7zM15 3v4h4M10 12h6M10 16h6",
        "moderation" => "M12 3l7 3v5c0 5-3.5 8-7 10-3.5-2-7-5-7-10V6zM9 12l2 2 4-4",
        "remind" => "M6 16v-5a6 6 0 0 1 12 0v5l2 2H4zM10 20a2 2 0 0 0 4 0",
        "tags" => "M3 12V4h8l10 10-8 8zM7.5 8h.01",
        "welcome" => {
            "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01"
        }
        _ => "M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5",
    };
    format!(
        "<svg class=\"module-icon\" aria-hidden=\"true\" width=\"20\" height=\"20\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{path}\"/></svg>"
    )
}

/// The categories of the list, in the order shown, with the modules each one holds.
const CATEGORIES: [(&str, &[&str]); 3] = [
    ("moderation", &["moderation", "automod", "logs"]),
    ("community", &["welcome", "channels", "tags", "remind"]),
    ("system", &["diagnostics", "dashboard"]),
];

/// The line icon of a category heading.
fn category_icon(category: usize) -> &'static str {
    match CATEGORIES.get(category).map(|(key, _)| *key) {
        Some("moderation") => "M12 3l7 3v5c0 5-3.5 8-7 10-3.5-2-7-5-7-10V6z",
        Some("community") => {
            "M16 20v-1a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v1 M10 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z M20 20v-1a3 3 0 0 0-2-2.8 M16 4.2a3.5 3.5 0 0 1 0 6.6"
        }
        Some("system") => "M12 8a4 4 0 100 8 4 4 0 000-8z M12 3v3 M12 18v3 M3 12h3 M18 12h3",
        _ => "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z",
    }
}

fn module_tile(l: Locale<'_>, id: &str, module: &str, schema: &Value, effective: &Value) -> String {
    let enabled = effective["modules"][module]["enabled"]
        .as_bool()
        .unwrap_or(false);
    let description = schema["description"].as_str().unwrap_or("");
    let action = if form::fields(schema).is_empty() {
        String::new()
    } else {
        format!(
            "<a class=\"button secondary module-action\" href=\"/g/{}/modules/{}\">{}</a>",
            escape(id),
            escape(module),
            l.text(keys::SETTINGS)
        )
    };
    format!(
        "<li class=\"card module-tile {}\"><div class=\"module-tile-heading\"><span class=\"module-title\">{}<span class=\"module-name\">{}</span></span>{}</div><p class=\"muted module-description\" title=\"{}\">{}</p>{action}</li>",
        if enabled { "enabled" } else { "disabled" },
        module_icon(module),
        escape(module),
        toggle(l, id, module, enabled),
        escape(description),
        escape(&super::first_sentence(description))
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
    let mut tiles: Vec<(usize, usize, String)> = Vec::new();
    for (module, schema) in form::modules(schema) {
        if !crate::routes::valid_module(&module) {
            continue;
        }
        let place = CATEGORIES.iter().enumerate().find_map(|(c, (_, members))| {
            members
                .iter()
                .position(|m| *m == module)
                .map(|rank| (c, rank))
        });
        let (category, rank) = place.unwrap_or((CATEGORIES.len(), 0));
        tiles.push((
            category,
            rank,
            module_tile(l, id, &module, &schema, effective),
        ));
    }
    tiles.sort_by_key(|(category, rank, _)| (*category, *rank));
    let mut body = format!(
        "<header class=\"page-heading\">{}</header>",
        super::heading(l, keys::MODULES)
    );
    for category in 0..=CATEGORIES.len() {
        let list: String = tiles
            .iter()
            .filter(|(c, _, _)| *c == category)
            .map(|(_, _, tile)| tile.as_str())
            .collect();
        let count = tiles.iter().filter(|(c, _, _)| *c == category).count();
        if list.is_empty() {
            continue;
        }
        let heading = match CATEGORIES.get(category).map(|(key, _)| *key) {
            Some("moderation") => keys::CAT_MODERATION,
            Some("community") => keys::CAT_COMMUNITY,
            Some("system") => keys::CAT_SYSTEM,
            _ => keys::CAT_OTHER,
        };
        let _ = write!(
            body,
            "<section class=\"module-section\"><h2>{}{}<span class=\"count\">{count}</span></h2><ul class=\"module-grid\">{list}</ul></section>",
            super::icon(category_icon(category), 16, "category-icon"),
            l.text(heading)
        );
    }
    shell(l, headers, id, name, keys::MODULES, &body, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn modules_are_listed_by_category_in_a_fixed_order() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let schema = json!({"properties":{"modules":{"properties":{
            "zeta":{"description":"z"},
            "remind":{"description":"r"},
            "logs":{"description":"l"},
            "automod":{"description":"a"},
            "diagnostics":{"description":"d"}
        }}}});
        let html = modules_page(
            catalogs.locale("en"),
            &HeaderMap::new(),
            "42",
            "server",
            &schema,
            &json!({}),
        );
        let at = |needle: &str| html.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(at(">Moderation<") < at("modules/automod/toggle"));
        assert!(at("modules/automod/toggle") < at("modules/logs/toggle"));
        assert!(at("modules/logs/toggle") < at(">Community<"));
        assert!(at(">Community<") < at("modules/remind/toggle"));
        assert!(at("modules/remind/toggle") < at(">System<"));
        assert!(at(">System<") < at("modules/diagnostics/toggle"));
        assert!(at("modules/diagnostics/toggle") < at(">Other<"));
        assert!(at(">Other<") < at("modules/zeta/toggle"));
        let one = json!({"properties":{"modules":{"properties":{"remind":{}}}}});
        let html = modules_page(
            catalogs.locale("en"),
            &HeaderMap::new(),
            "42",
            "server",
            &one,
            &json!({}),
        );
        assert!(!html.contains(">Moderation<") && !html.contains(">Other<"));
    }

    #[test]
    fn every_known_module_has_its_own_icon_and_unknown_ones_a_plain_box() {
        let known = [
            "automod",
            "channels",
            "dashboard",
            "diagnostics",
            "logs",
            "moderation",
            "remind",
            "tags",
            "welcome",
        ];
        let icons: Vec<String> = known.iter().map(|m| module_icon(m)).collect();
        for (i, icon) in icons.iter().enumerate() {
            assert!(icon.contains("aria-hidden=\"true\""));
            assert!(!icon.contains("style="));
            assert!(
                icons.iter().skip(i + 1).all(|other| other != icon),
                "{}",
                known[i]
            );
        }
        assert!(!icons.contains(&module_icon("something-else")));
        assert_eq!(module_icon("something-else"), module_icon("another"));
    }

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
