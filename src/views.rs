use std::fmt::Write;

use axum::http::HeaderMap;

use crate::{
    html::escape,
    i18n::{Locale, keys},
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

mod charts;
mod history;
mod overview;
mod permissions;

pub(crate) use history::history;
pub(crate) use overview::overview;
pub(crate) use permissions::permissions;

fn shell(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    current: &str,
    content: &str,
) -> String {
    let mut nav = String::new();
    for (path, key, icon) in [
        ("", keys::OVERVIEW, "M4 11l8-7 8 7v9H4z"),
        (
            "/history",
            keys::HISTORY,
            "M12 21a9 9 0 100-18 9 9 0 000 18z M12 7v5l3 2",
        ),
        (
            "/permissions",
            keys::PERMISSIONS,
            "M6 11h12v9H6z M8 11V8a4 4 0 118 0v3",
        ),
    ] {
        let active = if key == current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            nav,
            "<a href=\"/g/{}{path}\"{active}><svg aria-hidden=\"true\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{icon}\"/></svg>{}</a>",
            escape(id),
            l.text(key)
        );
    }
    page(
        l,
        headers,
        &format!(
            "<div class=\"shell\"><aside class=\"sidebar\"><div class=\"server-name\"><span class=\"avatar\">{}</span><span>{}</span><span aria-hidden=\"true\">⌄</span></div><nav aria-label=\"{}\">{nav}</nav><div class=\"sidebar-footer\">{}{}</div></aside><div class=\"workspace\"><div class=\"breadcrumb\">{} / {}</div><main class=\"content\">{content}</main></div></div>",
            avatar(name),
            escape(name),
            l.text(keys::NAVIGATION),
            segmented(l, headers),
            logout(l),
            escape(name),
            l.text(current)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};

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
        assert!(css.len() <= 16_384);
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
