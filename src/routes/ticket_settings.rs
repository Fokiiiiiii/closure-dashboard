use super::{
    ApiError, App, HashMap, HeaderMap, Html, IntoResponse, Path, Query, Redirect, Response, State,
    StatusCode, api_error, error_page, guild_name, keys, lang, modules, user, views,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

const TEAM_PERMISSIONS: [(&str, bool); 10] = [
    ("add_reactions", true),
    ("send_messages", true),
    ("send_tts_messages", true),
    ("embed_links", true),
    ("attach_files", true),
    ("mention_everyone", false),
    ("use_external_emojis", true),
    ("use_application_commands", true),
    ("use_external_stickers", true),
    ("send_voice_messages", true),
];

#[derive(Default)]
struct TeamRow {
    id: Option<String>,
    name: Option<String>,
    roles: Vec<String>,
    roles_present: bool,
    users: Vec<String>,
    users_present: bool,
    permissions: HashMap<String, Vec<String>>,
    remove: bool,
    member_query: Option<String>,
}

#[derive(Default)]
struct TeamForm {
    intent: Option<String>,
    etag: Option<String>,
    present: bool,
    rows: BTreeMap<usize, TeamRow>,
    member_search: Option<usize>,
    input: HashMap<String, String>,
}

#[derive(Default)]
struct IntegrationForm {
    intent: bool,
    etag: Option<String>,
    present: bool,
    input: HashMap<String, String>,
}

fn schema_property<'a>(
    root: &'a Value,
    schema: &'a Value,
    name: &str,
    depth: u8,
) -> Option<&'a Value> {
    if depth > 8 {
        return None;
    }
    if let Some(property) = schema["properties"].get(name) {
        return Some(property);
    }
    if let Some(reference) = schema["$ref"].as_str() {
        let target = root.pointer(reference.strip_prefix('#')?)?;
        return schema_property(root, target, name, depth + 1);
    }
    for branch in schema["anyOf"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(schema["oneOf"].as_array().into_iter().flatten())
        .chain(schema["allOf"].as_array().into_iter().flatten())
        .filter(|branch| branch["type"] != "null")
    {
        if let Some(property) = schema_property(root, branch, name, depth + 1) {
            return Some(property);
        }
    }
    None
}

fn ticket_schema(schema: &Value) -> Option<&Value> {
    let modules = schema_property(schema, schema, "modules", 0)?;
    schema_property(schema, modules, "tickets", 0)
}

fn ticket_config_schema(schema: &Value) -> Option<&Value> {
    let tickets = ticket_schema(schema)?;
    schema_property(schema, tickets, "config", 0)
}

fn ticket_teams_schema(schema: &Value) -> Option<&Value> {
    let config = ticket_config_schema(schema)?;
    schema_property(schema, config, "staff_teams", 0)
}

fn ticket_integrations_schema(schema: &Value) -> Option<&Value> {
    let config = ticket_config_schema(schema)?;
    schema_property(schema, config, "integrations", 0)
}

/// The advanced schema and document: integrations are not in the basic view.
async fn advanced_document(
    app: &App,
    id: &str,
    language: &str,
) -> Result<(Value, Value, String), ApiError> {
    let (schema, stored) = tokio::join!(app.api.schema(language), app.api.config(id));
    let (schema, _) = schema?;
    let (stored, etag) = stored?;
    let etag = etag.ok_or_else(ApiError::unavailable)?;
    Ok((schema, stored, etag))
}

fn set_ticket_integrations(document: &mut Value, integrations: Value) -> Option<()> {
    let config = document
        .as_object_mut()?
        .entry("modules")
        .or_insert_with(|| json!({}))
        .as_object_mut()?
        .entry("tickets")
        .or_insert_with(|| json!({}))
        .as_object_mut()?
        .entry("config")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    config.insert("integrations".to_owned(), integrations);
    Some(())
}

fn ticket_config(effective: &Value) -> &Value {
    &effective["modules"]["tickets"]["config"]
}

fn valid_panel_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn scope_value(kind: &str, id: &str) -> String {
    if kind == "guild" {
        "guild".into()
    } else {
        format!("panel:{id}")
    }
}

fn api_message(error: &ApiError, l: crate::i18n::Locale<'_>) -> String {
    match error.status {
        StatusCode::FORBIDDEN => l.text(keys::DENIED),
        StatusCode::SERVICE_UNAVAILABLE => l.text(keys::UNAVAILABLE),
        StatusCode::NOT_FOUND => l.text(keys::NOTFOUND),
        _ if error.message.is_empty() => l.text(keys::INVALID),
        _ => error.message.clone(),
    }
}

fn hex_key(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decimal_snowflake(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.as_bytes()[0] != b'0'
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn integration_id_valid(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn generation_valid(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn integration_secret_name_valid(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn integration_declares_secret(integration: &Value, name: &str) -> bool {
    (name == "token" && matches!(integration["auth"].as_str(), Some("bearer" | "api_key")))
        || (integration_secret_name_valid(name)
            && integration["secrets"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|secret| secret.as_str() == Some(name)))
}

fn integration_may_have_secrets(integration: &Value) -> bool {
    matches!(integration["auth"].as_str(), Some("bearer" | "api_key"))
        || integration["secrets"]
            .as_array()
            .is_some_and(|secrets| !secrets.is_empty())
}

fn integration_secret_error(error: &ApiError, l: crate::i18n::Locale<'_>) -> String {
    match error.status {
        StatusCode::FORBIDDEN => l.text(keys::TICKET_INTEGRATION_SECRETS_OWNER_ONLY),
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => l.text(keys::CONFLICT),
        StatusCode::SERVICE_UNAVAILABLE => l.text(keys::UNAVAILABLE),
        StatusCode::NOT_FOUND => l.text(keys::NOTFOUND),
        _ => l.text(keys::INVALID),
    }
}

fn team_id_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value != "default"
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn permission_value(values: &[String]) -> Option<bool> {
    match values {
        [value] if value == "false" => Some(false),
        [first, second] if first == "false" && second == "true" => Some(true),
        _ => None,
    }
}

fn parse_team_form(pairs: Vec<(String, String)>) -> Option<TeamForm> {
    let mut form = TeamForm::default();
    for (name, value) in pairs {
        match name.as_str() {
            "intent" if form.intent.is_none() => form.intent = Some(value),
            "etag" if form.etag.is_none() => form.etag = Some(value),
            "staff_teams_present" if !form.present && value == "1" => form.present = true,
            "member_search" if form.member_search.is_none() => {
                form.member_search = Some(value.parse().ok()?);
            }
            _ => {
                let rest = name.strip_prefix("team.")?;
                let (index, field) = rest.split_once('.')?;
                let index = index.parse::<usize>().ok()?;
                if index > 10_000 {
                    return None;
                }
                let row = form.rows.entry(index).or_default();
                match field {
                    "id" if row.id.is_none() => row.id = Some(value),
                    "name" if row.name.is_none() => row.name = Some(value),
                    "roles_present" if !row.roles_present && value == "1" => {
                        row.roles_present = true;
                    }
                    "roles" => row.roles.push(value),
                    "users_present" if !row.users_present && value == "1" => {
                        row.users_present = true;
                    }
                    "users" => row.users.push(value),
                    "remove" if !row.remove && value == "1" => row.remove = true,
                    "member_query" if row.member_query.is_none() => row.member_query = Some(value),
                    _ if field.starts_with("permissions.") => {
                        let permission = field.strip_prefix("permissions.")?;
                        if !TEAM_PERMISSIONS.iter().any(|(name, _)| *name == permission) {
                            return None;
                        }
                        row.permissions
                            .entry(permission.to_owned())
                            .or_default()
                            .push(value);
                    }
                    _ => return None,
                }
            }
        }
    }
    for (index, row) in &form.rows {
        let prefix = format!("team.{index}");
        if let Some(id) = &row.id {
            form.input.insert(format!("{prefix}.id"), id.clone());
        }
        if let Some(name) = &row.name {
            form.input.insert(format!("{prefix}.name"), name.clone());
        }
        if row.roles_present {
            form.input
                .insert(format!("{prefix}.roles_present"), "1".into());
            form.input.insert(
                format!("{prefix}.roles"),
                if row.roles.is_empty() {
                    "-".into()
                } else {
                    row.roles.join(",")
                },
            );
        }
        if row.users_present {
            form.input
                .insert(format!("{prefix}.users_present"), "1".into());
            form.input.insert(
                format!("{prefix}.users"),
                if row.users.is_empty() {
                    "-".into()
                } else {
                    row.users.join(",")
                },
            );
        }
        if row.remove {
            form.input.insert(format!("{prefix}.remove"), "1".into());
        }
        if let Some(query) = &row.member_query {
            form.input
                .insert(format!("{prefix}.member_query"), query.clone());
        }
        for (permission, default) in TEAM_PERMISSIONS {
            if let Some(values) = row.permissions.get(permission) {
                let value = permission_value(values)?;
                form.input.insert(
                    format!("{prefix}.permissions.{permission}"),
                    value.to_string(),
                );
            } else {
                let _ = default;
            }
        }
    }
    Some(form)
}

fn exact_fields(pairs: Vec<(String, String)>) -> Option<HashMap<String, String>> {
    let mut fields = HashMap::new();
    for (name, value) in pairs {
        if fields.insert(name, value).is_some() {
            return None;
        }
    }
    Some(fields)
}

fn parse_integration_form(pairs: Vec<(String, String)>) -> Option<IntegrationForm> {
    let mut form = IntegrationForm::default();
    for (name, value) in pairs {
        match name.as_str() {
            "intent" if !form.intent && value == "integrations" => form.intent = true,
            "etag" if form.etag.is_none() => form.etag = Some(value),
            "integrations_present" if !form.present && value == "1" => form.present = true,
            _ => {
                let field = name.strip_prefix("f.")?;
                let rest = field.strip_prefix("integrations.")?;
                let row = rest.split('.').next()?.parse::<usize>().ok()?;
                if row > 20 {
                    return None;
                }
                if let Some(previous) = form.input.insert(field.to_owned(), value.clone())
                    && !(field.ends_with(".send_form_data")
                        && previous == "false"
                        && value == "true")
                {
                    return None;
                }
            }
        }
    }
    (form.intent && form.present && form.etag.is_some()).then_some(form)
}

fn team_error(_l: crate::i18n::Locale<'_>, message: &str) -> HashMap<String, Vec<String>> {
    HashMap::from([(String::new(), vec![message.to_owned()])])
}

fn panel_refs_after_team_removal(effective: &Value, removed: &HashSet<String>) -> Option<Value> {
    if removed.is_empty() {
        return None;
    }
    let mut panels = ticket_config(effective)["panels"].as_array()?.clone();
    let mut changed = false;
    for panel in &mut panels {
        let Some(teams) = panel["staff_teams"].as_array_mut() else {
            continue;
        };
        let before = teams.len();
        teams.retain(|team| !team.as_str().is_some_and(|id| removed.contains(id)));
        changed |= before != teams.len();
    }
    changed.then_some(Value::Array(panels))
}

fn config_patch(teams: Value, panels: Option<Value>) -> Value {
    let mut config = Map::new();
    config.insert("staff_teams".into(), teams);
    if let Some(panels) = panels {
        config.insert("panels".into(), panels);
    }
    let mut tickets = Map::new();
    tickets.insert("config".into(), Value::Object(config));
    let mut modules = Map::new();
    modules.insert("tickets".into(), Value::Object(tickets));
    let mut root = Map::new();
    root.insert("modules".into(), Value::Object(modules));
    Value::Object(root)
}

fn role_exists(directory: Option<&Value>, guild: &str, id: &str) -> bool {
    directory
        .and_then(|directory| directory["roles"].as_array())
        .into_iter()
        .flatten()
        .any(|role| {
            role["id"].as_str() == Some(id)
                && id != guild
                && role["managed"] != true
                && role["name"] != "@everyone"
        })
}

fn saved_roles(current: &Value, submitted: &TeamRow) -> Option<Vec<Value>> {
    let values: Vec<String> = if submitted.roles_present {
        submitted.roles.clone()
    } else {
        current["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let mut unique = HashSet::new();
    values
        .into_iter()
        .map(|id| {
            (decimal_snowflake(&id) && unique.insert(id.clone())).then_some(Value::String(id))
        })
        .collect()
}

fn saved_users(current: &Value, submitted: &TeamRow) -> Option<Vec<Value>> {
    let values: Vec<String> = if submitted.users_present {
        submitted.users.clone()
    } else {
        current["users"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let mut unique = HashSet::new();
    values
        .into_iter()
        .map(|id| {
            (decimal_snowflake(&id) && unique.insert(id.clone())).then_some(Value::String(id))
        })
        .collect()
}

fn saved_permissions(submitted: &TeamRow) -> Option<Map<String, Value>> {
    let mut permissions = Map::new();
    for (name, _) in TEAM_PERMISSIONS {
        let values = submitted.permissions.get(name)?;
        permissions.insert(name.to_owned(), json!(permission_value(values)?));
    }
    Some(permissions)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn render(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    data: &modules::Data,
    input: &HashMap<String, String>,
    errors: &HashMap<String, Vec<String>>,
    saved: bool,
    _scope_kind: &str,
    _scope_id: &str,
    secret_saved: bool,
    secret_error: Option<String>,
    member_search_kind: Option<&str>,
    member_search_index: Option<usize>,
    member_results: Value,
    status: StatusCode,
) -> Response {
    let l = lang(headers, app);
    let blacklist_state = Value::Null;
    let blacklist_panels = data.effective["modules"]["tickets"]["config"]["panels"].clone();
    let blacklist_etag = String::new();
    let role_add_key = String::new();
    let remove_keys = Vec::new();
    let member_keys = Vec::new();
    let teams_schema = ticket_teams_schema(&data.schema);
    let advanced = advanced_document(app, id, l.bot_language()).await.ok();
    let no_document = Value::Null;
    let advanced_stored = advanced
        .as_ref()
        .map_or(&no_document, |(_, stored, _)| stored);
    let integrations_schema = advanced
        .as_ref()
        .and_then(|(schema, _, _)| ticket_integrations_schema(schema));
    let (integration_secret_state, integration_secrets_available, can_manage_secrets) =
        if integrations_schema.is_some() {
            match app.api.ticket_integration_secrets(id, actor).await {
                Ok((response, _)) => {
                    let state = response.get("state").cloned().unwrap_or(Value::Null);
                    let available = state.is_object();
                    let can_manage = state["can_manage_secrets"] == true;
                    (state, available, can_manage)
                }
                Err(_) => (Value::Null, false, false),
            }
        } else {
            (Value::Null, false, false)
        };
    let mut integration_action_keys = HashMap::new();
    if can_manage_secrets {
        for integration in ticket_config(&data.effective)["integrations"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(generation)) = (
                integration["id"].as_str(),
                integration["generation"]
                    .as_str()
                    .filter(|value| generation_valid(value)),
            ) else {
                continue;
            };
            let mut names: Vec<&str> = integration["secrets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|name| *name != "token")
                .collect();
            if matches!(integration["auth"].as_str(), Some("bearer" | "api_key")) {
                names.push("token");
            }
            for name in names {
                let prefix = format!("{id}:{generation}:{name}");
                integration_action_keys.insert(format!("{prefix}:set"), app.nonces.fresh());
                integration_action_keys.insert(format!("{prefix}:remove"), app.nonces.fresh());
            }
        }
    }
    let team_count = ticket_config(&data.effective)["staff_teams"]
        .as_array()
        .map_or(0, Vec::len);
    let new_team_id = input
        .get(&format!("team.{team_count}.id"))
        .cloned()
        .unwrap_or_else(|| app.nonces.fresh());
    let page = views::TicketSettingsPage {
        id,
        guild: guild_name(&data.guilds, id),
        effective: &data.effective,
        stored: advanced_stored,
        directory: data.directory.as_ref(),
        integrations_schema,
        integration_secret_state: &integration_secret_state,
        integration_action_keys: &integration_action_keys,
        integration_secrets_available,
        can_manage_secrets,
        integration_secret_error: secret_error.as_deref(),
        secret_saved,
        etag: &data.etag,
        input,
        errors,
        teams_available: teams_schema.is_some(),
        teams_schema,
        module_available: ticket_schema(&data.schema).is_some(),
        saved,
        new_team_id: &new_team_id,
        scope_kind: "guild",
        scope_id: "guild",
        blacklist_state: &blacklist_state,
        blacklist_panels: &blacklist_panels,
        blacklist_guild_available: false,
        blacklist_etag: &blacklist_etag,
        blacklist_available: false,
        action_saved: false,
        action_error: None,
        member_search_kind,
        member_search_index,
        member_results: &member_results,
        role_add_key: &role_add_key,
        remove_keys: &remove_keys,
        member_keys: &member_keys,
    };
    (status, Html(views::ticket_settings_page(l, headers, &page))).into_response()
}

#[allow(clippy::result_large_err)]
async fn load_data(app: &App, headers: &HeaderMap, id: &str) -> Result<modules::Data, Response> {
    let l = lang(headers, app);
    modules::load(app, id, l.bot_language())
        .await
        .map_err(|error| api_error(&error, l, headers))
}

#[allow(clippy::too_many_arguments)]
async fn team_form_response(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    data: &modules::Data,
    input: &HashMap<String, String>,
    errors: &HashMap<String, Vec<String>>,
    status: StatusCode,
) -> Response {
    render(
        app,
        headers,
        id,
        actor,
        data,
        input,
        errors,
        false,
        "guild",
        "guild",
        false,
        None,
        None,
        None,
        json!({"members": []}),
        status,
    )
    .await
}

struct BlacklistContext {
    guild: String,
    directory: Option<Value>,
    panels: Value,
    can_manage_guild: bool,
    scope_kind: String,
    scope_id: String,
    state: Value,
    etag: String,
}

fn blacklist_scope(
    scope: &str,
    panels: &Value,
    can_manage_guild: bool,
) -> Option<(String, String)> {
    if scope.is_empty() {
        if can_manage_guild {
            return Some(("guild".into(), "guild".into()));
        }
        let id = panels
            .as_array()?
            .iter()
            .find_map(|panel| panel["id"].as_str().filter(|id| valid_panel_id(id)))?;
        return Some(("panel".into(), id.into()));
    }
    if scope == "guild" {
        return can_manage_guild.then(|| ("guild".into(), "guild".into()));
    }
    let id = scope.strip_prefix("panel:")?;
    if !valid_panel_id(id)
        || !panels
            .as_array()
            .into_iter()
            .flatten()
            .any(|panel| panel["id"].as_str() == Some(id))
    {
        return None;
    }
    Some(("panel".into(), id.into()))
}

#[allow(clippy::result_large_err)]
async fn load_blacklist_context(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    requested_scope: &str,
) -> Result<BlacklistContext, Response> {
    let l = lang(headers, app);
    let (panel_list, _) = app
        .api
        .ticket_action_panels(id, actor)
        .await
        .map_err(|error| api_error(&error, l, headers))?;
    let can_manage_guild = panel_list["can_manage_guild"] == true;
    let panels = panel_list["panels"].clone();
    let scope = blacklist_scope(requested_scope, &panels, can_manage_guild)
        .ok_or_else(|| error_page(StatusCode::FORBIDDEN, l, keys::DENIED, headers))?;
    let (response, etag) = app
        .api
        .ticket_action_state(id, &scope.0, &scope.1, actor)
        .await
        .map_err(|error| api_error(&error, l, headers))?;
    let Some(etag) = etag else {
        return Err(error_page(
            StatusCode::BAD_GATEWAY,
            l,
            keys::UNAVAILABLE,
            headers,
        ));
    };
    let directory = app
        .api
        .directory(id)
        .await
        .ok()
        .map(|(directory, _)| directory);
    let guild = app.api.guilds().await.ok().map_or_else(
        || id.to_owned(),
        |(guilds, _)| guild_name(&guilds, id).to_owned(),
    );
    Ok(BlacklistContext {
        guild,
        directory,
        panels,
        can_manage_guild,
        scope_kind: scope.0,
        scope_id: scope.1,
        state: response["state"].clone(),
        etag,
    })
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn render_blacklist(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    context: &BlacklistContext,
    input: &HashMap<String, String>,
    errors: &HashMap<String, Vec<String>>,
    action_saved: bool,
    action_error: Option<String>,
    member_search_kind: Option<&str>,
    member_results: Value,
    status: StatusCode,
) -> Response {
    let l = lang(headers, app);
    let blacklist_state = &context.state;
    let remove_keys = blacklist_state["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|_| app.nonces.fresh())
        .collect::<Vec<_>>();
    let member_keys = member_results["members"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|_| app.nonces.fresh())
        .collect::<Vec<_>>();
    let role_add_key = app.nonces.fresh();
    let effective = Value::Null;
    let integration_action_keys = HashMap::new();
    let new_team_id = String::new();
    let page = views::TicketSettingsPage {
        id,
        guild: &context.guild,
        effective: &effective,
        stored: &effective,
        directory: context.directory.as_ref(),
        integrations_schema: None,
        integration_secret_state: &effective,
        integration_action_keys: &integration_action_keys,
        integration_secrets_available: false,
        can_manage_secrets: false,
        integration_secret_error: None,
        secret_saved: false,
        etag: "",
        input,
        errors,
        teams_available: false,
        teams_schema: None,
        module_available: false,
        saved: false,
        new_team_id: &new_team_id,
        scope_kind: &context.scope_kind,
        scope_id: &context.scope_id,
        blacklist_state,
        blacklist_panels: &context.panels,
        blacklist_guild_available: context.can_manage_guild,
        blacklist_etag: &context.etag,
        blacklist_available: true,
        action_saved,
        action_error: action_error.as_deref(),
        member_search_kind,
        member_search_index: None,
        member_results: &member_results,
        role_add_key: &role_add_key,
        remove_keys: &remove_keys,
        member_keys: &member_keys,
    };
    (
        status,
        Html(views::ticket_blacklist_page(l, headers, &page)),
    )
        .into_response()
}

async fn blacklist_conflict(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    scope: &str,
) -> Response {
    let l = lang(headers, app);
    let context = match load_blacklist_context(app, headers, id, actor, scope).await {
        Ok(context) => context,
        Err(response) => return response,
    };
    render_blacklist(
        app,
        headers,
        id,
        &context,
        &HashMap::new(),
        &team_error(l, &l.text(keys::CONFLICT_RELOAD)),
        false,
        None,
        None,
        json!({"members": []}),
        StatusCode::PRECONDITION_FAILED,
    )
}

async fn conflict(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    scope_kind: &str,
    scope_id: &str,
) -> Response {
    let l = lang(headers, app);
    let Ok(data) = load_data(app, headers, id).await else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::UNAVAILABLE, headers);
    };
    render(
        app,
        headers,
        id,
        actor,
        &data,
        &HashMap::new(),
        &team_error(l, &l.text(keys::CONFLICT_RELOAD)),
        false,
        scope_kind,
        scope_id,
        false,
        None,
        None,
        None,
        json!({"members": []}),
        StatusCode::PRECONDITION_FAILED,
    )
    .await
}

pub(super) async fn edit(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let l = lang(&headers, &app);
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let data = match modules::load(&app, &id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    let scope = ("guild".to_owned(), "guild".to_owned());
    render(
        &app,
        &headers,
        &id,
        &actor,
        &data,
        &HashMap::new(),
        &HashMap::new(),
        query.get("saved").is_some_and(|value| value == "1"),
        &scope.0,
        &scope.1,
        query.get("secret_saved").is_some_and(|value| value == "1"),
        None,
        None,
        None,
        json!({"members": []}),
        StatusCode::OK,
    )
    .await
}

pub(super) async fn blacklist(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let scope = query.get("scope").map_or("", String::as_str);
    let context = match load_blacklist_context(&app, &headers, &id, &actor, scope).await {
        Ok(context) => context,
        Err(response) => return response,
    };
    render_blacklist(
        &app,
        &headers,
        &id,
        &context,
        &HashMap::new(),
        &HashMap::new(),
        query.get("action").is_some_and(|value| value == "1"),
        None,
        None,
        json!({"members": []}),
        StatusCode::OK,
    )
}

pub(super) async fn blacklist_action_route(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let intent = pairs
        .iter()
        .find(|(name, _)| name == "intent")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    if intent == "blacklist-search" {
        return blacklist_search(&app, &headers, &id, &actor, pairs).await;
    }
    if matches!(intent.as_str(), "blacklist.add" | "blacklist.remove") {
        return perform_blacklist_action(&app, &headers, &id, &actor, &intent, pairs).await;
    }
    error_page(
        StatusCode::BAD_REQUEST,
        lang(&headers, &app),
        keys::INVALID,
        &headers,
    )
}

#[allow(clippy::too_many_arguments)]
async fn integration_form_error(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    data: &modules::Data,
    input: &HashMap<String, String>,
    message: &str,
    status: StatusCode,
) -> Response {
    let errors = HashMap::from([("integrations".to_owned(), vec![message.to_owned()])]);
    render(
        app,
        headers,
        id,
        actor,
        data,
        input,
        &errors,
        false,
        "guild",
        "guild",
        false,
        None,
        None,
        None,
        json!({"members": []}),
        status,
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn save_integrations(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let Some(form) = parse_integration_form(pairs) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    let basic = match modules::load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return api_error(&error, l, headers),
    };
    let (schema_document, stored, etag) = match advanced_document(app, id, l.bot_language()).await {
        Ok(document) => document,
        Err(error) => return api_error(&error, l, headers),
    };
    let data = modules::Data {
        schema: schema_document,
        stored,
        etag,
        effective: basic.effective,
        directory: basic.directory,
        guilds: basic.guilds,
    };
    let Some(schema) = ticket_integrations_schema(&data.schema) else {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, headers);
    };
    if form.etag.as_deref() != Some(data.etag.as_str()) {
        return conflict(app, headers, id, actor, "guild", "guild").await;
    }
    let Some(columns) = crate::form::columns(&schema["items"]) else {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, headers);
    };
    let field = crate::form::Field {
        name: "integrations".into(),
        label: "integrations".into(),
        kind: crate::form::Kind::List(columns.clone()),
        schema: schema.clone(),
    };
    let Some(Value::Array(mut integrations)) = crate::form::rows(&field, &columns, &form.input)
    else {
        return integration_form_error(
            app,
            headers,
            id,
            actor,
            &data,
            &form.input,
            &l.text(keys::FIELD_INVALID),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    };
    let maximum = schema["maxItems"].as_u64().unwrap_or(20).min(20) as usize;
    if integrations.len() > maximum {
        return integration_form_error(
            app,
            headers,
            id,
            actor,
            &data,
            &form.input,
            &l.text(keys::FIELD_INVALID),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    let mut ids = HashSet::new();
    let stored_integrations = ticket_config(&data.stored)["integrations"].as_array();
    let effective_integrations = ticket_config(&data.effective)["integrations"].as_array();
    for integration in &mut integrations {
        let Some(integration_id) = integration["id"].as_str().map(str::to_owned) else {
            return integration_form_error(
                app,
                headers,
                id,
                actor,
                &data,
                &form.input,
                &l.text(keys::FIELD_INVALID),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        };
        if !integration_id_valid(&integration_id) || !ids.insert(integration_id.clone()) {
            return integration_form_error(
                app,
                headers,
                id,
                actor,
                &data,
                &form.input,
                &l.text(keys::FIELD_INVALID),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        let existing = stored_integrations
            .into_iter()
            .flatten()
            .find(|old| old["id"].as_str() == Some(integration_id.as_str()))
            .or_else(|| {
                effective_integrations
                    .into_iter()
                    .flatten()
                    .find(|old| old["id"].as_str() == Some(integration_id.as_str()))
            });
        if let Some(generation) = existing.and_then(|old| old["generation"].as_str())
            && let Some(object) = integration.as_object_mut()
        {
            object.insert("generation".into(), Value::String(generation.to_owned()));
        }
    }
    let submitted_ids = integrations
        .iter()
        .filter_map(|integration| integration["id"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    let removed_with_secrets = effective_integrations.into_iter().flatten().any(|old| {
        old["id"]
            .as_str()
            .is_some_and(|old_id| !submitted_ids.contains(old_id))
            && integration_may_have_secrets(old)
    });
    let mut candidate = data.stored.clone();
    if set_ticket_integrations(&mut candidate, Value::Array(integrations)).is_none() {
        return error_page(StatusCode::BAD_GATEWAY, l, keys::INVALID, headers);
    }
    match app.api.put_config(id, candidate, &data.etag, actor).await {
        Ok(_) => Redirect::to(&format!("/g/{id}/tickets/settings?saved=1")).into_response(),
        Err(error) if error.status == StatusCode::PRECONDITION_FAILED => {
            conflict(app, headers, id, actor, "guild", "guild").await
        }
        Err(error) if error.status == StatusCode::FORBIDDEN => {
            let message = if removed_with_secrets {
                l.text(keys::TICKET_INTEGRATION_DELETE_OWNER_ONLY)
            } else {
                l.text(keys::DENIED)
            };
            integration_form_error(
                app,
                headers,
                id,
                actor,
                &data,
                &form.input,
                &message,
                StatusCode::FORBIDDEN,
            )
            .await
        }
        Err(error) if error.status == StatusCode::UNPROCESSABLE_ENTITY => {
            integration_form_error(
                app,
                headers,
                id,
                actor,
                &data,
                &form.input,
                &l.text(keys::FIELD_INVALID),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(error) => api_error(&error, l, headers),
    }
}

async fn integration_secret_feedback(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    data: &modules::Data,
    message: String,
    status: StatusCode,
) -> Response {
    let input = HashMap::new();
    let errors = HashMap::new();
    render(
        app,
        headers,
        id,
        actor,
        data,
        &input,
        &errors,
        false,
        "guild",
        "guild",
        false,
        Some(message),
        None,
        None,
        json!({"members": []}),
        status,
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn integration_secret_action(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    intent: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let Some(fields) = exact_fields(pairs) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    let is_set = intent == "integration.secret.set";
    let expected_len = if is_set { 6 } else { 5 };
    if fields.len() != expected_len
        || fields.get("intent").map(String::as_str) != Some(intent)
        || (is_set && !fields.contains_key("value"))
        || (!is_set && fields.contains_key("value"))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let (Some(integration_id), Some(generation), Some(secret_name), Some(operation_key)) = (
        fields.get("integration_id"),
        fields.get("generation"),
        fields.get("secret_name"),
        fields.get("operation_key"),
    ) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    if !integration_id_valid(integration_id)
        || !generation_valid(generation)
        || !integration_secret_name_valid(secret_name)
        || !hex_key(operation_key)
        || (is_set && fields.get("value").is_some_and(String::is_empty))
    {
        return error_page(
            StatusCode::UNPROCESSABLE_ENTITY,
            l,
            keys::FIELD_INVALID,
            headers,
        );
    }
    // Management is checked before any settings are loaded: the loader uses the service credential.
    let state = match app.api.ticket_integration_secrets(id, actor).await {
        Ok((response, _)) => response.get("state").cloned().unwrap_or(Value::Null),
        Err(error) if error.status.is_server_error() => return api_error(&error, l, headers),
        Err(_) => return error_page(StatusCode::FORBIDDEN, l, keys::DENIED, headers),
    };
    if state["can_manage_secrets"] != true {
        return error_page(
            StatusCode::FORBIDDEN,
            l,
            keys::TICKET_INTEGRATION_SECRETS_OWNER_ONLY,
            headers,
        );
    }
    let data = match modules::load(app, id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return api_error(&error, l, headers),
    };
    let Some(integration) = ticket_config(&data.effective)["integrations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|integration| integration["id"].as_str() == Some(integration_id))
    else {
        return integration_secret_feedback(
            app,
            headers,
            id,
            actor,
            &data,
            l.text(keys::NOTFOUND),
            StatusCode::NOT_FOUND,
        )
        .await;
    };
    if integration["generation"].as_str() != Some(generation)
        || !integration_declares_secret(integration, secret_name)
    {
        return integration_secret_feedback(
            app,
            headers,
            id,
            actor,
            &data,
            l.text(keys::CONFLICT),
            StatusCode::CONFLICT,
        )
        .await;
    }
    let mut action_data = json!({
        "integration_id": integration_id,
        "generation": generation,
        "secret_name": secret_name,
    });
    if is_set {
        action_data["value"] = json!(fields.get("value").map(String::as_str).unwrap_or_default());
    }
    match app
        .api
        .ticket_integration_secret_action(id, intent, action_data, actor, operation_key)
        .await
    {
        Ok(_) => Redirect::to(&format!("/g/{id}/tickets/settings?secret_saved=1")).into_response(),
        Err(error) => {
            let message = integration_secret_error(&error, l);
            let status = if error.status.is_client_error() {
                error.status
            } else {
                StatusCode::BAD_GATEWAY
            };
            integration_secret_feedback(app, headers, id, actor, &data, message, status).await
        }
    }
}

pub(super) async fn integration_secret_action_route(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    axum::Form(pairs): axum::Form<Vec<(String, String)>>,
) -> Response {
    let Some(actor) = user(&headers, &app) else {
        return Redirect::to("/login").into_response();
    };
    let intents = pairs
        .iter()
        .filter(|(name, _)| name == "intent")
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    if intents.len() != 1
        || !matches!(
            intents[0].as_str(),
            "integration.secret.set" | "integration.secret.remove"
        )
    {
        return error_page(
            StatusCode::BAD_REQUEST,
            lang(&headers, &app),
            keys::INVALID,
            &headers,
        );
    }
    integration_secret_action(&app, &headers, &id, &actor, &intents[0], pairs).await
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
    let intents = pairs
        .iter()
        .filter(|(name, _)| name == "intent")
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    if intents.len() != 1 {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    match intents[0].as_str() {
        "integrations" => return save_integrations(&app, &headers, &id, &actor, pairs).await,
        "teams" => {}
        _ => return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers),
    }
    let Some(form) = parse_team_form(pairs) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    let data = match modules::load(&app, &id, l.bot_language()).await {
        Ok(data) => data,
        Err(error) => return api_error(&error, l, &headers),
    };
    if !form.present || form.intent.as_deref() != Some("teams") {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let scope = ("guild".to_owned(), "guild".to_owned());
    let Some(etag) = form.etag.as_deref() else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    };
    if etag != data.etag {
        return conflict(&app, &headers, &id, &actor, &scope.0, &scope.1).await;
    }
    if let Some(index) = form.member_search {
        let current_len = ticket_config(&data.effective)["staff_teams"]
            .as_array()
            .map_or(0, Vec::len);
        if index > current_len || !form.rows.contains_key(&index) {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        let query = form.rows[&index]
            .member_query
            .as_deref()
            .unwrap_or("")
            .trim();
        if !(1..=100).contains(&query.chars().count()) {
            let errors = team_error(l, &l.text(keys::FIELD_INVALID));
            return render(
                &app,
                &headers,
                &id,
                &actor,
                &data,
                &form.input,
                &errors,
                false,
                &scope.0,
                &scope.1,
                false,
                None,
                Some("team"),
                Some(index),
                json!({"members": []}),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        return match app
            .api
            .member_search(&id, "guild", "guild", query, &actor)
            .await
        {
            Ok((members, _)) => {
                render(
                    &app,
                    &headers,
                    &id,
                    &actor,
                    &data,
                    &form.input,
                    &HashMap::new(),
                    false,
                    &scope.0,
                    &scope.1,
                    false,
                    None,
                    Some("team"),
                    Some(index),
                    members,
                    StatusCode::OK,
                )
                .await
            }
            Err(error) => {
                let errors = team_error(l, &api_message(&error, l));
                render(
                    &app,
                    &headers,
                    &id,
                    &actor,
                    &data,
                    &form.input,
                    &errors,
                    false,
                    &scope.0,
                    &scope.1,
                    false,
                    None,
                    Some("team"),
                    Some(index),
                    json!({"members": []}),
                    error.status,
                )
                .await
            }
        };
    }
    if ticket_teams_schema(&data.schema).is_none() {
        return error_page(StatusCode::NOT_FOUND, l, keys::NOTFOUND, &headers);
    }
    let current = ticket_config(&data.effective)["staff_teams"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    if form.rows.len() != current.len() + 1
        || (0..=current.len()).any(|index| !form.rows.contains_key(&index))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
    }
    let mut teams = Vec::<Value>::new();
    let mut removed = HashSet::<String>::new();
    let mut ids = HashSet::<String>::new();
    let mut names = HashSet::<String>::new();
    for index in 0..=current.len() {
        let Some(row) = form.rows.get(&index) else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        let Some(team_id) = row.id.as_deref() else {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        };
        if index < current.len() {
            let current_id = current[index]["id"].as_str().unwrap_or("");
            if team_id != current_id {
                return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
            }
            if row.remove {
                removed.insert(team_id.to_owned());
                ids.insert(team_id.to_owned());
                continue;
            }
        } else if row.remove {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, &headers);
        }
        if !team_id_valid(team_id) || !ids.insert(team_id.to_owned()) {
            return error_page(StatusCode::UNPROCESSABLE_ENTITY, l, keys::INVALID, &headers);
        }
        let name = row.name.as_deref().unwrap_or("").trim();
        let current_team = current.get(index);
        let roles = saved_roles(current_team.unwrap_or(&Value::Null), row);
        let users = saved_users(current_team.unwrap_or(&Value::Null), row);
        let permissions = saved_permissions(row);
        let untouched_new = index == current.len()
            && name.is_empty()
            && roles.as_ref().is_some_and(Vec::is_empty)
            && users.as_ref().is_some_and(Vec::is_empty)
            && row.permissions.iter().all(|(permission, values)| {
                permission_value(values).is_some_and(|value| {
                    TEAM_PERMISSIONS
                        .iter()
                        .find(|(name, _)| *name == permission.as_str())
                        .is_some_and(|(_, default)| value == *default)
                })
            });
        if untouched_new {
            continue;
        }
        let invalid_name = name.is_empty() || name.chars().count() > 80 || name.contains('\0');
        let duplicate_name = !invalid_name && !names.insert(name.to_lowercase());
        let permissions_complete = row.permissions.len() == TEAM_PERMISSIONS.len();
        if invalid_name || duplicate_name || !permissions_complete {
            let field = if invalid_name || duplicate_name {
                format!("team.{index}.name")
            } else {
                String::new()
            };
            let errors = HashMap::from([(field, vec![l.text(keys::FIELD_INVALID)])]);
            return team_form_response(
                &app,
                &headers,
                &id,
                &actor,
                &data,
                &form.input,
                &errors,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        let (Some(roles), Some(users), Some(permissions)) = (roles, users, permissions) else {
            return error_page(
                StatusCode::UNPROCESSABLE_ENTITY,
                l,
                keys::FIELD_INVALID,
                &headers,
            );
        };
        if row.roles_present {
            let allowed: HashSet<String> = current_team
                .and_then(|team| team["roles"].as_array())
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .chain(
                    data.directory
                        .as_ref()
                        .and_then(|directory| directory["roles"].as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|role| {
                            let id = role["id"].as_str()?;
                            (role["managed"] != true && role["name"] != "@everyone")
                                .then(|| id.to_owned())
                        }),
                )
                .collect();
            if roles.iter().any(|role| {
                role.as_str()
                    .is_none_or(|id| !allowed.contains(id) || !decimal_snowflake(id))
            }) {
                let errors = team_error(l, &l.text(keys::FIELD_INVALID));
                return team_form_response(
                    &app,
                    &headers,
                    &id,
                    &actor,
                    &data,
                    &form.input,
                    &errors,
                    StatusCode::UNPROCESSABLE_ENTITY,
                )
                .await;
            }
        }
        if users
            .iter()
            .any(|member| member.as_str().is_none_or(|id| !decimal_snowflake(id)))
        {
            let errors = team_error(l, &l.text(keys::FIELD_INVALID));
            return team_form_response(
                &app,
                &headers,
                &id,
                &actor,
                &data,
                &form.input,
                &errors,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        if row.users_present {
            let existing: HashSet<&str> = current_team
                .and_then(|team| team["users"].as_array())
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let added: Vec<&str> = users
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| !existing.contains(id))
                .collect();
            if !added.is_empty() {
                let query = row.member_query.as_deref().unwrap_or("").trim();
                if !(1..=100).contains(&query.chars().count()) {
                    let errors = team_error(l, &l.text(keys::FIELD_INVALID));
                    return team_form_response(
                        &app,
                        &headers,
                        &id,
                        &actor,
                        &data,
                        &form.input,
                        &errors,
                        StatusCode::UNPROCESSABLE_ENTITY,
                    )
                    .await;
                }
                let members = match app
                    .api
                    .member_search(&id, "guild", "guild", query, &actor)
                    .await
                {
                    Ok((members, _)) => members,
                    Err(error) => return api_error(&error, l, &headers),
                };
                if added.iter().any(|selected| {
                    !members["members"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|member| member["id"].as_str() == Some(*selected))
                }) {
                    let errors = team_error(l, &l.text(keys::FIELD_INVALID));
                    return team_form_response(
                        &app,
                        &headers,
                        &id,
                        &actor,
                        &data,
                        &form.input,
                        &errors,
                        StatusCode::UNPROCESSABLE_ENTITY,
                    )
                    .await;
                }
            }
        }
        let mut team = current_team
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        team.insert("id".into(), Value::String(team_id.to_owned()));
        team.insert("name".into(), Value::String(name.to_owned()));
        team.insert("roles".into(), Value::Array(roles));
        team.insert("users".into(), Value::Array(users));
        let mut kept_permissions = current_team
            .and_then(|team| team["permissions"].as_object())
            .cloned()
            .unwrap_or_default();
        kept_permissions.extend(permissions);
        team.insert("permissions".into(), Value::Object(kept_permissions));
        teams.push(Value::Object(team));
    }
    let panels = panel_refs_after_team_removal(&data.effective, &removed);
    let patch = config_patch(Value::Array(teams), panels);
    match app.api.put_basic_config(&id, patch, etag, &actor).await {
        Ok(_) => Redirect::to(&format!("/g/{id}/tickets/settings?saved=1")).into_response(),
        Err(error) if error.status == StatusCode::PRECONDITION_FAILED => {
            conflict(&app, &headers, &id, &actor, &scope.0, &scope.1).await
        }
        Err(error)
            if error.status == StatusCode::FORBIDDEN
                || error.status == StatusCode::UNPROCESSABLE_ENTITY =>
        {
            let mut errors = HashMap::<String, Vec<String>>::new();
            for issue in error.issues {
                errors
                    .entry(if issue.path.contains("staff_teams") {
                        "staff_teams".to_owned()
                    } else {
                        String::new()
                    })
                    .or_default()
                    .push(issue.message);
            }
            if error.status == StatusCode::FORBIDDEN {
                errors.insert(String::new(), vec![l.text(keys::DENIED)]);
            } else if errors.is_empty() {
                errors.insert(String::new(), vec![error.message]);
            }
            let current = match modules::load(&app, &id, l.bot_language()).await {
                Ok(data) => data,
                Err(error) => return api_error(&error, l, &headers),
            };
            render(
                &app,
                &headers,
                &id,
                &actor,
                &current,
                &form.input,
                &errors,
                false,
                &scope.0,
                &scope.1,
                false,
                None,
                None,
                None,
                json!({"members": []}),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(error) => api_error(&error, l, &headers),
    }
}

async fn blacklist_search(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let Some(fields) = exact_fields(pairs) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    if fields.get("intent").map(String::as_str) != Some("blacklist-search")
        || fields
            .keys()
            .any(|key| !matches!(key.as_str(), "intent" | "scope" | "member_query"))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let scope_value = fields.get("scope").map_or("guild", String::as_str);
    let context = match load_blacklist_context(app, headers, id, actor, scope_value).await {
        Ok(context) => context,
        Err(response) => return response,
    };
    let query = fields.get("member_query").map_or("", String::as_str).trim();
    let input = HashMap::from([("member_query".into(), query.into())]);
    if !(1..=100).contains(&query.chars().count()) {
        let errors = team_error(l, &l.text(keys::FIELD_INVALID));
        return render_blacklist(
            app,
            headers,
            id,
            &context,
            &input,
            &errors,
            false,
            None,
            Some("blacklist"),
            json!({"members": []}),
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    match app
        .api
        .member_search(id, &context.scope_kind, &context.scope_id, query, actor)
        .await
    {
        Ok((members, _)) => render_blacklist(
            app,
            headers,
            id,
            &context,
            &input,
            &HashMap::new(),
            false,
            None,
            Some("blacklist"),
            members,
            StatusCode::OK,
        ),
        Err(error) => {
            let errors = team_error(l, &api_message(&error, l));
            render_blacklist(
                app,
                headers,
                id,
                &context,
                &input,
                &errors,
                false,
                None,
                Some("blacklist"),
                json!({"members": []}),
                error.status,
            )
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn perform_blacklist_action(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    actor: &str,
    intent: &str,
    pairs: Vec<(String, String)>,
) -> Response {
    let l = lang(headers, app);
    let Some(fields) = exact_fields(pairs) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    let allowed_fields = [
        "intent",
        "scope",
        "etag",
        "target_kind",
        "target_id",
        "operation_key",
        "member_query",
    ];
    if fields.get("intent").map(String::as_str) != Some(intent)
        || fields
            .keys()
            .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let scope = fields.get("scope").map_or("", String::as_str);
    let context = match load_blacklist_context(app, headers, id, actor, scope).await {
        Ok(context) => context,
        Err(response) => return response,
    };
    let Some(etag) = fields.get("etag") else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    if etag != &context.etag {
        return blacklist_conflict(app, headers, id, actor, scope).await;
    }
    let Some(kind) = fields.get("target_kind").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    let Some(target_id) = fields.get("target_id").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    let Some(key) = fields.get("operation_key").map(String::as_str) else {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    };
    if !matches!(kind, "role" | "user") || !decimal_snowflake(target_id) || !hex_key(key) {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    let entries = context.state["entries"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let exists = entries.iter().any(|entry| {
        entry["kind"].as_str() == Some(kind) && entry["id"].as_str() == Some(target_id)
    });
    if intent == "blacklist.remove" && !exists {
        return error_page(StatusCode::CONFLICT, l, keys::CONFLICT, headers);
    }
    if intent == "blacklist.add" && exists {
        return error_page(StatusCode::CONFLICT, l, keys::CONFLICT, headers);
    }
    if intent == "blacklist.add"
        && kind == "role"
        && !role_exists(context.directory.as_ref(), id, target_id)
    {
        return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
    }
    if intent == "blacklist.add" && kind == "user" {
        let query = fields.get("member_query").map_or("", String::as_str).trim();
        if !(1..=100).contains(&query.chars().count()) {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
        }
        let members = match app
            .api
            .member_search(id, &context.scope_kind, &context.scope_id, query, actor)
            .await
        {
            Ok((members, _)) => members,
            Err(error) => return api_error(&error, l, headers),
        };
        if !members["members"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|member| member["id"].as_str() == Some(target_id))
        {
            return error_page(StatusCode::BAD_REQUEST, l, keys::INVALID, headers);
        }
    }
    let action = if intent == "blacklist.add" {
        "blacklist.add"
    } else {
        "blacklist.remove"
    };
    match app
        .api
        .ticket_action(
            id,
            &context.scope_kind,
            &context.scope_id,
            action,
            json!({"target":{"kind":kind,"id":target_id}}),
            etag,
            actor,
            key,
        )
        .await
    {
        Ok(_) => {
            let query = serde_urlencoded::to_string([(
                "scope",
                scope_value(&context.scope_kind, &context.scope_id),
            )])
            .unwrap_or_default();
            Redirect::to(&format!(
                "/g/{id}/tickets/settings/blacklist?{query}&action=1"
            ))
            .into_response()
        }
        Err(error) if error.status == StatusCode::PRECONDITION_FAILED => {
            blacklist_conflict(app, headers, id, actor, scope).await
        }
        Err(error) => render_blacklist(
            app,
            headers,
            id,
            &context,
            &HashMap::new(),
            &HashMap::new(),
            false,
            Some(api_message(&error, l)),
            None,
            json!({"members": []}),
            error.status,
        ),
    }
}
