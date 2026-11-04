use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, escape, guild_name, keys, lang, modules, user, views,
};
use crate::form::{self, Kind};
use serde_json::{Map, Value};

#[allow(clippy::too_many_arguments)]
fn render(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    data: &modules::Data,
    input: &HashMap<String, String>,
    errors: &HashMap<String, Vec<String>>,
    saved: bool,
    status: StatusCode,
) -> Response {
    let page = views::SettingsPage {
        id,
        guild: guild_name(&data.guilds, id),
        schema: &data.schema,
        stored: &data.stored,
        effective: &data.effective,
        directory: data.directory.as_ref(),
        etag: &data.etag,
        input,
        errors,
        saved,
    };
    (
        status,
        Html(views::settings_page(lang(headers, app), headers, &page)),
    )
        .into_response()
}

fn conflict(app: &App, headers: &HeaderMap, id: &str) -> Response {
    let l = lang(headers, app);
    (StatusCode::PRECONDITION_FAILED, Html(views::shell(l, headers, id, id, keys::SERVER_SETTINGS, &format!("<p role=\"alert\">{}</p><a class=\"button secondary\" href=\"/g/{}/settings\">{}</a>", l.text(keys::CONFLICT_RELOAD), escape(id), l.text(keys::RELOAD))))).into_response()
}

pub(super) async fn edit(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    match modules::load(&app, &id, l.bot_language()).await {
        Ok(data) => render(
            &app,
            &headers,
            &id,
            &data,
            &HashMap::new(),
            &HashMap::new(),
            query.get("saved").is_some_and(|v| v == "1"),
            StatusCode::OK,
        ),
        Err(e) => api_error(&e, l, &headers),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) async fn save(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let data = match modules::load(&app, &id, l.bot_language()).await {
        Ok(data) => data,
        Err(e) => return api_error(&e, l, &headers),
    };
    let fields = form::settings(&data.schema);
    let mut input = HashMap::<String, String>::new();
    let mut etag = None;
    let mut roles_present = false;
    for (name, value) in pairs {
        if name == "etag" {
            if etag.replace(value).is_some() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        } else if name == "roles_present" && value == "1" && !roles_present {
            roles_present = true;
        } else if let Some(name) = name.strip_prefix("f.") {
            let Some(field) = fields.iter().find(|f| f.name == name) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            if let Some(previous) = input.get_mut(name) {
                if field.kind != Kind::Roles || data.directory.is_none() {
                    return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
                }
                previous.push(',');
                previous.push_str(&value);
            } else {
                input.insert(name.to_owned(), value);
            }
        } else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
    }
    if roles_present {
        input.entry("moderator_roles".into()).or_default();
    }
    let Some(etag) = etag else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if etag != data.etag {
        return conflict(&app, &headers, &id);
    }
    let mut errors = HashMap::new();
    let mut changes = Map::new();
    for field in &fields {
        let Some(value) = input.get(&field.name) else {
            continue;
        };
        let Some(value) = form::parse(field, value) else {
            errors.insert(field.name.clone(), vec![l.text(keys::FIELD_INVALID)]);
            continue;
        };
        if form::settings_should_write(
            &value,
            data.stored.get(&field.name),
            data.effective.get(&field.name),
        ) {
            changes.insert(field.name.clone(), value);
        }
    }
    if !errors.is_empty() {
        return render(
            &app,
            &headers,
            &id,
            &data,
            &input,
            &errors,
            false,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    match app
        .api
        .put_basic_config(&id, Value::Object(changes), &etag, &actor)
        .await
    {
        Ok(_) => Redirect::to(&format!("/g/{id}/settings?saved=1")).into_response(),
        Err(e) if e.status == StatusCode::PRECONDITION_FAILED => conflict(&app, &headers, &id),
        Err(e)
            if e.status == StatusCode::UNPROCESSABLE_ENTITY
                || e.status == StatusCode::FORBIDDEN =>
        {
            let status = e.status;
            if status == StatusCode::FORBIDDEN {
                errors.insert(String::new(), vec![l.text(keys::DENIED)]);
            }
            for issue in e.issues {
                let key = if status == StatusCode::FORBIDDEN {
                    String::new()
                } else {
                    fields
                        .iter()
                        .find(|f| issue.path == f.name)
                        .map_or_else(String::new, |f| f.name.clone())
                };
                errors
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(issue.message);
            }
            if errors.is_empty() {
                errors.insert(
                    String::new(),
                    vec![if status == StatusCode::FORBIDDEN {
                        l.text(keys::DENIED)
                    } else {
                        e.message
                    }],
                );
            }
            render(&app, &headers, &id, &data, &input, &errors, false, status)
        }
        Err(e) => api_error(&e, l, &headers),
    }
}
