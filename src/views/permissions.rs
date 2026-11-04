use super::shell;
use crate::{
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

pub(crate) fn permissions(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    effective: &Value,
    directory: &Value,
) -> String {
    let mut rows: Vec<(String, Value, &str)> = Vec::new();
    if let Some(roles) = effective["moderator_roles"].as_array() {
        for role in roles {
            if let Some(id) = role["id"]
                .as_str()
                .or_else(|| role["role_id"].as_str())
                .or_else(|| role.as_str())
            {
                rows.push((
                    id.into(),
                    role.get("level").cloned().unwrap_or(Value::Null),
                    "moderator_roles",
                ));
            }
        }
    }
    if let Some(roles) = effective["permissions"]["roles"].as_object() {
        for (id, level) in roles {
            rows.push((id.clone(), level.clone(), "permissions.roles"));
        }
    }
    let mut body = format!(
        "<header class=\"page-heading\"><h1>{}</h1></header><section class=\"card panel permission-panel\"><h2>{}</h2>",
        l.text(keys::PERMISSIONS),
        l.text(keys::ROLES)
    );
    if rows.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", l.text(keys::ROLE_EMPTY));
    } else {
        body.push_str("<div class=\"table-scroll\"><table><thead><tr>");
        for key in [keys::NAME, keys::LEVEL, keys::VALUE, keys::SOURCE] {
            let _ = write!(body, "<th>{}</th>", l.text(key));
        }
        body.push_str("</tr></thead><tbody>");
        for (role_id, level, source) in rows {
            let role = directory["roles"]
                .as_array()
                .and_then(|a| a.iter().find(|r| r["id"].as_str() == Some(&role_id)));
            let role_name = role.and_then(|r| r["name"].as_str()).unwrap_or(&role_id);
            let color = role
                .and_then(|r| r["color"].as_u64())
                .unwrap_or(0)
                .min(0xff_ffff);
            let level = if level.is_null() {
                "—".into()
            } else {
                level
                    .as_str()
                    .map_or_else(|| level.to_string(), str::to_owned)
            };
            let width = level.parse::<u64>().unwrap_or(0).min(100);
            let _ = write!(
                body,
                "<tr><td><svg class=\"role-dot\" viewBox=\"0 0 10 10\" aria-hidden=\"true\"><circle cx=\"5\" cy=\"5\" r=\"5\" fill=\"#{color:06x}\"/></svg> {}</td><td><svg class=\"level-bar\" viewBox=\"0 0 100 6\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><rect class=\"bar-background\" width=\"100\" height=\"6\" rx=\"3\"/><rect class=\"bar-fill\" width=\"{width}\" height=\"6\" rx=\"3\"/></svg></td><td>{}</td><td class=\"mono\">{source}</td></tr>",
                escape(role_name),
                escape(&level)
            );
        }
        body.push_str("</tbody></table></div>");
    }
    body.push_str("</section>");
    let mut cards = String::new();
    for (key, value) in [
        (keys::USER_ASSIGNMENTS, &effective["permissions"]["users"]),
        (keys::KEY_OVERRIDES, &effective["permissions"]["keys"]),
    ] {
        let count = value.as_object().map_or(0, serde_json::Map::len);
        if count == 0 {
            continue;
        }
        let _ = write!(
            cards,
            "<div class=\"card stat\"><div>{}</div><strong>{}</strong></div>",
            l.text(key),
            l.number(keys::COUNT, count as u64)
        );
    }
    if !cards.is_empty() {
        let _ = write!(
            body,
            "<section class=\"assignment-cards\">{cards}</section>"
        );
    }
    shell(l, headers, id, name, keys::PERMISSIONS, &body, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;

    #[test]
    fn assignment_cards_only_show_nonzero_counts() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        for (users, keys, expected) in [(0, 0, 0), (1, 0, 1), (0, 1, 1), (1, 1, 2)] {
            let data = json!({"permissions": {
                "users": if users == 1 { json!({"a": 1}) } else { json!({}) },
                "keys": if keys == 1 { json!({"b": 1}) } else { json!({}) }
            }});
            let html = permissions(
                catalogs.locale("ja"),
                &HeaderMap::new(),
                "42",
                "guild",
                &data,
                &json!({}),
            );
            assert_eq!(html.matches("<div class=\"card stat\">").count(), expected);
            assert_eq!(html.contains("assignment-cards"), expected > 0);
            assert!(!html.contains("class=\"pill\""));
        }
    }
}
