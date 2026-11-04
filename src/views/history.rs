use super::shell;
use crate::{
    html::escape,
    i18n::{Locale, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{collections::BTreeSet, fmt::Write};

fn diff(
    old: Option<&Value>,
    new: Option<&Value>,
    path: &str,
    out: &mut Vec<(String, String, String)>,
) {
    if old == new {
        return;
    }
    if let (Some(Value::Object(a)), Some(Value::Object(b))) = (old, new) {
        let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            let next = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            diff(a.get(key), b.get(key), &next, out);
        }
    } else {
        let format_value = |v: Option<&Value>| {
            let s = v.map_or_else(|| "—".into(), Value::to_string);
            if s.chars().count() > 40 {
                format!("{}…", s.chars().take(40).collect::<String>())
            } else {
                s
            }
        };
        out.push((path.into(), format_value(old), format_value(new)));
    }
}
pub(crate) fn history(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    history: &Value,
) -> String {
    let items = history.as_array().map_or(&[][..], Vec::as_slice);
    let mut body = format!(
        "<header class=\"page-heading\"><h1>{}</h1></header><section class=\"card panel\"><div class=\"table-scroll\"><table><thead><tr>",
        l.text(keys::HISTORY)
    );
    for key in [keys::REVISION, keys::CHANGE, keys::ACTOR, keys::TIME] {
        let _ = write!(body, "<th>{}</th>", l.text(key));
    }
    body.push_str("</tr></thead><tbody>");
    for (i, item) in items.iter().take(50).enumerate() {
        let mut changes = Vec::new();
        if let Some(previous) = items.get(i + 1) {
            diff(
                Some(&previous["document"]),
                Some(&item["document"]),
                "",
                &mut changes,
            );
        }
        let details = if i + 1 == items.len() {
            l.text(keys::INITIAL)
        } else if changes.is_empty() {
            l.text(keys::NO_CHANGE)
        } else {
            let mut result = String::new();
            for (path, old, new) in changes.iter().take(3) {
                let _ = write!(
                    result,
                    "<div><span class=\"mono\">{}</span> {} → {}</div>",
                    escape(path),
                    escape(old),
                    escape(new)
                );
            }
            if changes.len() > 3 {
                let _ = write!(
                    result,
                    "<div>{}</div>",
                    l.number(keys::MORE, (changes.len() - 3) as u64)
                );
            }
            result
        };
        let when = item["created_at_unix"]
            .as_i64()
            .map_or_else(|| "—".into(), |v| l.relative_time(v, session::now()));
        let _ = write!(
            body,
            "<tr><td>#{}</td><td>{details}</td><td class=\"mono\">{}</td><td>{when}</td></tr>",
            item["revision"].as_u64().unwrap_or(0),
            escape(item["actor"].as_str().unwrap_or("—"))
        );
    }
    body.push_str("</tbody></table></div>");
    if items.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", l.text(keys::NONE));
    }
    body.push_str("</section>");
    shell(l, headers, id, name, keys::HISTORY, &body, false)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn changes_are_nested_and_arrays_atomic() {
        let mut out = Vec::new();
        diff(
            Some(&json!({"a":{"x":1,"gone":2},"array":[1]})),
            Some(&json!({"a":{"x":2,"added":3},"array":[2]})),
            "",
            &mut out,
        );
        assert_eq!(
            out.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["a.added", "a.gone", "a.x", "array"]
        );
        let mut same = Vec::new();
        diff(Some(&json!([1])), Some(&json!([1])), "", &mut same);
        assert_eq!(same.len(), 0);
    }
}
