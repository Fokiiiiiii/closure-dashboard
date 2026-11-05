use std::fmt::Write;

use axum::http::HeaderMap;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

pub(crate) fn page(l: Locale<'_>, _headers: &HeaderMap, content: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\" dir=\"{}\" data-theme=\"auto\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"/app.css\"></head><body>{content}</body></html>",
        escape(l.tag()),
        escape(l.dir()),
        l.text(keys::TITLE)
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
            "<main class=\"center\"><div class=\"center-column\"><div class=\"card auth-card\">{body}</div></div></main>"
        ),
    )
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
        "</div><div class=\"center-footer\">{}</div></div></main>",
        logout(l)
    );
    page(l, headers, &body)
}

mod charts;
mod editor;
mod form;
mod history;
mod modules;
mod overview;
mod permissions;
mod settings;

/// The path of a page's line icon, shared by the sidebar and the page heading.
fn page_icon(key: &str) -> &'static str {
    match key {
        keys::OVERVIEW => "M4 11l8-7 8 7v9H4z",
        keys::MODULES => "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z",
        keys::SERVER_SETTINGS => {
            "M12 3v3 M12 18v3 M3 12h3 M18 12h3 M5.6 5.6l2.1 2.1 M16.3 16.3l2.1 2.1 M18.4 5.6l-2.1 2.1 M7.7 16.3l-2.1 2.1 M12 8a4 4 0 100 8 4 4 0 000-8z"
        }
        keys::HISTORY => "M12 21a9 9 0 100-18 9 9 0 000 18z M12 7v5l3 2",
        keys::PERMISSIONS => "M6 11h12v9H6z M8 11V8a4 4 0 118 0v3",
        _ => "",
    }
}

/// A line icon, drawn in the current text colour.
fn icon(path: &str, size: u8, class: &str) -> String {
    format!(
        "<svg class=\"{class}\" aria-hidden=\"true\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{path}\"/></svg>"
    )
}

/// The heading of a page: its icon in front of its name.
fn heading(l: Locale<'_>, key: &str) -> String {
    format!(
        "<h1 class=\"with-icon\">{}{}</h1>",
        icon(page_icon(key), 20, "page-icon"),
        l.text(key)
    )
}

/// The fill of a role's dot: its Discord colour, or the text colour for a role that has none.
fn role_fill(color: u64) -> String {
    if color == 0 {
        "currentColor".into()
    } else {
        format!("#{color:06x}")
    }
}

fn first_sentence(description: &str) -> String {
    let description = description.trim_start();
    let end = description.find('。').map(|i| i + '。'.len_utf8());
    let dot = description.find(". ").map(|i| i + 1);
    let sentence = &description[..end
        .into_iter()
        .chain(dot)
        .min()
        .unwrap_or(description.len())];
    if sentence.chars().count() > 80 {
        format!("{}…", sentence.chars().take(80).collect::<String>())
    } else {
        sentence.to_owned()
    }
}

pub(crate) use editor::{
    Ctx, Item, collection_page, datalist_html, editor_field, editor_node, editor_page, object_card,
    rule_body, rule_summary,
};
pub(crate) use form::{FormPage, form_page, part_path};
pub(crate) use history::history;
pub(crate) use modules::{feature_toggle, modules_page, part_name};
pub(crate) use overview::overview;
pub(crate) use permissions::permissions;
pub(crate) use settings::{SettingsPage, settings_page};

pub(crate) fn shell(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    current: &str,
    content: &str,
    narrow: bool,
) -> String {
    let mut nav = String::new();
    for (path, key) in [
        ("", keys::OVERVIEW),
        ("/modules", keys::MODULES),
        ("/settings", keys::SERVER_SETTINGS),
        ("/history", keys::HISTORY),
        ("/permissions", keys::PERMISSIONS),
    ] {
        let active = if key == current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            nav,
            "<a href=\"/g/{}{path}\"{active}>{}{}</a>",
            escape(id),
            icon(page_icon(key), 16, "nav-icon"),
            l.text(key)
        );
    }
    page(
        l,
        headers,
        &format!(
            "<div class=\"shell\"><aside class=\"sidebar\"><div class=\"server-name\"><span class=\"avatar\">{}</span><span>{}</span></div><nav aria-label=\"{}\">{nav}</nav><div class=\"sidebar-footer\">{}</div></aside><div class=\"workspace\"><main class=\"content{}\">{content}</main></div></div>",
            avatar(name),
            escape(name),
            l.text(keys::NAVIGATION),
            logout(l),
            if narrow { " narrow" } else { "" }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};

    #[test]
    fn sentence_boundaries_and_length() {
        assert_eq!(first_sentence("  一文。 次です。"), "一文。");
        assert_eq!(first_sentence("  First. Second."), "First.");
        assert_eq!(first_sentence("<script> hello"), "<script> hello");
        assert_eq!(
            first_sentence(&"あ".repeat(81)),
            format!("{}…", "あ".repeat(80))
        );
    }

    #[test]
    fn card_pages_have_no_preference_controls() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let l = catalogs.locale("en");
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::COOKIE, "theme=dark".parse().unwrap());
        let pages = [
            servers(l, &headers, &[("42".into(), "<script>".into())]),
            error(l, &headers, "error"),
            shell(l, &headers, "42", "server", keys::MODULES, "content", false),
        ];
        for html in &pages {
            assert!(html.contains("data-theme=\"auto\""));
            for forbidden in [
                "name=\"lang\"",
                "name=\"theme\"",
                "aria-label=\"Language\"",
                "aria-label=\"Theme\"",
                "style=",
            ] {
                assert!(!html.contains(forbidden), "{forbidden}");
            }
        }
        assert!(pages[0].contains("&lt;script&gt;"));
        assert!(pages[1].contains("role=\"alert\""));
        assert!(pages[2].contains("action=\"/logout\""));
        assert!(!pages[2].contains("breadcrumb"));
        assert!(!pages[2].contains('⌄'));
    }

    #[test]
    fn stylesheet_constraints() {
        let css = include_str!("../assets/app.css");
        assert!(css.len() <= 24_576);
        for required in [
            ".module-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:12px",
            "-webkit-line-clamp:3",
            ".role-option:has(input:checked)",
            "max-inline-size:1200px; margin-inline:auto",
            ".content.narrow { max-inline-size:760px",
            ".form-segments",
            "@media (prefers-color-scheme:dark)",
        ] {
            assert!(css.contains(required), "{required}");
        }
        assert!(!css.contains(".segments"));
        // Tiles size to their content: a square would leave most of each one empty.
        assert!(!css.contains("aspect-ratio"));
        for unused in [
            ".breadcrumb",
            ".form-breadcrumb",
            ".chart-note",
            ".chart-empty",
            ".pill.enabled",
            ".pill.disabled",
            ".settings-row small",
            ".panel-heading span",
        ] {
            assert!(!css.contains(unused), "{unused}");
        }
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
        for (light, dark) in [
            ("#2a78d6", "#3987e5"),
            ("#eb6834", "#d95926"),
            ("#1baf7a", "#199e70"),
            ("#eda100", "#c98500"),
            ("#e87ba4", "#d55181"),
        ] {
            assert!(css.contains(light) && css.contains(dark));
        }
    }
}
