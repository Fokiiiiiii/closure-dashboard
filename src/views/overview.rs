use super::{charts, shell};
use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

#[allow(clippy::too_many_lines)]
pub(crate) fn overview(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    days: u8,
    stats: Option<&Value>,
) -> String {
    let mut body = format!(
        "<header class=\"page-heading\">{}<div class=\"range\" role=\"group\" aria-label=\"{}\">",
        super::heading(l, keys::OVERVIEW),
        l.text(keys::PERIOD)
    );
    for n in [7, 14, 30] {
        let current = if n == days {
            " aria-current=\"true\""
        } else {
            ""
        };
        let _ = write!(
            body,
            "<a href=\"/g/{}?days={n}\"{current}>{}</a>",
            escape(id),
            l.number(keys::DAYS, n.into())
        );
    }
    body.push_str("</div></header>");
    let Some(s) = stats else {
        let _ = write!(
            body,
            "<section class=\"card empty\">{}</section>",
            l.text(keys::STATS_FAILED)
        );
        return shell(l, headers, id, name, keys::OVERVIEW, &body, false);
    };
    let members = &s["members"];
    let messages = &s["messages"];
    let cases = &s["cases"];
    let joins = members["joins"].as_u64().unwrap_or(0);
    let leaves = members["leaves"].as_u64().unwrap_or(0);
    let message_total = messages["total"].as_u64().unwrap_or(0);
    let net = i128::from(joins) - i128::from(leaves);
    let net = if members["by_day"].as_array().is_some_and(|a| {
        a.iter()
            .any(|d| !d["joins"].is_null() || !d["leaves"].is_null())
    }) {
        format!("{net:+}")
    } else {
        String::new()
    };
    let tiles = [
        (
            keys::MEMBERS,
            members["total"]
                .as_u64()
                .map_or_else(|| "—".into(), group_digits),
            net,
        ),
        (keys::MESSAGES, group_digits(message_total), String::new()),
        (
            keys::MODERATION,
            group_digits(cases["total"].as_u64().unwrap_or(0)),
            String::new(),
        ),
    ];
    let _ = write!(
        body,
        "<section class=\"stats\" aria-label=\"{}\">",
        l.text(keys::SUMMARY)
    );
    for (key, value, sub) in tiles {
        let _ = write!(
            body,
            "<div class=\"card stat\"><div>{}</div><strong>{value}</strong>{}</div>",
            l.text(key),
            if sub.is_empty() {
                String::new()
            } else {
                format!("<small>{sub}</small>")
            }
        );
    }
    let _ = write!(
        body,
        "</section><section class=\"charts\" aria-label=\"{}\">",
        l.text(keys::CHARTS)
    );
    body.push_str(&charts::chart(
        l,
        keys::JOIN_LEAVE,
        &members["by_day"],
        &[("joins", keys::JOINS), ("leaves", keys::LEAVES)],
        false,
    ));
    body.push_str(&charts::chart(
        l,
        keys::MESSAGES,
        &messages["by_day"],
        &[("count", keys::MESSAGES)],
        false,
    ));
    body.push_str(&charts::chart(
        l,
        keys::MODERATION,
        &cases["by_day"],
        &charts::KINDS,
        true,
    ));
    body.push_str("</section>");
    let recent = cases["recent"].as_array().map_or(&[][..], Vec::as_slice);
    let _ = write!(
        body,
        "<section class=\"card panel\"><div class=\"panel-heading\"><h2>{}</h2></div><div class=\"table-scroll\"><table><thead><tr>",
        l.text(keys::LOG)
    );
    for key in [
        keys::NUMBER,
        keys::KIND,
        keys::TARGET,
        keys::MODERATOR,
        keys::REASON,
        keys::TIME,
    ] {
        let _ = write!(body, "<th>{}</th>", l.text(key));
    }
    body.push_str("</tr></thead><tbody>");
    for item in recent.iter().take(10) {
        let kind = item["kind"].as_str().unwrap_or("");
        let index = charts::kind_index(kind);
        let reason = item["reason"].as_str().unwrap_or("—");
        let when = item["created_at_unix"]
            .as_i64()
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
        let revoked = item["revoked"] == true;
        let _ = write!(
            body,
            "<tr{}><td>#{}</td><td><span class=\"pill\"><i class=\"legend-dot series-{}\"></i>{}</span>{}</td><td>{}</td><td>{}</td><td class=\"reason\" title=\"{}\">{}</td><td>{when}</td></tr>",
            if revoked { " class=\"muted\"" } else { "" },
            item["number"].as_u64().unwrap_or(0),
            index + 1,
            if index == 5 {
                l.find(&format!("ed-o-{kind}"))
                    .unwrap_or_else(|| escape(kind))
            } else {
                l.text(charts::KINDS[index].1)
            },
            if revoked {
                format!("<span class=\"pill\">{}</span>", l.text(keys::REVOKED))
            } else {
                String::new()
            },
            super::short_id(item["target"].as_str().unwrap_or("")),
            super::short_id(item["moderator"].as_str().unwrap_or("")),
            escape(reason),
            escape(reason)
        );
    }
    body.push_str("</tbody></table></div>");
    if recent.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", l.text(keys::NO_DATA));
    }
    body.push_str("</section>");
    shell(l, headers, id, name, keys::OVERVIEW, &body, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn net_change_respects_sign_and_missing_days() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        for (joins, leaves, expected) in [(5, 2, "+3"), (2, 5, "-3")] {
            let data = json!({"members":{"joins":joins,"leaves":leaves,"by_day":[{"joins":joins,"leaves":leaves}]}});
            let html = overview(
                catalogs.locale("ja"),
                &HeaderMap::new(),
                "42",
                "test",
                7,
                Some(&data),
            );
            assert!(html.contains(expected));
        }
        let data =
            json!({"members":{"joins":0,"leaves":0,"by_day":[{"joins":null,"leaves":null}]}});
        let html = overview(
            catalogs.locale("ja"),
            &HeaderMap::new(),
            "42",
            "test",
            7,
            Some(&data),
        );
        assert!(!html.contains("+0"));
        assert_eq!(html.matches("class=\"card stat\"").count(), 3);
        assert!(!html.contains("<div class=\"card stat\"><div>参加</div>"));
        assert!(!html.contains("<div class=\"card stat\"><div>脱退</div>"));
        assert!(html.contains("参加") && html.contains("脱退"));
        for forbidden in [
            "chart-note",
            "chart-empty",
            "日平均",
            "新しい順",
            "保存されます",
        ] {
            assert!(!html.contains(forbidden), "{forbidden}");
        }
        assert!(html.contains("class=\"missing\""));
        assert!(html.contains("<details>"));
    }
}
