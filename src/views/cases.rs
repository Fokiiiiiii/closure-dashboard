//! The pages of the cases of a server: a list to search, with the counts of the last days, and one
//! case, where a moderator who may edit changes its reason or hides it.

use super::{charts, heading, shell};
use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

/// What a search asked for, as it is put back in the form.
#[derive(Debug, Default, Clone)]
pub(crate) struct Filter {
    pub user: String,
    pub moderator: String,
    pub kind: String,
    pub state: String,
    pub source: String,
    pub text: String,
    pub hidden: bool,
}

/// The kinds and origins a search can pick, besides any.
pub(crate) const KINDS: [&str; 8] = [
    "warn", "note", "timeout", "kick", "ban", "softban", "vcmute", "unban",
];
pub(crate) const STATES: [&str; 3] = ["active", "expired", "revoked"];
pub(crate) const SOURCES: [&str; 7] = [
    "command",
    "button",
    "automod",
    "manual",
    "dashboard",
    "script",
    "appeal",
];

/// A message above a page: whether it went well, and what it says.
pub(crate) type Banner = Option<(bool, String)>;

pub(crate) struct CasesPage<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub filter: &'a Filter,
    pub list: &'a Value,
    pub summary: Option<&'a Value>,
    pub days: u16,
    pub may_edit: bool,
    /// The query that makes the same search, for the links to the next page and to the CSV.
    pub query: &'a str,
    pub banner: Banner,
}

fn kind_label(l: Locale<'_>, kind: &str) -> String {
    let index = charts::kind_index(kind);
    if index == 5 {
        l.find(&format!("ed-o-{kind}"))
            .unwrap_or_else(|| escape(kind))
    } else {
        l.text(charts::KINDS[index].1)
    }
}

fn state_label(l: Locale<'_>, state: &str) -> String {
    l.text(match state {
        "revoked" => keys::REVOKED,
        "expired" => keys::CASE_EXPIRED,
        _ => keys::CASE_ACTIVE,
    })
}

fn banner_html(banner: &Banner) -> String {
    banner.as_ref().map_or_else(String::new, |(good, text)| {
        let role = if *good { "status" } else { "alert" };
        format!("<p class=\"form-status\" role=\"{role}\">{text}</p>")
    })
}

fn id_text(id: &str) -> String {
    if id.is_empty() {
        "—".to_owned()
    } else {
        format!("<span class=\"mono\">{}</span>", escape(id))
    }
}

fn option(value: &str, label: &str, current: &str) -> String {
    format!(
        "<option value=\"{}\"{}>{label}</option>",
        escape(value),
        if value == current { " selected" } else { "" }
    )
}

fn select(name: &str, label: &str, any: &str, values: &[(&str, String)], current: &str) -> String {
    let mut options = option("", any, current);
    for (value, text) in values {
        options.push_str(&option(value, text, current));
    }
    format!(
        "<label class=\"ed-field\"><span class=\"ed-label\">{label}</span><select name=\"{name}\">{options}</select></label>"
    )
}

fn input(name: &str, label: &str, value: &str, max: u16) -> String {
    format!(
        "<label class=\"ed-field\"><span class=\"ed-label\">{label}</span><input name=\"{name}\" value=\"{}\" maxlength=\"{max}\" autocomplete=\"off\"></label>",
        escape(value)
    )
}

fn search_form(l: Locale<'_>, id: &str, filter: &Filter) -> String {
    let any = l.text(keys::CASE_ANY);
    let kinds: Vec<(&str, String)> = KINDS.iter().map(|k| (*k, kind_label(l, k))).collect();
    let states: Vec<(&str, String)> = STATES.iter().map(|s| (*s, state_label(l, s))).collect();
    let sources: Vec<(&str, String)> = SOURCES.iter().map(|s| (*s, escape(s))).collect();
    let mut form = format!(
        "<form class=\"ed-form\" method=\"get\" action=\"/g/{}/cases\">",
        escape(id)
    );
    form.push_str(&input("user", &l.text(keys::CASE_USER), &filter.user, 20));
    form.push_str(&input(
        "moderator",
        &l.text(keys::MODERATOR),
        &filter.moderator,
        20,
    ));
    form.push_str(&select(
        "kind",
        &l.text(keys::KIND),
        &any,
        &kinds,
        &filter.kind,
    ));
    form.push_str(&select(
        "state",
        &l.text(keys::CASE_STATE),
        &any,
        &states,
        &filter.state,
    ));
    form.push_str(&select(
        "source",
        &l.text(keys::CASE_SOURCE),
        &any,
        &sources,
        &filter.source,
    ));
    form.push_str(&input("text", &l.text(keys::CASE_WORD), &filter.text, 100));
    let _ = write!(
        form,
        "<label class=\"ed-field\"><span class=\"ed-label\">{}</span><input type=\"checkbox\" name=\"hidden\" value=\"1\"{}></label>",
        l.text(keys::CASE_HIDDEN),
        if filter.hidden { " checked" } else { "" }
    );
    let _ = write!(
        form,
        "<div class=\"head-actions\"><button class=\"button primary\" type=\"submit\">{}</button></div></form>",
        l.text(keys::CASE_SEARCH)
    );
    form
}

fn people(l: Locale<'_>, heading_key: &str, rows: &Value) -> String {
    let rows = rows.as_array().map_or(&[][..], Vec::as_slice);
    if rows.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "<section class=\"card panel\"><div class=\"panel-heading\"><h2>{}</h2></div><div class=\"table-scroll\"><table><tbody>",
        l.text(heading_key)
    );
    for row in rows {
        let _ = write!(
            out,
            "<tr><td>{}</td><td>{}</td></tr>",
            id_text(row["id"].as_str().unwrap_or("")),
            group_digits(row["count"].as_u64().unwrap_or(0))
        );
    }
    out.push_str("</tbody></table></div></section>");
    out
}

fn summary_html(l: Locale<'_>, summary: &Value, days: u16) -> String {
    let state = |name: &str| group_digits(summary["by_state"][name].as_u64().unwrap_or(0));
    let mut out = format!(
        "<section class=\"stats\" aria-label=\"{}\">",
        l.text(keys::SUMMARY)
    );
    for (label, value, sub) in [
        (
            l.text(keys::CASE_TOTAL),
            group_digits(summary["total"].as_u64().unwrap_or(0)),
            l.number(keys::CASE_LAST_DAYS, days.into()),
        ),
        (l.text(keys::CASE_ACTIVE), state("active"), String::new()),
        (l.text(keys::CASE_EXPIRED), state("expired"), String::new()),
        (l.text(keys::REVOKED), state("revoked"), String::new()),
    ] {
        let _ = write!(
            out,
            "<div class=\"card stat\"><div>{label}</div><strong>{value}</strong>{}</div>",
            if sub.is_empty() {
                String::new()
            } else {
                format!("<small>{sub}</small>")
            }
        );
    }
    out.push_str("</section>");
    if let Some(kinds) = summary["by_kind"].as_object() {
        out.push_str("<p class=\"list-caption\">");
        for (kind, count) in kinds {
            let _ = write!(
                out,
                "<span class=\"pill\">{} {}</span> ",
                kind_label(l, kind),
                group_digits(count.as_u64().unwrap_or(0))
            );
        }
        out.push_str("</p>");
    }
    out.push_str(&people(
        l,
        keys::CASE_TOP_MODERATORS,
        &summary["moderators"],
    ));
    out.push_str(&people(l, keys::CASE_TOP_TARGETS, &summary["targets"]));
    out
}

fn note_form(l: Locale<'_>, id: &str) -> String {
    let text = format!(
        "<label class=\"ed-field\"><span class=\"ed-label\">{}</span><textarea name=\"reason\" maxlength=\"512\" rows=\"3\" required></textarea></label>",
        l.text(keys::CASE_NOTE_TEXT)
    );
    format!(
        "<section class=\"card panel\"><div class=\"panel-heading\"><h2>{}</h2></div><form class=\"ed-form\" method=\"post\" action=\"/g/{}/cases/note\">{}{text}<div class=\"head-actions\"><button class=\"button primary\" type=\"submit\">{}</button></div></form></section>",
        l.text(keys::CASE_NOTE_ADD),
        escape(id),
        input("target", &l.text(keys::CASE_USER), "", 20),
        l.text(keys::SAVE)
    )
}

pub(crate) fn cases(l: Locale<'_>, headers: &HeaderMap, page: &CasesPage<'_>) -> String {
    let id = escape(page.id);
    let items = page.list["cases"].as_array().map_or(&[][..], Vec::as_slice);
    let mut body = format!("<header class=\"page-heading\">{}", heading(l, keys::CASES));
    if page.may_edit {
        let _ = write!(
            body,
            "<div class=\"head-actions\"><a class=\"button secondary\" href=\"/g/{id}/cases/export?{}\">{}</a></div>",
            escape(page.query),
            l.text(keys::CASE_EXPORT)
        );
    }
    body.push_str("</header>");
    body.push_str(&banner_html(&page.banner));
    if let Some(summary) = page.summary {
        body.push_str(&summary_html(l, summary, page.days));
    }
    let _ = write!(
        body,
        "<section class=\"card panel\">{}</section>",
        search_form(l, page.id, page.filter)
    );
    let _ = write!(
        body,
        "<section class=\"card panel\"><div class=\"table-scroll\"><table><thead><tr>"
    );
    for key in [
        keys::NUMBER,
        keys::KIND,
        keys::TARGET,
        keys::MODERATOR,
        keys::CASE_STATE,
        keys::REASON,
        keys::TIME,
    ] {
        let _ = write!(body, "<th>{}</th>", l.text(key));
    }
    body.push_str("</tr></thead><tbody>");
    for item in items {
        let number = item["number"].as_i64().unwrap_or(0);
        let state = item["state"].as_str().unwrap_or("active");
        let kind = item["kind"].as_str().unwrap_or("");
        let reason = item["reason"].as_str().unwrap_or("—");
        let when = item["created_at"]
            .as_i64()
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
        let _ = write!(
            body,
            "<tr{}><td><a href=\"/g/{id}/cases/{number}\">#{number}</a></td><td><span class=\"pill\">{}</span>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"reason\" title=\"{}\">{}</td><td>{when}</td></tr>",
            if state == "revoked" {
                " class=\"muted\""
            } else {
                ""
            },
            kind_label(l, kind),
            if item["hidden"] == true {
                format!(" <span class=\"pill\">{}</span>", l.text(keys::CASE_HIDDEN))
            } else {
                String::new()
            },
            id_text(item["target"].as_str().unwrap_or("")),
            id_text(item["moderator"].as_str().unwrap_or("")),
            state_label(l, state),
            escape(reason),
            escape(reason)
        );
    }
    body.push_str("</tbody></table></div>");
    if items.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", l.text(keys::CASES_NONE));
    }
    if let Some(before) = page.list["next_before"].as_i64() {
        let query = if page.query.is_empty() {
            format!("before={before}")
        } else {
            format!("{}&before={before}", page.query)
        };
        let _ = write!(
            body,
            "<p><a class=\"button secondary\" href=\"/g/{id}/cases?{}\">{}</a></p>",
            escape(&query),
            l.text(keys::OLDER)
        );
    }
    body.push_str("</section>");
    if page.may_edit {
        body.push_str(&note_form(l, page.id));
    }
    shell(l, headers, page.id, page.name, keys::CASES, &body, false)
}

/// One case: all it holds, and, for a moderator who may edit, the reason and whether it is hidden.
pub(crate) fn case(
    l: Locale<'_>,
    headers: &HeaderMap,
    (id, name): (&str, &str),
    case: &Value,
    may_edit: bool,
    banner: &Banner,
) -> String {
    let id_html = escape(id);
    let number = case["number"].as_i64().unwrap_or(0);
    let state = case["state"].as_str().unwrap_or("active");
    let kind = case["kind"].as_str().unwrap_or("");
    let when = |key: &str| {
        case[key]
            .as_i64()
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()))
    };
    let mut body = format!(
        "<header class=\"page-heading\"><h1>{}</h1><div class=\"head-actions\"><a class=\"button secondary\" href=\"/g/{id_html}/cases\">{}</a></div></header>",
        l.number(keys::CASE_TITLE, number.unsigned_abs()),
        l.text(keys::CASE_BACK)
    );
    body.push_str(&banner_html(banner));
    body.push_str("<section class=\"card panel\"><div class=\"table-scroll\"><table><tbody>");
    let mut row = |label: String, value: String| {
        let _ = write!(body, "<tr><th>{label}</th><td>{value}</td></tr>");
    };
    row(
        l.text(keys::KIND),
        format!("<span class=\"pill\">{}</span>", kind_label(l, kind)),
    );
    row(l.text(keys::CASE_STATE), state_label(l, state));
    row(
        l.text(keys::TARGET),
        id_text(case["target"].as_str().unwrap_or("")),
    );
    row(
        l.text(keys::MODERATOR),
        id_text(case["moderator"].as_str().unwrap_or("")),
    );
    row(
        l.text(keys::CASE_SOURCE),
        escape(case["source"].as_str().unwrap_or("")),
    );
    row(
        l.text(keys::CASE_MODULE),
        escape(case["module"].as_str().unwrap_or("")),
    );
    row(l.text(keys::TIME), when("created_at"));
    if !case["expires_at"].is_null() {
        row(l.text(keys::CASE_EXPIRES), when("expires_at"));
    }
    if let Some(revoked) = case["revoked"].as_object() {
        let by = revoked.get("by").and_then(Value::as_str).unwrap_or("");
        let at = revoked
            .get("at")
            .and_then(Value::as_i64)
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
        let why = revoked.get("reason").and_then(Value::as_str).unwrap_or("");
        row(
            l.text(keys::CASE_REVOKED_BY),
            format!("{} {at} {}", id_text(by), escape(why)),
        );
    }
    row(
        l.text(keys::REASON),
        escape(case["reason"].as_str().unwrap_or("—")),
    );
    if case["hidden"] == true {
        row(l.text(keys::CASE_HIDDEN), l.text(keys::ENABLED));
    }
    body.push_str("</tbody></table></div></section>");
    if may_edit {
        let reason = case["reason"].as_str().unwrap_or("");
        let _ = write!(
            body,
            "<section class=\"card panel\"><form class=\"ed-form\" method=\"post\" action=\"/g/{id_html}/cases/{number}\"><input type=\"hidden\" name=\"op\" value=\"reason\"><label class=\"ed-field\"><span class=\"ed-label\">{}</span><textarea name=\"reason\" maxlength=\"512\" rows=\"3\" required>{}</textarea></label><div class=\"head-actions\"><button class=\"button primary\" type=\"submit\">{}</button></div></form><form class=\"ed-form\" method=\"post\" action=\"/g/{id_html}/cases/{number}\"><input type=\"hidden\" name=\"op\" value=\"{}\"><div class=\"head-actions\"><button class=\"button secondary\" type=\"submit\">{}</button></div></form></section>",
            l.text(keys::CASE_REASON_EDIT),
            escape(reason),
            l.text(keys::SAVE),
            if case["hidden"] == true {
                "show"
            } else {
                "hide"
            },
            l.text(if case["hidden"] == true {
                keys::CASE_SHOW
            } else {
                keys::CASE_HIDE
            })
        );
    }
    shell(l, headers, id, name, keys::CASES, &body, true)
}
