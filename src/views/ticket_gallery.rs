use std::collections::HashSet;
use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
    views::shell,
};

fn utc_date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn text<'a>(value: &'a Value) -> &'a str {
    value.as_str().unwrap_or("")
}

fn category_key(category: &str) -> &'static str {
    match category {
        "support" => keys::TICKET_GALLERY_CATEGORY_SUPPORT,
        "moderation" => keys::TICKET_GALLERY_CATEGORY_MODERATION,
        "sales" => keys::TICKET_GALLERY_CATEGORY_SALES,
        "applications" => keys::TICKET_GALLERY_CATEGORY_APPLICATIONS,
        "feedback" => keys::TICKET_GALLERY_CATEGORY_FEEDBACK,
        "general" => keys::TICKET_GALLERY_CATEGORY_GENERAL,
        "other" => keys::TICKET_GALLERY_CATEGORY_OTHER,
        _ => keys::TICKET_GALLERY_CATEGORY_ALL,
    }
}

fn kind_key(kind: &str) -> &'static str {
    match kind {
        "panel" => keys::TICKET_GALLERY_TYPE_PANEL,
        "tag" => keys::TICKET_GALLERY_TYPE_TAG,
        "form" => keys::TICKET_GALLERY_TYPE_FORM,
        _ => keys::TICKET_GALLERY_TYPE_ALL,
    }
}

fn gallery_path(id: &str) -> String {
    format!("/g/{}/tickets/gallery", escape(id))
}

fn query_string(values: &[(&str, &str)]) -> String {
    serde_urlencoded::to_string(values).unwrap_or_default()
}

fn card(l: Locale<'_>, guild: &str, item: &Value) -> String {
    let id = item["id"].as_i64().unwrap_or_default().to_string();
    let kind = text(&item["kind"]);
    let title = text(&item["title"]);
    let description = text(&item["description"]);
    let author = text(&item["author_name"]);
    let category = text(&item["category"]);
    let imports = item["import_count"].as_u64().unwrap_or_default();
    let detail = format!("{}/templates/{}", gallery_path(guild), escape(&id));
    let mut html = format!(
        r#"<article class="card panel gallery-card"><div class="gallery-card-heading"><span class="badge">{}</span><span class="badge">{}</span></div><h2><a href="{}">{}</a></h2><p>{}</p><p class="settings-help">{} · {} · {} {}</p>"#,
        l.text(kind_key(kind)),
        l.text(category_key(category)),
        detail,
        escape(title),
        escape(description),
        l.text(keys::TICKET_GALLERY_AUTHOR),
        escape(author),
        imports,
        l.text(keys::TICKET_GALLERY_IMPORT_COUNT),
    );
    if let Some(tags) = item["tags"].as_array() {
        html.push_str("<ul class=\"tag-list\">");
        for tag in tags.iter().filter_map(Value::as_str) {
            let _ = write!(html, "<li>{}</li>", escape(tag));
        }
        html.push_str("</ul>");
    }
    let _ = write!(
        html,
        r#"<a class="button secondary" href="{}">{}</a></article>"#,
        detail,
        l.text(keys::TICKET_GALLERY_OPEN)
    );
    html
}

pub(crate) fn ticket_gallery_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    data: &Value,
    query: &str,
    kind: &str,
    category: &str,
    sort: &str,
) -> String {
    let base = gallery_path(id);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="{}/submissions">{}</a></div></header>"#,
        l.text(keys::TICKET_GALLERY_TITLE),
        base,
        l.text(keys::TICKET_GALLERY_SUBMISSIONS)
    );
    if data["featured"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        let _ = write!(
            body,
            r#"<section class="card panel"><h2>{}</h2><div class="gallery-grid">"#,
            l.text(keys::TICKET_GALLERY_FEATURED)
        );
        for item in data["featured"].as_array().into_iter().flatten() {
            body.push_str(&card(l, id, item));
        }
        body.push_str("</div></section>");
    }
    let _ = write!(
        body,
        r#"<section class="card panel"><form method="get" action="{}" class="gallery-filters"><label class="field"><span>{}</span><input type="search" name="q" maxlength="100" value="{}"></label><label class="field"><span>{}</span><select name="kind">"#,
        base,
        l.text(keys::TICKET_GALLERY_SEARCH),
        escape(query),
        l.text(keys::TICKET_GALLERY_TYPE)
    );
    for (value, key) in [
        ("all", keys::TICKET_GALLERY_TYPE_ALL),
        ("panel", keys::TICKET_GALLERY_TYPE_PANEL),
        ("tag", keys::TICKET_GALLERY_TYPE_TAG),
        ("form", keys::TICKET_GALLERY_TYPE_FORM),
    ] {
        let selected = if value == kind { " selected" } else { "" };
        let _ = write!(
            body,
            r#"<option value="{value}"{selected}>{}</option>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><label class="field"><span>{}</span><select name="category">"#,
        l.text(keys::TICKET_GALLERY_CATEGORY)
    );
    for (value, key) in [
        ("all", keys::TICKET_GALLERY_CATEGORY_ALL),
        ("support", keys::TICKET_GALLERY_CATEGORY_SUPPORT),
        ("moderation", keys::TICKET_GALLERY_CATEGORY_MODERATION),
        ("sales", keys::TICKET_GALLERY_CATEGORY_SALES),
        ("applications", keys::TICKET_GALLERY_CATEGORY_APPLICATIONS),
        ("feedback", keys::TICKET_GALLERY_CATEGORY_FEEDBACK),
        ("general", keys::TICKET_GALLERY_CATEGORY_GENERAL),
        ("other", keys::TICKET_GALLERY_CATEGORY_OTHER),
    ] {
        let selected = if value == category { " selected" } else { "" };
        let _ = write!(
            body,
            r#"<option value="{value}"{selected}>{}</option>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><label class="field"><span>{}</span><select name="sort">"#,
        l.text(keys::TICKET_GALLERY_SORT)
    );
    for (value, key) in [
        ("newest", keys::TICKET_GALLERY_SORT_NEWEST),
        ("popular", keys::TICKET_GALLERY_SORT_POPULAR),
    ] {
        let selected = if value == sort { " selected" } else { "" };
        let _ = write!(
            body,
            r#"<option value="{value}"{selected}>{}</option>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><button class="button secondary" type="submit">{}</button><a class="button ghost" href="{}">{}</a></form>"#,
        l.text(keys::TICKET_GALLERY_SEARCH),
        base,
        l.text(keys::TICKET_GALLERY_CLEAR)
    );
    let templates = data["templates"].as_array().cloned().unwrap_or_default();
    let featured_ids: HashSet<i64> = data["featured"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_i64())
        .collect();
    let visible_templates = templates
        .iter()
        .filter(|item| {
            item["id"]
                .as_i64()
                .is_none_or(|id| !featured_ids.contains(&id))
        })
        .collect::<Vec<_>>();
    if visible_templates.is_empty() && featured_ids.is_empty() {
        let _ = write!(
            body,
            r#"<p class="card empty">{}</p>"#,
            l.text(keys::TICKET_GALLERY_EMPTY)
        );
    } else if !visible_templates.is_empty() {
        body.push_str("<div class=\"gallery-grid\">");
        for item in visible_templates {
            body.push_str(&card(l, id, item));
        }
        body.push_str("</div>");
    }
    let offset = data["offset"].as_u64().unwrap_or_default();
    let next_offset = data["next_offset"].as_u64();
    if offset > 0 || next_offset.is_some() {
        let _ = write!(
            body,
            r#"<nav class="pagination" aria-label="{}">"#,
            l.text(keys::TICKET_GALLERY_PAGES)
        );
        if offset > 0 {
            let previous = offset.saturating_sub(24).to_string();
            let query = query_string(&[
                ("q", query),
                ("kind", kind),
                ("category", category),
                ("sort", sort),
                ("offset", &previous),
            ]);
            let _ = write!(
                body,
                r#"<a class="button secondary" href="{}?{}">{}</a>"#,
                base,
                escape(&query),
                l.text(keys::TICKET_GALLERY_BACK)
            );
        }
        if let Some(next_offset) = next_offset {
            let next = next_offset.to_string();
            let query = query_string(&[
                ("q", query),
                ("kind", kind),
                ("category", category),
                ("sort", sort),
                ("offset", &next),
            ]);
            let _ = write!(
                body,
                r#"<a class="button secondary" href="{}?{}">{}</a>"#,
                base,
                escape(&query),
                l.text(keys::TICKET_GALLERY_MORE)
            );
        }
        body.push_str("</nav>");
    }
    body.push_str("</section>");
    shell(
        l,
        headers,
        id,
        name,
        keys::TICKET_GALLERY_TITLE,
        &body,
        false,
    )
}

pub(crate) fn ticket_gallery_detail(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    template: &Value,
    key: &str,
    tag_id: &str,
) -> String {
    let base = gallery_path(id);
    let kind = text(&template["kind"]);
    let title = text(&template["title"]);
    let description = text(&template["description"]);
    let category = text(&template["category"]);
    let imports = template["import_count"].as_u64().unwrap_or_default();
    let author = text(&template["author_name"]);
    let published = template["created_at"]
        .as_i64()
        .map(utc_date)
        .unwrap_or_default();
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="{}">{}</a></div></header><section class="card panel"><div class="gallery-card-heading"><span class="badge">{}</span><span class="badge">{}</span></div><h2>{}</h2><p>{}</p><p class="settings-help">{} {} · {} {} · {} {}</p>"#,
        l.text(keys::TICKET_GALLERY_TITLE),
        base,
        l.text(keys::TICKET_GALLERY_BACK),
        l.text(kind_key(kind)),
        l.text(category_key(category)),
        escape(title),
        escape(description),
        l.text(keys::TICKET_GALLERY_PUBLISHED),
        escape(&published),
        imports,
        l.text(keys::TICKET_GALLERY_IMPORT_COUNT),
        l.text(keys::TICKET_GALLERY_AUTHOR),
        escape(author),
    );
    if let Some(tags) = template["tags"].as_array() {
        body.push_str("<ul class=\"tag-list\">");
        for tag in tags.iter().filter_map(Value::as_str) {
            let _ = write!(body, "<li>{}</li>", escape(tag));
        }
        body.push_str("</ul>");
    }
    render_preview(&mut body, l, kind, &template["payload"]);
    body.push_str("</section>");
    let _ = write!(
        body,
        r#"<section class="card panel"><h2>{}</h2><form method="post" action="{}/import"><input type="hidden" name="template_id" value="{}"><input type="hidden" name="key" value="{}">"#,
        match kind {
            "panel" => l.text(keys::TICKET_GALLERY_IMPORT_PANEL),
            "tag" => l.text(keys::TICKET_GALLERY_IMPORT_TAG),
            _ => l.text(keys::TICKET_GALLERY_IMPORT_FORM),
        },
        base,
        escape(&template["id"].as_i64().unwrap_or_default().to_string()),
        escape(key)
    );
    if kind == "tag" {
        let _ = write!(
            body,
            r#"<label class="field"><span>{}</span><input name="new_id" value="{}" minlength="1" maxlength="16" pattern="[a-z0-9_-]{{1,16}}" required></label>"#,
            l.text(keys::TICKET_GALLERY_IMPORT_ID),
            escape(tag_id)
        );
    }
    let _ = write!(
        body,
        r#"<button class="button primary" type="submit">{}</button></form></section>"#,
        match kind {
            "panel" => l.text(keys::TICKET_GALLERY_IMPORT_PANEL),
            "tag" => l.text(keys::TICKET_GALLERY_IMPORT_TAG),
            _ => l.text(keys::TICKET_GALLERY_IMPORT_FORM),
        }
    );
    shell(
        l,
        headers,
        id,
        name,
        keys::TICKET_GALLERY_TITLE,
        &body,
        false,
    )
}

pub(crate) fn ticket_gallery_submissions(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    data: &Value,
    key: &str,
    saved: bool,
) -> String {
    let base = gallery_path(id);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="{}">{}</a></div></header>"#,
        l.text(keys::TICKET_GALLERY_SUBMISSIONS),
        base,
        l.text(keys::TICKET_GALLERY_BACK)
    );
    if saved {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_GALLERY_SAVED)
        );
    }
    let Some(rows) = data["submissions"].as_array() else {
        return shell(
            l,
            headers,
            id,
            name,
            keys::TICKET_GALLERY_TITLE,
            &body,
            false,
        );
    };
    if rows.is_empty() {
        let _ = write!(
            body,
            r#"<p class="card empty">{}</p>"#,
            l.text(keys::TICKET_GALLERY_SUBMISSIONS_EMPTY)
        );
    } else {
        for row in rows {
            let submission_id = row["id"].as_i64().unwrap_or_default().to_string();
            let revision = row["revision"].as_i64().unwrap_or_default().to_string();
            let status = text(&row["status"]);
            let edit_query = query_string(&[
                ("submission_id", &submission_id),
                ("kind", text(&row["kind"])),
            ]);
            let _ = write!(
                body,
                r#"<section class="card panel"><div class="gallery-card-heading"><span class="badge">{}</span><span class="badge">{}</span></div><h2>{}</h2><p>{}</p><p class="settings-help">{} · {}</p>"#,
                l.text(kind_key(text(&row["kind"]))),
                l.text(status_key(status)),
                escape(text(&row["title"])),
                escape(text(&row["description"])),
                l.text(category_key(text(&row["category"]))),
                l.text(keys::TICKET_GALLERY_AUTHOR_NAME)
            );
            if let Some(reason) = row["review_reason"]
                .as_str()
                .filter(|reason| !reason.is_empty())
            {
                let _ = write!(
                    body,
                    r#"<p class="error" role="status"><strong>{}</strong> {}</p>"#,
                    l.text(keys::TICKET_GALLERY_REJECTION_REASON),
                    escape(reason)
                );
            }
            if !matches!(status, "withdrawn" | "unpublished") {
                let _ = write!(
                    body,
                    r#"<a class="button secondary" href="{}/publish?{}">{}</a>"#,
                    base,
                    escape(&edit_query),
                    l.text(keys::TICKET_GALLERY_EDIT)
                );
            }
            match status {
                "pending" => submission_action_form(
                    &mut body,
                    &base,
                    &submission_id,
                    &revision,
                    &format!("{key}-{submission_id}-withdraw"),
                    "withdraw",
                    l.text(keys::TICKET_GALLERY_WITHDRAW),
                ),
                "approved" => submission_action_form(
                    &mut body,
                    &base,
                    &submission_id,
                    &revision,
                    &format!("{key}-{submission_id}-unpublish"),
                    "unpublish",
                    l.text(keys::TICKET_GALLERY_UNPUBLISH),
                ),
                _ => {}
            }
            body.push_str("</section>");
        }
    }
    shell(
        l,
        headers,
        id,
        name,
        keys::TICKET_GALLERY_TITLE,
        &body,
        false,
    )
}

fn submission_action_form(
    body: &mut String,
    base: &str,
    submission_id: &str,
    revision: &str,
    key: &str,
    action: &str,
    label: String,
) {
    let _ = write!(
        body,
        r#"<form method="post" action="{}/submissions/actions"><input type="hidden" name="action" value="{}"><input type="hidden" name="submission_id" value="{}"><input type="hidden" name="revision" value="{}"><input type="hidden" name="key" value="{}"><button class="button secondary" type="submit">{}</button></form>"#,
        base,
        escape(action),
        escape(submission_id),
        escape(revision),
        escape(key),
        label
    );
}

pub(crate) fn ticket_gallery_publish(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    sources: &Value,
    kind: &str,
    source_id: &str,
    submission: Option<&Value>,
    key: &str,
    saved: bool,
) -> String {
    let base = gallery_path(id);
    let editing = submission.is_some();
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="{}/submissions">{}</a></div></header>"#,
        if editing {
            l.text(keys::TICKET_GALLERY_EDIT)
        } else {
            l.text(keys::TICKET_GALLERY_PUBLISH)
        },
        base,
        l.text(keys::TICKET_GALLERY_SUBMISSIONS)
    );
    if saved {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_GALLERY_SAVED)
        );
    }
    let rows = sources["sources"].as_array().cloned().unwrap_or_default();
    let selected = format!("{kind}:{source_id}");
    let _ = write!(
        body,
        r#"<section class="card panel"><form method="post" action="{}/publish">"#,
        base
    );
    if let Some(submission) = submission {
        let _ = write!(
            body,
            r#"<input type="hidden" name="submission_id" value="{}"><input type="hidden" name="revision" value="{}">"#,
            submission["id"].as_i64().unwrap_or_default(),
            submission["revision"].as_i64().unwrap_or_default()
        );
    }
    let _ = write!(
        body,
        r#"<input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><select name="source" required>"#,
        escape(key),
        l.text(keys::TICKET_GALLERY_SOURCES)
    );
    if source_id.is_empty() {
        let _ = write!(
            body,
            r#"<option value="" disabled selected>{}</option>"#,
            l.text(keys::TICKET_GALLERY_SELECT_SOURCE)
        );
    }
    if rows.is_empty() && source_id.is_empty() {
        let _ = write!(
            body,
            r#"<p class="settings-help">{}</p>"#,
            l.text(keys::TICKET_GALLERY_NO_SOURCES)
        );
    }
    if !source_id.is_empty()
        && !rows
            .iter()
            .any(|row| row["kind"].as_str() == Some(kind) && row["id"].as_str() == Some(source_id))
    {
        let _ = write!(
            body,
            r#"<option value="{}" selected>{}</option>"#,
            escape(&selected),
            escape(source_id)
        );
    }
    for row in &rows {
        let source_kind = text(&row["kind"]);
        let source_id = text(&row["id"]);
        let source_name = text(&row["name"]);
        let value = format!("{source_kind}:{source_id}");
        let selected_attr = if value == selected { " selected" } else { "" };
        let _ = write!(
            body,
            r#"<option value="{}"{selected_attr}>{}: {}</option>"#,
            escape(&value),
            l.text(kind_key(source_kind)),
            escape(source_name)
        );
    }
    body.push_str("</select></label>");
    let title = submission.map_or("", |submission| text(&submission["title"]));
    let description = submission.map_or("", |submission| text(&submission["description"]));
    let tags = submission
        .and_then(|submission| submission["tags"].as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let author = submission.map_or("", |submission| text(&submission["author_name"]));
    for (name, label, value, max) in [
        ("title", keys::TICKET_GALLERY_NAME, title, 100),
        (
            "description",
            keys::TICKET_GALLERY_DESCRIPTION,
            description,
            500,
        ),
        ("tags", keys::TICKET_GALLERY_TAGS, tags.as_str(), 72),
        ("author_name", keys::TICKET_GALLERY_AUTHOR_NAME, author, 80),
    ] {
        let _ = write!(
            body,
            r#"<label class="field"><span>{}</span><input name="{}" value="{}" maxlength="{}"{}></label>"#,
            l.text(label),
            name,
            escape(value),
            max,
            if name == "title" || name == "description" {
                " required"
            } else {
                ""
            }
        );
    }
    let current_category = submission.map_or("general", |submission| text(&submission["category"]));
    let _ = write!(
        body,
        r#"<label class="field"><span>{}</span><select name="category">"#,
        l.text(keys::TICKET_GALLERY_CATEGORY)
    );
    for (value, key) in [
        ("support", keys::TICKET_GALLERY_CATEGORY_SUPPORT),
        ("moderation", keys::TICKET_GALLERY_CATEGORY_MODERATION),
        ("sales", keys::TICKET_GALLERY_CATEGORY_SALES),
        ("applications", keys::TICKET_GALLERY_CATEGORY_APPLICATIONS),
        ("feedback", keys::TICKET_GALLERY_CATEGORY_FEEDBACK),
        ("general", keys::TICKET_GALLERY_CATEGORY_GENERAL),
        ("other", keys::TICKET_GALLERY_CATEGORY_OTHER),
    ] {
        let selected = if value == current_category {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<option value="{value}"{selected}>{}</option>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><label class="field checkbox"><input type="checkbox" name="consent" value="1" required><span>{}</span></label><button class="button primary" type="submit">{}</button></form></section>"#,
        l.text(keys::TICKET_GALLERY_SHARE_CONSENT),
        l.text(keys::TICKET_GALLERY_SUBMIT)
    );
    shell(
        l,
        headers,
        id,
        name,
        keys::TICKET_GALLERY_TITLE,
        &body,
        false,
    )
}

fn status_key(status: &str) -> &'static str {
    match status {
        "pending" => keys::TICKET_GALLERY_STATUS_PENDING,
        "approved" => keys::TICKET_GALLERY_STATUS_APPROVED,
        "rejected" => keys::TICKET_GALLERY_STATUS_REJECTED,
        "withdrawn" => keys::TICKET_GALLERY_STATUS_WITHDRAWN,
        _ => keys::TICKET_GALLERY_STATUS_UNPUBLISHED,
    }
}

fn render_embed_fields(body: &mut String, fields: &Value) {
    if let Some(fields) = fields.as_array() {
        for field in fields {
            let name = text(&field["name"]);
            let value = text(&field["value"]);
            if !name.is_empty() || !value.is_empty() {
                let _ = write!(
                    body,
                    r#"<div class="gallery-preview-field"><strong>{}</strong><p>{}</p></div>"#,
                    escape(name),
                    escape(value)
                );
            }
        }
    }
}

fn render_preview(body: &mut String, l: Locale<'_>, kind: &str, payload: &Value) {
    match kind {
        "panel" => {
            let custom = &payload["customization"];
            for (label, value) in [
                (keys::TICKET_GALLERY_PANEL_TITLE, text(&custom["title"])),
                (keys::TICKET_GALLERY_PANEL_CONTENT, text(&custom["content"])),
                (
                    keys::TICKET_GALLERY_WELCOME_MESSAGE,
                    text(&custom["welcome_description"]),
                ),
            ] {
                if !value.is_empty() {
                    let _ = write!(
                        body,
                        r#"<div class="gallery-preview-field"><h3>{}</h3><pre>{}</pre></div>"#,
                        l.text(label),
                        escape(value)
                    );
                }
            }
            render_embed_fields(body, &custom["welcome_fields"]);
            let _ = write!(
                body,
                r#"<p class="settings-help">{}</p>"#,
                escape(text(&payload["button_style"]))
            );
        }
        "tag" => {
            if !text(&payload["content"]).is_empty() {
                let _ = write!(
                    body,
                    r#"<div class="gallery-preview-field"><pre>{}</pre></div>"#,
                    escape(text(&payload["content"]))
                );
            }
            let embed = &payload["embed"];
            for key in ["title", "description", "footer_text"] {
                let value = text(&embed[key]);
                if !value.is_empty() {
                    let _ = write!(
                        body,
                        r#"<div class="gallery-preview-field"><pre>{}</pre></div>"#,
                        escape(value)
                    );
                }
            }
            render_embed_fields(body, &embed["fields"]);
        }
        "form" => {
            let _ = write!(body, r"<h3>{}</h3>", escape(text(&payload["title"])));
            if let Some(questions) = payload["questions"].as_array() {
                body.push_str("<ol>");
                for question in questions {
                    let label = question
                        .as_str()
                        .unwrap_or_else(|| text(&question["label"]));
                    let _ = write!(body, "<li>{}</li>", escape(label));
                    if let Some(options) = question["options"].as_array() {
                        if !options.is_empty() {
                            body.push_str("<ul>");
                            for option in options {
                                let _ = write!(body, "<li>{}</li>", escape(text(&option["label"])));
                            }
                            body.push_str("</ul>");
                        }
                    }
                }
                body.push_str("</ol>");
            }
        }
        _ => {}
    }
}
