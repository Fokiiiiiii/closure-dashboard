use super::shell;
use crate::{
    html::escape,
    i18n::{Locale, keys},
    session,
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{collections::BTreeSet, fmt::Write};

fn diff<'a>(
    old: Option<&'a Value>,
    new: Option<&'a Value>,
    path: &str,
    out: &mut Vec<(String, Option<&'a Value>, Option<&'a Value>)>,
) {
    if old == new {
        return;
    }
    let a = old.and_then(Value::as_object);
    let b = new.and_then(Value::as_object);
    if (a.is_some() && (b.is_some() || new.is_none() || new == Some(&Value::Null)))
        || (b.is_some() && (old.is_none() || old == Some(&Value::Null)))
    {
        let keys: BTreeSet<_> = a
            .into_iter()
            .flat_map(|v| v.keys())
            .chain(b.into_iter().flat_map(|v| v.keys()))
            .collect();
        for key in keys {
            let next = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            diff(
                a.and_then(|v| v.get(key)),
                b.and_then(|v| v.get(key)),
                &next,
                out,
            );
        }
    } else {
        out.push((path.into(), old, new));
    }
}

fn describe_change(l: Locale<'_>, path: &str, old: Option<&Value>, new: Option<&Value>) -> String {
    let parts: Vec<_> = path.split('.').collect();
    match parts.as_slice() {
        ["modules", module, "enabled"] if new.and_then(Value::as_bool).is_some() => l.with_text(
            if new == Some(&Value::Bool(true)) {
                keys::HISTORY_ENABLED
            } else {
                keys::HISTORY_DISABLED
            },
            "module",
            module,
        ),
        ["modules", module, "config", field] => {
            let key = if new.is_none_or(Value::is_null) {
                keys::HISTORY_RESET
            } else if old.is_none_or(Value::is_null) {
                keys::HISTORY_ADD
            } else {
                keys::HISTORY_CONFIG
            };
            l.with_texts(key, &[("module", module), ("field", field)])
        }
        ["language" | "timezone" | "prefix"] => {
            let value = match new {
                Some(Value::String(s)) if s.chars().count() <= 20 => Some(s.clone()),
                Some(Value::Bool(_) | Value::Number(_)) => new
                    .map(Value::to_string)
                    .filter(|s| s.chars().count() <= 20),
                _ => None,
            };
            value.map_or_else(
                || l.text(keys::HISTORY_GENERIC),
                |value| {
                    l.with_text(
                        match path {
                            "language" => keys::HISTORY_LANGUAGE,
                            "timezone" => keys::HISTORY_TIMEZONE,
                            _ => keys::HISTORY_PREFIX,
                        },
                        "value",
                        &value,
                    )
                },
            )
        }
        ["log_channel"] => l.text(keys::HISTORY_LOG_CHANNEL),
        ["moderator_roles"] => l.text(keys::HISTORY_ROLES),
        ["preset", ..] => l.text(keys::HISTORY_PRESET),
        ["log_webhook", ..] => l.text(keys::HISTORY_WEBHOOK),
        ["success_emoji" | "error_emoji"] => l.text(keys::HISTORY_EMOJI),
        ["permissions", ..] => l.text(keys::HISTORY_PERMISSIONS),
        _ => l.text(keys::HISTORY_GENERIC),
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
            let mut descriptions = Vec::new();
            for (path, old, new) in &changes {
                let description = describe_change(l, path, *old, *new);
                if !descriptions.contains(&description) {
                    descriptions.push(description);
                }
            }
            for description in descriptions.iter().take(3) {
                let _ = write!(result, "<div>{description}</div>");
            }
            if descriptions.len() > 3 {
                let _ = write!(
                    result,
                    "<div>{}</div>",
                    l.number(keys::MORE, (descriptions.len() - 3) as u64)
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
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn descriptions_hide_paths_values_and_escape() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let l = catalogs.locale("ja");
        assert_eq!(
            describe_change(l, "modules.x.enabled", None, Some(&json!(true))),
            "x を有効にしました"
        );
        assert_eq!(
            describe_change(l, "modules.x.enabled", None, Some(&json!(false))),
            "x を無効にしました"
        );
        assert!(
            describe_change(l, "modules.<script>.config.foo", None, Some(&json!(1)))
                .contains("&lt;script&gt;")
        );
        assert!(
            describe_change(l, "modules.x.config.foo", Some(&json!(1)), None).contains("リセット")
        );
        assert!(
            describe_change(l, "modules.x.config.foo", Some(&json!(1)), Some(&json!(2)))
                .contains("変更")
        );
        for path in ["language", "timezone", "prefix"] {
            assert!(
                describe_change(l, path, None, Some(&json!("<script>"))).contains("&lt;script&gt;")
            );
            assert_eq!(
                describe_change(l, path, None, Some(&json!({"secret":123}))),
                "設定を変更しました"
            );
        }
        for path in [
            "log_channel",
            "moderator_roles",
            "preset.x",
            "log_webhook",
            "success_emoji",
            "error_emoji",
            "permissions.x",
            "unknown.path",
        ] {
            let s = describe_change(l, path, None, Some(&json!([123])));
            assert!(!s.contains(path) && !s.contains("123"));
        }
        let mut out = Vec::new();
        let new = json!({"modules":{"channels":{"enabled":true}}});
        diff(None, Some(&new), "", &mut out);
        assert_eq!(out[0].0, "modules.channels.enabled");
    }
    #[test]
    fn history_deduplicates_and_limits_sentences() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let items = json!([
            {"revision":2,"document":{"language":"ja","timezone":"UTC","prefix":"!","permissions":{"a":1,"b":2},"unknown":"x"}},
            {"revision":1,"document":{}}
        ]);
        let page = history(
            catalogs.locale("ja"),
            &HeaderMap::new(),
            "1",
            "test",
            &items,
        );
        assert!(page.contains("他 2 件"));
        assert_eq!(page.matches("権限を変更しました").count(), 1);
        assert!(!page.contains("permissions.a"));
        assert!(!page.contains("{&quot;"));
    }
    #[test]
    fn changes_are_nested_and_arrays_atomic() {
        let mut out = Vec::new();
        let old = json!({"a":{"x":1,"gone":2},"array":[1]});
        let new = json!({"a":{"x":2,"added":3},"array":[2]});
        diff(Some(&old), Some(&new), "", &mut out);
        assert_eq!(
            out.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["a.added", "a.gone", "a.x", "array"]
        );
        let mut same = Vec::new();
        let array = json!([1]);
        diff(Some(&array), Some(&array), "", &mut same);
        assert_eq!(same.len(), 0);
    }
}
