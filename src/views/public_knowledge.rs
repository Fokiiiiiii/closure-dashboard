use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use super::page;
use crate::{
    html::escape,
    i18n::{Locale, keys},
};

const MAX_CATEGORY_SUGGESTIONS: usize = 100;

fn valid_slug(slug: &str) -> bool {
    (1..=32).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_cursor(cursor: &str) -> bool {
    cursor.len() == 16 && cursor.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn article_values<'a>(article: &'a Value, field: &str) -> Vec<&'a str> {
    article[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty()
                && value.chars().count() <= 100
                && !value.chars().any(char::is_control)
        })
        .collect()
}

fn list_url(guild: &str, query: &str, category: &str, cursor: Option<&str>) -> String {
    let mut parameters = Vec::new();
    if !query.is_empty() {
        parameters.push(("q", query.to_owned()));
    }
    if !category.is_empty() {
        parameters.push(("category", category.to_owned()));
    }
    if let Some(cursor) = cursor {
        parameters.push(("cursor", cursor.to_owned()));
    }
    let path = format!("/knowledge/{guild}");
    let query = serde_urlencoded::to_string(parameters).unwrap_or_default();
    if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    }
}

fn keyword_line(l: Locale<'_>, article: &Value) -> String {
    let keywords = article_values(article, "keywords");
    if keywords.is_empty() {
        return String::new();
    }
    format!(
        r#"<p class="public-knowledge-meta"><strong>{}</strong> {}</p>"#,
        l.text(keys::PUBLIC_KNOWLEDGE_KEYWORDS),
        escape(&keywords.join(", "))
    )
}

pub(crate) fn public_knowledge_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    guild: &str,
    state: &Value,
    query: &str,
    category: &str,
) -> String {
    let guild = escape(guild);
    let page_url = format!("/knowledge/{guild}");
    let mut body = format!(
        r#"<main class="content narrow"><header class="page-heading"><h1>{}</h1></header>"#,
        l.text(keys::PUBLIC_KNOWLEDGE_TITLE)
    );
    let _ = write!(
        body,
        r#"<section class="card settings-card"><form method="get" action="{page_url}"><label class="field"><span>{}</span><input type="search" name="q" value="{}"></label><p class="settings-help">{}</p><label class="field"><span>{}</span><input type="search" name="category" list="public-knowledge-categories" value="{}" placeholder="{}"><datalist id="public-knowledge-categories">"#,
        l.text(keys::PUBLIC_KNOWLEDGE_SEARCH),
        escape(query),
        l.text(keys::PUBLIC_KNOWLEDGE_SEARCH_HINT),
        l.text(keys::PUBLIC_KNOWLEDGE_CATEGORY),
        escape(category),
        escape(l.text(keys::PUBLIC_KNOWLEDGE_ALL_CATEGORIES))
    );
    for suggestion in state["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_CATEGORY_SUGGESTIONS)
        .filter_map(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty()
                && value.chars().count() <= 100
                && !value.chars().any(char::is_control)
        })
    {
        let _ = write!(body, r#"<option value="{}"></option>"#, escape(suggestion));
    }
    let _ = write!(
        body,
        r#"</datalist></label><button class="button secondary" type="submit">{}</button></form></section>"#,
        l.text(keys::PUBLIC_KNOWLEDGE_SEARCH_SUBMIT)
    );

    let articles = state["articles"].as_array().map_or(&[][..], Vec::as_slice);
    if articles.is_empty() {
        let _ = write!(
            body,
            r#"<p class="empty" role="status">{}</p>"#,
            l.text(keys::PUBLIC_KNOWLEDGE_NO_RESULTS)
        );
    }
    for article in articles {
        let Some(slug) = article["id"].as_str().filter(|slug| valid_slug(slug)) else {
            continue;
        };
        let title = article["title"].as_str().unwrap_or("");
        let href = format!("/knowledge/{guild}/{slug}");
        let categories = article_values(article, "categories");
        let _ = write!(
            body,
            r#"<article class="card settings-card"><h2><a href="{}">{}</a></h2>"#,
            escape(&href),
            escape(title)
        );
        if !categories.is_empty() {
            let _ = write!(
                body,
                r#"<p class="public-knowledge-meta"><strong>{}</strong> "#,
                l.text(keys::PUBLIC_KNOWLEDGE_CATEGORY)
            );
            for name in categories {
                let href = list_url(&guild, query, name, None);
                let _ = write!(
                    body,
                    r#"<a class="tag" href="{}">{}</a> "#,
                    escape(&href),
                    escape(name)
                );
            }
            body.push_str("</p>");
        }
        body.push_str(&keyword_line(l, article));
        body.push_str("</article>");
    }

    if let Some(cursor) = state["next_cursor"]
        .as_str()
        .filter(|cursor| valid_cursor(cursor))
    {
        let href = list_url(&guild, query, category, Some(cursor));
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{}">{}</a></p>"#,
            escape(&href),
            l.text(keys::PUBLIC_KNOWLEDGE_NEXT)
        );
    }
    body.push_str("</main>");
    page(l, headers, &body)
}

pub(crate) fn public_knowledge_article_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    guild: &str,
    article: &Value,
) -> String {
    let guild = escape(guild);
    let mut body = format!(
        r#"<main class="content narrow"><p><a class="button secondary" href="/knowledge/{guild}">{}</a></p><article class="card settings-card"><h1>{}</h1>"#,
        l.text(keys::PUBLIC_KNOWLEDGE_BACK),
        escape(article["title"].as_str().unwrap_or(""))
    );
    let categories = article_values(article, "categories");
    if !categories.is_empty() {
        let _ = write!(
            body,
            r#"<p class="public-knowledge-meta"><strong>{}</strong> {}</p>"#,
            l.text(keys::PUBLIC_KNOWLEDGE_CATEGORY),
            escape(&categories.join(", "))
        );
    }
    body.push_str(&keyword_line(l, article));
    let article_body = article["body"]
        .as_str()
        .unwrap_or("")
        .lines()
        .map(escape)
        .collect::<Vec<_>>()
        .join("<br>");
    let _ = write!(
        body,
        r#"<div class="public-knowledge-body">{}</div>"#,
        article_body
    );
    body.push_str("</article></main>");
    page(l, headers, &body)
}

pub(crate) fn public_knowledge_not_found_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    guild: &str,
) -> String {
    let guild = escape(guild);
    let body = format!(
        r#"<main class="content narrow"><h1>{}</h1><p class="empty" role="status">{}</p><p><a class="button secondary" href="/knowledge/{guild}">{}</a></p></main>"#,
        l.text(keys::PUBLIC_KNOWLEDGE_TITLE),
        l.text(keys::PUBLIC_KNOWLEDGE_NOT_FOUND),
        l.text(keys::PUBLIC_KNOWLEDGE_BACK)
    );
    page(l, headers, &body)
}
