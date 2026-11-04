use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Boolean,
    Integer,
    String,
    Enum(Vec<String>),
    Roles,
}

pub const SETTINGS: &[&str] = &[
    "language",
    "timezone",
    "preset",
    "prefix",
    "log_channel",
    "log_webhook",
    "moderator_roles",
    "success_emoji",
    "error_emoji",
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
            let kind = if *name == "moderator_roles" {
                Kind::Roles
            } else if let Some(values) = resolved["enum"].as_array() {
                Kind::Enum(
                    values
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()?,
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

fn non_null(schema: &Value) -> Option<Value> {
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
        .filter_map(|(name, value)| {
            let schema = non_null(value)?;
            let ty = &schema["type"];
            let string = ty == "string"
                || ty.as_array().is_some_and(|a| {
                    a.len() == 2 && a.contains(&json!("string")) && a.contains(&json!("null"))
                });
            let kind = if ty == "boolean" {
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
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()?,
                    )
                } else {
                    Kind::String
                }
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
                name: name.clone(),
                label: label.to_owned(),
                kind,
                schema,
            })
        })
        .collect()
}

// shortcut: only anchored character classes are checked here; any other syntax passes because the bot validates on save.
fn pattern_matches(pattern: &str, value: &str) -> bool {
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
        config.insert(key.clone(), value.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_schema_and_roles() {
        let schema = json!({"properties":{"language":{"anyOf":[{"type":"string","enum":["auto","en","ja"]},{"type":"string"},{"type":"null"}]},"preset":{"$ref":"#/$defs/Preset"},"moderator_roles":{"type":"array"},"log_channel":{"anyOf":[{"type":"string"},{"type":"integer"}]},"version":{"type":"string"}},"$defs":{"Preset":{"anyOf":[{"type":"string","enum":["small","medium","large"]},{"type":"object"}]}}});
        let fields = settings(&schema);
        assert_eq!(fields.len(), 4);
        assert_eq!(
            fields[0].kind,
            Kind::Enum(vec!["auto".into(), "en".into(), "ja".into()])
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
            "choice":{"type":"string","enum":["a","b"]},"skip":{"type":"array"}
        }}]}}});
        let f = fields(&module);
        assert_eq!(f.len(), 4);
        let get = |name: &str| f.iter().find(|f| f.name == name).unwrap();
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
