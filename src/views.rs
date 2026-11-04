use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
    session,
};

fn theme(headers: &HeaderMap) -> &str {
    super::routes::cookie(headers, "theme")
        .filter(|v| matches!(*v, "light" | "dark"))
        .unwrap_or("auto")
}

pub(crate) fn page(l: Locale<'_>, headers: &HeaderMap, content: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\" dir=\"{}\" data-theme=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"/app.css\"></head><body>{content}</body></html>",
        escape(l.tag()),
        escape(l.dir()),
        theme(headers),
        l.text(keys::TITLE)
    )
}

fn segmented(l: Locale<'_>, headers: &HeaderMap) -> String {
    let mut languages = String::new();
    for d in l.catalogs().definitions() {
        let _ = write!(
            languages,
            "<button type=\"submit\" name=\"lang\" value=\"{}\" aria-pressed=\"{}\">{}</button>",
            escape(d.tag),
            d.tag == l.tag(),
            escape(d.name)
        );
    }
    let mut themes = String::new();
    for (value, key) in [
        ("auto", keys::AUTO),
        ("light", keys::LIGHT),
        ("dark", keys::DARK),
    ] {
        let _ = write!(
            themes,
            "<button type=\"submit\" name=\"theme\" value=\"{value}\" aria-pressed=\"{}\">{}</button>",
            value == theme(headers),
            l.text(key)
        );
    }
    format!(
        "<form class=\"segments\" method=\"post\" action=\"/lang\" aria-label=\"{}\">{languages}</form><form class=\"segments\" method=\"post\" action=\"/theme\" aria-label=\"{}\">{themes}</form>",
        l.text(keys::LANGUAGE),
        l.text(keys::THEME)
    )
}

fn logout(l: Locale<'_>) -> String {
    format!(
        "<form method=\"post\" action=\"/logout\"><button class=\"button ghost\" type=\"submit\">{}</button></form>",
        l.text(keys::LOGOUT)
    )
}

fn avatar(name: &str) -> String {
    escape(&name.chars().next().unwrap_or('?').to_string())
}

pub(crate) fn card_page(l: Locale<'_>, headers: &HeaderMap, body: &str) -> String {
    page(
        l,
        headers,
        &format!(
            "<main class=\"center\"><div class=\"center-column\"><div class=\"card auth-card\">{body}</div><div class=\"center-footer\">{}</div></div></main>",
            segmented(l, headers)
        ),
    )
}

pub(crate) fn login(l: Locale<'_>, headers: &HeaderMap, dev: bool) -> String {
    let mut body = format!(
        "<div class=\"mark\" aria-hidden=\"true\">c</div><h1>{}</h1><a class=\"button primary large\" href=\"/auth/discord\">{}</a>",
        l.text(keys::TITLE),
        l.text(keys::LOGIN)
    );
    if dev {
        let _ = write!(
            body,
            "<form method=\"post\" action=\"/auth/dev\"><button class=\"button secondary large\" type=\"submit\">{}</button></form>",
            l.text(keys::DEV)
        );
    }
    card_page(l, headers, &body)
}

pub(crate) fn error(l: Locale<'_>, headers: &HeaderMap, message: &str) -> String {
    card_page(
        l,
        headers,
        &format!(
            "<p class=\"error\" role=\"alert\">{message}</p><a class=\"button secondary\" href=\"/\">{}</a>",
            l.text(keys::SERVERS)
        ),
    )
}

pub(crate) fn servers(l: Locale<'_>, headers: &HeaderMap, guilds: &[(String, String)]) -> String {
    let mut body = format!(
        "<main class=\"center\"><div class=\"server-column\"><h1>{}</h1><div class=\"server-list\">",
        l.text(keys::SERVERS)
    );
    for (id, name) in guilds {
        let _ = write!(
            body,
            "<a class=\"card server-link\" href=\"/g/{}\"><span class=\"avatar\">{}</span><span>{}</span><span aria-hidden=\"true\">›</span></a>",
            escape(id),
            avatar(name),
            escape(name)
        );
    }
    if guilds.is_empty() {
        let _ = write!(body, "<p class=\"card empty\">{}</p>", l.text(keys::EMPTY));
    }
    let _ = write!(
        body,
        "</div><div class=\"center-footer\">{}{}</div></div></main>",
        logout(l),
        segmented(l, headers)
    );
    page(l, headers, &body)
}

fn sidebar(l: Locale<'_>, headers: &HeaderMap, id: &str, name: &str) -> String {
    format!(
        "<aside class=\"sidebar\"><div class=\"server-name\"><span class=\"avatar\">{}</span><span>{}</span><span aria-hidden=\"true\">⌄</span></div><nav aria-label=\"{}\"><a href=\"/g/{}\" aria-current=\"page\"><svg aria-hidden=\"true\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M4 11l8-7 8 7v9H4z\"/></svg>{}</a></nav><div class=\"sidebar-footer\">{}{}</div></aside>",
        avatar(name),
        escape(name),
        l.text(keys::NAVIGATION),
        escape(id),
        l.text(keys::OVERVIEW),
        segmented(l, headers),
        logout(l)
    )
}

pub(crate) fn overview(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    modules: &serde_json::Map<String, Value>,
    effective: &Value,
    history: &[Value],
) -> String {
    let active = modules
        .keys()
        .filter(|k| effective["modules"][*k]["enabled"] == true)
        .count();
    let last = history
        .first()
        .and_then(|v| v["created_at_unix"].as_i64())
        .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
    let mut body = format!(
        "<div class=\"shell\">{}<div class=\"workspace\"><div class=\"breadcrumb\">{} / {}</div><main class=\"content\"><h1>{}</h1><section class=\"stats\" aria-label=\"{}\"><div class=\"card stat\"><div>{}</div><strong><span class=\"dot\"></span>{}</strong></div><div class=\"card stat\"><div>{}</div><strong>{active}<span class=\"muted\"> / {}</span></strong></div><div class=\"card stat\"><div>{}</div><strong>{last}</strong></div></section><section class=\"card panel\"><h2>{}</h2><div class=\"module-heading\"><span>{}</span><span>{}</span><span></span></div><ul class=\"rows\">",
        sidebar(l, headers, id, name),
        escape(name),
        l.text(keys::OVERVIEW),
        l.text(keys::OVERVIEW),
        l.text(keys::STATUS),
        l.text(keys::BOT),
        l.text(keys::CONNECTED),
        l.text(keys::ACTIVE),
        modules.len(),
        l.text(keys::LAST),
        l.text(keys::MODULES),
        l.text(keys::NAME),
        l.text(keys::STATUS)
    );
    for (module, def) in modules {
        if !super::routes::valid_module(module) {
            continue;
        }
        let title = def["title"].as_str().unwrap_or(module);
        let enabled = effective["modules"][module]["enabled"] == true;
        let _ = write!(
            body,
            "<li class=\"module-row\"><span class=\"module-name\">{}</span><span class=\"pill {}\">{}</span><form method=\"post\" action=\"/g/{}/modules/{}/toggle\"><input type=\"hidden\" name=\"enabled\" value=\"{}\"><button class=\"switch\" type=\"submit\" role=\"switch\" aria-checked=\"{enabled}\" aria-label=\"{}\"></button></form></li>",
            escape(title),
            if enabled { "enabled" } else { "disabled" },
            l.text(if enabled {
                keys::ENABLED
            } else {
                keys::DISABLED
            }),
            escape(id),
            escape(module),
            !enabled,
            escape(title)
        );
    }
    let _ = write!(
        body,
        "</ul></section><section class=\"card panel\"><h2>{}</h2><ul class=\"rows\">",
        l.text(keys::RECENT)
    );
    if history.is_empty() {
        let _ = write!(body, "<li class=\"empty\">{}</li>", l.text(keys::NONE));
    }
    for item in history.iter().take(4) {
        let when = item["created_at_unix"]
            .as_i64()
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
        let _ = write!(
            body,
            "<li class=\"history-row\"><span>{}</span><span>{when}</span></li>",
            escape(item["actor"].as_str().unwrap_or("—"))
        );
    }
    body.push_str("</ul></section></main></div></div>");
    page(l, headers, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn overview_switches_and_navigation() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let modules = json!({"on":{"title":"On"}, "off":{"title":"<Off>"}});
        let html = overview(
            catalogs.locale("ja"),
            &HeaderMap::new(),
            "42",
            "<script>",
            modules.as_object().unwrap(),
            &json!({"modules":{"on":{"enabled":true},"off":{"enabled":false}}}),
            &[],
        );
        for expected in [
            "action=\"/g/42/modules/on/toggle\"><input type=\"hidden\" name=\"enabled\" value=\"false\"",
            "action=\"/g/42/modules/off/toggle\"><input type=\"hidden\" name=\"enabled\" value=\"true\"",
            "role=\"switch\" aria-checked=\"true\" aria-label=\"On\"",
            "role=\"switch\" aria-checked=\"false\" aria-label=\"&lt;Off&gt;\"",
            "class=\"pill enabled\"",
            "class=\"pill disabled\"",
            "<nav aria-label=",
            "aria-current=\"page\"",
            "&lt;script&gt;",
            "変更はありません",
        ] {
            assert!(html.contains(expected), "{expected}");
        }
        for forbidden in ["href=\"#\"", "<script>", "style=", "<select", "<header>"] {
            assert!(!html.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn segments_and_card_pages() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let l = catalogs.locale("en");
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::COOKIE, "theme=dark".parse().unwrap());
        let pages = [
            login(l, &headers, true),
            servers(l, &headers, &[("42".into(), "<script>".into())]),
            error(l, &headers, "error"),
        ];
        for html in &pages {
            for expected in [
                "class=\"card",
                "data-theme=\"dark\"",
                "name=\"lang\" value=\"en\" aria-pressed=\"true\"",
                "name=\"lang\" value=\"ja\" aria-pressed=\"false\"",
                "name=\"theme\" value=\"dark\" aria-pressed=\"true\"",
                "name=\"theme\" value=\"auto\" aria-pressed=\"false\"",
                "name=\"theme\" value=\"light\" aria-pressed=\"false\"",
            ] {
                assert!(html.contains(expected), "{expected}");
            }
            assert_eq!(html.matches("aria-pressed=\"true\"").count(), 2);
            assert!(!html.contains("style="));
        }
        assert!(pages[1].contains("&lt;script&gt;"));
        assert!(pages[2].contains("role=\"alert\""));
    }

    #[test]
    fn stylesheet_constraints() {
        let css = include_str!("../assets/app.css");
        assert!(css.len() <= 10_240);
        for forbidden in [
            "http:",
            "https:",
            "margin-left",
            "margin-right",
            "padding-left",
            "padding-right",
            "border-left",
            "border-right",
            "left:",
            "right:",
            "transition:all",
        ] {
            assert!(!css.contains(forbidden), "{forbidden}");
        }
    }
}
