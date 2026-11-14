use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, Value, api_error, error_page, escape, guild_name, header, keys, lang, own_origin,
    user, valid_module, views,
};
use crate::form;
use std::collections::HashSet;

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

pub(super) fn shell_error(app: &App, headers: &HeaderMap, id: &str, error: &ApiError) -> Response {
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
            false,
        )),
    )
        .into_response()
}

/// Whether the input `name` belongs to `field`: it is the field, or a cell of its rows.
fn in_field(field: &form::Field, name: &str) -> bool {
    name == field.name
        || (matches!(
            field.kind,
            form::Kind::List(_)
                | form::Kind::Entries
                | form::Kind::TextList
                | form::Kind::FormQuestions
        ) && name
            .strip_prefix(field.name.as_str())
            .is_some_and(|rest| rest.starts_with('.')))
}

fn role_input_value(name: &str, schema: &Value, mode: &str, values: Vec<String>) -> Option<String> {
    if mode == "default" {
        return Some(String::new());
    }
    if values.is_empty() || (values.len() == 1 && values[0].trim().is_empty()) {
        return Some("-".into());
    }
    if values.len() == 1 {
        return values.into_iter().next();
    }
    let mut role_schema = schema.clone();
    if let Some(object) = role_schema.as_object_mut() {
        object.remove("minItems");
        object.remove("maxItems");
    }
    let cell = form::Field {
        name: name.to_owned(),
        label: name.to_owned(),
        kind: form::Kind::Roles,
        schema: role_schema,
    };
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let Some(Value::Array(parsed)) = form::parse(&cell, &value) else {
            return None;
        };
        if parsed.len() != 1 {
            return None;
        }
        ids.push(parsed[0].as_str()?.to_owned());
    }
    Some(ids.join(","))
}

fn role_field(field: &form::Field) -> bool {
    field.kind == form::Kind::Roles && field.schema["x-ui"]["widget"] == "roles"
}

fn default_role_cell_text(
    effective: &Value,
    field: &form::Field,
    row: usize,
    column: &str,
) -> Option<String> {
    let form::Kind::List(columns) = &field.kind else {
        return None;
    };
    if !columns.iter().any(|cell| {
        cell.name == column
            && cell.kind == form::Kind::Roles
            && cell.schema["x-ui"]["widget"] == "roles"
    }) {
        return None;
    }
    let ids = form::lookup(effective, &field.name)?
        .as_array()?
        .get(row)?
        .get(column)?
        .as_array()?;
    let ids = ids.iter().map(Value::as_str).collect::<Option<Vec<_>>>()?;
    Some(if ids.is_empty() {
        "-".to_owned()
    } else {
        ids.join(",")
    })
}

fn role_column<'a>(field: &'a form::Field, name: &str) -> Option<(usize, &'a form::Column)> {
    let prefix = format!("{}.", field.name);
    let rest = name.strip_prefix(&prefix)?;
    let (row, column_name) = rest.split_once('.')?;
    let row = row.parse().ok()?;
    let form::Kind::List(columns) = &field.kind else {
        return None;
    };
    let column = columns.iter().find(|column| {
        column.name == column_name
            && column.kind == form::Kind::Roles
            && column.schema["x-ui"]["widget"] == "roles"
    })?;
    Some((row, column))
}

fn team_column<'a>(field: &'a form::Field, name: &str) -> Option<(usize, &'a form::Column)> {
    let (row, name) = list_cell(field, name)?;
    let form::Kind::List(columns) = &field.kind else {
        return None;
    };
    let column = columns.iter().find(|column| {
        column.name == name
            && column.kind == form::Kind::Words
            && column.schema["x-ui"]["widget"] == "ticket-staff-teams"
    })?;
    Some((row, column))
}

fn list_cell<'a>(field: &form::Field, name: &'a str) -> Option<(usize, &'a str)> {
    let rest = name.strip_prefix(&format!("{}.", field.name))?;
    let (row, column) = rest.split_once('.')?;
    Some((row.parse().ok()?, column))
}

fn current_list<'a>(data: &'a Data, module: &str, field: &form::Field) -> Option<&'a Value> {
    form::lookup(&data.stored["modules"][module]["config"], &field.name)
        .filter(|value| !value.is_null())
        .or_else(|| form::lookup(&data.effective["modules"][module]["config"], &field.name))
}

fn role_row_allowed(field: &form::Field, row: usize, current_len: usize) -> bool {
    if row < current_len {
        return true;
    }
    row == current_len
        && field.schema["maxItems"]
            .as_u64()
            .is_none_or(|max| (current_len as u64) < max)
}

fn role_defaults_for_compare(
    field: &form::Field,
    value: &Value,
    baseline: Option<&Value>,
    modes: &HashMap<String, String>,
    input: &HashMap<String, String>,
) -> Value {
    let form::Kind::List(columns) = &field.kind else {
        return value.clone();
    };
    let Some(baseline_rows) = baseline.and_then(Value::as_array) else {
        return value.clone();
    };
    let Some(value_rows) = value.as_array() else {
        return value.clone();
    };
    let prefix = format!("{}.", field.name);
    if value_rows.len() != baseline_rows.len()
        || input.iter().any(|(key, value)| {
            key.strip_prefix(&prefix)
                .and_then(|rest| rest.split_once('.'))
                .is_some_and(|(_, part)| part == "x" && value == "1")
        })
    {
        return value.clone();
    }
    let mut compared = value.clone();
    let Some(rows) = compared.as_array_mut() else {
        return compared;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        for column in columns.iter().filter(|column| {
            column.kind == form::Kind::Roles && column.schema["x-ui"]["widget"] == "roles"
        }) {
            let cell = format!("{}.{index}.{}", field.name, column.name);
            if modes.get(&cell).is_some_and(|mode| mode == "default") {
                if let Some(original) = baseline_rows
                    .get(index)
                    .and_then(|row| row.get(&column.name))
                    .filter(|value| !value.is_null())
                {
                    object.insert(column.name.clone(), original.clone());
                } else {
                    object.remove(&column.name);
                }
            }
        }
    }
    compared
}

fn has_role_columns(field: &form::Field) -> bool {
    matches!(&field.kind, form::Kind::List(columns) if columns.iter().any(|column| {
        column.kind == form::Kind::Roles && column.schema["x-ui"]["widget"] == "roles"
    }))
}

fn role_mode_sets(field: &form::Field, modes: &HashMap<String, String>) -> bool {
    if role_field(field) {
        modes.get(&field.name).is_some_and(|mode| mode == "set")
    } else {
        modes
            .iter()
            .any(|(cell, mode)| mode == "set" && role_column(field, cell).is_some())
    }
}

fn should_write_role_field(
    field: &form::Field,
    value: &Value,
    stored: Option<&Value>,
    effective: Option<&Value>,
    modes: &HashMap<String, String>,
) -> bool {
    form::should_write(value, stored, effective)
        || (role_mode_sets(field, modes) && stored.is_none())
}

fn unset_default_role_overrides(
    config: &mut Value,
    fields: &[form::Field],
    modes: &HashMap<String, String>,
) {
    let Some(config) = config.as_object_mut() else {
        return;
    };
    for (cell, mode) in modes {
        if mode != "default" {
            continue;
        }
        if let Some(field) = fields
            .iter()
            .find(|field| field.name == *cell && role_field(field))
        {
            config.remove(&field.name);
            continue;
        }
        if let Some(field) = fields
            .iter()
            .find(|field| role_column(field, cell).is_some())
            && let Some((row, column)) = role_column(field, cell)
            && let Some(row) = config
                .get_mut(&field.name)
                .and_then(Value::as_array_mut)
                .and_then(|rows| rows.get_mut(row))
                .and_then(Value::as_object_mut)
        {
            row.remove(&column.name);
        }
    }
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

pub(super) fn conflict(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
) -> Response {
    let l = lang(headers, app);
    (
        StatusCode::PRECONDITION_FAILED,
        Html(views::shell(
            l,
            headers,
            id,
            id,
            keys::MODULES,
            &format!(
                "<p role=\"alert\">{}</p><a class=\"button secondary\" href=\"{}\">{}</a>",
                l.text(keys::CONFLICT_RELOAD),
                escape(&views::part_path(id, module, part)),
                l.text(keys::RELOAD)
            ),
            true,
        )),
    )
        .into_response()
}

#[allow(clippy::too_many_arguments)]
fn render(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    module: &str,
    part: &str,
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
    let fields = form::fields(schema);
    let parts = form::parts(module, &fields);
    if part.is_empty() && !parts.is_empty() && !parts.contains(&"") {
        return Redirect::to(&views::part_path(id, module, parts[0])).into_response();
    }
    if !(parts.contains(&part) || part.is_empty() && parts.is_empty()) || form::is_editor_part(part)
    {
        return error_page(
            StatusCode::NOT_FOUND,
            lang(headers, app),
            keys::NOTFOUND,
            headers,
        );
    }
    let mut directory = data.directory.clone();
    if module == "tickets" {
        let directory = directory.get_or_insert_with(|| Value::Object(serde_json::Map::default()));
        directory["ticket_staff_teams"] =
            data.effective["modules"]["tickets"]["config"]["staff_teams"].clone();
        directory["ticket_forms"] = data.effective["forms"].clone();
    }
    let page = views::FormPage {
        id,
        guild: guild_name(&data.guilds, id),
        module,
        part,
        schema,
        stored: &data.stored,
        effective: &data.effective,
        directory: directory.as_ref(),
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
    edit_part(
        State(app),
        Path((id, module, String::new())),
        headers,
        Query(query),
    )
    .await
}

pub(super) async fn edit_part(
    State(app): State<App>,
    Path((id, module, part)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let part = part.as_str();
    if form::is_editor_part(part) {
        let saved = query.get("saved").is_some_and(|v| v == "1");
        return super::editor::list(&app, &headers, &id, &module, part, saved).await;
    }
    let l = lang(&headers, &app);
    match load(&app, &id, l.bot_language()).await {
        Ok(data) => render(
            &app,
            &headers,
            &id,
            &module,
            part,
            &data,
            &HashMap::new(),
            &HashMap::new(),
            query.get("saved").is_some_and(|v| v == "1"),
            StatusCode::OK,
        ),
        Err(error) => shell_error(&app, &headers, &id, &error),
    }
}

pub(super) async fn save(
    State(app): State<App>,
    Path((id, module)): Path<(String, String)>,
    headers: HeaderMap,
    form: axum::Form<Vec<(String, String)>>,
) -> Response {
    save_part(State(app), Path((id, module, String::new())), headers, form).await
}

fn form_question_options(prefix: &str, input: &HashMap<String, String>) -> Value {
    let prefix = format!("{prefix}.options.");
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .filter(|number| *number < 30)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut options = Vec::new();
    for number in numbers {
        let option_prefix = format!("{prefix}{number}");
        if input
            .get(&format!("{option_prefix}.x"))
            .is_some_and(|value| value == "1")
        {
            continue;
        }
        let mut option = serde_json::Map::new();
        for name in ["label", "value", "description"] {
            if let Some(value) = input
                .get(&format!("{option_prefix}.{name}"))
                .filter(|value| !value.is_empty())
            {
                option.insert(name.to_owned(), Value::String(value.clone()));
            }
        }
        if !option.is_empty() {
            options.push(Value::Object(option));
        }
    }
    Value::Array(options)
}

fn insert_form_question_string(
    object: &mut serde_json::Map<String, Value>,
    prefix: &str,
    name: &str,
    input: &HashMap<String, String>,
) {
    if let Some(value) = input
        .get(&format!("{prefix}.{name}"))
        .filter(|value| !value.is_empty())
    {
        object.insert(name.to_owned(), Value::String(value.clone()));
    }
}

fn insert_form_question_number(
    object: &mut serde_json::Map<String, Value>,
    prefix: &str,
    name: &str,
    input: &HashMap<String, String>,
) -> Option<()> {
    if let Some(value) = input
        .get(&format!("{prefix}.{name}"))
        .filter(|value| !value.is_empty())
    {
        object.insert(
            name.to_owned(),
            serde_json::json!(value.parse::<u64>().ok()?),
        );
    }
    Some(())
}

#[allow(clippy::too_many_lines)]
fn form_questions_json(
    field_name: &str,
    input: &HashMap<String, String>,
    stored: Option<&Value>,
) -> Option<String> {
    if input.contains_key(field_name) {
        return None;
    }
    let prefix = format!("{field_name}.");
    let mut numbers = Vec::new();
    for key in input.keys().filter(|key| key.starts_with(&prefix)) {
        let mut parts = key.strip_prefix(&prefix)?.split('.');
        let number = parts.next()?.parse::<usize>().ok()?;
        if number >= form::FORM_QUESTIONS_LIMIT.saturating_add(5) {
            return None;
        }
        match parts.next()? {
            "x" if parts.next().is_none() && input.get(key).is_some_and(|value| value == "1") => {}
            "type" | "label" | "required" | "description" | "content" | "style" | "min_length"
            | "max_length" | "placeholder" | "min_values" | "max_values"
                if parts.next().is_none() => {}
            "options" => {
                let option = parts.next()?.parse::<usize>().ok()?;
                if option >= 30 {
                    return None;
                }
                match parts.next()? {
                    "x" if parts.next().is_none()
                        && input.get(key).is_some_and(|value| value == "1") => {}
                    "label" | "value" | "description" if parts.next().is_none() => {}
                    _ => return None,
                }
            }
            _ => return None,
        }
        numbers.push(number);
    }
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.is_empty() {
        return None;
    }

    let mut questions = Vec::new();
    for number in numbers {
        let row = format!("{field_name}.{number}");
        if input
            .get(&format!("{row}.x"))
            .is_some_and(|value| value == "1")
        {
            continue;
        }
        let kind = input.get(&format!("{row}.type"))?.as_str();
        let label = input
            .get(&format!("{row}.label"))
            .map_or("", String::as_str);
        if kind == "legacy" {
            if !label.is_empty() {
                questions.push(Value::String(label.to_owned()));
            }
            continue;
        }
        let content = input
            .get(&format!("{row}.content"))
            .map_or("", String::as_str);
        let has_options = input.keys().any(|key| {
            key.starts_with(&format!("{row}.options."))
                && input.get(key).is_some_and(|value| !value.is_empty())
        });
        let has_content = !label.is_empty()
            || !content.is_empty()
            || [
                "description",
                "min_length",
                "max_length",
                "placeholder",
                "min_values",
                "max_values",
            ]
            .iter()
            .any(|name| {
                input
                    .get(&format!("{row}.{name}"))
                    .is_some_and(|value| !value.is_empty())
            })
            || has_options;
        if !has_content {
            continue;
        }
        if !matches!(
            kind,
            "text"
                | "string_select"
                | "user_select"
                | "role_select"
                | "mentionable_select"
                | "channel_select"
                | "radio_group"
                | "checkbox_group"
                | "display"
        ) {
            return None;
        }
        let mut question = serde_json::Map::new();
        question.insert("type".to_owned(), Value::String(kind.to_owned()));
        if kind == "display" {
            insert_form_question_string(&mut question, &row, "content", input);
        } else {
            insert_form_question_string(&mut question, &row, "label", input);
            insert_form_question_string(&mut question, &row, "description", input);
            question.insert(
                "required".to_owned(),
                Value::Bool(
                    input
                        .get(&format!("{row}.required"))
                        .is_some_and(|value| value == "true"),
                ),
            );
            match kind {
                "text" => {
                    insert_form_question_string(&mut question, &row, "style", input);
                    insert_form_question_string(&mut question, &row, "placeholder", input);
                    insert_form_question_number(&mut question, &row, "min_length", input)?;
                    insert_form_question_number(&mut question, &row, "max_length", input)?;
                }
                "string_select" | "radio_group" | "checkbox_group" => {
                    question.insert("options".to_owned(), form_question_options(&row, input));
                    if kind != "radio_group" {
                        insert_form_question_number(&mut question, &row, "min_values", input)?;
                        insert_form_question_number(&mut question, &row, "max_values", input)?;
                    }
                }
                "user_select" | "role_select" | "mentionable_select" | "channel_select" => {
                    insert_form_question_number(&mut question, &row, "min_values", input)?;
                    insert_form_question_number(&mut question, &row, "max_values", input)?;
                }
                _ => return None,
            }
        }
        // The editor cannot show dynamic options; keep the stored ones of the same question.
        let stored_question = stored
            .and_then(Value::as_array)
            .and_then(|questions| questions.get(number));
        if let Some(dynamic) = stored_question.and_then(|stored| stored.get("dynamic_options"))
            && matches!(kind, "string_select" | "radio_group" | "checkbox_group")
        {
            if question
                .get("options")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                question.remove("options");
            }
            question.insert("dynamic_options".to_owned(), dynamic.clone());
        }
        questions.push(Value::Object(question));
        if questions.len() > form::FORM_QUESTIONS_LIMIT {
            return None;
        }
    }
    Some(Value::Array(questions).to_string())
}

fn prepare_form_question_inputs(
    fields: &[form::Field],
    input: &mut HashMap<String, String>,
    config: &Value,
) {
    let mut cells: Vec<(String, Option<Value>)> = Vec::new();
    for field in fields {
        match &field.kind {
            form::Kind::FormQuestions => {
                if input.contains_key(&field.name)
                    || input
                        .keys()
                        .any(|key| key.starts_with(&format!("{}.", field.name)))
                {
                    cells.push((field.name.clone(), config.get(&field.name).cloned()));
                }
            }
            form::Kind::List(columns)
                if columns
                    .iter()
                    .any(|column| column.kind == form::Kind::FormQuestions) =>
            {
                let prefix = format!("{}.", field.name);
                let mut rows: Vec<usize> = input
                    .keys()
                    .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
                    .collect();
                rows.sort_unstable();
                rows.dedup();
                for row in rows {
                    for column in columns
                        .iter()
                        .filter(|column| column.kind == form::Kind::FormQuestions)
                    {
                        let cell = format!("{prefix}{row}.");
                        let cell = format!("{cell}{}", column.name);
                        if input.contains_key(&cell)
                            || input.keys().any(|key| key.starts_with(&format!("{cell}.")))
                        {
                            let stored = config
                                .get(&field.name)
                                .and_then(|rows| rows.get(row))
                                .and_then(|question| question.get(&column.name))
                                .cloned();
                            cells.push((cell, stored));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    cells.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    cells.dedup_by(|left, right| left.0 == right.0);
    for (cell, stored) in cells {
        let value =
            form_questions_json(&cell, input, stored.as_ref()).unwrap_or_else(|| "{".to_owned());
        input.insert(cell, value);
    }
}

fn hours_panel_references_valid(fields: &[form::Field], config: &Value) -> bool {
    let Some(field) = fields.iter().find(|field| field.name == "hours") else {
        return true;
    };
    let form::Kind::List(columns) = &field.kind else {
        return true;
    };
    if !columns
        .iter()
        .any(|column| column.name == "panel" && column.schema["x-ui"]["widget"] == "ticket-panel")
    {
        return true;
    }
    let Some(hours) = config.get("hours") else {
        return true;
    };
    if hours.is_null() {
        return true;
    }
    let Some(hours) = hours.as_array() else {
        return false;
    };
    let mut panel_counts = HashMap::<&str, usize>::new();
    for panel in config["panels"].as_array().into_iter().flatten() {
        if let Some(id) = panel["id"].as_str().filter(|id| !id.is_empty()) {
            *panel_counts.entry(id).or_default() += 1;
        }
    }
    hours.iter().all(|row| match row.get("panel") {
        None | Some(Value::Null) => true,
        Some(Value::String(id)) => panel_counts.get(id.as_str()) == Some(&1),
        Some(_) => false,
    })
}

fn global_timezone_is_valid(config: &Value) -> bool {
    match config.get("timezone") {
        None => true,
        Some(Value::String(timezone)) => timezone.parse::<chrono_tz::Tz>().is_ok(),
        Some(_) => false,
    }
}

fn panel_timezones_are_valid(config: &Value) -> bool {
    match config.get("panels") {
        None | Some(Value::Null) => true,
        Some(Value::Array(panels)) => panels.iter().all(|panel| {
            let Some(panel) = panel.as_object() else {
                return false;
            };
            match panel.get("timezone") {
                None | Some(Value::Null) => true,
                Some(Value::String(timezone)) => timezone.parse::<chrono_tz::Tz>().is_ok(),
                Some(_) => false,
            }
        }),
        Some(_) => false,
    }
}

fn hours_intervals_are_valid(config: &Value) -> bool {
    match config.get("hours") {
        None | Some(Value::Null) => true,
        Some(Value::Array(hours)) => hours.iter().all(|window| {
            let Some(start) = window.get("start_minute").and_then(Value::as_i64) else {
                return false;
            };
            let Some(end) = window.get("end_minute").and_then(Value::as_i64) else {
                return false;
            };
            start != end
        }),
        Some(_) => false,
    }
}

fn ticket_hours_error(
    fields: &[form::Field],
    config: &Value,
) -> Option<(&'static str, &'static str)> {
    let field = fields.iter().find(|field| field.name == "hours")?;
    let form::Kind::List(columns) = &field.kind else {
        return None;
    };
    if !columns
        .iter()
        .any(|column| column.name == "panel" && column.schema["x-ui"]["widget"] == "ticket-panel")
    {
        return None;
    }
    if !global_timezone_is_valid(config) {
        return Some(("timezone", "hours-invalid-timezone"));
    }
    if !panel_timezones_are_valid(config) {
        return Some(("panels", "hours-invalid-timezone"));
    }
    if !hours_panel_references_valid(fields, config) {
        return Some(("hours", "hours-panel-reference-invalid"));
    }
    if !hours_intervals_are_valid(config) {
        return Some(("hours", "hours-invalid-interval"));
    }
    None
}

#[allow(clippy::too_many_lines)]
pub(super) async fn save_part(
    State(app): State<App>,
    Path((id, module, part)): Path<(String, String, String)>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let part = part.as_str();
    if form::is_editor_part(part) {
        return if part == "antiraid" {
            super::editor::save_item(&app, &headers, &id, &module, part, "", pairs).await
        } else {
            super::editor::list_save(&app, &headers, &id, &module, part, pairs).await
        };
    }
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
    let mut role_values = HashMap::<String, Vec<String>>::new();
    let mut team_values = HashMap::<String, Vec<String>>::new();
    let mut role_modes = HashMap::<String, String>::new();
    let mut etag = None;
    for (name, value) in pairs {
        if name == "etag" {
            if etag.replace(value).is_some() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        } else if let Some(cell) = name.strip_prefix("role_mode:") {
            if role_modes.insert(cell.to_owned(), value).is_some() {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
        } else if let Some(name) = name.strip_prefix("f.") {
            let Some(field) = fields.iter().find(|f| in_field(f, name)) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            if has_role_columns(field) {
                let Some((row, column)) = list_cell(field, name) else {
                    return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
                };
                let form::Kind::List(columns) = &field.kind else {
                    return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
                };
                let current_len = current_list(&data, &module, field)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let nested_question_editor = columns.iter().any(|cell| {
                    (matches!(cell.kind, form::Kind::TextList | form::Kind::FormQuestions)
                        && column.strip_prefix(&format!("{}.", cell.name)).is_some())
                        || (matches!(&cell.kind, form::Kind::NestedList(_))
                            && (column.strip_prefix(&format!("{}.", cell.name)).is_some()
                                || column == format!("{}_present", cell.name)))
                });
                if !role_row_allowed(field, row, current_len)
                    || (column != "x"
                        && !columns.iter().any(|cell| cell.name == column)
                        && !nested_question_editor)
                {
                    return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
                }
            }
            if team_column(field, name).is_some() {
                team_values.entry(name.to_owned()).or_default().push(value);
                continue;
            }
            if role_column(field, name).is_some() || (role_field(field) && field.name == name) {
                role_values.entry(name.to_owned()).or_default().push(value);
                continue;
            }
            // A switch is sent twice, as "false" and then "true" when it is on; a list holds switches.
            if let Some(previous) = input.insert(name.to_owned(), value.clone())
                && !(matches!(field.kind, form::Kind::Boolean | form::Kind::List(_))
                    && previous == "false"
                    && value == "true")
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
        return conflict(&app, &headers, &id, &module, part);
    }
    let valid_teams: HashSet<String> =
        data.effective["modules"]["tickets"]["config"]["staff_teams"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|team| team["id"].as_str().map(str::to_owned))
            .chain(std::iter::once("default".to_owned()))
            .collect();
    for (cell, values) in team_values {
        let Some(field) = fields
            .iter()
            .find(|field| team_column(field, &cell).is_some())
        else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let Some((row, _)) = team_column(field, &cell) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let current_len = current_list(&data, &module, field)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if !role_row_allowed(field, row, current_len) {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        let empty_count = values.iter().filter(|value| value.is_empty()).count();
        let selected: Vec<&str> = values
            .iter()
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .collect();
        let unique: HashSet<&str> = selected.iter().copied().collect();
        let invalid = empty_count > 1
            || unique.len() != selected.len()
            || selected.iter().any(|team| !valid_teams.contains(*team));
        let raw = if selected.is_empty() {
            "-".to_owned()
        } else {
            selected.join(",")
        };
        input.insert(cell, raw);
        if invalid {
            let errors = HashMap::from([(String::new(), vec![l.text(keys::FIELD_INVALID)])]);
            return render(
                &app,
                &headers,
                &id,
                &module,
                part,
                &data,
                &input,
                &errors,
                false,
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    }
    for (cell, mode) in &role_modes {
        if mode != "default" && mode != "set" {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        if let Some(field) = fields
            .iter()
            .find(|field| field.name == *cell && role_field(field))
        {
            let values = role_values.remove(cell).unwrap_or_default();
            let Some(raw) = role_input_value(cell, &field.schema, mode, values) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            if mode == "default" {
                input.remove(cell);
            } else {
                input.insert(cell.clone(), raw);
            }
            input.insert(format!("role-mode:{cell}"), mode.clone());
            continue;
        }
        let Some(field) = fields
            .iter()
            .find(|field| role_column(field, cell).is_some())
        else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let Some((row, column)) = role_column(field, cell) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let current_len = current_list(&data, &module, field)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if !role_row_allowed(field, row, current_len) || (mode == "default" && column.required) {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        let values = role_values.remove(cell).unwrap_or_default();
        let Some(mut raw) = role_input_value(cell, &column.schema, mode, values) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        if mode == "default"
            && let Some(value) = default_role_cell_text(
                &data.effective["modules"][&module]["config"],
                field,
                row,
                &column.name,
            )
        {
            raw = value;
        }
        input.insert(cell.clone(), raw);
        input.insert(format!("role-mode:{cell}"), mode.clone());
    }
    for (cell, values) in role_values {
        if values.len() != 1 {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        let Some(field) = fields.iter().find(|field| {
            (field.name == cell && role_field(field)) || role_column(field, &cell).is_some()
        }) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let mode = if values[0].trim().is_empty() {
            "default"
        } else {
            "set"
        };
        if role_field(field) && mode == "default" {
            input.remove(&cell);
        } else if mode == "default" {
            let Some((row, column)) = role_column(field, &cell) else {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            };
            let value = default_role_cell_text(
                &data.effective["modules"][&module]["config"],
                field,
                row,
                &column.name,
            )
            .unwrap_or_else(|| values[0].clone());
            input.insert(cell.clone(), value);
        } else {
            input.insert(cell.clone(), values[0].clone());
        }
        input.insert(format!("role-mode:{cell}"), mode.to_owned());
        role_modes.insert(cell, mode.to_owned());
    }
    prepare_form_question_inputs(
        &fields,
        &mut input,
        &data.stored["modules"][&module]["config"],
    );
    let mut errors = HashMap::new();
    let mut changes = HashMap::new();
    for field in &fields {
        // A list or a set of entries that is not in the default configuration is an empty one.
        let (read, empty) = match &field.kind {
            form::Kind::List(columns) => (
                form::rows(field, columns, &input).ok_or_else(|| l.text(keys::FIELD_INVALID)),
                Value::Array(Vec::new()),
            ),
            form::Kind::Entries => (
                form::entries(field, &input),
                Value::Object(serde_json::Map::new()),
            ),
            form::Kind::TextList => (
                form::text_list(field, &input).ok_or_else(|| l.text(keys::FIELD_INVALID)),
                Value::Array(Vec::new()),
            ),
            form::Kind::FormQuestions => (
                form::parse(field, input.get(&field.name).map_or("", String::as_str))
                    .ok_or_else(|| l.text(keys::FIELD_INVALID)),
                Value::Array(Vec::new()),
            ),
            _ => (Err(String::new()), Value::Null),
        };
        if matches!(
            field.kind,
            form::Kind::List(_)
                | form::Kind::Entries
                | form::Kind::TextList
                | form::Kind::FormQuestions
        ) {
            if input.keys().any(|name| in_field(field, name)) {
                match read {
                    Ok(value) => {
                        let config = &data.stored["modules"][&module]["config"];
                        let effective = &data.effective["modules"][&module]["config"];
                        let stored =
                            form::lookup(config, &field.name).filter(|value| !value.is_null());
                        let baseline = form::lookup(effective, &field.name).or(Some(&empty));
                        let should_write = if matches!(
                            field.kind,
                            form::Kind::TextList | form::Kind::FormQuestions
                        ) {
                            form::text_list_should_write(&value, stored, baseline)
                        } else {
                            let compared = if stored.is_none() {
                                role_defaults_for_compare(
                                    field,
                                    &value,
                                    baseline,
                                    &role_modes,
                                    &input,
                                )
                            } else {
                                value.clone()
                            };
                            should_write_role_field(field, &compared, stored, baseline, &role_modes)
                        };
                        if should_write {
                            changes.insert(field.name.clone(), value);
                        }
                    }
                    Err(message) => {
                        errors.insert(field.name.clone(), vec![message]);
                    }
                }
            }
            continue;
        }
        let Some(value) = input.get(&field.name) else {
            continue;
        };
        if let Some((name, expected)) = form::dependency(&module, &field.name) {
            let selected = input.get(name).map(String::as_str).or_else(|| {
                data.stored["modules"][&module]["config"]
                    .get(name)
                    .filter(|v| !v.is_null())
                    .or_else(|| data.effective["modules"][&module]["config"].get(name))
                    .and_then(Value::as_str)
            });
            if selected != Some(expected) {
                continue;
            }
        }
        let Some(value) = form::parse(field, value) else {
            errors.insert(field.name.clone(), vec![l.text(keys::FIELD_INVALID)]);
            continue;
        };
        let stored = data.stored["modules"][&module]["config"]
            .get(&field.name)
            .filter(|value| !value.is_null());
        let effective = data.effective["modules"][&module]["config"].get(&field.name);
        if should_write_role_field(field, &value, stored, effective, &role_modes) {
            changes.insert(field.name.clone(), value);
        }
    }
    if !errors.is_empty() {
        return render(
            &app,
            &headers,
            &id,
            &module,
            part,
            &data,
            &input,
            &errors,
            false,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    if changes
        .keys()
        .any(|field| matches!(field.as_str(), "hours" | "panels" | "timezone"))
    {
        let mut candidate = data.effective.clone();
        if form::update(&mut candidate, &module, &changes).is_err() {
            return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, &headers);
        }
        if let Some((field, message)) =
            ticket_hours_error(&fields, &candidate["modules"][&module]["config"])
        {
            errors.insert(field.to_owned(), vec![l.text(message)]);
            return render(
                &app,
                &headers,
                &id,
                &module,
                part,
                &data,
                &input,
                &errors,
                false,
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    }
    if !changes.is_empty() && form::update(&mut data.stored, &module, &changes).is_err() {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, &headers);
    }
    if let Some(config) = data
        .stored
        .get_mut("modules")
        .and_then(|modules| modules.get_mut(&module))
        .and_then(|module| module.get_mut("config"))
    {
        unset_default_role_overrides(config, &fields, &role_modes);
    }
    match app
        .api
        .put_basic_config(&id, data.stored.clone(), &etag, &actor)
        .await
    {
        Ok(_) => Redirect::to(&format!("{}?saved=1", views::part_path(&id, &module, part)))
            .into_response(),
        Err(e) if e.status == StatusCode::PRECONDITION_FAILED => {
            conflict(&app, &headers, &id, &module, part)
        }
        Err(e) if e.status == StatusCode::UNPROCESSABLE_ENTITY => {
            for issue in e.issues {
                let key = fields
                    .iter()
                    .find(|f| {
                        let path = format!("modules.{module}.config.{}", f.name);
                        issue.path == f.name
                            || issue.path == path
                            || (matches!(
                                f.kind,
                                form::Kind::List(_)
                                    | form::Kind::Entries
                                    | form::Kind::TextList
                                    | form::Kind::FormQuestions
                            ) && issue.path.starts_with(&path))
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
                part,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilding_a_question_keeps_its_dynamic_options() {
        let stored = serde_json::json!([{
            "type": "string_select",
            "label": "Pick",
            "dynamic_options": {"items_pointer": "/items"}
        }]);
        let input = HashMap::from([
            ("questions.0.type".to_owned(), "string_select".to_owned()),
            ("questions.0.label".to_owned(), "Pick".to_owned()),
        ]);
        let rebuilt = form_questions_json("questions", &input, Some(&stored)).unwrap();
        let questions: serde_json::Value = serde_json::from_str(&rebuilt).unwrap();
        assert_eq!(
            questions[0]["dynamic_options"],
            serde_json::json!({"items_pointer": "/items"}),
            "the editor does not show dynamic options, so rebuilding must keep them"
        );
        assert!(
            questions[0].get("options").is_none(),
            "an empty static option list would turn a dynamic question into a static one"
        );
    }

    fn role_list(max_items: Option<u64>) -> form::Field {
        let mut schema = serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": {"type": "string"},
                    "category": {"type": "string"},
                    "staff_roles": {
                        "type": "array",
                        "items": {"type": "string"},
                        "x-ui": {"widget": "roles"}
                    }
                }
            }
        });
        if let Some(max_items) = max_items {
            schema["maxItems"] = serde_json::json!(max_items);
        }
        let module = serde_json::json!({
            "properties": {
                "config": {"properties": {"panels": schema}}
            }
        });
        form::fields(&module).remove(0)
    }

    #[test]
    fn ticket_hours_save_rejects_invalid_time_zones_and_zero_length_windows() {
        let schema = serde_json::json!({
            "properties": { "config": { "properties": {
                "timezone": { "type": "string" },
                "hours": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "panel": {
                            "anyOf": [{ "type": "string" }, { "type": "null" }],
                            "x-ui": { "widget": "ticket-panel" }
                        },
                        "start_minute": {
                            "type": "integer", "minimum": 0, "maximum": 1439,
                            "x-ui": { "widget": "time-minute", "max": 1439 }
                        },
                        "end_minute": {
                            "type": "integer", "minimum": 0, "maximum": 1440,
                            "x-ui": { "widget": "time-minute", "max": 1440 }
                        }
                    }}
                },
                "panels": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "id": { "type": "string" },
                        "timezone": { "type": "string" }
                    }}
                }
            }}}
        });
        let fields = form::fields(&schema);
        let valid = serde_json::json!({
            "timezone": "Asia/Tokyo",
            "hours": [{
                "panel": "billing",
                "start_minute": 480,
                "end_minute": 1020
            }],
            "panels": [{ "id": "billing", "timezone": null }]
        });
        assert_eq!(ticket_hours_error(&fields, &valid), None);

        let invalid_global_timezone = serde_json::json!({
            "timezone": "Mars/Olympus",
            "hours": [{
                "panel": "billing",
                "start_minute": 480,
                "end_minute": 1020
            }],
            "panels": [{ "id": "billing", "timezone": null }]
        });
        assert_eq!(
            ticket_hours_error(&fields, &invalid_global_timezone),
            Some(("timezone", "hours-invalid-timezone"))
        );

        let invalid_panel_timezone = serde_json::json!({
            "timezone": "Asia/Tokyo",
            "hours": [{
                "panel": "billing",
                "start_minute": 480,
                "end_minute": 1020
            }],
            "panels": [{ "id": "billing", "timezone": "Mars/Olympus" }]
        });
        assert_eq!(
            ticket_hours_error(&fields, &invalid_panel_timezone),
            Some(("panels", "hours-invalid-timezone"))
        );

        let zero_length_window = serde_json::json!({
            "timezone": "Asia/Tokyo",
            "hours": [{
                "panel": "billing",
                "start_minute": 480,
                "end_minute": 480
            }],
            "panels": [{ "id": "billing", "timezone": null }]
        });
        assert_eq!(
            ticket_hours_error(&fields, &zero_length_window),
            Some(("hours", "hours-invalid-interval"))
        );
    }

    #[test]
    fn ticket_hours_panel_references_must_resolve_once() {
        let schema = serde_json::json!({
            "properties": { "config": { "properties": {
                "hours": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "panel": {
                            "anyOf": [{ "type": "string" }, { "type": "null" }],
                            "x-ui": { "widget": "ticket-panel" }
                        }
                    }}
                },
                "panels": {
                    "type": "array",
                    "items": { "type": "object", "properties": {
                        "id": { "type": "string" }
                    }}
                }
            }}}
        });
        let fields = form::fields(&schema);
        let valid = serde_json::json!({
            "hours": [
                { "panel": "billing" },
                { "panel": null },
                { "weekday": "monday" }
            ],
            "panels": [{ "id": "billing" }]
        });
        assert!(hours_panel_references_valid(&fields, &valid));

        let missing = serde_json::json!({
            "hours": [{ "panel": "deleted" }],
            "panels": [{ "id": "billing" }]
        });
        assert!(!hours_panel_references_valid(&fields, &missing));

        let duplicate = serde_json::json!({
            "hours": [{ "panel": "billing" }],
            "panels": [{ "id": "billing" }, { "id": "billing" }]
        });
        assert!(!hours_panel_references_valid(&fields, &duplicate));

        let default_schedule = serde_json::json!({
            "hours": [{ "panel": null }],
            "panels": []
        });
        assert!(hours_panel_references_valid(&fields, &default_schedule));
    }

    #[test]
    fn role_inputs_match_only_the_configured_role_field_and_existing_row_range() {
        let field = role_list(Some(2));

        assert!(role_column(&field, "panels.0.staff_roles").is_some());
        assert!(role_column(&field, "panels.0.category").is_none());
        assert!(role_column(&field, "panels.0.staff_roles.other").is_none());
        assert!(role_row_allowed(&field, 0, 1));
        assert!(role_row_allowed(&field, 1, 1));
        assert!(!role_row_allowed(&field, 2, 2));
        assert!(!role_row_allowed(&field, 999, 1));

        let unbounded = role_list(None);
        assert!(role_row_allowed(&unbounded, 2, 2));
        assert!(!role_row_allowed(&unbounded, 999, 2));
    }

    #[test]
    fn an_unset_role_cell_compares_as_its_effective_value() {
        let field = role_list(None);
        let baseline = serde_json::json!([{
            "id": "panel",
            "staff_roles": ["13", "99"]
        }]);
        let submitted = serde_json::json!([{"id": "panel"}]);
        let modes = HashMap::from([("panels.0.staff_roles".to_owned(), "default".to_owned())]);
        let input = HashMap::from([("panels.0.staff_roles".to_owned(), String::new())]);

        let compared =
            role_defaults_for_compare(&field, &submitted, Some(&baseline), &modes, &input);
        assert_eq!(compared, baseline);
        assert!(!should_write_role_field(
            &field,
            &compared,
            None,
            Some(&baseline),
            &modes,
        ));
    }

    #[test]
    fn explicit_empty_roles_are_written_even_when_the_effective_value_is_empty() {
        let field = role_list(None);
        let empty = serde_json::json!([{"id": "panel", "staff_roles": []}]);
        let modes = HashMap::from([("panels.0.staff_roles".to_owned(), "set".to_owned())]);

        assert!(should_write_role_field(
            &field,
            &empty,
            None,
            Some(&empty),
            &modes,
        ));
    }

    #[test]
    fn removing_a_row_never_reuses_the_next_rows_default_roles() {
        let field = role_list(None);
        let baseline = serde_json::json!([
            {"id": "first", "staff_roles": ["13"]},
            {"id": "second", "staff_roles": ["99"]}
        ]);
        let submitted = serde_json::json!([{"id": "second"}]);
        let modes = HashMap::from([("panels.1.staff_roles".to_owned(), "default".to_owned())]);
        let input = HashMap::from([("panels.0.x".to_owned(), "1".to_owned())]);

        assert_eq!(
            role_defaults_for_compare(&field, &submitted, Some(&baseline), &modes, &input,),
            submitted
        );
    }

    #[test]
    fn root_role_widgets_use_the_same_schema_limited_mode() {
        let module = serde_json::json!({
            "properties": {
                "config": {"properties": {
                    "staff_roles": {
                        "type": "array",
                        "items": {"type": "string"},
                        "x-ui": {"widget": "roles"}
                    }
                }}
            }
        });
        let field = form::fields(&module).remove(0);
        let modes = HashMap::from([("staff_roles".to_owned(), "set".to_owned())]);

        assert!(role_field(&field));
        assert!(role_mode_sets(&field, &modes));
        assert!(
            role_input_value(
                "staff_roles",
                &field.schema,
                "set",
                vec!["13".into(), "99".into()]
            )
            .is_some_and(|value| value == "13,99")
        );
    }

    #[test]
    fn deleting_an_earlier_row_keeps_default_roles_at_the_original_index() {
        let field = role_list(None);
        let effective = serde_json::json!({
            "panels": [
                {"id": "first", "staff_roles": ["13"]},
                {"id": "second", "staff_roles": ["99"]}
            ]
        });
        let roles = default_role_cell_text(&effective, &field, 1, "staff_roles").unwrap();
        let input = HashMap::from([
            ("panels.0.x".to_owned(), "1".to_owned()),
            ("panels.1.id".to_owned(), "second".to_owned()),
            ("panels.1.staff_roles".to_owned(), roles),
        ]);
        let form::Kind::List(columns) = &field.kind else {
            panic!("test schema must be a list");
        };

        assert_eq!(
            form::rows(&field, columns, &input),
            Some(serde_json::json!([{"id": "second", "staff_roles": ["99"]}]))
        );
    }

    #[test]
    fn defaulting_a_root_role_field_removes_the_saved_override() {
        let module = serde_json::json!({
            "properties": {
                "config": {"properties": {
                    "staff_roles": {
                        "type": "array",
                        "items": {"type": "string"},
                        "x-ui": {"widget": "roles"}
                    }
                }}
            }
        });
        let field = form::fields(&module).remove(0);
        let mut config = serde_json::json!({"staff_roles": ["13"], "other": ["99"]});
        let modes = HashMap::from([("staff_roles".to_owned(), "default".to_owned())]);

        assert!(role_field(&field));
        unset_default_role_overrides(&mut config, &[field], &modes);

        assert_eq!(config, serde_json::json!({"other": ["99"]}));
    }
}
