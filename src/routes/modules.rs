use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, Value, api_error, error_page, escape, guild_name, header, keys, lang, own_origin,
    user, valid_module, views,
};
use crate::form;

pub(super) struct Data {
    pub(super) guilds: Value,
    pub(super) schema: Value,
    pub(super) stored: Value,
    pub(super) effective: Value,
    pub(super) directory: Option<Value>,
    pub(super) etag: String,
}

pub(super) async fn load(app: &App, id: &str, language: &str) -> Result<Data, ApiError> {
    let (guilds, schema, stored, effective, directory) = tokio::join!(
        app.api.guilds(),
        app.api.basic_schema(language),
        app.api.basic_config(id),
        app.api.effective(id),
        app.api.directory(id)
    );
    let (stored, etag) = stored?;
    let Some(etag) = etag else {
        return Err(ApiError {
            status: StatusCode::BAD_GATEWAY,
            kind: crate::closure_api::ErrorKind::Other,
            code: String::new(),
            message: String::new(),
            issues: vec![],
        });
    };
    Ok(Data {
        guilds: guilds?.0,
        schema: schema?.0,
        stored,
        effective: effective?.0,
        directory: directory.ok().map(|v| v.0),
        etag,
    })
}

fn shell_error(app: &App, headers: &HeaderMap, id: &str, error: &ApiError) -> Response {
    let l = lang(headers, app);
    let key = match error.status {
        StatusCode::FORBIDDEN => keys::DENIED,
        StatusCode::SERVICE_UNAVAILABLE => keys::UNAVAILABLE,
        StatusCode::NOT_FOUND => keys::NOTFOUND,
        _ => keys::INVALID,
    };
    (
        error.status,
        Html(views::shell(
            l,
            headers,
            id,
            id,
            keys::MODULES,
            &format!("<p class=\"error\" role=\"alert\">{}</p>", l.text(key)),
        )),
    )
        .into_response()
}

pub(super) fn return_path(app: &App, headers: &HeaderMap, id: &str) -> String {
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<axum::http::Uri>().ok())
        .filter(|u| {
            u.scheme_str()
                .zip(u.authority())
                .is_some_and(|(s, a)| Some(format!("{s}://{a}")) == own_origin(app))
        })
        .and_then(|u| u.path_and_query().map(|p| p.as_str().to_owned()))
        .filter(|p| p.starts_with('/') && !p.starts_with("//") && !p.contains('\\'))
        .unwrap_or_else(|| format!("/g/{id}/modules"))
}

fn conflict(app: &App, headers: &HeaderMap, id: &str, module: &str) -> Response {
    let l = lang(headers, app);
    (StatusCode::PRECONDITION_FAILED, Html(views::shell(l, headers, id, id, keys::MODULES, &format!("<p role=\"alert\">{}</p><a class=\"button secondary\" href=\"/g/{}/modules/{}\">{}</a>", l.text(keys::CONFLICT_RELOAD), escape(id), escape(module), l.text(keys::RELOAD))))).into_response()
}

#[allow(clippy::too_many_arguments)]
fn render(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    data: &Data,
    input: &HashMap<String, String>,
    errors: &HashMap<String, Vec<String>>,
    saved: bool,
    status: StatusCode,
) -> Response {
    let modules = form::modules(&data.schema);
    let Some(schema) = modules.get(module).filter(|_| valid_module(module)) else {
        return error_page(
            StatusCode::BAD_REQUEST,
            lang(headers, app),
            keys::INVALID,
            headers,
        );
    };
    let page = views::FormPage {
        id,
        guild: guild_name(&data.guilds, id),
        module,
        schema,
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
        Html(views::form_page(lang(headers, app), headers, &page)),
    )
        .into_response()
}

pub(super) async fn list(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let l = lang(&headers, &app);
    match load(&app, &id, l.bot_language()).await {
        Ok(data) => Html(views::modules_page(
            l,
            &headers,
            &id,
            guild_name(&data.guilds, &id),
            &data.schema,
            &data.effective,
        ))
        .into_response(),
        Err(error) => shell_error(&app, &headers, &id, &error),
    }
}

pub(super) async fn edit(
    State(app): State<App>,
    Path((id, module)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    match load(&app, &id, l.bot_language()).await {
        Ok(data) => render(
            &app,
            &headers,
            &id,
            &module,
            &data,
            &HashMap::new(),
            &HashMap::new(),
            query.get("saved").is_some_and(|v| v == "1"),
            StatusCode::OK,
        ),
        Err(error) => shell_error(&app, &headers, &id, &error),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) async fn save(
    State(app): State<App>,
    Path((id, module)): Path<(String, String)>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let mut data = match load(&app, &id, l.bot_language()).await {
        Ok(v) => v,
        Err(e) => return shell_error(&app, &headers, &id, &e),
    };
    let modules = form::modules(&data.schema);
    let Some(schema) = modules.get(&module).filter(|_| valid_module(&module)) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let fields = form::fields(schema);
    let mut input = HashMap::new();
    let mut etag = None;
    for (name, value) in pairs {
        if name == "etag" {
            if etag.replace(value).is_some() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        } else if let Some(name) = name.strip_prefix("f.") {
            let Some(field) = fields.iter().find(|f| f.name == name) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            if let Some(previous) = input.insert(name.to_owned(), value.clone())
                && !(field.kind == form::Kind::Boolean && previous == "false" && value == "true")
            {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        } else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
    }
    let Some(etag) = etag else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if etag != data.etag {
        return conflict(&app, &headers, &id, &module);
    }
    let mut errors = HashMap::new();
    let mut changes = HashMap::new();
    for field in &fields {
        let Some(value) = input.get(&field.name) else {
            continue;
        };
        let Some(value) = form::parse(field, value) else {
            errors.insert(field.name.clone(), vec![l.text(keys::FIELD_INVALID)]);
            continue;
        };
        let stored = data.stored["modules"][&module]["config"].get(&field.name);
        let effective = data.effective["modules"][&module]["config"].get(&field.name);
        if form::should_write(&value, stored, effective) {
            changes.insert(field.name.clone(), value);
        }
    }
    if !errors.is_empty() {
        return render(
            &app,
            &headers,
            &id,
            &module,
            &data,
            &input,
            &errors,
            false,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    if !changes.is_empty() && form::update(&mut data.stored, &module, &changes).is_err() {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, &headers);
    }
    match app
        .api
        .put_basic_config(&id, data.stored.clone(), &etag, &actor)
        .await
    {
        Ok(_) => Redirect::to(&format!("/g/{id}/modules/{module}?saved=1")).into_response(),
        Err(e) if e.status == StatusCode::PRECONDITION_FAILED => {
            conflict(&app, &headers, &id, &module)
        }
        Err(e) if e.status == StatusCode::UNPROCESSABLE_ENTITY => {
            for issue in e.issues {
                let key = fields
                    .iter()
                    .find(|f| {
                        issue.path == f.name
                            || issue.path == format!("modules.{module}.config.{}", f.name)
                    })
                    .map_or_else(String::new, |f| f.name.clone());
                errors
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(issue.message);
            }
            if errors.is_empty() {
                errors.insert(String::new(), vec![e.message]);
            }
            render(
                &app,
                &headers,
                &id,
                &module,
                &data,
                &input,
                &errors,
                false,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
        Err(e) => api_error(&e, l, &headers),
    }
}
