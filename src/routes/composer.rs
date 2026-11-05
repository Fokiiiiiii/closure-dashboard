//! The composer pages: a message to send now, and the settings that hold a message.

use super::modules::{conflict, load, shell_error};
use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, Value, api_error, error_page, escape, guild_name, keys, lang, user, valid_module,
    views,
};
use crate::{
    composer::{self, Form},
    form::{self, Kind},
};

/// What a request of the composer says: the form, and the few inputs that steer it.
struct Posted {
    form: Form,
    op: String,
    channel: String,
    etag: Option<String>,
}

fn posted(pairs: Vec<(String, String)>) -> Posted {
    let mut out = Posted {
        form: Form::new(),
        op: String::new(),
        channel: String::new(),
        etag: None,
    };
    for (name, value) in pairs {
        match name.as_str() {
            "op" => out.op = value,
            "channel" => out.channel = value,
            "etag" => out.etag = Some(value),
            _ => {
                out.form.insert(name, value);
            }
        }
    }
    out
}

/// Runs the operations that only change the form: a row added or taken out. `false` for any other.
fn edit(form: &mut Form, op: &str) -> bool {
    if op == "add_embed" {
        composer::add_embed(form);
    } else if let Some(n) = op.strip_prefix("addf:").and_then(|n| n.parse().ok()) {
        composer::add_field(form, n);
    } else if let Some(row) = op.strip_prefix("del:").filter(|r| r.starts_with("e.")) {
        composer::remove(form, row);
    } else {
        return false;
    }
    true
}

fn samples(l: crate::i18n::Locale<'_>, names: &[String]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|name| {
            let sample = l
                .find(&format!("ed-sample-{name}"))
                .unwrap_or_else(|| format!("{{{name}}}"));
            (name.clone(), sample)
        })
        .collect()
}

fn text_channels(directory: Option<&Value>) -> Vec<(String, String)> {
    directory
        .map(|d| &d["channels"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| matches!(c["kind"].as_str(), Some("text" | "announcement")))
        .filter_map(|c| Some((c["id"].as_str()?.to_owned(), c["name"].as_str()?.to_owned())))
        .collect()
}

fn fragment(l: crate::i18n::Locale<'_>, form: &Form, names: &[String]) -> Response {
    Html(views::composer_preview(l, form, &samples(l, names))).into_response()
}

pub(super) async fn send_page(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    send_view(&app, &id, &headers, Form::new(), String::new(), Vec::new()).await
}

async fn send_view(
    app: &App,
    id: &str,
    headers: &HeaderMap,
    form: Form,
    channel: String,
    banners: Vec<(bool, String)>,
) -> Response {
    let l = lang(headers, app);
    let (guilds, directory) = tokio::join!(app.api.guilds(), app.api.directory(id));
    let Ok((guilds, _)) = guilds else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, headers);
    };
    let directory = directory.ok().map(|v| v.0);
    let side = views::ComposerSide::Send {
        channels: text_channels(directory.as_ref()),
        chosen: &channel,
    };
    Html(views::composer_page(
        l,
        headers,
        id,
        guild_name(&guilds, id),
        &l.text(keys::COMPOSE),
        None,
        &format!("/g/{id}/compose"),
        &form,
        &side,
        &[],
        &banners,
    ))
    .into_response()
}

pub(super) async fn send_post(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let l = lang(&headers, &app);
    let mut request = posted(pairs);
    if query.contains_key("preview") {
        return fragment(l, &request.form, &[]);
    }
    if edit(&mut request.form, &request.op) {
        return send_view(
            &app,
            &id,
            &headers,
            request.form,
            request.channel,
            Vec::new(),
        )
        .await;
    }
    if request.op != "send" {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let message = composer::parse(&request.form);
    let problem = if request.channel.is_empty() {
        Some(l.text(keys::CP_CHOOSE_CHANNEL))
    } else if message.as_object().is_some_and(serde_json::Map::is_empty) {
        Some(l.text(keys::CP_NOTHING))
    } else {
        None
    };
    if let Some(problem) = problem {
        let banners = vec![(false, problem)];
        return send_view(&app, &id, &headers, request.form, request.channel, banners).await;
    }
    let banner = match app
        .api
        .send_message(&id, &request.channel, message, &actor)
        .await
    {
        Ok((sent, _)) => {
            let url = sent["url"].as_str().unwrap_or("");
            (
                true,
                format!(
                    "{} <a href=\"{}\" rel=\"noopener noreferrer\" target=\"_blank\">{}</a>",
                    l.text(keys::CP_SENT),
                    escape(url),
                    l.text(keys::CP_OPEN)
                ),
            )
        }
        Err(e) => (
            false,
            escape(&if e.message.is_empty() {
                l.text(keys::INVALID)
            } else {
                e.message
            }),
        ),
    };
    send_view(
        &app,
        &id,
        &headers,
        request.form,
        request.channel,
        vec![banner],
    )
    .await
}

/// The setting of a module that holds a message, and the variables it may use.
fn message_field(data: &super::modules::Data, module: &str, field: &str) -> Option<Vec<String>> {
    let modules = form::modules(&data.schema);
    let fields = form::fields(modules.get(module).filter(|_| valid_module(module))?);
    match &fields.iter().find(|f| f.name == field)?.kind {
        Kind::Message(variables) => Some(variables.clone()),
        _ => None,
    }
}

async fn setting_view(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    (module, field): (&str, &str),
    posted: Option<(Form, Vec<(bool, String)>)>,
    saved: bool,
) -> Response {
    let l = lang(headers, app);
    let data = match load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(app, headers, id, &error),
    };
    let Some(variables) = message_field(&data, module, field) else {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, headers);
    };
    let (form, mut banners) = posted.unwrap_or_else(|| {
        let config = &data.stored["modules"][module]["config"];
        let current = form::lookup(config, field)
            .filter(|v| !v.is_null())
            .or_else(|| form::lookup(&data.effective["modules"][module]["config"], field))
            .unwrap_or(&Value::Null);
        (composer::flatten(current), Vec::new())
    });
    if saved {
        banners.push((true, l.text(keys::SAVED)));
    }
    let part = form::part_of(module, field);
    let back = views::part_path(id, module, part);
    let title = format!("{} › {}", views::part_name(l, module, part), escape(field));
    let side = views::ComposerSide::Setting {
        variables: &variables,
    };
    let page = views::composer_page(
        l,
        headers,
        id,
        guild_name(&data.guilds, id),
        &title,
        Some(&back),
        &format!("/g/{id}/modules/{module}/compose/{field}"),
        &form,
        &side,
        &samples(l, &variables),
        &banners,
    );
    Html(page).into_response()
}

pub(super) async fn setting_page(
    State(app): State<App>,
    Path((id, module, field)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let saved = query.get("saved").is_some_and(|v| v == "1");
    setting_view(&app, &headers, &id, (&module, &field), None, saved).await
}

pub(super) async fn setting_post(
    State(app): State<App>,
    Path((id, module, field)): Path<(String, String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let l = lang(&headers, &app);
    let mut request = posted(pairs);
    if query.contains_key("preview") {
        let names = match load(&app, &id, l.bot_language()).await {
            Ok(data) => message_field(&data, &module, &field).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        return fragment(l, &request.form, &names);
    }
    if edit(&mut request.form, &request.op) {
        let shown = Some((request.form, Vec::new()));
        return setting_view(&app, &headers, &id, (&module, &field), shown, false).await;
    }
    if request.op != "save" {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let mut data = match load(&app, &id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(&app, &headers, &id, &error),
    };
    if message_field(&data, &module, &field).is_none() {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
    }
    let value = composer::setting(composer::parse(&request.form));
    let changes = HashMap::from([(field.clone(), value)]);
    if form::update(&mut data.stored, &module, &changes).is_err() {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, &headers);
    }
    let etag = request.etag.unwrap_or_else(|| data.etag.clone());
    match app
        .api
        .put_basic_config(&id, data.stored.clone(), &etag, &actor)
        .await
    {
        Ok(_) => Redirect::to(&format!("/g/{id}/modules/{module}/compose/{field}?saved=1"))
            .into_response(),
        Err(e) if e.status == StatusCode::PRECONDITION_FAILED => {
            conflict(&app, &headers, &id, &module, form::part_of(&module, &field))
        }
        Err(e) if e.status == StatusCode::UNPROCESSABLE_ENTITY => {
            let mut banners = vec![(false, escape(&e.message))];
            banners.extend(e.issues.into_iter().map(|i| (false, escape(&i.message))));
            let shown = Some((request.form, banners));
            setting_view(&app, &headers, &id, (&module, &field), shown, false).await
        }
        Err(e) => api_error(&e, l, &headers),
    }
}
