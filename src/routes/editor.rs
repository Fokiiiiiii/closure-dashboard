//! The pages that edit the structured settings of a module: its rules, counters and anti-raid.

use super::modules::{Data, conflict, load, shell_error};
use super::{
    App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, Value, api_error, error_page, escape, guild_name, keys, lang, user, valid_module,
    views,
};
use crate::{
    editor::{self, Errors, Node, State as Form, Why, one},
    form::{self, Field},
};
use serde_json::{Map, json};

/// What a page of the editor edits: the setting, its schema node and the module's other fields.
struct Target {
    fields: Vec<Field>,
    node: Node,
}

fn target(data: &Data, module: &str, part: &str) -> Option<Target> {
    let modules = form::modules(&data.schema);
    let fields = form::fields(modules.get(module).filter(|_| valid_module(module))?);
    let node = views::editor_node(views::editor_field(&fields, part)?)?;
    Some(Target { fields, node })
}

/// The setting as the form shows it: the stored one, else the configured one.
fn current<'a>(data: &'a Data, module: &str, part: &str) -> &'a Value {
    form::lookup(&data.stored["modules"][module]["config"], part)
        .filter(|v| !v.is_null())
        .or_else(|| form::lookup(&data.effective["modules"][module]["config"], part))
        .unwrap_or(&Value::Null)
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn not_found(app: &App, headers: &HeaderMap) -> Response {
    error_page(
        StatusCode::NOT_FOUND,
        lang(headers, app),
        keys::NOTFOUND,
        headers,
    )
}

fn is_key(key: &str) -> bool {
    form::pattern_matches(editor::ID_PATTERN, key)
}

/// The form of a request: its inputs, and the few that steer the page.
struct Posted {
    state: Form,
    etag: Option<String>,
    op: String,
    choice: HashMap<String, String>,
}

fn posted(pairs: Vec<(String, String)>) -> Posted {
    let mut out = Posted {
        state: Form::new(),
        etag: None,
        op: String::new(),
        choice: HashMap::new(),
    };
    for (name, value) in pairs {
        if name == "etag" {
            out.etag = Some(value);
        } else if name == "op" {
            out.op = value;
        } else if let Some(path) = name.strip_prefix("new.") {
            out.choice.insert(path.to_owned(), value);
        } else {
            out.state.entry(name).or_default().push(value);
        }
    }
    out
}

fn summary(l: crate::i18n::Locale<'_>, part: &str, value: &Value) -> String {
    if part == "rules" {
        views::rule_summary(l, value)
    } else {
        names(&value["triggers"]).join(", ")
    }
}

pub(super) async fn list(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
    saved: bool,
) -> Response {
    let l = lang(headers, app);
    let data = match load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(app, headers, id, &error),
    };
    let Some(target) = target(&data, module, part) else {
        return not_found(app, headers);
    };
    if part == "antiraid" {
        return item_page(
            app, headers, id, module, part, "", &data, &target, None, saved,
        );
    }
    let items: Vec<views::Item> = current(&data, module, part)
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, value)| views::Item {
            key: key.clone(),
            title: value["name"].as_str().unwrap_or(key).to_owned(),
            summary: summary(l, part, value),
            enabled: (part == "rules").then(|| value["enabled"].as_bool().unwrap_or(true)),
        })
        .collect();
    Html(views::collection_page(
        l,
        headers,
        id,
        guild_name(&data.guilds, id),
        module,
        part,
        &target.fields,
        &items,
        &data.etag,
        saved,
    ))
    .into_response()
}

#[allow(clippy::too_many_arguments)]
async fn write(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
    mut data: Data,
    etag: &str,
    value: Value,
) -> Option<Response> {
    let l = lang(headers, app);
    let Some(actor) = user(headers, app) else {
        return Some(Redirect::to("/login").into_response());
    };
    if etag != data.etag {
        return Some(conflict(app, headers, id, module, part));
    }
    let changes = HashMap::from([(part.to_owned(), value)]);
    if form::update(&mut data.stored, module, &changes).is_err() {
        return Some(error_page(
            StatusCode::BAD_GATEWAY,
            l,
            keys::INVALID,
            headers,
        ));
    }
    match app
        .api
        .put_basic_config(id, data.stored.clone(), etag, &actor)
        .await
    {
        Ok(_) => None,
        Err(e) if e.status == StatusCode::PRECONDITION_FAILED => {
            Some(conflict(app, headers, id, module, part))
        }
        Err(e) => Some(api_error(&e, l, headers)),
    }
}

/// The operations of the list: a rule or a counter is switched or deleted.
pub(super) async fn list_save(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let form_data = posted(pairs);
    let data = match load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(app, headers, id, &error),
    };
    if target(&data, module, part).is_none() || part == "antiraid" {
        return not_found(app, headers);
    }
    let mut map: Map<String, Value> = current(&data, module, part)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut operation = form_data.op.splitn(3, ':');
    match (operation.next(), operation.next(), operation.next()) {
        (Some("delete"), Some(key), _) => {
            map.remove(key);
        }
        (Some("toggle"), Some(key), Some(on)) if part == "rules" => {
            if let Some(rule) = map.get_mut(key).and_then(Value::as_object_mut) {
                rule.insert("enabled".into(), json!(on == "true"));
            }
        }
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers),
    }
    let Some(etag) = form_data.etag else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    match write(
        app,
        headers,
        id,
        module,
        part,
        data,
        &etag,
        Value::Object(map),
    )
    .await
    {
        None => {
            Redirect::to(&format!("{}?saved=1", views::part_path(id, module, part))).into_response()
        }
        Some(response) => response,
    }
}

pub(super) async fn item(
    State(app): State<App>,
    Path((id, module, part, key)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let data = match load(&app, &id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(&app, &headers, &id, &error),
    };
    let Some(target) = target(&data, &module, &part) else {
        return not_found(&app, &headers);
    };
    if !editor_collection(&part)
        || (key != "new" && !current(&data, &module, &part)[&key].is_object())
    {
        return not_found(&app, &headers);
    }
    let saved = query.get("saved").is_some_and(|v| v == "1");
    item_page(
        &app, &headers, &id, &module, &part, &key, &data, &target, None, saved,
    )
}

fn editor_collection(part: &str) -> bool {
    matches!(part, "rules" | "counters")
}

/// The state of a form that shows what is stored.
fn stored_state(node: &Node, value: &Value) -> Form {
    let mut state = Form::new();
    editor::flatten(node, "r", value, &mut state);
    state
}

/// The page of one rule, one counter, or the anti-raid. `posted` is the form as it was sent, when
/// the page comes back to it; the stored one shows otherwise.
#[allow(clippy::too_many_arguments)]
fn item_page(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
    key: &str,
    data: &Data,
    target: &Target,
    posted: Option<(Form, Errors, Vec<String>, StatusCode)>,
    saved: bool,
) -> Response {
    let l = lang(headers, app);
    let new = key == "new";
    let (node, base) = match (&target.node, editor_collection(part)) {
        (Node::Entries { value, .. }, true) => {
            ((**value).clone(), current(data, module, part)[key].clone())
        }
        (node, false) => (node.clone(), current(data, module, part).clone()),
        _ => return not_found(app, headers),
    };
    let (mut state, errors, banner, status) = posted.unwrap_or_else(|| {
        (
            stored_state(&node, &base),
            Errors::new(),
            Vec::new(),
            StatusCode::OK,
        )
    });
    if part == "antiraid" && !state.contains_key("on") && !base.is_null() {
        state.insert("on".into(), vec!["1".into()]);
    }
    let counters = names(current(data, module, "counters"));
    let levels: Vec<String> = current(data, module, "antiraid")["levels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let ctx = views::Ctx {
        l,
        directory: data.directory.as_ref(),
        counters: &counters,
        levels: &levels,
        errors: &errors,
    };
    let inner = match part {
        "rules" => views::rule_body(&ctx, &node, &state, new),
        "antiraid" => format!(
            "<section class=\"card ed-section\"><label class=\"ed-switch-label ed-master\"><input type=\"checkbox\" name=\"on\" value=\"1\"{}>{}</label></section>{}",
            if one(&state, "on") == "1" {
                " checked"
            } else {
                ""
            },
            l.text(keys::ED_ANTIRAID_ON),
            views::object_card(&ctx, &node, "r", &state, false)
        ),
        _ => views::object_card(&ctx, &node, "r", &state, new),
    };
    let title = if new {
        l.text(if part == "rules" {
            keys::ED_NEW_RULE
        } else {
            keys::ED_NEW_COUNTER
        })
    } else if editor_collection(part) {
        escape(base["name"].as_str().unwrap_or(key))
    } else {
        views::part_name(l, module, part)
    };
    let back = views::part_path(id, module, part);
    let action = if editor_collection(part) {
        format!("{back}/{}", escape(key))
    } else {
        back.clone()
    };
    let page = views::editor_page(
        l,
        headers,
        id,
        guild_name(&data.guilds, id),
        module,
        part,
        &target.fields,
        &title,
        &action,
        &back,
        &data.etag,
        &format!("{}{}", views::datalist_html(&ctx), inner),
        &banner,
        saved,
    );
    (status, Html(page)).into_response()
}

fn refuse_text(l: crate::i18n::Locale<'_>, why: Why) -> String {
    l.text(match why {
        Why::Required => keys::ED_ERR_REQUIRED,
        Why::Invalid => keys::ED_ERR_INVALID,
        Why::Repeated => keys::ED_ERR_REPEATED,
    })
}

/// Reads the posted form into the value to store, or says what is wrong with it.
fn read(
    part: &str,
    node: &Node,
    key: &str,
    form_data: &Posted,
    map: &Map<String, Value>,
) -> (Option<Value>, Errors) {
    let mut errors = Errors::new();
    let mut value = editor::parse(node, "r", &form_data.state, &mut errors);
    if part == "rules" {
        // A rule is on unless it says otherwise, so only a rule that is off writes it.
        if let Some(rule) = value.as_mut().and_then(Value::as_object_mut)
            && rule.get("enabled") == Some(&json!(true))
        {
            rule.remove("enabled");
        }
        for (name, label) in [("triggers", "r.triggers"), ("actions", "r.actions")] {
            if value.as_ref().is_none_or(|v| v[name].is_null()) {
                errors.push((label.to_owned(), Why::Required));
            }
        }
    }
    if key == "new" {
        let new_key = one(&form_data.state, "__id").trim();
        if !is_key(new_key) {
            errors.push(("__id".to_owned(), Why::Invalid));
        } else if map.contains_key(new_key) {
            errors.push(("__id".to_owned(), Why::Repeated));
        }
    }
    (value, errors)
}

pub(super) async fn item_save(
    State(app): State<App>,
    Path((id, module, part, key)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    save_item(&app, &headers, &id, &module, &part, &key, pairs).await
}

#[allow(clippy::too_many_lines)]
pub(super) async fn save_item(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
    key: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let mut form_data = posted(pairs);
    let data = match load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return shell_error(app, headers, id, &error),
    };
    let Some(target) = target(&data, module, part) else {
        return not_found(app, headers);
    };
    let single = part == "antiraid";
    let node = match (&target.node, single) {
        (Node::Entries { value, .. }, false) if editor_collection(part) => (**value).clone(),
        (node, true) => node.clone(),
        _ => return not_found(app, headers),
    };
    let operation = form_data.op.clone();
    if let Some(path) = operation.strip_prefix("add:") {
        let tag = form_data.choice.get(path).cloned();
        editor::add_row(&mut form_data.state, path, tag.as_deref());
    } else if let Some(row) = operation.strip_prefix("del:") {
        editor::remove_row(&mut form_data.state, row);
    } else if operation != "save" {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let Some(etag) = form_data.etag.clone() else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    if operation != "save" {
        return item_page(
            app,
            headers,
            id,
            module,
            part,
            key,
            &data,
            &target,
            Some((form_data.state, Errors::new(), Vec::new(), StatusCode::OK)),
            false,
        );
    }
    let map = current(&data, module, part)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let (value, errors) = read(part, &node, key, &form_data, &map);
    let switched_off = single && one(&form_data.state, "on") != "1";
    if !errors.is_empty() && !switched_off {
        let banner = vec![refuse_text(l, Why::Invalid)];
        return item_page(
            app,
            headers,
            id,
            module,
            part,
            key,
            &data,
            &target,
            Some((
                form_data.state,
                errors,
                banner,
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            false,
        );
    }
    let (stored, next) = if single {
        (
            if switched_off {
                Value::Null
            } else {
                value.unwrap_or_else(|| json!({}))
            },
            String::new(),
        )
    } else {
        let new_key = if key == "new" {
            one(&form_data.state, "__id").trim().to_owned()
        } else {
            key.to_owned()
        };
        let mut map = map;
        map.insert(new_key.clone(), value.unwrap_or_else(|| json!({})));
        (Value::Object(map), new_key)
    };
    match write(app, headers, id, module, part, data, &etag, stored).await {
        None => {
            let back = views::part_path(id, module, part);
            let to = if single {
                back
            } else {
                format!("{back}/{next}")
            };
            Redirect::to(&format!("{to}?saved=1")).into_response()
        }
        Some(response) => response,
    }
}
