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
        "<header class=\"page-heading\"><h1>{}</h1><div class=\"segments range\" role=\"group\" aria-label=\"{}\">",
        l.text(keys::OVERVIEW),
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
        return shell(l, headers, id, name, keys::OVERVIEW, &body);
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
    let avg = l.with_text(
        keys::AVERAGE,
        "n",
        &group_digits(message_total / u64::from(days)),
    );
    let tiles = [
        (
            keys::MEMBERS,
            members["total"]
                .as_u64()
                .map_or_else(|| "—".into(), group_digits),
            net,
        ),
        (keys::JOINS, group_digits(joins), String::new()),
        (keys::LEAVES, group_digits(leaves), String::new()),
        (keys::MESSAGES, group_digits(message_total), avg),
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
            "<div class=\"card stat\"><div>{}</div><strong>{value}</strong><small>{sub}</small></div>",
            l.text(key)
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
        "",
    ));
    let retention = messages["retention_days"].as_u64().unwrap_or(0);
    let note = if u64::from(days) > retention {
        l.number(keys::RETENTION, retention)
    } else {
        String::new()
    };
    body.push_str(&charts::chart(
        l,
        keys::MESSAGES,
        &messages["by_day"],
        &[("count", keys::MESSAGES)],
        false,
        &note,
    ));
    body.push_str(&charts::chart(
        l,
        keys::MODERATION,
        &cases["by_day"],
        &charts::KINDS,
        true,
        "",
    ));
    body.push_str("</section>");
    let recent = cases["recent"].as_array().map_or(&[][..], Vec::as_slice);
    let _ = write!(
        body,
        "<section class=\"card panel\"><div class=\"panel-heading\"><h2>{}</h2><span>{}</span></div><div class=\"table-scroll\"><table><thead><tr>",
        l.text(keys::LOG),
        l.number(keys::NEWEST, recent.len() as u64)
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
            "<tr{}><td>#{}</td><td><span class=\"pill\"><i class=\"legend-dot series-{}\"></i>{}</span>{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td class=\"reason\" title=\"{}\">{}</td><td>{when}</td></tr>",
            if revoked { " class=\"muted\"" } else { "" },
            item["number"].as_u64().unwrap_or(0),
            index + 1,
            if index == 5 {
                escape(kind)
            } else {
                l.text(charts::KINDS[index].1)
            },
            if revoked {
                format!("<span class=\"pill\">{}</span>", l.text(keys::REVOKED))
            } else {
                String::new()
            },
            escape(item["target"].as_str().unwrap_or("—")),
            escape(item["moderator"].as_str().unwrap_or("—")),
            escape(reason),
            escape(reason)
        );
    }
    body.push_str("</tbody></table></div>");
    if recent.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", l.text(keys::NO_DATA));
    }
    body.push_str("</section>");
    shell(l, headers, id, name, keys::OVERVIEW, &body)
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
    }
}
