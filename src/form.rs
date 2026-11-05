use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Boolean,
    Integer,
    String,
    Enum(Vec<String>),
    Roles,
    /// Texts told apart by commas, such as the emojis of a rule.
    Words,
    /// `false`, `true`, or role ids told apart by commas.
    FlagOrRoles,
    /// A message with embeds, edited in the composer; the variables its texts may use.
    Message(Vec<String>),
    /// A setting with pages of its own in the structured editor: rules, counters, anti-raid.
    Structured,
    /// Named entries, one row each, with a text as the value.
    Entries,
    /// Rows of one fixed shape, such as the rules of a channel setting.
    List(Vec<Column>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub kind: Kind,
    pub required: bool,
    pub schema: Value,
}

pub const SETTINGS: &[&str] = &[
    "language",
    "log_channel",
    "moderator_roles",
    "prefix",
    "timezone",
];

fn first_branch(schema: &Value, root: &Value) -> Value {
    let mut current = schema.clone();
    for _ in 0..8 {
        if let Some(reference) = current["$ref"].as_str() {
            let Some(path) = reference.strip_prefix('#') else {
                break;
            };
            let Some(next) = root.pointer(path) else {
                break;
            };
            current = next.clone();
        } else if let Some(branch) = current["anyOf"]
            .as_array()
            .and_then(|v| v.iter().find(|v| v["type"] != "null"))
        {
            let mut next = branch.clone();
            if let (Some(inner), Some(outer)) = (next.as_object_mut(), current.as_object()) {
                for (key, value) in outer {
                    if key != "anyOf" && !inner.contains_key(key) {
                        inner.insert(key.clone(), value.clone());
                    }
                }
            }
            current = next;
        } else {
            break;
        }
    }
    current
}

#[must_use]
pub fn settings(schema: &Value) -> Vec<Field> {
    SETTINGS
        .iter()
        .filter_map(|name| {
            let source = schema["properties"].get(*name)?;
            let mut resolved = first_branch(source, schema);
            resolved["x-ui"]["server_settings"] = json!(true);
            if *name == "log_channel" {
                resolved["x-ui"]["widget"] = json!("channel");
            }
            let kind = if *name == "language" {
                Kind::Enum(vec!["auto".into(), "ja".into(), "en".into()])
            } else if *name == "moderator_roles" {
                Kind::Roles
            } else if let Some(values) = resolved["enum"].as_array() {
                Kind::Enum(
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .collect(),
                )
            } else if resolved["type"] == "string" {
                Kind::String
            } else {
                return None;
            };
            Some(Field {
                name: (*name).to_owned(),
                label: (*name).to_owned(),
                kind,
                schema: resolved,
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub label: String,
    pub kind: Kind,
    pub schema: Value,
}

/// Where a part comes in the page: the order the cards are shown in.
#[must_use]
pub fn section_rank(section: &str) -> usize {
    match section {
        "warn" | "behavior" | "destination" | "auto_delete" => 0,
        "timeout" | "events" | "auto_react" | "rules" => 1,
        "voice" | "counters" => 2,
        "ban" | "antiraid" => 3,
        "escalation" => 4,
        "templates" => 5,
        "roles" => 6,
        "messages" => 7,
        "texts" => 8,
        _ => 9,
    }
}

/// The part of a big module a field belongs to; the settings page shows one card per part.
#[must_use]
pub fn section(module: &str, field: &str) -> &'static str {
    match (module, field) {
        ("moderation" | "automod" | "logs" | "channels", "replies" | "texts") => "texts",
        ("moderation" | "automod", "remove_roles_on_mute" | "restore_roles_on_mute") => "roles",
        ("moderation", "warn_expire_days" | "require_reason" | "dm_on_action") => "warn",
        ("moderation", "timeout_method" | "timeout_role") => "timeout",
        ("moderation", "voice_mute_method" | "voice_mute_role") => "voice",
        ("moderation", "ban_delete_days") => "ban",
        (
            "automod",
            "dry_run" | "dm_on_action" | "warn_expire_days" | "timeout_method" | "timeout_role",
        ) => "behavior",
        ("logs", "channel" | "webhook") => "destination",
        ("logs", _) => "events",
        ("channels", "auto_delete") => "auto_delete",
        ("channels", "auto_react") => "auto_react",
        ("automod", "rules") => "rules",
        ("automod", "counters") => "counters",
        ("automod", "antiraid") => "antiraid",
        ("moderation" | "automod", name) if name.starts_with("dm_messages.") => "messages",
        ("moderation", "escalation") => "escalation",
        ("moderation", name) if name.starts_with("templates.") => "templates",
        _ => "other",
    }
}

/// The page of a module a field belongs to: `""` is the module's main page, and every other
/// part of a big module (its escalation, its rules, its anti-raid...) is a page of its own.
#[must_use]
pub fn part_of(module: &str, field: &str) -> &'static str {
    match section(module, field) {
        "warn" | "timeout" | "voice" | "ban" | "behavior" | "destination" | "events" | "roles"
        | "other" => "",
        part => part,
    }
}

/// The settings that switch one feature of a big module on and off. A feature that is off keeps its
/// settings and runs nothing, so these are not shown as fields of the form.
pub const FEATURE_KEYS: [&str; 5] = [
    "rules_enabled",
    "escalation_enabled",
    "templates_enabled",
    "auto_delete_enabled",
    "auto_react_enabled",
];

/// The setting that switches a part of a module, when the part can be switched.
#[must_use]
pub fn feature(module: &str, part: &str) -> Option<&'static str> {
    match (module, part) {
        ("automod", "rules") => Some("rules_enabled"),
        ("automod", "antiraid") => Some("antiraid.enabled"),
        ("moderation", "escalation") => Some("escalation_enabled"),
        ("moderation", "templates") => Some("templates_enabled"),
        ("channels", "auto_delete") => Some("auto_delete_enabled"),
        ("channels", "auto_react") => Some("auto_react_enabled"),
        _ => None,
    }
}

/// Whether the feature runs, by the configuration in force. The anti-raid also needs its settings.
#[must_use]
pub fn feature_on(module: &str, part: &str, effective: &Value) -> bool {
    let config = &effective["modules"][module]["config"];
    let Some(key) = feature(module, part) else {
        return true;
    };
    let flag = lookup(config, key).and_then(Value::as_bool).unwrap_or(true);
    flag && (part != "antiraid" || config["antiraid"].is_object())
}

/// Whether a part is edited with the structured editor (rules, counters, anti-raid) rather than
/// with a flat form.
#[must_use]
pub fn is_editor_part(part: &str) -> bool {
    matches!(part, "rules" | "counters" | "antiraid")
}

/// The parts of a module in the order shown, each with the fields it holds.
#[must_use]
pub fn parts(module: &str, fields: &[Field]) -> Vec<&'static str> {
    let mut parts: Vec<&'static str> = Vec::new();
    for field in fields {
        let part = part_of(module, &field.name);
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.sort_by_key(|part| {
        if part.is_empty() {
            0
        } else {
            section_rank(part) + 1
        }
    });
    parts
}

#[must_use]
pub fn dependency(module: &str, field: &str) -> Option<(&'static str, &'static str)> {
    if !matches!(module, "moderation" | "automod") {
        return None;
    }
    match field {
        "timeout_role" => Some(("timeout_method", "role")),
        "voice_mute_role" => Some(("voice_mute_method", "role")),
        _ => None,
    }
}

fn non_null(schema: &Value) -> Option<Value> {
    // A message may be a text or an object; the composer reads it whichever it is.
    if schema["x-ui"]["widget"] == "message" {
        let mut message = schema.clone();
        message.as_object_mut()?.remove("anyOf");
        return Some(message);
    }
    let Some(alternatives) = schema.get("anyOf").and_then(Value::as_array) else {
        return Some(schema.clone());
    };
    let choices: Vec<_> = alternatives
        .iter()
        .filter(|v| v["type"] != "null")
        .collect();
    if choices.len() != 1 {
        return None;
    }
    let mut inner = non_null(choices[0])?;
    if let (Some(inner), Some(outer)) = (inner.as_object_mut(), schema.as_object()) {
        for (key, value) in outer {
            if key != "anyOf" {
                inner.insert(key.clone(), value.clone());
            }
        }
    }
    Some(inner)
}

#[must_use]
pub fn modules(schema: &Value) -> Map<String, Value> {
    non_null(&schema["properties"]["modules"])
        .and_then(|v| v["properties"].as_object().cloned())
        .unwrap_or_default()
}

/// The columns of a list: the required ones first, then the rest; `None` when one is of a kind the form cannot show.
fn columns(items: &Value) -> Option<Vec<Column>> {
    let properties = items["properties"].as_object()?;
    let required: Vec<&str> = items["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let names = required
        .iter()
        .copied()
        .filter(|name| properties.contains_key(*name))
        .chain(
            properties
                .keys()
                .map(String::as_str)
                .filter(|name| !required.contains(name)),
        );
    names
        .map(|name| {
            let schema = non_null(&properties[name])?;
            let kind = match schema["type"].as_str()? {
                "boolean" => Kind::Boolean,
                "integer"
                    if schema["minimum"].as_i64().is_some()
                        && schema["maximum"].as_i64().is_some() =>
                {
                    Kind::Integer
                }
                "string" => match schema["enum"].as_array() {
                    Some(values) => Kind::Enum(
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                    ),
                    None => Kind::String,
                },
                "array" if schema["items"]["type"] == "string" => Kind::Words,
                _ => return None,
            };
            Some(Column {
                name: name.to_owned(),
                kind,
                required: required.contains(&name),
                schema,
            })
        })
        .collect()
}

#[must_use]
pub fn fields(module: &Value) -> Vec<Field> {
    let Some(config) = non_null(&module["properties"]["config"]) else {
        return vec![];
    };
    let Some(properties) = config["properties"].as_object() else {
        return vec![];
    };
    properties
        .iter()
        .filter(|(name, _)| !FEATURE_KEYS.contains(&name.as_str()))
        .flat_map(|(name, value)| match nested_lists(name, value) {
            Some(fields) => fields,
            None => field(name, value).into_iter().collect(),
        })
        .collect()
}

/// A property that is an object of lists or of texts shows as one field per member, named
/// `property.member`.
fn nested_lists(name: &str, value: &Value) -> Option<Vec<Field>> {
    let schema = non_null(value)?;
    let members = schema["properties"].as_object().filter(|members| {
        !members.is_empty() && schema["type"] == "object" && schema["x-ui"]["widget"].is_null()
    })?;
    members
        .iter()
        .map(|(member, value)| field(&format!("{name}.{member}"), value))
        .collect()
}

/// The value at `name` of a module's configuration; a dotted name goes into nested objects.
#[must_use]
pub fn lookup<'a>(config: &'a Value, name: &str) -> Option<&'a Value> {
    name.split('.')
        .try_fold(config, |value, part| value.get(part))
}

fn field(name: &str, value: &Value) -> Option<Field> {
    let schema = non_null(value)?;
    let ty = &schema["type"];
    let widget = schema["x-ui"]["widget"].as_str();
    let string = ty == "string"
        || ty.as_array().is_some_and(|a| {
            a.len() == 2 && a.contains(&json!("string")) && a.contains(&json!("null"))
        });
    let kind = if widget == Some("message") {
        Kind::Message(
            schema["x-ui"]["variables"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
        )
    } else if widget == Some("flag-or-roles") {
        Kind::FlagOrRoles
    } else if matches!(widget, Some("json" | "entries-json")) {
        Kind::Structured
    } else if widget == Some("entries") {
        Kind::Entries
    } else if ty == "boolean" {
        Kind::Boolean
    } else if ty == "integer"
        && schema["minimum"].as_i64().is_some()
        && schema["maximum"].as_i64().is_some()
    {
        Kind::Integer
    } else if string {
        if let Some(values) = schema["enum"].as_array() {
            Kind::Enum(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .collect(),
            )
        } else {
            Kind::String
        }
    } else if ty == "array" && schema["items"]["type"] == "object" {
        Kind::List(columns(&schema["items"])?)
    } else {
        return None;
    };
    let description = schema["description"].as_str().unwrap_or("");
    let key_like = !description.is_empty()
        && description
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
    let label = if description.is_empty() || key_like {
        name
    } else {
        description
    };
    Some(Field {
        name: name.to_owned(),
        label: label.to_owned(),
        kind,
        schema,
    })
}

// shortcut: only anchored character classes are checked here; any other syntax passes because the bot validates on save.
pub(crate) fn pattern_matches(pattern: &str, value: &str) -> bool {
    let Some(mut body) = pattern.strip_prefix('^').and_then(|p| p.strip_suffix('$')) else {
        return true;
    };
    let input: Vec<char> = value.chars().collect();
    let mut reachable = vec![false; input.len() + 1];
    reachable[0] = true;
    while !body.is_empty() {
        let Some(after_open) = body.strip_prefix('[') else {
            return true;
        };
        let Some((class, rest)) = after_open.split_once(']') else {
            return true;
        };
        let chars: Vec<char> = class.chars().collect();
        let (min, max, rest) = if let Some(rest) = rest.strip_prefix('+') {
            (1, input.len(), rest)
        } else if let Some(rest) = rest.strip_prefix('*') {
            (0, input.len(), rest)
        } else if let Some(rest) = rest.strip_prefix('{') {
            let Some((bounds, rest)) = rest.split_once('}') else {
                return true;
            };
            let Some((min, max)) = bounds.split_once(',') else {
                return true;
            };
            let (Ok(min), Ok(max)) = (min.parse::<usize>(), max.parse::<usize>()) else {
                return true;
            };
            (min, max.min(input.len()), rest)
        } else {
            (1, 1, rest)
        };
        let allowed = |c: char| {
            let mut i = 0;
            while i < chars.len() {
                if i + 2 < chars.len() && chars[i + 1] == '-' {
                    if chars[i] <= c && c <= chars[i + 2] {
                        return true;
                    }
                    i += 3;
                } else {
                    if chars[i] == c {
                        return true;
                    }
                    i += 1;
                }
            }
            false
        };
        let mut prefix = vec![0_usize; input.len() + 2];
        for (i, valid) in reachable.iter().enumerate() {
            prefix[i + 1] = prefix[i] + usize::from(*valid);
        }
        let mut next = vec![false; input.len() + 1];
        let mut run_start = 0;
        for end in 0..=input.len() {
            if end > 0 && !allowed(input[end - 1]) {
                run_start = end;
            }
            if end >= min {
                let start = end.saturating_sub(max).max(run_start);
                let last = end - min;
                next[end] = start <= last && prefix[last + 1] > prefix[start];
            }
        }
        reachable = next;
        body = rest;
    }
    reachable[input.len()]
}

#[must_use]
pub fn parse(field: &Field, input: &str) -> Option<Value> {
    if input.is_empty() {
        return (field.kind != Kind::Boolean).then_some(Value::Null);
    }
    match &field.kind {
        Kind::Boolean => match input {
            "true" => Some(json!(true)),
            "false" => Some(json!(false)),
            _ => None,
        },
        Kind::Integer => {
            let n: i64 = input.parse().ok()?;
            let min = field.schema["minimum"].as_i64().unwrap_or(i64::MIN);
            let max = field.schema["maximum"].as_i64().unwrap_or(i64::MAX);
            (min..=max).contains(&n).then(|| json!(n))
        }
        Kind::List(_) | Kind::Entries | Kind::Structured | Kind::Message(_) => None,
        Kind::FlagOrRoles => match input.trim() {
            "true" => Some(json!(true)),
            "false" => Some(json!(false)),
            text => {
                let ids: Vec<_> = text
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .collect();
                (!ids.is_empty()
                    && ids.len() <= 50
                    && ids.iter().all(|v| {
                        (1..=20).contains(&v.len()) && v.bytes().all(|c| c.is_ascii_digit())
                    }))
                .then(|| json!(ids))
            }
        },
        Kind::Words => {
            let values: Vec<_> = input
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            (!values.is_empty()).then(|| json!(values))
        }
        Kind::Roles => {
            let values: Vec<_> = input
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            if values.is_empty() {
                return Some(Value::Null);
            }
            if values.iter().any(|v| {
                let digits = v.strip_prefix('+').unwrap_or(v);
                digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit())
            }) {
                return None;
            }
            Some(json!(values))
        }
        Kind::String | Kind::Enum(_) => {
            // A text area sends its line breaks as CR LF; the bot keeps plain LF.
            let normalized = input.replace("\r\n", "\n");
            let input = normalized.as_str();
            if let Kind::Enum(values) = &field.kind
                && !values.iter().any(|v| v == input)
            {
                return None;
            }
            if field.schema["maxLength"]
                .as_u64()
                .is_some_and(|max| input.chars().count() as u64 > max)
            {
                return None;
            }
            if field.schema["minLength"]
                .as_u64()
                .is_some_and(|min| (input.chars().count() as u64) < min)
                || (field.name == "prefix" && input.chars().any(char::is_whitespace))
                || (field.name == "log_channel" && {
                    let digits = input.strip_prefix('+').unwrap_or(input);
                    digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit())
                })
            {
                return None;
            }
            if field.schema["pattern"]
                .as_str()
                .is_some_and(|pattern| !pattern_matches(pattern, input))
            {
                return None;
            }
            Some(json!(input))
        }
    }
}

/// A kind whose cell always has a value to start with, so that it does not make a row worth keeping.
#[must_use]
pub fn is_preset(kind: &Kind) -> bool {
    matches!(kind, Kind::Boolean | Kind::Enum(_))
}

/// The entries of a field of named entries, read from the inputs `<field>.<row>.k` (the name) and
/// `<field>.<row>.v` (the value). A row ticked for removal (`.x`) or left empty is dropped.
///
/// # Errors
/// Returns the reason when an entry cannot be read: a repeated name, or a missing one.
pub fn entries<S: std::hash::BuildHasher>(
    field: &Field,
    input: &HashMap<String, String, S>,
) -> Result<Value, String> {
    let prefix = format!("{}.", field.name);
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut map = Map::new();
    for number in numbers {
        let cell = |part: &str| {
            input
                .get(&format!("{prefix}{number}.{part}"))
                .map_or("", |v| v.trim())
        };
        if cell("x") == "1" || (cell("k").is_empty() && cell("v").is_empty()) {
            continue;
        }
        let (key, text) = (cell("k"), cell("v"));
        if key.is_empty() {
            return Err("a name is missing".to_owned());
        }
        if text.is_empty() {
            continue;
        }
        let value = json!(text.replace("\r\n", "\n"));
        if map.insert(key.to_owned(), value).is_some() {
            return Err(format!("{key}: repeated"));
        }
    }
    Ok(Value::Object(map))
}

/// The rows of a list field, read from the inputs named `<field>.<row>.<column>`.
/// A row ticked for removal or left empty is dropped; `None` when a row cannot be read.
#[must_use]
pub fn rows<S: std::hash::BuildHasher>(
    field: &Field,
    columns: &[Column],
    input: &HashMap<String, String, S>,
) -> Option<Value> {
    let prefix = format!("{}.", field.name);
    let mut numbers: Vec<usize> = input
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.split('.').next()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut rows = Vec::new();
    for number in numbers {
        let key = |column: &str| format!("{prefix}{number}.{column}");
        if input.get(&key("x")).is_some_and(|v| v == "1") {
            continue;
        }
        let mut row = Map::new();
        for column in columns {
            let raw = input.get(&key(&column.name)).map_or("", String::as_str);
            let cell = Field {
                name: column.name.clone(),
                label: column.name.clone(),
                kind: column.kind.clone(),
                schema: column.schema.clone(),
            };
            match parse(&cell, raw) {
                Some(Value::Null) => {}
                Some(value) => {
                    row.insert(column.name.clone(), value);
                }
                None if column.kind == Kind::Boolean && raw.is_empty() => {}
                None => return None,
            }
        }
        if !columns
            .iter()
            .any(|c| !is_preset(&c.kind) && row.contains_key(&c.name))
        {
            continue;
        }
        if columns
            .iter()
            .any(|c| c.required && !row.contains_key(&c.name))
        {
            return None;
        }
        rows.push(Value::Object(row));
    }
    Some(Value::Array(rows))
}

#[must_use]
pub fn settings_should_write(
    value: &Value,
    stored: Option<&Value>,
    effective: Option<&Value>,
) -> bool {
    fn normalized(v: &Value) -> Value {
        match v {
            Value::Array(a) if a.is_empty() => Value::Null,
            Value::Array(a) => {
                let mut values: Vec<String> = a
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                    .collect();
                values.sort();
                values.dedup();
                json!(values)
            }
            Value::Number(n) => json!(n.to_string()),
            Value::Object(o) if o.len() == 1 => o
                .iter()
                .next()
                .filter(|(_, v)| v.is_null())
                .map_or_else(|| v.clone(), |(k, _)| json!(k)),
            _ => v.clone(),
        }
    }
    let baseline = stored.filter(|v| !v.is_null()).or(effective);
    normalized(baseline.unwrap_or(&Value::Null)) != normalized(value)
}

#[must_use]
pub fn should_write(value: &Value, stored: Option<&Value>, effective: Option<&Value>) -> bool {
    // A setting the configuration does not know yet, left empty, stays unset.
    if value.is_null() && stored.is_none() && effective.is_none() {
        return false;
    }
    stored.is_some() || effective != Some(value)
}

/// # Errors
/// Returns an error when the document is not an object.
pub fn update<S: std::hash::BuildHasher>(
    document: &mut Value,
    module: &str,
    changes: &HashMap<String, Value, S>,
) -> Result<(), &'static str> {
    let root = document.as_object_mut().ok_or("invalid document")?;
    if root.get("modules").is_some_and(Value::is_null) {
        root.remove("modules");
    }
    let modules = root
        .entry("modules")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid modules")?;
    if modules.get(module).is_some_and(Value::is_null) {
        modules.remove(module);
    }
    let entry = modules
        .entry(module)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid module")?;
    if entry.get("config").is_some_and(Value::is_null) {
        entry.remove("config");
    }
    let config = entry
        .entry("config")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid config")?;
    for (key, value) in changes {
        let mut parts: Vec<&str> = key.split('.').collect();
        let last = parts.pop().unwrap_or(key);
        let mut node = &mut *config;
        for part in parts {
            node = node
                .entry(part)
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("invalid config")?;
        }
        node.insert(last.to_owned(), value.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_of_a_big_module_fall_into_their_parts() {
        assert_eq!(section("moderation", "timeout_role"), "timeout");
        assert_eq!(section("moderation", "ban_delete_days"), "ban");
        assert_eq!(section("moderation", "require_reason"), "warn");
        assert_eq!(section("logs", "channel"), "destination");
        assert_eq!(section("logs", "member_joins"), "events");
        assert_eq!(section("automod", "dry_run"), "behavior");
        assert_eq!(section("welcome", "channel"), "other");
        assert_eq!(section("unknown", "anything"), "other");
    }

    #[test]
    fn cards_come_in_a_fixed_order() {
        let mut parts = ["ban", "voice", "other", "timeout", "warn"];
        parts.sort_by_key(|p| section_rank(p));
        assert_eq!(parts, ["warn", "timeout", "voice", "ban", "other"]);
    }
    fn channel_rules() -> Value {
        json!({"properties": {"config": {"properties": {"auto_delete": {
            "type": "array", "maxItems": 25, "items": {
                "type": "object", "required": ["channel", "after"],
                "properties": {
                    "skip_pinned": {"type": "boolean", "default": true},
                    "channel": {"type": "string", "pattern": "^[1-9][0-9]{0,19}$"},
                    "after": {"type": "string", "pattern": "^[0-9]+[smhdw]( *[0-9]+[smhdw])*$"},
                    "emojis": {"type": "array", "items": {"type": "string"}}
                }
            }
        }}}}})
    }

    fn rows_of(input: &[(&str, &str)]) -> Option<Value> {
        let all = fields(&channel_rules());
        let field = &all[0];
        let Kind::List(columns) = &field.kind else {
            panic!("not a list");
        };
        let input: HashMap<String, String> = input
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        rows(field, columns, &input)
    }

    #[test]
    fn a_list_shows_required_columns_first() {
        let all = fields(&channel_rules());
        let Kind::List(columns) = &all[0].kind else {
            panic!("not a list");
        };
        let names: Vec<_> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["channel", "after", "emojis", "skip_pinned"]);
        assert_eq!(columns[2].kind, Kind::Words);
        assert!(columns[0].required && !columns[3].required);
    }

    #[test]
    fn a_list_reads_its_rows_and_drops_removed_and_empty_ones() {
        let rows = rows_of(&[
            ("auto_delete.0.channel", "10"),
            ("auto_delete.0.after", "1h 30m"),
            ("auto_delete.0.skip_pinned", "true"),
            ("auto_delete.1.channel", "11"),
            ("auto_delete.1.after", "5m"),
            ("auto_delete.1.x", "1"),
            ("auto_delete.2.channel", ""),
            ("auto_delete.2.after", ""),
            ("auto_delete.2.skip_pinned", "true"),
        ]);
        assert_eq!(
            rows,
            Some(json!([{"channel": "10", "after": "1h 30m", "skip_pinned": true}]))
        );
        assert_eq!(
            rows_of(&[("auto_delete.0.skip_pinned", "false")]),
            Some(json!([]))
        );
    }

    #[test]
    fn a_list_row_needs_its_required_columns_and_valid_cells() {
        assert_eq!(rows_of(&[("auto_delete.0.channel", "10")]), None);
        assert_eq!(
            rows_of(&[
                ("auto_delete.0.channel", "ten"),
                ("auto_delete.0.after", "1h")
            ]),
            None
        );
        assert_eq!(
            rows_of(&[
                ("auto_delete.0.channel", "10"),
                ("auto_delete.0.after", "1h"),
                ("auto_delete.0.emojis", " a , b ,"),
            ]),
            Some(json!([{"channel": "10", "after": "1h", "emojis": ["a", "b"]}]))
        );
    }

    #[test]
    fn a_list_takes_numbers_and_choices_and_ignores_a_row_with_only_a_preset_choice() {
        let module = json!({"properties": {"config": {"properties": {"escalation": {
            "type": "array", "items": {"type": "object", "required": ["warns", "action"],
            "properties": {
                "warns": {"type": "integer", "minimum": 1, "maximum": 100},
                "action": {"type": "string", "enum": ["timeout", "kick", "ban"]},
                "duration": {"type": "string"}
            }}
        }}}}});
        let all = fields(&module);
        let Kind::List(columns) = &all[0].kind else {
            panic!("not a list");
        };
        assert_eq!(columns[0].kind, Kind::Integer);
        assert!(matches!(&columns[1].kind, Kind::Enum(v) if v.len() == 3));
        let read = |pairs: &[(&str, &str)]| {
            let input: HashMap<String, String> = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            rows(&all[0], columns, &input)
        };
        assert_eq!(
            read(&[("escalation.0.warns", "3"), ("escalation.0.action", "kick")]),
            Some(json!([{"warns": 3, "action": "kick"}]))
        );
        assert_eq!(
            read(&[
                ("escalation.0.warns", ""),
                ("escalation.0.action", "timeout")
            ]),
            Some(json!([]))
        );
        assert_eq!(
            read(&[
                ("escalation.0.warns", "101"),
                ("escalation.0.action", "ban")
            ]),
            None
        );
    }

    #[test]
    fn an_object_of_lists_shows_one_list_per_member_and_saves_inside_it() {
        let list = json!({"type": "array", "items": {"type": "object", "properties": {
            "id": {"type": "string"}
        }}});
        let module = json!({"properties": {"config": {"properties": {"templates": {
            "type": "object", "properties": {"timeout": list, "kick": list}
        }}}}});
        let all = fields(&module);
        let names: Vec<_> = all.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["templates.kick", "templates.timeout"]);
        assert_eq!(section("moderation", "templates.kick"), "templates");
        let mut document = json!({"modules": {"moderation": {"config": {
            "templates": {"ban": [{"id": "a"}]}
        }}}});
        let changes = HashMap::from([("templates.kick".to_owned(), json!([{"id": "b"}]))]);
        update(&mut document, "moderation", &changes).unwrap();
        let config = &document["modules"]["moderation"]["config"];
        assert_eq!(
            lookup(config, "templates.kick"),
            Some(&json!([{"id": "b"}]))
        );
        assert_eq!(lookup(config, "templates.ban"), Some(&json!([{"id": "a"}])));
        assert_eq!(lookup(config, "templates.vcmute"), None);
    }

    fn entries_field(widget: &str) -> Field {
        let module = json!({"properties": {"config": {"properties": {"rules": {
            "type": "object", "x-ui": {"widget": widget}
        }}}}});
        fields(&module).remove(0)
    }

    fn typed(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn named_texts_are_read_row_by_row() {
        let field = entries_field("entries");
        assert_eq!(field.kind, Kind::Entries);
        let input = typed(&[
            ("rules.0.k", "done"),
            ("rules.0.v", "a\r\nb"),
            ("rules.1.k", "empty"),
            ("rules.1.v", ""),
            ("rules.2.k", "old"),
            ("rules.2.v", "x"),
            ("rules.2.x", "1"),
            ("rules.3.k", ""),
            ("rules.3.v", ""),
        ]);
        assert_eq!(entries(&field, &input), Ok(json!({"done": "a\nb"})));
        let repeated = typed(&[
            ("rules.0.k", "a"),
            ("rules.0.v", "x"),
            ("rules.1.k", "a"),
            ("rules.1.v", "y"),
        ]);
        assert_eq!(entries(&field, &repeated), Err("a: repeated".into()));
        assert!(entries(&field, &typed(&[("rules.0.v", "x")])).is_err());
    }

    #[test]
    fn a_setting_with_its_own_pages_is_not_a_form_field_and_a_role_setting_takes_flags_or_ids() {
        assert_eq!(entries_field("json").kind, Kind::Structured);
        assert_eq!(entries_field("entries-json").kind, Kind::Structured);
        let roles = entries_field("flag-or-roles");
        assert_eq!(parse(&roles, "true"), Some(json!(true)));
        assert_eq!(parse(&roles, " false "), Some(json!(false)));
        assert_eq!(parse(&roles, "1, 22"), Some(json!(["1", "22"])));
        assert_eq!(parse(&roles, "x"), None);
        assert_eq!(parse(&roles, "+1"), None);
    }

    #[test]
    fn an_object_of_texts_shows_one_field_per_member() {
        let text = json!({"type": "string", "maxLength": 1500});
        let module = json!({"properties": {"config": {"properties": {"dm_messages": {
            "type": "object", "properties": {"warn": text, "ban": text}
        }}}}});
        let names: Vec<_> = fields(&module).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["dm_messages.ban", "dm_messages.warn"]);
        assert_eq!(section("automod", "dm_messages.ban"), "messages");
        assert_eq!(section("logs", "replies"), "texts");
        assert_eq!(section("welcome", "texts"), "other");
    }

    #[test]
    fn an_empty_setting_that_is_not_in_the_configuration_is_not_written() {
        assert!(!should_write(&Value::Null, None, None));
        assert!(should_write(&json!("x"), None, None));
        assert!(should_write(&Value::Null, None, Some(&json!("x"))));
        assert!(should_write(&Value::Null, Some(&json!("x")), None));
    }

    #[test]
    fn a_text_area_keeps_its_lines_as_plain_line_feeds() {
        let module = json!({"properties":{"config":{"properties":{
            "greeting":{"type":["string","null"],"maxLength":2000}
        }}}});
        let fields = fields(&module);
        let greeting = fields.iter().find(|f| f.name == "greeting").unwrap();
        assert_eq!(
            parse(greeting, "Hi {user}\r\nWelcome"),
            Some(json!("Hi {user}\nWelcome"))
        );
        assert_eq!(parse(greeting, ""), Some(Value::Null));
    }
    #[test]
    fn settings_schema_and_roles() {
        let schema = json!({"properties":{"language":{"anyOf":[{"type":"string","enum":["auto","en","ja"]},{"type":"string"},{"type":"null"}]},"preset":{"$ref":"#/$defs/Preset"},"moderator_roles":{"type":"array"},"log_channel":{"anyOf":[{"type":"string"},{"type":"integer"}]},"version":{"type":"string"}},"$defs":{"Preset":{"anyOf":[{"type":"string","enum":["small","medium","large"]},{"type":"object"}]}}});
        let fields = settings(&schema);
        assert_eq!(fields.len(), 3);
        assert_eq!(
            fields[0].kind,
            Kind::Enum(vec!["auto".into(), "ja".into(), "en".into()])
        );
        let roles = fields.iter().find(|f| f.name == "moderator_roles").unwrap();
        assert_eq!(parse(roles, "1,+2"), Some(json!(["1", "+2"])));
        assert_eq!(parse(roles, "1,bad"), None);
        assert_eq!(parse(roles, ""), Some(Value::Null));
        assert!(!settings_should_write(
            &json!(["2", "1"]),
            Some(&json!([1, 2])),
            None
        ));
        assert!(!settings_should_write(
            &json!("123"),
            Some(&json!(123)),
            None
        ));
        assert!(!settings_should_write(
            &json!("medium"),
            Some(&json!({"medium":null})),
            None
        ));
    }
    #[test]
    fn schema_and_values() {
        let module = json!({"properties":{"config":{"anyOf":[{"type":"null"},{"properties":{
            "flag":{"anyOf":[{"type":"null"},{"type":"boolean","description":"welcome-flag"}]},
            "count":{"type":"integer","minimum":1,"maximum":3},
            "name":{"type":["string","null"],"pattern":"^[0-9]{1,3}$","maxLength":3},
            "choice":{"type":"string","enum":["a",null,"", "b"]},"skip":{"type":"array"}
        }}]}}});
        let f = fields(&module);
        assert_eq!(f.len(), 4);
        let get = |name: &str| f.iter().find(|f| f.name == name).unwrap();
        assert_eq!(get("choice").kind, Kind::Enum(vec!["a".into(), "b".into()]));
        assert_eq!(get("flag").label, "flag");
        assert_eq!(parse(get("flag"), "false"), Some(json!(false)));
        assert_eq!(parse(get("count"), "2"), Some(json!(2)));
        assert_eq!(parse(get("count"), "4"), None);
        assert_eq!(parse(get("count"), "x"), None);
        assert_eq!(parse(get("name"), "12"), Some(json!("12")));
        assert_eq!(parse(get("name"), "a"), None);
        assert_eq!(parse(get("name"), "1234"), None);
        assert!(pattern_matches("^[1-9][0-9]*$", "123"));
        assert!(!pattern_matches("^[1-9][0-9]*$", "012"));
        // Syntax this checker does not know passes: the bot validates on save.
        assert!(pattern_matches("^(?:a|b)+$", "zzz"));
        assert!(pattern_matches("^[0-9]{3}$", "abc"));
        let open = Field {
            kind: Kind::Integer,
            schema: json!({"minimum": 5}),
            ..get("count").clone()
        };
        assert_eq!(parse(&open, "9999"), Some(json!(9999)));
        assert_eq!(parse(&open, "4"), None);
        assert_eq!(parse(get("name"), ""), Some(Value::Null));
        assert_eq!(parse(get("choice"), "c"), None);
        assert!(!should_write(&json!(2), None, Some(&json!(2))));
        assert!(should_write(&json!(2), Some(&json!(2)), Some(&json!(2))));
        assert!(should_write(&json!(3), None, Some(&json!(2))));
        let mut doc =
            json!({"language":"ja","modules":{"x":{"enabled":true},"y":{"enabled":false}}});
        update(
            &mut doc,
            "x",
            &HashMap::from([("name".into(), Value::Null)]),
        )
        .unwrap();
        assert_eq!(
            doc,
            json!({"language":"ja","modules":{"x":{"enabled":true,"config":{"name":null}},"y":{"enabled":false}}})
        );
    }
}
