use std::fmt::Write as _;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "Render the tag list and its filter state together."
)]
pub(crate) fn tags_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    guild_id: &str,
    guild_name: &str,
    state: &Value,
    edit_tag: Option<&Value>,
    query: &str,
    cursor: Option<&str>,
    article_query: &str,
    article_cursor: Option<&str>,
    saved: bool,
    nonce: &str,
) -> String {
    let guild = escape(guild_id);
    let page_url = format!("/g/{guild}/tickets/tags");
    let action_url = format!("{page_url}/actions");
    let state_fields = hidden_state(query, cursor, article_query, article_cursor);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1></header>"#,
        l.text(keys::TICKET_TAGS_TITLE),
    );
    if saved {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TAGS_SAVED)
        );
    }

    if let Some(tag) = edit_tag {
        let id = tag["id"].as_str().unwrap_or("");
        let back = page_query(&page_url, query, cursor, article_query, article_cursor);
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
            escape(&back),
            l.text(keys::TICKET_TAGS_BACK)
        );
        let edit_search_fields = hidden_tag_state(query, cursor);
        let _ = write!(
            body,
            r#"<section class="card settings-card"><form method="get" action="{page_url}"><input type="hidden" name="edit" value="{}">{edit_search_fields}<label class="field"><span>{}</span><input type="search" name="article_q" value="{}"></label><button class="button secondary">{}</button></form></section>"#,
            escape(id),
            l.text(keys::TICKET_TAGS_ARTICLE_SEARCH),
            escape(article_query),
            l.text(keys::TICKET_TAGS_SEARCH_SUBMIT)
        );
        body.push_str(&tag_editor(
            l,
            &action_url,
            &state["articles"],
            Some(tag),
            &state_fields,
            nonce,
        ));
        if let Some(next_cursor) = state["articles_next_cursor"]
            .as_str()
            .filter(|value| valid_article_cursor(value))
        {
            let href = page_query_edit(
                &page_url,
                id,
                query,
                cursor,
                article_query,
                Some(next_cursor),
            );
            let _ = write!(
                body,
                r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
                escape(&href),
                l.text(keys::TICKET_TAGS_ARTICLE_NEXT)
            );
        }
        if article_cursor.is_some() {
            let href = page_query_edit(&page_url, id, query, cursor, article_query, None);
            let _ = write!(
                body,
                r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
                escape(&href),
                l.text(keys::TICKET_TAGS_ARTICLE_FIRST)
            );
        }
    } else {
        let tag_search_fields = hidden_article_state(article_query, article_cursor);
        let _ = write!(
            body,
            r#"<section class="card settings-card"><form method="get" action="{page_url}"><label class="field"><span>{}</span><input type="search" name="q" value="{}"></label>{tag_search_fields}<button class="button secondary">{}</button></form></section>"#,
            l.text(keys::TICKET_TAGS_SEARCH),
            escape(query),
            l.text(keys::TICKET_TAGS_SEARCH_SUBMIT),
        );

        let article_search_fields = hidden_tag_state(query, cursor);
        let _ = write!(
            body,
            r#"<section class="card settings-card"><form method="get" action="{page_url}"><label class="field"><span>{}</span><input type="search" name="article_q" value="{}"></label>{article_search_fields}<button class="button secondary">{}</button></form></section>"#,
            l.text(keys::TICKET_TAGS_ARTICLE_SEARCH),
            escape(article_query),
            l.text(keys::TICKET_TAGS_SEARCH_SUBMIT),
        );

        body.push_str(&tag_editor(
            l,
            &action_url,
            &state["articles"],
            None,
            &state_fields,
            nonce,
        ));

        let tags = state["tags"].as_array().map_or(&[][..], Vec::as_slice);
        if tags.is_empty() {
            let _ = write!(
                body,
                r#"<p class="empty">{}</p>"#,
                l.text(keys::TICKET_TAGS_EMPTY)
            );
        }
        for tag in tags {
            let id = tag["id"].as_str().unwrap_or("");
            if valid_tag_id(id) {
                body.push_str(&tag_card(
                    l,
                    guild_id,
                    &action_url,
                    &page_url,
                    tag,
                    query,
                    cursor,
                    article_query,
                    article_cursor,
                    nonce,
                ));
            }
        }

        if let Some(next_cursor) = state["next_cursor"]
            .as_str()
            .filter(|value| valid_tag_id(value))
        {
            let href = page_query(
                &page_url,
                query,
                Some(next_cursor),
                article_query,
                article_cursor,
            );
            let _ = write!(
                body,
                r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
                escape(&href),
                l.text(keys::TICKET_TAGS_NEXT)
            );
        }
        if let Some(next_cursor) = state["articles_next_cursor"]
            .as_str()
            .filter(|value| valid_article_cursor(value))
        {
            let href = page_query(&page_url, query, cursor, article_query, Some(next_cursor));
            let _ = write!(
                body,
                r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
                escape(&href),
                l.text(keys::TICKET_TAGS_ARTICLE_NEXT)
            );
        }
        if article_cursor.is_some() {
            let href = page_query(&page_url, query, cursor, article_query, None);
            let _ = write!(
                body,
                r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
                escape(&href),
                l.text(keys::TICKET_TAGS_ARTICLE_FIRST)
            );
        }
    }

    super::shell(
        l,
        headers,
        guild_id,
        guild_name,
        keys::TICKET_TAGS_TITLE,
        &body,
        true,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "Keep the tag card fields explicit at the render callsite."
)]
fn tag_card(
    l: Locale<'_>,
    guild_id: &str,
    action_url: &str,
    page_url: &str,
    tag: &Value,
    query: &str,
    cursor: Option<&str>,
    article_query: &str,
    article_cursor: Option<&str>,
    nonce: &str,
) -> String {
    let id = tag["id"].as_str().unwrap_or("");
    let id_html = escape(id);
    let revision = tag["revision"].as_i64().unwrap_or(0);
    let type_label = match tag["type"].as_str().unwrap_or("") {
        "Embed" => l.text(keys::TICKET_TAGS_TYPE_EMBED),
        "Embed + Text" => l.text(keys::TICKET_TAGS_TYPE_EMBED_TEXT),
        _ => l.text(keys::TICKET_TAGS_TYPE_TEXT),
    };
    let clone_key = action_key(nonce, "clone", id);
    let delete_key = action_key(nonce, "delete", id);
    let state_fields = hidden_state(query, cursor, article_query, article_cursor);
    let etag = format!("&quot;{revision}&quot;");
    let edit_url = page_query_edit(page_url, id, query, cursor, article_query, article_cursor);
    let publish_url = format!(
        "/g/{}/tickets/gallery/publish?kind=tag&amp;source_id={}",
        escape(guild_id),
        id_html
    );
    format!(
        r#"<section class="card settings-card"><div class="panel-heading"><h2 class="mono">{id_html}</h2><span class="badge">{type_label}</span></div><p><a class="button secondary" href="{}">{}</a> <a class="button secondary" href="{publish_url}">{}</a></p><details><summary>{}</summary><form method="post" action="{action_url}"><input type="hidden" name="id" value="{id_html}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="tag.clone">{state_fields}<label class="field"><span>{}</span><input name="new_id" type="text" maxlength="16" autocapitalize="none" autocomplete="off" required></label><button class="button secondary" type="submit">{}</button></form></details><details><summary>{}</summary><form method="post" action="{action_url}"><input type="hidden" name="id" value="{id_html}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="tag.delete">{state_fields}<label class="field"><span><input type="checkbox" name="confirm_delete" value="1" required> {}</span></label><button class="button danger" type="submit">{}</button></form></details></section>"#,
        escape(&edit_url),
        l.text(keys::TICKET_TAGS_EDIT),
        l.text(keys::TICKET_TAGS_PUBLISH),
        l.text(keys::TICKET_TAGS_CLONE),
        escape(&clone_key),
        l.text(keys::TICKET_TAGS_CLONE_ID),
        l.text(keys::TICKET_TAGS_CLONE),
        l.text(keys::TICKET_TAGS_DELETE),
        escape(&delete_key),
        l.text(keys::TICKET_TAGS_DELETE_CONFIRM),
        l.text(keys::TICKET_TAGS_DELETE),
    )
}

fn tag_editor(
    l: Locale<'_>,
    action_url: &str,
    articles: &Value,
    tag: Option<&Value>,
    state_fields: &str,
    nonce: &str,
) -> String {
    let edit = tag.is_some();
    let id = tag.and_then(|tag| tag["id"].as_str()).unwrap_or("");
    let body = tag.and_then(|tag| tag["body"].as_str()).unwrap_or("");
    let embed = tag.and_then(|tag| tag["embed"].as_object().map(|_| &tag["embed"]));
    let article_id = tag.and_then(|tag| tag["article_id"].as_str());
    let article_title = tag.and_then(|tag| tag["article_title"].as_str());
    let custom_command_alias = tag
        .and_then(|tag| tag["custom_command_alias"].as_bool())
        .unwrap_or(false);
    let revision = tag.and_then(|tag| tag["revision"].as_i64()).unwrap_or(0);
    let action = if edit { "tag.update" } else { "tag.create" };
    let title_key = if edit {
        keys::TICKET_TAGS_EDIT
    } else {
        keys::TICKET_TAGS_CREATE
    };
    let submit_key = if edit {
        keys::TICKET_TAGS_SAVE
    } else {
        keys::TICKET_TAGS_CREATE
    };
    let etag = format!("&quot;{revision}&quot;");
    let id_input = if edit {
        format!(
            r#"<input name="id" value="{}" maxlength="16" readonly>"#,
            escape(id)
        )
    } else {
        r#"<input name="id" type="text" maxlength="16" autocapitalize="none" autocomplete="off" required>"#.to_owned()
    };
    let key = action_key(nonce, if edit { "update" } else { "create" }, id);
    let preview_url = format!("{action_url}?preview=1");
    let id_label = l.text(keys::TICKET_TAGS_ID);
    let body_label = l.text(keys::TICKET_TAGS_BODY);
    let article_label = l.text(keys::TICKET_TAGS_ARTICLE);
    let article_options = article_options(l, articles, article_id, article_title);
    let preview_label = l.text(keys::CP_PREVIEW);
    let alias_checked = if custom_command_alias { " checked" } else { "" };
    let alias_toggle = format!(
        r#"<label class="field"><span><input type="checkbox" name="custom_command_alias" value="1"{alias_checked}> {}</span><small class="settings-help">{}</small></label>"#,
        l.text(keys::TICKET_TAGS_ALIAS),
        l.text(keys::TICKET_TAGS_ALIAS_HELP),
    );
    let alias_sync_pending = tag
        .and_then(|tag| tag["alias_sync_pending"].as_bool())
        .unwrap_or(false);
    let alias_sync_notice = if alias_sync_pending {
        format!(
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TAGS_ALIAS_SYNC_PENDING)
        )
    } else {
        String::new()
    };
    let mut form = format!(
        r#"<form class="card settings-card composer-edit" method="post" action="{action_url}" data-preview="{}"><div class="panel-heading"><h2>{}</h2></div><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><input type="hidden" name="action" value="{action}">{state_fields}<label class="field"><span>{id_label}</span>{id_input}</label>{alias_toggle}{alias_sync_notice}<label class="field"><span>{body_label}</span><textarea name="body" rows="5" maxlength="2000">{}</textarea></label>{}<label class="field"><span>{article_label}</span><select name="article_id">{article_options}</select></label><button class="button" type="submit">{submit_key}</button></form>"#,
        escape(&preview_url),
        l.text(title_key),
        escape(&key),
        escape(body),
        embed_editor(l, embed, nonce, if edit { id } else { "create" }),
    );
    let preview = tags_preview(l, body, embed);
    let _ = write!(
        form,
        r#"<aside class="composer-side"><span class="cp-preview-label">{preview_label}</span><div class="card" id="preview">{preview}</div></aside>"#
    );
    format!(r#"<div class="composer">{form}</div>"#)
}

pub(crate) fn tags_preview(l: Locale<'_>, body: &str, embed: Option<&Value>) -> String {
    let mut form = crate::composer::Form::new();
    form.insert("c".to_owned(), body.to_owned());
    if let Some(embed) = embed.and_then(Value::as_object) {
        form.insert("e.0.__".to_owned(), "1".to_owned());
        for (source, destination) in [
            ("title", "title"),
            ("color", "color"),
            ("description", "description"),
            ("author_name", "author_name"),
            ("author_url", "author_url"),
            ("author_icon_url", "author_icon"),
            ("thumbnail_url", "thumbnail"),
            ("image_url", "image"),
            ("footer_text", "footer_text"),
            ("footer_icon_url", "footer_icon"),
        ] {
            if let Some(value) = embed.get(source).and_then(Value::as_str) {
                form.insert(format!("e.0.{destination}"), value.to_owned());
            }
        }
        if embed.get("footer_timestamp").and_then(Value::as_bool) == Some(true) {
            form.insert("e.0.timestamp".to_owned(), "1".to_owned());
        }
        if let Some(fields) = embed.get("fields").and_then(Value::as_array) {
            for (index, field) in fields.iter().enumerate() {
                let prefix = format!("e.0.f.{index}");
                if let Some(name) = field.get("name").and_then(Value::as_str) {
                    form.insert(format!("{prefix}.name"), name.to_owned());
                }
                if let Some(value) = field.get("value").and_then(Value::as_str) {
                    form.insert(format!("{prefix}.value"), value.to_owned());
                }
                if field.get("inline").and_then(Value::as_bool) == Some(true) {
                    form.insert(format!("{prefix}.inline"), "1".to_owned());
                }
            }
        }
    }
    let names = super::ComposerNames {
        roles: &[],
        channels: &[],
    };
    super::composer_preview(l, &form, &[], names)
}

fn embed_editor(l: Locale<'_>, embed: Option<&Value>, nonce: &str, suffix: &str) -> String {
    let enabled = embed.is_some();
    let checked = if enabled { " checked" } else { "" };
    let data = embed
        .and_then(|embed| serde_json::to_string_pretty(embed).ok())
        .unwrap_or_default();
    let id = format!(
        "embed-{}-{}",
        escape(&nonce.chars().take(8).collect::<String>()),
        escape(suffix)
    );
    format!(
        r#"<details class="editor-section"><summary>{}</summary><label class="field"><span><input type="checkbox" name="use_embed" value="1"{checked}> {}</span></label><label class="field"><span>{}</span><textarea id="{id}" name="embed" rows="10" maxlength="6000" aria-describedby="{id}-help">{}</textarea><small class="settings-help" id="{id}-help">{}</small></label></details>"#,
        l.text(keys::TICKET_TAGS_EMBED),
        l.text(keys::TICKET_TAGS_EMBED),
        l.text(keys::TICKET_TAGS_EMBED_HELP),
        escape(&data),
        l.text(keys::TICKET_TAGS_EMBED_HELP),
    )
}

fn article_options(
    l: Locale<'_>,
    articles: &Value,
    selected_id: Option<&str>,
    selected_title: Option<&str>,
) -> String {
    let mut options = format!(
        r#"<option value="">{}</option>"#,
        l.text(keys::TICKET_TAGS_ARTICLE_NONE)
    );
    let listed = articles
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|article| Some((article["id"].as_str()?, article["title"].as_str()?)))
        .collect::<Vec<_>>();
    if let Some(id) = selected_id.filter(|id| valid_article_id(id))
        && !listed.iter().any(|(listed_id, _)| *listed_id == id)
    {
        let title = selected_title
            .filter(|title| !title.trim().is_empty())
            .map_or_else(
                || l.text(keys::TICKET_TAGS_ARTICLE_UNAVAILABLE).clone(),
                escape,
            );
        let _ = write!(
            options,
            r#"<option value="{}" selected>{}</option>"#,
            escape(id),
            title
        );
    }
    for (id, title) in listed {
        if valid_article_id(id) {
            let selected = if selected_id == Some(id) {
                " selected"
            } else {
                ""
            };
            let _ = write!(
                options,
                r#"<option value="{}"{selected}>{}</option>"#,
                escape(id),
                escape(title)
            );
        }
    }
    options
}

fn hidden_state(
    query: &str,
    cursor: Option<&str>,
    article_query: &str,
    article_cursor: Option<&str>,
) -> String {
    let mut hidden = hidden_tag_state(query, cursor);
    hidden.push_str(&hidden_article_state(article_query, article_cursor));
    hidden
}

fn hidden_tag_state(query: &str, cursor: Option<&str>) -> String {
    let mut hidden = String::new();
    if !query.is_empty() {
        let _ = write!(
            hidden,
            r#"<input type="hidden" name="q" value="{}">"#,
            escape(query)
        );
    }
    if let Some(cursor) = cursor.filter(|cursor| valid_tag_id(cursor)) {
        let _ = write!(
            hidden,
            r#"<input type="hidden" name="cursor" value="{}">"#,
            escape(cursor)
        );
    }
    hidden
}

fn hidden_article_state(query: &str, cursor: Option<&str>) -> String {
    let mut hidden = String::new();
    if !query.is_empty() {
        let _ = write!(
            hidden,
            r#"<input type="hidden" name="article_q" value="{}">"#,
            escape(query)
        );
    }
    if let Some(cursor) = cursor.filter(|cursor| valid_article_cursor(cursor)) {
        let _ = write!(
            hidden,
            r#"<input type="hidden" name="article_cursor" value="{}">"#,
            escape(cursor)
        );
    }
    hidden
}

fn page_query(
    base: &str,
    query: &str,
    cursor: Option<&str>,
    article_query: &str,
    article_cursor: Option<&str>,
) -> String {
    let mut parameters = Vec::new();
    if !query.is_empty() {
        parameters.push(("q", query.to_owned()));
    }
    if let Some(cursor) = cursor {
        parameters.push(("cursor", cursor.to_owned()));
    }
    if !article_query.is_empty() {
        parameters.push(("article_q", article_query.to_owned()));
    }
    if let Some(cursor) = article_cursor {
        parameters.push(("article_cursor", cursor.to_owned()));
    }
    let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
    if query.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{query}")
    }
}

fn page_query_edit(
    base: &str,
    edit_id: &str,
    query: &str,
    cursor: Option<&str>,
    article_query: &str,
    article_cursor: Option<&str>,
) -> String {
    let mut parameters = vec![("edit", edit_id.to_owned())];
    if !query.is_empty() {
        parameters.push(("q", query.to_owned()));
    }
    if let Some(cursor) = cursor {
        parameters.push(("cursor", cursor.to_owned()));
    }
    if !article_query.is_empty() {
        parameters.push(("article_q", article_query.to_owned()));
    }
    if let Some(cursor) = article_cursor {
        parameters.push(("article_cursor", cursor.to_owned()));
    }
    let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
    format!("{base}?{query}")
}

fn action_key(nonce: &str, action: &str, id: &str) -> String {
    let nonce = nonce.chars().take(12).collect::<String>();
    format!("{nonce}_{action}_{id}")
}

fn valid_tag_id(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_article_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_article_cursor(cursor: &str) -> bool {
    !cursor.is_empty() && cursor.len() <= 256 && !cursor.chars().any(char::is_control)
}
