use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, user, views,
};
use crate::session;
use axum::Form;
use serde_json::{Map, Value, json};

const GALLERY_PAGE_SIZE: u32 = 24;
const MAX_GALLERY_OFFSET: u32 = 10_000;

pub(super) async fn browse(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || query.keys().any(|key| {
            !matches!(
                key.as_str(),
                "q" | "kind" | "category" | "sort" | "offset" | "saved"
            )
        })
        || query.get("saved").is_some_and(|value| value != "1")
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let q = query.get("q").map(String::as_str).unwrap_or("").trim();
    if q.chars().count() > 100 || q.chars().any(char::is_control) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let kind = match query.get("kind").map(String::as_str).unwrap_or("all") {
        "all" => None,
        kind @ ("panel" | "tag" | "form") => Some(kind),
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    let category = match query.get("category").map(String::as_str).unwrap_or("all") {
        "all" => None,
        category @ ("support" | "moderation" | "sales" | "applications" | "feedback"
        | "general" | "other") => Some(category),
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    let sort = match query.get("sort").map(String::as_str).unwrap_or("newest") {
        sort @ ("newest" | "popular") => sort,
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    };
    let offset = match query.get("offset") {
        None => 0,
        Some(value) => match value.parse::<u32>() {
            Ok(offset) if offset <= MAX_GALLERY_OFFSET => offset,
            _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        },
    };
    let search = (!q.is_empty()).then_some(q);
    let templates = app.api.gallery_templates(
        &id,
        kind,
        category,
        search,
        sort,
        offset,
        GALLERY_PAGE_SIZE,
        &actor,
    );
    let guilds = app.api.guilds();
    let (templates, guilds) = tokio::join!(templates, guilds);
    let data = match templates {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    if !valid_gallery_page(&data, offset) {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    }
    let name = guilds
        .ok()
        .map(|(guilds, _)| guild_name(&guilds, &id))
        .unwrap_or_else(|| id.clone());
    Html(views::ticket_gallery_page(
        l,
        &headers,
        &id,
        &name,
        &data,
        q,
        kind.unwrap_or("all"),
        category.unwrap_or("all"),
        sort,
    ))
    .into_response()
}

pub(super) async fn detail(
    State(app): State<App>,
    Path((id, template_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) || !valid_template_id(&template_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let (data, guilds) = tokio::join!(
        app.api.gallery_template(&id, &template_id, &actor),
        app.api.guilds()
    );
    let data = match data {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let Some(template) = data
        .get("template")
        .filter(|template| valid_template(template))
    else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    };
    let name = guilds
        .ok()
        .map(|(guilds, _)| guild_name(&guilds, &id))
        .unwrap_or_else(|| id.clone());
    let default_tag_id = template["title"]
        .as_str()
        .map(default_tag_id)
        .unwrap_or_default();
    Html(views::ticket_gallery_detail(
        l,
        &headers,
        &id,
        &name,
        template,
        &app.nonces.fresh(),
        &default_tag_id,
    ))
    .into_response()
}

pub(super) async fn submissions(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || query.keys().any(|key| key != "saved")
        || query.get("saved").is_some_and(|value| value != "1")
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let submissions = app.api.gallery_submissions(&id, &actor).await;
    let data = match submissions {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    if !data["submissions"]
        .as_array()
        .is_some_and(|rows| rows.len() <= 100 && rows.iter().all(valid_submission))
    {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    }
    let guilds = app.api.guilds().await;
    let name = guilds
        .ok()
        .map(|(guilds, _)| guild_name(&guilds, &id))
        .unwrap_or_else(|| id.clone());
    Html(views::ticket_gallery_submissions(
        l,
        &headers,
        &id,
        &name,
        &data,
        &app.nonces.fresh(),
        query.get("saved").is_some_and(|value| value == "1"),
    ))
    .into_response()
}

pub(super) async fn publish(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || query.keys().any(|key| {
            !matches!(
                key.as_str(),
                "kind" | "source_id" | "submission_id" | "saved"
            )
        })
        || query.get("saved").is_some_and(|value| value != "1")
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let mut kind = query.get("kind").cloned().unwrap_or_default();
    let source_id = query.get("source_id").cloned().unwrap_or_default();
    if !kind.is_empty() && !matches!(kind.as_str(), "panel" | "tag" | "form") {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    if !source_id.is_empty() && !valid_source_id(&source_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let editing = match query.get("submission_id") {
        Some(value) if valid_template_id(value) => Some(value.clone()),
        Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        None => None,
    };
    // shortcut: the source picker uses the first plugin page; add cursor paging when guilds need later sources.
    let (sources, submissions, guilds) = tokio::join!(
        app.api
            .ticket_action_state(&id, "gallery", "gallery", &actor),
        async {
            if editing.is_some() {
                app.api.gallery_submissions(&id, &actor).await
            } else {
                Ok((json!({ "submissions": [] }), None))
            }
        },
        app.api.guilds()
    );
    let sources = match sources {
        Ok((data, _)) => match data.get("state").filter(|state| valid_sources_state(state)) {
            Some(state) => state.clone(),
            None => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
        },
        Err(error) => return api_error(&error, l, &headers),
    };
    let submissions = match submissions {
        Ok((data, _)) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    if !submissions["submissions"]
        .as_array()
        .is_some_and(|rows| rows.len() <= 100 && rows.iter().all(valid_submission))
    {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    }
    let submission = if let Some(edit_id) = editing {
        let rows = submissions["submissions"]
            .as_array()
            .expect("validated API list");
        let Some(row) = rows
            .iter()
            .find(|row| row["id"].as_i64() == edit_id.parse::<i64>().ok())
        else {
            return error_page(StatusCode::NOT_FOUND, l, keys::INVALID, &headers);
        };
        if kind.is_empty() {
            kind = row["kind"].as_str().unwrap_or_default().to_owned();
        }
        Some(row.clone())
    } else {
        None
    };
    if kind.is_empty() {
        kind = "panel".to_owned();
    }
    let name = guilds
        .ok()
        .map(|(guilds, _)| guild_name(&guilds, &id))
        .unwrap_or_else(|| id.clone());
    Html(views::ticket_gallery_publish(
        l,
        &headers,
        &id,
        &name,
        &sources,
        &kind,
        &source_id,
        submission.as_ref(),
        &app.nonces.fresh(),
        query.get("saved").is_some_and(|value| value == "1"),
    ))
    .into_response()
}

pub(super) async fn publish_post(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let allowed = [
        "source",
        "submission_id",
        "revision",
        "title",
        "description",
        "category",
        "tags",
        "author_name",
        "consent",
        "key",
    ];
    if form.keys().any(|key| !allowed.contains(&key.as_str())) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(key) = form.get("key").filter(|key| valid_key(key)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(source) = form.get("source").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some((kind, source_id)) = source.split_once(":") else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if !matches!(kind, "panel" | "tag" | "form") || !valid_source_id(source_id) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(title) = form.get("title").filter(|title| valid_text(title, 100)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(description) = form
        .get("description")
        .filter(|description| valid_text(description, 500))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(category) = form
        .get("category")
        .filter(|category| valid_category(category))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if form.get("consent").map(String::as_str) != Some("1") {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let tags = form
        .get("tags")
        .map(|tags| {
            tags.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let author_name = form
        .get("author_name")
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    if tags.len() > 3
        || tags.iter().any(|tag| !valid_text(tag, 24))
        || author_name.chars().count() > 80
        || has_controls(author_name)
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let submission_id = match form.get("submission_id") {
        Some(value) if valid_template_id(value) => Some(value.clone()),
        Some(_) => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
        None => None,
    };
    let (submission_id, data) = if let Some(submission_id) = submission_id {
        let revision = form
            .get("revision")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|revision| *revision > 0);
        let Some(revision) = revision else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        (
            Some(submission_id),
            json!({
                "action": "update",
                "revision": revision,
                "kind": kind,
                "source_id": source_id,
                "title": title,
                "description": description,
                "category": category,
                "tags": tags,
                "author_name": author_name,
                "consent": true,
            }),
        )
    } else {
        if form.contains_key("revision") {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        (
            None,
            json!({
                "kind": kind,
                "source_id": source_id,
                "title": title,
                "description": description,
                "category": category,
                "tags": tags,
                "author_name": author_name,
                "consent": true,
            }),
        )
    };
    let result = if let Some(submission_id) = submission_id {
        app.api
            .gallery_submission_action(&id, &submission_id, data, &actor, key)
            .await
    } else {
        app.api.gallery_submit(&id, data, &actor, key).await
    };
    match result {
        Ok(_) => {
            Redirect::to(&format!("/g/{id}/tickets/gallery/submissions?saved=1")).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

pub(super) async fn import(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || form
            .keys()
            .any(|key| !matches!(key.as_str(), "template_id" | "new_id" | "key"))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(template_id) = form.get("template_id").filter(|id| valid_template_id(id)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form.get("key").filter(|key| valid_key(key)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let (template, action_state) = tokio::join!(
        app.api.gallery_template(&id, template_id, &actor),
        app.api
            .ticket_action_state(&id, "gallery", "gallery", &actor)
    );
    let template = match template {
        Ok((data, _)) if valid_template(&data["template"]) => data["template"].clone(),
        Ok(_) => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
        Err(error) => return api_error(&error, l, &headers),
    };
    let etag = match action_state {
        Ok((_data, Some(etag))) => etag,
        Ok(_) => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
        Err(error) => return api_error(&error, l, &headers),
    };
    let kind = template["kind"].as_str().unwrap_or_default();
    let mut data = json!({
        "kind": kind,
        "name": template["title"],
        "payload": template["payload"],
    });
    let new_id = if kind == "tag" {
        let Some(new_id) = form.get("new_id").filter(|new_id| valid_tag_id(new_id)) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        data["new_id"] = json!(new_id);
        Some(new_id.clone())
    } else {
        if form.contains_key("new_id") {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        None
    };
    let import_result = app
        .api
        .ticket_action(
            &id,
            "gallery",
            "gallery",
            "gallery.import",
            data,
            &etag,
            &actor,
            key,
        )
        .await;
    let action = match import_result {
        Ok((data, _)) => match data
            .get("result")
            .filter(|result| result["kind"].as_str() == Some(kind))
        {
            Some(result) => result.clone(),
            None => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
        },
        Err(error) => return api_error(&error, l, &headers),
    };
    let imported_id = action["id"].as_str().filter(|id| valid_source_id(id));
    let Some(imported_id) = imported_id else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
    };
    if let Some(new_id) = new_id.as_deref() {
        if imported_id != new_id {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
        }
    } else {
        let field = if kind == "panel" { "panels" } else { "forms" };
        let definition = action.get("definition").filter(|definition| {
            definition.is_object() && definition["id"].as_str() == Some(imported_id)
        });
        let Some(definition) = definition else {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers);
        };
        let (config, etag) = match app.api.basic_config(&id).await {
            Ok((config, Some(etag))) => (config, etag),
            Ok(_) => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
            Err(error) => return api_error(&error, l, &headers),
        };
        let mut definitions = config["modules"]["tickets"]["config"][field]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(existing) = definitions
            .iter()
            .find(|item| item["id"].as_str() == Some(imported_id))
        {
            if existing != definition {
                return error_page(StatusCode::CONFLICT, l, keys::INVALID, &headers);
            }
        } else {
            definitions.push(definition.clone());
            let mut ticket_config = Map::new();
            ticket_config.insert(field.to_owned(), Value::Array(definitions));
            let patch =
                json!({ "modules": { "tickets": { "config": Value::Object(ticket_config) } } });
            if let Err(error) = app.api.put_basic_config(&id, patch, &etag, &actor).await {
                return api_error(&error, l, &headers);
            }
        }
    }
    if let Err(error) = app
        .api
        .gallery_record_import(&id, template_id, &actor, key)
        .await
    {
        return api_error(&error, l, &headers);
    }
    let target = match kind {
        "tag" => format!(
            "/g/{id}/tickets/tags?{}",
            serde_urlencoded::to_string([("saved", "1"), ("q", imported_id)]).unwrap_or_default()
        ),
        "panel" | "form" => format!("/g/{id}/modules/tickets?imported={kind}"),
        _ => return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, &headers),
    };
    Redirect::to(&target).into_response()
}

pub(super) async fn submission_action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    if !session::snowflake(&id)
        || form.keys().any(|key| {
            !matches!(
                key.as_str(),
                "action" | "submission_id" | "revision" | "key"
            )
        })
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(action) = form
        .get("action")
        .filter(|action| matches!(action.as_str(), "withdraw" | "unpublish"))
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(submission_id) = form.get("submission_id").filter(|id| valid_template_id(id)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(revision) = form
        .get("revision")
        .and_then(|revision| revision.parse::<i64>().ok())
        .filter(|revision| *revision > 0)
    else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let Some(key) = form.get("key").filter(|key| valid_key(key)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let data = json!({ "action": action, "revision": revision });
    match app
        .api
        .gallery_submission_action(&id, submission_id, data, &actor, key)
        .await
    {
        Ok(_) => {
            Redirect::to(&format!("/g/{id}/tickets/gallery/submissions?saved=1")).into_response()
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

fn valid_sources_state(value: &Value) -> bool {
    value["sources"].as_array().is_some_and(|sources| {
        sources.len() <= 1000
            && sources.iter().all(|source| {
                matches!(source["kind"].as_str(), Some("panel" | "tag" | "form"))
                    && source["id"].as_str().is_some_and(valid_source_id)
                    && source["name"].as_str().is_some()
            })
    })
}

fn valid_gallery_page(value: &Value, offset: u32) -> bool {
    value["templates"]
        .as_array()
        .is_some_and(|rows| rows.len() <= GALLERY_PAGE_SIZE as usize && rows.iter().all(valid_card))
        && value["featured"]
            .as_array()
            .is_some_and(|rows| rows.len() <= 6 && rows.iter().all(valid_card))
        && value["total"].as_u64().is_some()
        && value["offset"].as_u64() == Some(u64::from(offset))
        && (value["next_offset"].is_null() || value["next_offset"].as_u64().is_some())
}

fn valid_card(value: &Value) -> bool {
    value["id"].as_i64().is_some_and(|id| id > 0)
        && matches!(value["kind"].as_str(), Some("panel" | "tag" | "form"))
        && value["title"].as_str().is_some()
        && value["description"].as_str().is_some()
        && value["category"].as_str().is_some_and(valid_category)
        && value["tags"]
            .as_array()
            .is_some_and(|tags| tags.len() <= 3 && tags.iter().all(Value::is_string))
        && value["author_name"].as_str().is_some()
        && value["import_count"].as_u64().is_some()
        && value["created_at"].as_i64().is_some()
        && value["featured"].as_bool().is_some()
}

fn valid_template(value: &Value) -> bool {
    valid_card(value) && value["payload"].is_object()
}

fn valid_submission(value: &Value) -> bool {
    value["id"].as_i64().is_some_and(|id| id > 0)
        && matches!(value["kind"].as_str(), Some("panel" | "tag" | "form"))
        && value["title"].as_str().is_some()
        && value["description"].as_str().is_some()
        && value["category"].as_str().is_some_and(valid_category)
        && value["tags"]
            .as_array()
            .is_some_and(|tags| tags.len() <= 3 && tags.iter().all(Value::is_string))
        && value["payload"].is_object()
        && value["status"].as_str().is_some_and(|status| {
            matches!(
                status,
                "pending" | "approved" | "rejected" | "withdrawn" | "unpublished"
            )
        })
        && value["revision"]
            .as_i64()
            .is_some_and(|revision| revision > 0)
}

fn valid_template_id(value: &str) -> bool {
    value.parse::<i64>().ok().is_some_and(|id| id > 0)
}

fn valid_source_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_tag_id(value: &str) -> bool {
    (1..=16).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !has_controls(value)
}

fn has_controls(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn valid_category(value: &str) -> bool {
    matches!(
        value,
        "support" | "moderation" | "sales" | "applications" | "feedback" | "general" | "other"
    )
}

fn default_tag_id(title: &str) -> String {
    title
        .to_ascii_lowercase()
        .bytes()
        .filter(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        .take(16)
        .map(char::from)
        .collect()
}
