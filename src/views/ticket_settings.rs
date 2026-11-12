use super::{heading, role_fill, shell};
use crate::{
    form::{self, Field, Kind},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

const PERMISSIONS: [(&str, bool); 10] = [
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

pub(crate) struct TicketSettingsPage<'a> {
    pub id: &'a str,
    pub guild: &'a str,
    pub effective: &'a Value,
    pub stored: &'a Value,
    pub directory: Option<&'a Value>,
    pub integrations_schema: Option<&'a Value>,
    pub integration_secret_state: &'a Value,
    pub integration_action_keys: &'a HashMap<String, String>,
    pub integration_secrets_available: bool,
    pub can_manage_secrets: bool,
    pub integration_secret_error: Option<&'a str>,
    pub secret_saved: bool,
    pub etag: &'a str,
    pub input: &'a HashMap<String, String>,
    pub errors: &'a HashMap<String, Vec<String>>,
    pub teams_available: bool,
    pub teams_schema: Option<&'a Value>,
    pub module_available: bool,
    pub saved: bool,
    pub new_team_id: &'a str,
    pub scope_kind: &'a str,
    pub scope_id: &'a str,
    pub blacklist_state: &'a Value,
    pub blacklist_panels: &'a Value,
    pub blacklist_guild_available: bool,
    pub blacklist_etag: &'a str,
    pub blacklist_available: bool,
    pub action_saved: bool,
    pub action_error: Option<&'a str>,
    pub member_search_kind: Option<&'a str>,
    pub member_search_index: Option<usize>,
    pub member_results: &'a Value,
    pub role_add_key: &'a str,
    pub remove_keys: &'a [String],
    pub member_keys: &'a [String],
}

fn ticket_config(effective: &Value) -> &Value {
    &effective["modules"]["tickets"]["config"]
}

fn values(input: &HashMap<String, String>, key: &str, stored: Option<&Value>) -> Vec<String> {
    if let Some(value) = input.get(key) {
        return value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "-")
            .map(str::to_owned)
            .collect();
    }
    stored
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn role_options(
    l: Locale<'_>,
    guild: &str,
    directory: Option<&Value>,
    selected: &[String],
    name: &str,
    present: bool,
    group_label: &str,
) -> String {
    let mut out = format!(
        "<div class=\"role-list\" role=\"group\" aria-label=\"{}\">",
        escape(group_label)
    );
    let known: HashSet<String> = directory
        .and_then(|directory| directory["roles"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|role| {
            let id = role["id"].as_str()?;
            if id == guild || role["managed"] == true || role["name"] == "@everyone" {
                return None;
            }
            Some(id.to_owned())
        })
        .collect();
    if present {
        let _ = write!(
            out,
            "<input type=\"hidden\" name=\"{name}_present\" value=\"1\">"
        );
        for role in directory
            .and_then(|directory| directory["roles"].as_array())
            .into_iter()
            .flatten()
        {
            let Some(id) = role["id"].as_str().filter(|id| known.contains(*id)) else {
                continue;
            };
            let color = role["color"].as_u64().unwrap_or(0).min(0xFF_FFFF);
            let checked = selected.iter().any(|selected| selected == id);
            let _ = write!(
                out,
                "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\"{}><svg aria-hidden=\"true\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><circle cx=\"6\" cy=\"6\" r=\"5\" fill=\"{}\"/></svg>{}</label>",
                escape(id),
                if checked { " checked" } else { "" },
                role_fill(color),
                escape(role["name"].as_str().unwrap_or(id))
            );
        }
    } else {
        let _ = write!(
            out,
            "<p class=\"settings-help\">{}</p>",
            l.text("ticket-team-roles-unavailable")
        );
    }
    for id in selected.iter().filter(|id| !known.contains(*id)) {
        if present {
            let _ = write!(
                out,
                "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\" checked>{}</label>",
                escape(id),
                l.with_text(keys::UNKNOWN_ROLE, "id", id)
            );
        } else {
            let _ = write!(out, "<span class=\"mono\">{}</span>", escape(id));
        }
    }
    out.push_str("</div>");
    out
}

fn decimal_snowflake(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && id.as_bytes()[0] != b'0'
        && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn member_label(member: &Value) -> Option<(&str, String)> {
    let id = member["id"].as_str()?;
    let name = member["name"].as_str().unwrap_or(id);
    let display = member["display_name"]
        .as_str()
        .filter(|display| !display.is_empty())
        .unwrap_or(name);
    let label = if display == name {
        name.to_owned()
    } else {
        format!("{display} (@{name})")
    };
    Some((id, label))
}

fn member_choices(
    l: Locale<'_>,
    members: &Value,
    selected: &[String],
    name: &str,
    group_label: &str,
) -> String {
    let candidates = members["members"].as_array();
    let mut out = format!(
        "<div class=\"role-list\" role=\"group\" aria-label=\"{}\">",
        escape(group_label)
    );
    let mut shown = HashSet::new();
    for member in candidates.into_iter().flatten() {
        let Some((id, label)) = member_label(member).filter(|(id, _)| decimal_snowflake(id)) else {
            continue;
        };
        if !shown.insert(id.to_owned()) {
            continue;
        }
        let checked = selected.iter().any(|selected| selected == id);
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\"{}>{}</label>",
            escape(id),
            if checked { " checked" } else { "" },
            escape(&label)
        );
    }
    for id in selected.iter().filter(|id| !shown.contains(*id)) {
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\" checked>{}</label>",
            escape(id),
            escape(id)
        );
    }
    if candidates.is_some() && shown.is_empty() && selected.is_empty() {
        let _ = write!(
            out,
            "<p class=\"empty\">{}</p>",
            l.text("ticket-team-users-none")
        );
    }
    out.push_str("</div>");
    out
}

fn panel_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn team_indices(input: &HashMap<String, String>, count: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix("team.")?.split('.').next()?.parse().ok())
        .collect();
    indices.sort_unstable();
    indices.dedup();
    if indices.is_empty() {
        (0..=count).collect()
    } else {
        indices
    }
}

fn team_name_max(schema: Option<&Value>) -> u64 {
    let schema = schema
        .and_then(|schema| {
            schema["anyOf"]
                .as_array()?
                .iter()
                .find(|branch| branch["type"] != "null")
        })
        .or(schema)
        .unwrap_or(&Value::Null);
    schema["items"]["properties"]["name"]["maxLength"]
        .as_u64()
        .unwrap_or(80)
}

fn team_editor(l: Locale<'_>, page: &TicketSettingsPage<'_>) -> String {
    let teams = ticket_config(page.effective)["staff_teams"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let has_roles = page
        .directory
        .and_then(|directory| directory["roles"].as_array())
        .is_some();
    let max_name = team_name_max(page.teams_schema);
    let mut body = format!(
        "<section class=\"card settings-card\"><div class=\"panel-heading\"><h2>{}</h2></div>",
        l.text("ticket-settings-staff-teams")
    );
    if let Some(errors) = page.errors.get("staff_teams") {
        for error in errors {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(error)
            );
        }
    }
    let _ = write!(
        body,
        "<form method=\"post\" action=\"/g/{}/tickets/settings\"><input type=\"hidden\" name=\"intent\" value=\"teams\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><input type=\"hidden\" name=\"staff_teams_present\" value=\"1\">",
        escape(page.id),
        escape(page.etag)
    );
    for index in team_indices(page.input, teams.len()) {
        let current = teams.get(index);
        let prefix = format!("team.{index}");
        let id = page
            .input
            .get(&format!("{prefix}.id"))
            .map(String::as_str)
            .or_else(|| current.and_then(|team| team["id"].as_str()))
            .unwrap_or(if index == teams.len() {
                page.new_team_id
            } else {
                ""
            });
        let stored_name = current.and_then(|team| team["name"].as_str()).unwrap_or("");
        let name = page
            .input
            .get(&format!("{prefix}.name"))
            .map(String::as_str)
            .unwrap_or(stored_name);
        let name_id = format!("team-{index}-name");
        let _ = write!(
            body,
            "<fieldset class=\"card settings-card\"><legend>{}</legend><input type=\"hidden\" name=\"{prefix}.id\" value=\"{}\"><label for=\"{name_id}\">{}</label><input id=\"{name_id}\" type=\"text\" name=\"{prefix}.name\" value=\"{}\" maxlength=\"{max_name}\">",
            l.text(if index < teams.len() {
                "ticket-team-existing"
            } else {
                "ticket-team-new"
            }),
            escape(id),
            l.text("ticket-team-name"),
            escape(name)
        );
        if let Some(messages) = page.errors.get(&format!("{prefix}.name")) {
            for error in messages {
                let _ = write!(
                    body,
                    "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                    escape(error)
                );
            }
        }
        let roles_key = format!("{prefix}.roles");
        let roles = values(page.input, &roles_key, current.map(|team| &team["roles"]));
        let _ = write!(
            body,
            "<div class=\"settings-row\"><span class=\"list-caption\">{}</span>{}</div>",
            l.text("ticket-team-roles"),
            role_options(
                l,
                page.id,
                page.directory,
                &roles,
                &roles_key,
                has_roles,
                &l.text("ticket-team-roles"),
            )
        );
        let users_key = format!("{prefix}.users");
        let users = values(page.input, &users_key, current.map(|team| &team["users"]));
        let query_key = format!("{prefix}.member_query");
        let query = page.input.get(&query_key).map(String::as_str).unwrap_or("");
        let _ = write!(
            body,
            "<div class=\"settings-row\"><span class=\"list-caption\">{}</span><input type=\"hidden\" name=\"{prefix}.users_present\" value=\"1\"><label for=\"team-{index}-member-query\">{}</label><input id=\"team-{index}-member-query\" type=\"search\" name=\"{query_key}\" value=\"{}\" minlength=\"1\" maxlength=\"100\"><button class=\"button secondary\" type=\"submit\" name=\"member_search\" value=\"{index}\">{}</button>",
            l.text("ticket-team-users"),
            l.text("ticket-team-user-search"),
            escape(query),
            l.text("ticket-team-search")
        );
        let results =
            if page.member_search_kind == Some("team") && page.member_search_index == Some(index) {
                page.member_results
            } else {
                &Value::Null
            };
        body.push_str(&member_choices(
            l,
            results,
            &users,
            &users_key,
            &l.text("ticket-team-users"),
        ));
        if page.member_search_kind == Some("team")
            && page.member_search_index == Some(index)
            && page.member_results["truncated"] == true
        {
            let _ = write!(
                body,
                "<p class=\"settings-help\">{}</p>",
                l.text("ticket-members-refine-search")
            );
        }
        let _ = write!(
            body,
            "<p class=\"settings-help\">{}</p>",
            l.text("ticket-team-search-save-help")
        );
        body.push_str("</div>");
        let _ = write!(
            body,
            "<fieldset class=\"settings-row\"><legend>{}</legend>",
            l.text("ticket-team-permissions")
        );
        for (permission, default) in PERMISSIONS {
            let key = format!("{prefix}.permissions.{permission}");
            let checked = page.input.get(&key).map_or_else(
                || {
                    current
                        .and_then(|team| team["permissions"][permission].as_bool())
                        .unwrap_or(default)
                },
                |value| value == "true",
            );
            let input_id = format!("team-{index}-permission-{permission}");
            let _ = write!(
                body,
                "<label class=\"role-option\" for=\"{input_id}\"><input type=\"hidden\" name=\"{key}\" value=\"false\"><input id=\"{input_id}\" type=\"checkbox\" name=\"{key}\" value=\"true\"{}>{}</label>",
                if checked { " checked" } else { "" },
                l.text(&format!("ticket-team-permission-{permission}"))
            );
        }
        body.push_str("</fieldset>");
        if index < teams.len() {
            let _ = write!(
                body,
                "<label class=\"list-remove\"><input type=\"checkbox\" name=\"{prefix}.remove\" value=\"1\"{}>{}</label>",
                if page
                    .input
                    .get(&format!("{prefix}.remove"))
                    .is_some_and(|value| value == "1")
                {
                    " checked"
                } else {
                    ""
                },
                l.text("ticket-team-remove")
            );
        }
        body.push_str("</fieldset>");
    }
    let _ = write!(
        body,
        "<div class=\"save-bar\"><a class=\"button ghost\" href=\"/g/{}/tickets\">{}</a><button class=\"button primary\" type=\"submit\">{}</button></div></form></section>",
        escape(page.id),
        l.text(keys::DISCARD),
        l.text(keys::SAVE)
    );
    body
}

fn scope_link(id: &str, scope: &str, label: &str, selected: bool) -> String {
    format!(
        "<a class=\"button secondary\" href=\"/g/{}/tickets/settings/blacklist?scope={}\"{}>{}</a>",
        escape(id),
        escape(scope),
        if selected {
            " aria-current=\"page\""
        } else {
            ""
        },
        escape(label)
    )
}

fn role_name<'a>(directory: Option<&'a Value>, id: &'a str) -> &'a str {
    directory
        .and_then(|directory| directory["roles"].as_array())
        .into_iter()
        .flatten()
        .find(|role| role["id"].as_str() == Some(id))
        .and_then(|role| role["name"].as_str())
        .unwrap_or(id)
}

fn blacklist_page(l: Locale<'_>, page: &TicketSettingsPage<'_>) -> String {
    let panels = page
        .blacklist_panels
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let scope = if page.scope_kind == "guild" {
        "guild".to_owned()
    } else {
        format!("panel:{}", page.scope_id)
    };
    let entries = page.blacklist_state["entries"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let mut body = format!(
        "<section class=\"card settings-card\"><div class=\"panel-heading\"><h2>{}</h2></div><div class=\"head-actions\">",
        l.text("ticket-settings-blacklist")
    );
    if page.blacklist_guild_available {
        body.push_str(&scope_link(
            page.id,
            "guild",
            &l.text("ticket-blacklist-guild"),
            page.scope_kind == "guild",
        ));
    }
    for panel in panels {
        let Some(id) = panel["id"].as_str().filter(|id| panel_id_valid(id)) else {
            continue;
        };
        let name = panel["name"].as_str().unwrap_or(id);
        body.push_str(&scope_link(
            page.id,
            &format!("panel:{id}"),
            name,
            page.scope_kind == "panel" && page.scope_id == id,
        ));
    }
    body.push_str("</div>");
    if let Some(error) = page.action_error {
        let _ = write!(
            body,
            "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
            escape(error)
        );
    }
    if !page.blacklist_available {
        body.push_str("</section>");
        return body;
    }
    if page.action_saved {
        let _ = write!(
            body,
            "<p class=\"form-status\" role=\"status\">{}</p>",
            l.text("ticket-blacklist-saved")
        );
    }
    let _ = write!(
        body,
        "<p class=\"settings-help\">{}</p><ul class=\"list-plain\">",
        l.text("ticket-blacklist-scope-help")
    );
    for (index, entry) in entries.iter().enumerate() {
        let kind = match entry["kind"].as_str() {
            Some("user") => "user",
            Some("role") => "role",
            _ => continue,
        };
        let id = entry["id"].as_str().unwrap_or("");
        if !decimal_snowflake(id) {
            continue;
        }
        let label = if kind == "role" {
            role_name(page.directory, id)
        } else {
            id
        };
        let key = page.remove_keys.get(index).map_or("", String::as_str);
        let _ = write!(
            body,
            "<li>{}: {}<form method=\"post\" action=\"/g/{}/tickets/settings/blacklist/actions\"><input type=\"hidden\" name=\"intent\" value=\"blacklist.remove\"><input type=\"hidden\" name=\"scope\" value=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><input type=\"hidden\" name=\"target_kind\" value=\"{}\"><input type=\"hidden\" name=\"target_id\" value=\"{}\"><input type=\"hidden\" name=\"operation_key\" value=\"{}\"><button class=\"button secondary\" type=\"submit\">{}</button></form></li>",
            l.text(&format!("ticket-blacklist-{kind}")),
            escape(label),
            escape(page.id),
            escape(&scope),
            escape(page.blacklist_etag),
            kind,
            escape(id),
            escape(key),
            l.text("ticket-blacklist-remove")
        );
    }
    body.push_str("</ul><div class=\"settings-row\">");
    let roles = page
        .directory
        .and_then(|directory| directory["roles"].as_array());
    if let Some(roles) = roles {
        let _ = write!(
            body,
            "<form method=\"post\" action=\"/g/{}/tickets/settings/blacklist/actions\"><input type=\"hidden\" name=\"intent\" value=\"blacklist.add\"><input type=\"hidden\" name=\"scope\" value=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><input type=\"hidden\" name=\"target_kind\" value=\"role\"><input type=\"hidden\" name=\"operation_key\" value=\"{}\"><label for=\"blacklist-role\">{}</label><select id=\"blacklist-role\" name=\"target_id\">",
            escape(page.id),
            escape(&scope),
            escape(page.blacklist_etag),
            escape(page.role_add_key),
            l.text("ticket-blacklist-role")
        );
        body.push_str("<option value=\"\">");
        body.push_str(&escape(&l.text(keys::UNSET)));
        body.push_str("</option>");
        for role in roles {
            let Some(id) = role["id"].as_str() else {
                continue;
            };
            if id == page.id
                || role["managed"] == true
                || role["name"] == "@everyone"
                || entries.iter().any(|entry| {
                    entry["kind"].as_str() == Some("role") && entry["id"].as_str() == Some(id)
                })
            {
                continue;
            }
            let _ = write!(
                body,
                "<option value=\"{}\">{}</option>",
                escape(id),
                escape(role["name"].as_str().unwrap_or(id))
            );
        }
        let _ = write!(
            body,
            "</select><button class=\"button primary\" type=\"submit\">{}</button></form>",
            l.text("ticket-blacklist-add-role")
        );
    }
    let query = if page.member_search_kind == Some("blacklist") {
        page.input
            .get("member_query")
            .map(String::as_str)
            .unwrap_or("")
    } else {
        ""
    };
    let _ = write!(
        body,
        "<form method=\"post\" action=\"/g/{}/tickets/settings/blacklist/actions\"><input type=\"hidden\" name=\"intent\" value=\"blacklist-search\"><input type=\"hidden\" name=\"scope\" value=\"{}\"><label for=\"blacklist-member-query\">{}</label><input id=\"blacklist-member-query\" type=\"search\" name=\"member_query\" value=\"{}\" minlength=\"2\" minlength=\"1\" maxlength=\"100\"><button class=\"button secondary\" type=\"submit\">{}</button></form>",
        escape(page.id),
        escape(&scope),
        l.text("ticket-blacklist-user-search"),
        escape(query),
        l.text("ticket-team-search")
    );
    if page.member_search_kind == Some("blacklist") && page.member_results["truncated"] == true {
        let _ = write!(
            body,
            "<p class=\"settings-help\">{}</p>",
            l.text("ticket-members-refine-search")
        );
    }
    if page.member_search_kind == Some("blacklist") {
        for (index, member) in page.member_results["members"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some((id, name)) = member_label(member).filter(|(id, _)| decimal_snowflake(id))
            else {
                continue;
            };
            if entries.iter().any(|entry| {
                entry["kind"].as_str() == Some("user") && entry["id"].as_str() == Some(id)
            }) {
                continue;
            }
            let key = page.member_keys.get(index).map_or("", String::as_str);
            let _ = write!(
                body,
                "<form method=\"post\" action=\"/g/{}/tickets/settings/blacklist/actions\"><input type=\"hidden\" name=\"intent\" value=\"blacklist.add\"><input type=\"hidden\" name=\"scope\" value=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><input type=\"hidden\" name=\"target_kind\" value=\"user\"><input type=\"hidden\" name=\"target_id\" value=\"{}\"><input type=\"hidden\" name=\"member_query\" value=\"{}\"><input type=\"hidden\" name=\"operation_key\" value=\"{}\"><button class=\"button secondary\" type=\"submit\">{}: {}</button></form>",
                escape(page.id),
                escape(&scope),
                escape(page.blacklist_etag),
                escape(id),
                escape(page.input.get("member_query").map_or("", String::as_str)),
                escape(key),
                l.text("ticket-blacklist-add-user"),
                escape(&name)
            );
        }
    }
    body.push_str("</div></section>");
    body
}

fn integration_generation(value: &Value) -> Option<&str> {
    value["generation"].as_str().filter(|generation| {
        generation.len() == 32
            && generation
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn secret_configured(
    state: &Value,
    integration_id: &str,
    generation: &str,
    secret_name: &str,
) -> bool {
    state["integrations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|integration| {
            integration["integration_id"].as_str() == Some(integration_id)
                && integration["generation"].as_str() == Some(generation)
        })
        .and_then(|integration| integration["secrets"].as_array())
        .into_iter()
        .flatten()
        .find(|secret| secret["name"].as_str() == Some(secret_name))
        .is_some_and(|secret| secret["configured"] == true)
}

fn integration_editor(l: Locale<'_>, page: &TicketSettingsPage<'_>) -> String {
    let Some(schema) = page.integrations_schema else {
        return String::new();
    };
    let Some(columns) = form::columns(&schema["items"]) else {
        return String::new();
    };
    let field = Field {
        name: "integrations".into(),
        label: "integrations".into(),
        kind: Kind::List(columns.clone()),
        schema: schema.clone(),
    };
    let current = &page.effective["integrations"];
    let stored = page.stored["integrations"]
        .as_array()
        .map(|_| &page.stored["integrations"]);
    let rows = super::form::config_list_editor(
        l,
        &field,
        &columns,
        100,
        current,
        stored,
        page.input,
        page.directory,
    );
    let mut body = format!(
        "<section class=\"card settings-card\"><h2>{}</h2><p>{}</p>",
        l.text("config-tickets-integrations"),
        l.text(keys::TICKET_INTEGRATION_SECRET_HELP)
    );
    if let Some(errors) = page.errors.get("integrations") {
        for error in errors {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(error)
            );
        }
    }
    let _ = write!(
        body,
        "<form method=\"post\" action=\"/g/{}/tickets/settings\"><input type=\"hidden\" name=\"intent\" value=\"integrations\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><input type=\"hidden\" name=\"integrations_present\" value=\"1\">{rows}<div class=\"save-bar\"><button class=\"button primary\" type=\"submit\">{}</button></div></form>",
        escape(page.id),
        escape(page.etag),
        l.text(keys::SAVE)
    );
    if page.secret_saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::TICKET_INTEGRATION_SECRET_SAVED)
        );
    }
    if let Some(error) = page.integration_secret_error {
        let _ = write!(
            body,
            "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
            escape(error)
        );
    }
    if !page.integration_secrets_available {
        let _ = write!(
            body,
            "<p class=\"settings-help\">{}</p>",
            l.text(keys::TICKET_INTEGRATION_SECRET_STATUS_UNAVAILABLE)
        );
        body.push_str("</section>");
        return body;
    }
    if !page.can_manage_secrets {
        let has_secrets = current.as_array().is_some_and(|integrations| {
            integrations.iter().any(|integration| {
                matches!(integration["auth"].as_str(), Some("bearer" | "api_key"))
                    || integration["secrets"]
                        .as_array()
                        .is_some_and(|secrets| !secrets.is_empty())
            })
        });
        if has_secrets {
            let _ = write!(
                body,
                "<p class=\"settings-help\">{}</p>",
                l.text(keys::TICKET_INTEGRATION_SECRETS_OWNER_ONLY)
            );
        }
        body.push_str("</section>");
        return body;
    }
    let integrations = current.as_array().map_or(&[][..], Vec::as_slice);
    for integration in integrations {
        let Some(id) = integration["id"].as_str() else {
            continue;
        };
        let generation = integration_generation(integration);
        let mut secrets: Vec<String> = integration["secrets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|name| *name != "token")
            .map(str::to_owned)
            .collect();
        if matches!(integration["auth"].as_str(), Some("bearer" | "api_key")) {
            secrets.insert(0, "token".into());
        }
        if secrets.is_empty() {
            continue;
        }
        let _ = write!(
            body,
            "<section class=\"settings-row\"><h3>{}</h3><ul class=\"list-plain\">",
            escape(id)
        );
        for secret_name in secrets {
            let configured = generation.is_some_and(|generation| {
                secret_configured(page.integration_secret_state, id, generation, &secret_name)
            });
            let _ = write!(
                body,
                "<li><span class=\"mono\">{}</span>: {}",
                escape(&secret_name),
                l.text(if configured {
                    keys::TICKET_INTEGRATION_SECRET_CONFIGURED
                } else {
                    keys::TICKET_INTEGRATION_SECRET_MISSING
                })
            );
            let Some(generation) = generation else {
                let _ = write!(
                    body,
                    "<p class=\"settings-help\">{}</p>",
                    l.text(keys::TICKET_INTEGRATION_GENERATION_MISSING)
                );
                body.push_str("</li>");
                continue;
            };
            let action_prefix = format!("{id}:{generation}:{secret_name}");
            let set_key = page
                .integration_action_keys
                .get(&format!("{action_prefix}:set"))
                .map_or("", String::as_str);
            let remove_key = page
                .integration_action_keys
                .get(&format!("{action_prefix}:remove"))
                .map_or("", String::as_str);
            let _ = write!(
                body,
                "<form method=\"post\" action=\"/g/{}/tickets/settings/integrations/actions\"><input type=\"hidden\" name=\"intent\" value=\"integration.secret.set\"><input type=\"hidden\" name=\"integration_id\" value=\"{}\"><input type=\"hidden\" name=\"generation\" value=\"{}\"><input type=\"hidden\" name=\"secret_name\" value=\"{}\"><input type=\"hidden\" name=\"operation_key\" value=\"{}\"><label for=\"secret-{}-{}\">{}</label><input id=\"secret-{}-{}\" type=\"password\" name=\"value\" autocomplete=\"new-password\" required><button class=\"button secondary\" type=\"submit\">{}</button></form>",
                escape(page.id),
                escape(id),
                escape(generation),
                escape(&secret_name),
                escape(set_key),
                escape(id),
                escape(&secret_name),
                l.text(keys::TICKET_INTEGRATION_SECRET_VALUE),
                escape(id),
                escape(&secret_name),
                l.text(keys::TICKET_INTEGRATION_SECRET_SET)
            );
            if configured {
                let _ = write!(
                    body,
                    "<form method=\"post\" action=\"/g/{}/tickets/settings/integrations/actions\"><input type=\"hidden\" name=\"intent\" value=\"integration.secret.remove\"><input type=\"hidden\" name=\"integration_id\" value=\"{}\"><input type=\"hidden\" name=\"generation\" value=\"{}\"><input type=\"hidden\" name=\"secret_name\" value=\"{}\"><input type=\"hidden\" name=\"operation_key\" value=\"{}\"><button class=\"button secondary\" type=\"submit\">{}</button></form>",
                    escape(page.id),
                    escape(id),
                    escape(generation),
                    escape(&secret_name),
                    escape(remove_key),
                    l.text(keys::TICKET_INTEGRATION_SECRET_REMOVE)
                );
            }
            body.push_str("</li>");
        }
        body.push_str("</ul></section>");
    }
    body.push_str("</section>");
    body
}

pub(crate) fn ticket_blacklist_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    page: &TicketSettingsPage<'_>,
) -> String {
    let mut body = format!(
        "<header class=\"page-heading\">{}</header>",
        heading(l, keys::TICKETS)
    );
    if let Some(messages) = page.errors.get("") {
        for message in messages {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(message)
            );
        }
    }
    body.push_str(&blacklist_page(l, page));
    shell(l, headers, page.id, page.guild, keys::TICKETS, &body, false)
}

pub(crate) fn ticket_settings_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    page: &TicketSettingsPage<'_>,
) -> String {
    let mut body = format!(
        "<header class=\"page-heading\">{}</header>",
        heading(l, keys::TICKETS)
    );
    if let Some(messages) = page.errors.get("") {
        for message in messages {
            let _ = write!(
                body,
                "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
                escape(message)
            );
        }
    }
    if page.saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::SAVED)
        );
    }
    if page.teams_available {
        body.push_str(&team_editor(l, page));
    }
    body.push_str(&integration_editor(l, page));
    if page.module_available {
        let _ = write!(
            body,
            "<section class=\"card settings-card\"><h2>{}</h2><p>{}</p><p>{}</p><a class=\"button secondary\" href=\"/g/{}/modules/tickets\">{}</a></section>",
            l.text("ticket-settings-module"),
            l.text("ticket-settings-module-help"),
            l.text("ticket-settings-knowledge-help"),
            escape(page.id),
            l.text("ticket-settings-module-open")
        );
        let _ = write!(
            body,
            "<p><a class=\"button secondary\" href=\"/g/{}/tickets/settings/knowledge\">{}</a></p>",
            escape(page.id),
            l.text("ticket-settings-knowledge-open")
        );
        let _ = write!(
            body,
            "<p><a class=\"button secondary\" href=\"/g/{}/tickets/settings/blacklist\">{}</a></p>",
            escape(page.id),
            l.text("ticket-settings-blacklist-open")
        );
    }
    shell(l, headers, page.id, page.guild, keys::TICKETS, &body, false)
}
