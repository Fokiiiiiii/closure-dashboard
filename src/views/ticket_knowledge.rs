use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

fn valid_knowledge_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn list_values(article: &Value, field: &str) -> String {
    article[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .collect::<Vec<_>>()
        .join("\n")
}

fn custom_embed_text(article: &Value) -> String {
    article["custom_embed"]
        .as_object()
        .and_then(|_| serde_json::to_string_pretty(&article["custom_embed"]).ok())
        .unwrap_or_default()
}

fn query_string(cursor: &str, query: &str) -> String {
    let mut parameters = vec![("cursor", cursor.to_owned())];
    if !query.is_empty() {
        parameters.push(("q", query.to_owned()));
    }
    serde_urlencoded::to_string(parameters).unwrap_or_default()
}

pub(crate) fn ticket_knowledge_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    state: &Value,
    query: &str,
    cursor: Option<&str>,
    saved: bool,
    nonce: &str,
) -> String {
    let id_html = escape(id);
    let action_url = format!("/g/{id_html}/tickets/settings/knowledge/actions");
    let page_url = format!("/g/{id_html}/tickets/settings/knowledge");
    let search_field = if query.is_empty() {
        String::new()
    } else {
        format!(
            r#"<input type="hidden" name="q" value="{}">"#,
            escape(query)
        )
    };
    let cursor_field = cursor
        .filter(|cursor| valid_knowledge_id(cursor))
        .map(|cursor| {
            format!(
                r#"<input type="hidden" name="cursor" value="{}">"#,
                escape(cursor)
            )
        })
        .unwrap_or_default();

    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets/settings">{}</a></div></header>"#,
        l.text(keys::TICKET_KNOWLEDGE_TITLE),
        l.text(keys::TICKET_KNOWLEDGE_BACK)
    );
    if saved {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_KNOWLEDGE_SAVED)
        );
    }
    let _ = write!(
        body,
        r#"<section class="card settings-card"><form method="get" action="{page_url}"><label class="field"><span>{}</span><input type="search" name="q" value="{}"></label><button class="button secondary">{}</button></form></section>"#,
        l.text(keys::TICKET_KNOWLEDGE_SEARCH),
        escape(query),
        l.text(keys::TICKET_KNOWLEDGE_SEARCH_SUBMIT)
    );

    let create_key = format!("{nonce}_create");
    let _ = write!(
        body,
        r#"<section class="card settings-card"><div class="panel-heading"><h2>{}</h2></div><form method="post" action="{action_url}"><input type="hidden" name="etag" value="&quot;0&quot;"><input type="hidden" name="key" value="{}">{search_field}<label class="field"><span>{}</span><input name="id" type="text" maxlength="32" required></label><label class="field"><span>{}</span><input name="title" type="text" maxlength="100" required></label><label class="field"><span>{}</span><textarea name="body" maxlength="4096" required></textarea></label><label class="field"><span>{}</span><textarea name="categories" rows="3"></textarea><span class="settings-help">{}</span></label><label class="field"><span>{}</span><textarea name="keywords" rows="3"></textarea><span class="settings-help">{}</span></label><input type="hidden" name="published_present" value="1"><label class="field"><span><input type="checkbox" name="published" value="true"> {}</span><small>{}</small></label><label class="field"><span>{}</span><textarea name="custom_embed" rows="8" maxlength="6000"></textarea><span class="settings-help">{}</span></label><button class="button" name="action" value="knowledge.create">{}</button></form></section>"#,
        l.text(keys::TICKET_KNOWLEDGE_CREATE),
        escape(&create_key),
        l.text(keys::TICKET_KNOWLEDGE_ID),
        l.text(keys::TICKET_KNOWLEDGE_ARTICLE_TITLE),
        l.text(keys::TICKET_KNOWLEDGE_BODY),
        l.text(keys::TICKET_KNOWLEDGE_CATEGORIES),
        l.text(keys::TICKET_KNOWLEDGE_CATEGORIES_HELP),
        l.text(keys::TICKET_KNOWLEDGE_KEYWORDS),
        l.text(keys::TICKET_KNOWLEDGE_KEYWORDS_HELP),
        l.text(keys::TICKET_KNOWLEDGE_PUBLISHED),
        l.text(keys::TICKET_KNOWLEDGE_PUBLISHED_HELP),
        l.text(keys::TICKET_KNOWLEDGE_CUSTOM_EMBED),
        l.text(keys::TICKET_KNOWLEDGE_CUSTOM_EMBED_HELP),
        l.text(keys::TICKET_KNOWLEDGE_CREATE)
    );

    let articles = state["articles"].as_array().map_or(&[][..], Vec::as_slice);
    if articles.is_empty() {
        let _ = write!(
            body,
            r#"<p class="empty">{}</p>"#,
            l.text(keys::TICKET_KNOWLEDGE_NONE)
        );
    }

    for article in articles {
        let article_id = article["id"].as_str().unwrap_or("");
        let title = article["title"].as_str().unwrap_or("");
        let content = article["body"].as_str().unwrap_or("");
        let categories = list_values(article, "categories");
        let keywords = list_values(article, "keywords");
        let published = article["published"].as_bool().unwrap_or(false);
        let published_checked = if published { " checked" } else { "" };
        let custom_embed = custom_embed_text(article);
        let revision = article["revision"]
            .as_i64()
            .filter(|revision| *revision > 0)
            .map(|revision| format!(r#""{revision}""#));

        let _ = write!(
            body,
            r#"<section class="card settings-card"><div class="panel-heading"><h2>{}</h2><span class="mono">{}</span></div>"#,
            escape(title),
            escape(article_id)
        );
        if valid_knowledge_id(article_id) {
            if let Some(etag) = revision {
                let key_seed: String = nonce.chars().take(20).collect();
                let update_key = format!("{key_seed}_u_{article_id}");
                let delete_key = format!("{key_seed}_d_{article_id}");
                let _ = write!(
                    body,
                    r#"<form method="post" action="{action_url}"><input type="hidden" name="id" value="{}"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}">{cursor_field}{search_field}<label class="field"><span>{}</span><input name="title" type="text" maxlength="100" value="{}" required></label><label class="field"><span>{}</span><textarea name="body" maxlength="4096" required>{}</textarea></label><label class="field"><span>{}</span><textarea name="categories" rows="3">{}</textarea><small class="settings-help">{}</small></label><label class="field"><span>{}</span><textarea name="keywords" rows="3">{}</textarea><small class="settings-help">{}</small></label><input type="hidden" name="published_present" value="1"><label class="field"><span><input type="checkbox" name="published" value="true"{published_checked}> {}</span><small class="settings-help">{}</small></label><label class="field"><span>{}</span><textarea name="custom_embed" rows="8" maxlength="6000">{}</textarea><small class="settings-help">{}</small></label><button class="button" name="action" value="knowledge.update">{}</button></form>"#,
                    escape(article_id),
                    escape(&etag),
                    escape(&update_key),
                    l.text(keys::TICKET_KNOWLEDGE_ARTICLE_TITLE),
                    escape(title),
                    l.text(keys::TICKET_KNOWLEDGE_BODY),
                    escape(content),
                    l.text(keys::TICKET_KNOWLEDGE_CATEGORIES),
                    escape(&categories),
                    l.text(keys::TICKET_KNOWLEDGE_CATEGORIES_HELP),
                    l.text(keys::TICKET_KNOWLEDGE_KEYWORDS),
                    escape(&keywords),
                    l.text(keys::TICKET_KNOWLEDGE_KEYWORDS_HELP),
                    l.text(keys::TICKET_KNOWLEDGE_PUBLISHED),
                    l.text(keys::TICKET_KNOWLEDGE_PUBLISHED_HELP),
                    l.text(keys::TICKET_KNOWLEDGE_CUSTOM_EMBED),
                    escape(&custom_embed),
                    l.text(keys::TICKET_KNOWLEDGE_CUSTOM_EMBED_HELP),
                    l.text(keys::TICKET_KNOWLEDGE_SAVE)
                );
                if published {
                    let public_url = format!("/knowledge/{id_html}/{}", escape(article_id));
                    let _ = write!(
                        body,
                        r#"<p><a class="button secondary" href="{}" target="_blank" rel="noopener noreferrer">{}</a></p>"#,
                        escape(&public_url),
                        l.text(keys::TICKET_KNOWLEDGE_PUBLIC_LINK)
                    );
                }
                let _ = write!(
                    body,
                    r#"<form method="post" action="{action_url}"><input type="hidden" name="id" value="{}"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}">{cursor_field}{search_field}<button class="button secondary" name="action" value="knowledge.delete">{}</button></form>"#,
                    escape(article_id),
                    escape(&etag),
                    escape(&delete_key),
                    l.text(keys::TICKET_KNOWLEDGE_DELETE)
                );
            } else {
                let _ = write!(
                    body,
                    r#"<p class="settings-help">{}</p>"#,
                    l.text(keys::TICKET_KNOWLEDGE_UNAVAILABLE)
                );
            }
        } else {
            let _ = write!(
                body,
                r#"<p class="settings-help">{}</p>"#,
                l.text(keys::TICKET_KNOWLEDGE_UNAVAILABLE)
            );
        }
        body.push_str("</section>");
    }

    if let Some(next_cursor) = state["next_cursor"]
        .as_str()
        .filter(|cursor| valid_knowledge_id(cursor))
    {
        let next_query = query_string(next_cursor, query);
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{page_url}?{}">{}</a></p>"#,
            escape(&next_query),
            l.text(keys::TICKET_KNOWLEDGE_NEXT)
        );
    }

    super::shell(l, headers, id, name, keys::TICKETS, &body, true)
}
