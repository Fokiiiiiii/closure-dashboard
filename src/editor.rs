//! A schema-driven editor for the structured settings: the rules, the counters and the anti-raid of
//! the automod module. The schema says what a setting holds; the editor turns it into a tree of
//! fields, keeps the form as flat inputs (`State`), and reads the inputs back into a value.
//!
//! The inputs are named by their path: `r.triggers.0.words` is the `words` of the first trigger.
//! A list of variants keeps its rows as `<path>.<n>.__t` (the variant), a set of entries as
//! `<path>.<n>.__k` (the name); `<row>.__x` marks a row removed, and `<object>.__on` says that an
//! optional object is there.

use crate::form::pattern_matches;
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};

/// The inputs of an editor form, flat; a name may hold several values (checkboxes).
pub type State = HashMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// On, off, or left to the default.
    Flag,
    Int {
        min: i64,
        max: i64,
    },
    Num {
        min: f64,
        max: f64,
    },
    Text {
        pattern: Option<String>,
        min: usize,
        max: usize,
    },
    Pick(Vec<String>),
    /// Several of a few choices.
    Many(Vec<String>),
    /// Free texts, one per line.
    Words,
    /// Role ids, picked from the server's roles.
    Roles,
    /// Channel ids, picked from the server's channels.
    Channels,
    Channel,
    Role,
    /// A counter condition such as `>= 5`, with an optional reverse condition.
    Condition,
    Object(Vec<Prop>),
    /// Named entries, each a value of one shape.
    Entries {
        key_pattern: Option<String>,
        value: Box<Node>,
    },
    /// A list of objects told apart by a tag.
    Variants {
        tag: String,
        variants: Vec<Variant>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub name: String,
    pub node: Node,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub tag: String,
    pub props: Vec<Prop>,
}

/// Why a field cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Required,
    Invalid,
    Repeated,
}

pub type Errors = Vec<(String, Why)>;

/// The names of rules, counters and anti-raid levels: plain characters.
pub const ID_PATTERN: &str = "^[a-z0-9_-]{1,32}$";

/// A schema node without the `null` alternative and the single-branch `allOf`, with the keywords
/// of the wrapper kept.
fn plain(schema: &Value) -> Value {
    let mut current = schema.clone();
    for _ in 0..8 {
        let inner = if let Some(branch) = current["anyOf"]
            .as_array()
            .and_then(|a| a.iter().find(|a| a["type"] != "null"))
        {
            Some(("anyOf", branch.clone()))
        } else if let Some([only]) = current["allOf"].as_array().map(Vec::as_slice) {
            Some(("allOf", only.clone()))
        } else {
            None
        };
        let Some((key, mut next)) = inner else { break };
        if let (Some(inner), Some(outer)) = (next.as_object_mut(), current.as_object()) {
            for (name, value) in outer {
                if name != key && !inner.contains_key(name) {
                    inner.insert(name.clone(), value.clone());
                }
            }
        }
        current = next;
    }
    current
}

fn main_type(schema: &Value) -> &str {
    match &schema["type"] {
        Value::String(t) => t,
        Value::Array(types) => types
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or(""),
        _ => "",
    }
}

fn strings(values: &Value) -> Vec<String> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn props(schema: &Value, skip: Option<&str>) -> Option<Vec<Prop>> {
    let required = strings(&schema["required"]);
    schema["properties"]
        .as_object()?
        .iter()
        .filter(|(name, _)| Some(name.as_str()) != skip)
        // A property with no type is a reserved name: the bot reads nothing from it.
        .filter(|(_, value)| {
            !main_type(&plain(value)).is_empty() || value.get("properties").is_some()
        })
        .map(|(name, value)| {
            Some(Prop {
                name: name.clone(),
                node: node(name, value)?,
                required: required.contains(name),
            })
        })
        .collect()
}

/// The node of a schema; `None` when it has a shape the editor cannot show, so that a setting is
/// never shown in part.
#[must_use]
pub fn node(name: &str, schema: &Value) -> Option<Node> {
    if schema.to_string().contains("reverse_condition") && main_type(&plain(schema)) != "object" {
        return Some(Node::Condition);
    }
    let s = plain(schema);
    let pattern = s["pattern"].as_str().map(str::to_owned);
    match main_type(&s) {
        "boolean" => Some(Node::Flag),
        "integer" => Some(Node::Int {
            min: s["minimum"].as_i64().unwrap_or(i64::MIN),
            max: s["maximum"].as_i64().unwrap_or(i64::MAX),
        }),
        "number" => Some(Node::Num {
            min: s["minimum"].as_f64().unwrap_or(f64::MIN),
            max: s["maximum"].as_f64().unwrap_or(f64::MAX),
        }),
        "string" => Some(if let Some(values) = s["enum"].as_array() {
            Node::Pick(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            )
        } else if matches!(name, "channel" | "notify_channel") {
            Node::Channel
        } else if matches!(name, "role" | "marker_role") {
            Node::Role
        } else {
            Node::Text {
                pattern,
                min: s["minLength"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(0),
                max: s["maxLength"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(usize::MAX),
            }
        }),
        "array" => array_node(name, &s),
        "object" => {
            if s["properties"].as_object().is_some_and(|p| !p.is_empty()) {
                Some(Node::Object(props(&s, None)?))
            } else if let Some(value) = s["additionalProperties"]
                .as_object()
                .filter(|v| v.get("type") != Some(&json!("null")) || v.len() > 1)
                .map(|_| &s["additionalProperties"])
            {
                Some(Node::Entries {
                    key_pattern: Some(
                        s["propertyNames"]["pattern"]
                            .as_str()
                            .map_or_else(|| ID_PATTERN.to_owned(), str::to_owned),
                    ),
                    value: Box::new(node(name, value)?),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn array_node(name: &str, s: &Value) -> Option<Node> {
    let items = plain(&s["items"]);
    if let Some(alternatives) = items["oneOf"].as_array() {
        let tag = alternatives
            .first()?
            .get("properties")?
            .as_object()?
            .iter()
            .find(|(_, v)| v.get("const").is_some())?
            .0
            .clone();
        let variants = alternatives
            .iter()
            .map(|alternative| {
                Some(Variant {
                    tag: alternative["properties"][&tag]["const"]
                        .as_str()?
                        .to_owned(),
                    props: props(alternative, Some(&tag))?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Node::Variants { tag, variants })
    } else if let Some(values) = items["enum"].as_array() {
        Some(Node::Many(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ))
    } else if main_type(&items) == "string" {
        Some(match name {
            "roles" | "exclude_roles" => Node::Roles,
            "channels" | "exclude_channels" => Node::Channels,
            _ => Node::Words,
        })
    } else {
        None
    }
}

#[must_use]
pub fn join(path: &str, name: &str) -> String {
    format!("{path}.{name}")
}

fn put(out: &mut State, name: &str, value: String) {
    out.entry(name.to_owned()).or_default().push(value);
}

/// The first value of an input, or an empty text.
#[must_use]
pub fn one<'a>(state: &'a State, name: &str) -> &'a str {
    state
        .get(name)
        .and_then(|v| v.first())
        .map_or("", String::as_str)
}

/// The inputs that show `value`.
pub fn flatten(node: &Node, path: &str, value: &Value, out: &mut State) {
    match node {
        Node::Flag => {
            if let Some(on) = value.as_bool() {
                put(out, path, on.to_string());
            }
        }
        Node::Int { .. } | Node::Num { .. } => {
            if value.is_number() {
                put(out, path, value.to_string());
            }
        }
        Node::Text { .. } | Node::Pick(_) | Node::Channel | Node::Role => {
            if let Some(text) = value.as_str() {
                put(out, path, text.to_owned());
            }
        }
        Node::Many(_) | Node::Roles | Node::Channels => {
            for item in strings(value) {
                put(out, path, item);
            }
        }
        Node::Words => put(out, path, strings(value).join("\n")),
        Node::Condition => match value {
            Value::String(text) => put(out, path, text.clone()),
            Value::Object(map) => {
                if let Some(text) = map.get("condition").and_then(Value::as_str) {
                    put(out, path, text.to_owned());
                }
                if let Some(text) = map.get("reverse_condition").and_then(Value::as_str) {
                    put(out, &join(path, "rev"), text.to_owned());
                }
            }
            _ => {}
        },
        Node::Object(members) => flatten_members(members, path, value, out),
        Node::Entries { value: inner, .. } => {
            for (n, (key, item)) in value.as_object().into_iter().flatten().enumerate() {
                let row = join(path, &n.to_string());
                put(out, &join(&row, "__k"), key.clone());
                flatten(inner, &row, item, out);
            }
        }
        Node::Variants { tag, variants } => {
            for (n, item) in value.as_array().into_iter().flatten().enumerate() {
                let row = join(path, &n.to_string());
                let name = item[tag].as_str().unwrap_or("");
                put(out, &join(&row, "__t"), name.to_owned());
                if let Some(variant) = variants.iter().find(|v| v.tag == name) {
                    flatten_members(&variant.props, &row, item, out);
                }
            }
        }
    }
}

fn flatten_members(members: &[Prop], path: &str, value: &Value, out: &mut State) {
    for prop in members {
        if let Some(item) = value.get(&prop.name).filter(|v| !v.is_null()) {
            let child = join(path, &prop.name);
            if matches!(prop.node, Node::Object(_)) {
                put(out, &join(&child, "__on"), "1".to_owned());
            }
            flatten(&prop.node, &child, item, out);
        }
    }
}

/// The numbers of the rows under `path`, found by the marker input each row has.
#[must_use]
pub fn rows(state: &State, path: &str, marker: &str) -> Vec<usize> {
    let prefix = format!("{path}.");
    let suffix = format!(".{marker}");
    let numbers: BTreeSet<usize> = state
        .keys()
        .filter_map(|key| {
            key.strip_prefix(&prefix)?
                .strip_suffix(&suffix)?
                .parse()
                .ok()
        })
        .collect();
    numbers.into_iter().collect()
}

/// Adds an empty row under `path`: an entry, or a variant of the given tag.
pub fn add_row(state: &mut State, path: &str, tag: Option<&str>) {
    let (marker, value) = tag.map_or(("__k", ""), |tag| ("__t", tag));
    let next = rows(state, path, marker).last().map_or(0, |n| n + 1);
    put(
        state,
        &join(&join(path, &next.to_string()), marker),
        value.to_owned(),
    );
}

/// Takes a row, and everything in it, out of the form.
pub fn remove_row(state: &mut State, row: &str) {
    let prefix = format!("{row}.");
    state.retain(|name, _| !name.starts_with(&prefix));
}

/// A duration such as `1h 30m`: numbers with a unit of `s`, `m`, `h`, `d` or `w`.
fn duration(text: &str) -> bool {
    let mut rest = text.trim_end();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let after = &rest[digits..];
        if !(1..=10).contains(&digits) || !after.starts_with(['s', 'm', 'h', 'd', 'w']) {
            return false;
        }
        rest = after[1..].trim_start_matches(' ');
    }
    true
}

fn digits(text: &str) -> bool {
    (1..=20).contains(&text.len()) && text.bytes().all(|c| c.is_ascii_digit())
}

fn condition(text: &str) -> bool {
    let text = text.trim();
    let rest = [">=", "<=", "!=", ">", "<", "="]
        .iter()
        .find_map(|op| text.strip_prefix(op))
        .unwrap_or("")
        .trim();
    (1..=10).contains(&rest.len()) && rest.bytes().all(|c| c.is_ascii_digit())
}

fn member(prop: &Prop, path: &str, state: &State, errors: &mut Errors) -> Option<Value> {
    let name = join(path, &prop.name);
    let object = matches!(prop.node, Node::Object(_));
    if object && one(state, &join(&name, "__on")) != "1" {
        return None;
    }
    let value = parse(&prop.node, &name, state, errors);
    match value {
        None if object => Some(json!({})),
        None if prop.required => {
            errors.push((name, Why::Required));
            None
        }
        value => value,
    }
}

/// The value the inputs under `path` hold; `None` when they hold nothing.
pub fn parse(node: &Node, path: &str, state: &State, errors: &mut Errors) -> Option<Value> {
    let text = one(state, path).trim();
    let refuse = |errors: &mut Errors| {
        errors.push((path.to_owned(), Why::Invalid));
        None
    };
    match node {
        Node::Flag => match text {
            "true" => Some(json!(true)),
            "false" => Some(json!(false)),
            _ => None,
        },
        Node::Int { min, max } => match text {
            "" => None,
            _ => match text.parse::<i64>() {
                Ok(n) if (*min..=*max).contains(&n) => Some(json!(n)),
                _ => refuse(errors),
            },
        },
        Node::Num { min, max } => match text {
            "" => None,
            _ => match text.parse::<f64>() {
                Ok(n) if (*min..=*max).contains(&n) => Some(json!(n)),
                _ => refuse(errors),
            },
        },
        Node::Text { pattern, min, max } => {
            if text.is_empty() {
                return None;
            }
            let length = text.chars().count();
            let fits = length >= *min
                && length <= *max
                && pattern.as_deref().is_none_or(|p| {
                    if p.contains("[smhdw]") {
                        duration(text)
                    } else {
                        pattern_matches(p, text)
                    }
                });
            if fits {
                Some(json!(text))
            } else {
                refuse(errors)
            }
        }
        Node::Pick(values) => match text {
            "" => None,
            _ if values.iter().any(|v| v == text) => Some(json!(text)),
            _ => refuse(errors),
        },
        Node::Many(values) => {
            let picked: Vec<&String> = state
                .get(path)
                .into_iter()
                .flatten()
                .filter(|v| values.contains(v))
                .collect();
            (!picked.is_empty()).then(|| json!(picked))
        }
        Node::Words => {
            let lines: Vec<&str> = one(state, path)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            (!lines.is_empty()).then(|| json!(lines))
        }
        Node::Roles | Node::Channels => {
            let ids: BTreeSet<&String> = state
                .get(path)
                .into_iter()
                .flatten()
                .filter(|v| digits(v))
                .collect();
            (!ids.is_empty()).then(|| json!(ids))
        }
        Node::Channel | Node::Role => match text {
            "" => None,
            _ if digits(text) => Some(json!(text)),
            _ => refuse(errors),
        },
        Node::Condition => parse_condition(path, state, errors),
        Node::Object(members) => {
            let mut map = Map::new();
            for prop in members {
                if let Some(value) = member(prop, path, state, errors) {
                    map.insert(prop.name.clone(), value);
                }
            }
            (!map.is_empty()).then_some(Value::Object(map))
        }
        Node::Entries { key_pattern, value } => Some(parse_entries(
            key_pattern.as_deref(),
            value,
            path,
            state,
            errors,
        )),
        Node::Variants { tag, variants } => parse_variants(tag, variants, path, state, errors),
    }
}

fn parse_condition(path: &str, state: &State, errors: &mut Errors) -> Option<Value> {
    let text = one(state, path).trim();
    let reverse = one(state, &join(path, "rev")).trim();
    let valid = (text.is_empty() || condition(text)) && (reverse.is_empty() || condition(reverse));
    match (valid, text.is_empty(), reverse.is_empty()) {
        (true, true, true) => None,
        (true, false, true) => Some(json!(text)),
        (true, false, false) => Some(json!({"condition": text, "reverse_condition": reverse})),
        _ => {
            errors.push((path.to_owned(), Why::Invalid));
            None
        }
    }
}

fn parse_entries(
    key_pattern: Option<&str>,
    value: &Node,
    path: &str,
    state: &State,
    errors: &mut Errors,
) -> Value {
    let mut map = Map::new();
    for n in rows(state, path, "__k") {
        let row = join(path, &n.to_string());
        if one(state, &join(&row, "__x")) == "1" {
            continue;
        }
        let key = one(state, &join(&row, "__k")).trim();
        let item = parse(value, &row, state, errors);
        if key.is_empty() {
            if item.is_some() {
                errors.push((join(&row, "__k"), Why::Required));
            }
            continue;
        }
        if !key_pattern.is_none_or(|p| pattern_matches(p, key)) {
            errors.push((join(&row, "__k"), Why::Invalid));
            continue;
        }
        if map.contains_key(key) {
            errors.push((join(&row, "__k"), Why::Repeated));
            continue;
        }
        let item = item.or_else(|| matches!(value, Node::Object(_)).then(|| json!({})));
        if let Some(item) = item {
            map.insert(key.to_owned(), item);
        }
    }
    Value::Object(map)
}

fn parse_variants(
    tag: &str,
    variants: &[Variant],
    path: &str,
    state: &State,
    errors: &mut Errors,
) -> Option<Value> {
    let mut items = Vec::new();
    for n in rows(state, path, "__t") {
        let row = join(path, &n.to_string());
        if one(state, &join(&row, "__x")) == "1" {
            continue;
        }
        let name = one(state, &join(&row, "__t"));
        let Some(variant) = variants.iter().find(|v| v.tag == name) else {
            errors.push((row, Why::Invalid));
            continue;
        };
        let mut item = Map::new();
        item.insert(tag.to_owned(), json!(name));
        for prop in &variant.props {
            if let Some(value) = member(prop, &row, state, errors) {
                item.insert(prop.name.clone(), value);
            }
        }
        items.push(Value::Object(item));
    }
    (!items.is_empty()).then_some(Value::Array(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(name: &str) -> Value {
        let all: Value =
            serde_json::from_str(include_str!("../tests/fixtures/automod-structured.json"))
                .unwrap();
        all[name].clone()
    }

    fn rule_node() -> Node {
        match node("rules", &schema("rules")).unwrap() {
            Node::Entries { value, .. } => *value,
            other => panic!("{other:?}"),
        }
    }

    fn state_of(node: &Node, path: &str, value: &Value) -> State {
        let mut state = State::new();
        flatten(node, path, value, &mut state);
        state
    }

    #[test]
    fn a_rule_is_a_name_conditions_actions_and_a_scope() {
        let Node::Object(props) = rule_node() else {
            panic!("not an object");
        };
        let names: Vec<_> = props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "actions",
                "continue_after_match",
                "cooldown",
                "enabled",
                "name",
                "scope",
                "triggers"
            ]
        );
        let triggers = props.iter().find(|p| p.name == "triggers").unwrap();
        let Node::Variants { tag, variants } = &triggers.node else {
            panic!("not variants");
        };
        assert_eq!((tag.as_str(), variants.len()), ("type", 23));
        let actions = props.iter().find(|p| p.name == "actions").unwrap();
        let Node::Variants { tag, variants } = &actions.node else {
            panic!("not variants");
        };
        assert_eq!((tag.as_str(), variants.len()), ("do", 16));
        let words = variants.iter().find(|v| v.tag == "reply").unwrap();
        assert!(words.props.iter().any(|p| p.name == "text" && p.required));
    }

    #[test]
    fn a_rule_comes_back_as_it_went_in() {
        let rule = json!({
            "name": "テスト",
            "enabled": true,
            "cooldown": "10m",
            "scope": {"exclude_roles": ["5"], "member_age": {"max_days": 3}, "exempt_moderators": true},
            "triggers": [
                {"type": "words", "words": ["a", "b"], "whole_words": false, "targets": ["message"]},
                {"type": "spam", "measure": "mentions", "amount": 5, "within": "10s"}
            ],
            "actions": [
                {"do": "delete"},
                {"do": "reply", "text": "だめです", "auto_delete": "15s"},
                {"do": "timeout", "duration": "1h"}
            ]
        });
        let node = rule_node();
        let state = state_of(&node, "r", &rule);
        let mut errors = Errors::new();
        let back = parse(&node, "r", &state, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(back, Some(rule));
    }

    #[test]
    fn rows_are_added_and_taken_out_and_a_missing_required_value_is_named() {
        let node = rule_node();
        let mut state = state_of(&node, "r", &json!({"name": "x"}));
        add_row(&mut state, "r.triggers", Some("words"));
        add_row(&mut state, "r.triggers", Some("any_message"));
        assert_eq!(rows(&state, "r.triggers", "__t"), [0, 1]);
        let mut errors = Errors::new();
        let value = parse(&node, "r", &state, &mut errors).unwrap();
        assert_eq!(errors, [("r.triggers.0.words".to_owned(), Why::Required)]);
        assert_eq!(value["triggers"][1], json!({"type": "any_message"}));
        remove_row(&mut state, "r.triggers.0");
        let mut errors = Errors::new();
        parse(&node, "r", &state, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        state.insert("r.triggers.1.__x".into(), vec!["1".into()]);
        assert_eq!(
            parse(&node, "r", &state, &mut Errors::new()),
            Some(json!({"name": "x"}))
        );
    }

    #[test]
    fn values_out_of_range_and_unknown_choices_are_refused() {
        let node = rule_node();
        let mut state = State::new();
        add_row(&mut state, "r.triggers", Some("repeat"));
        state.insert("r.triggers.0.max_run".into(), vec!["1".into()]);
        add_row(&mut state, "r.actions", Some("timeout"));
        state.insert("r.actions.0.duration".into(), vec!["soon".into()]);
        add_row(&mut state, "r.triggers", Some("spam"));
        state.insert("r.triggers.1.measure".into(), vec!["bananas".into()]);
        let mut errors = Errors::new();
        parse(&node, "r", &state, &mut errors);
        let paths: Vec<_> = errors.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"r.triggers.0.max_run"), "{paths:?}");
        assert!(paths.contains(&"r.actions.0.duration"), "{paths:?}");
        assert!(paths.contains(&"r.triggers.1.measure"), "{paths:?}");
    }

    #[test]
    fn counters_and_anti_raid_are_edited_as_trees() {
        let counters = node("counters", &schema("counters")).unwrap();
        let value = json!({
            "warns": {"per_user": true, "decay": {"amount": 1, "every": "1h"},
                       "triggers": {"bad": ">= 1", "pair": {"condition": ">= 2", "reverse_condition": "< 1"}}}
        });
        let state = state_of(&counters, "c", &value);
        let mut errors = Errors::new();
        assert_eq!(parse(&counters, "c", &state, &mut errors), Some(value));
        assert!(errors.is_empty(), "{errors:?}");
        let raid = node("antiraid", &schema("antiraid")).unwrap();
        let value = json!({
            "levels": ["none", "low", "high"],
            "effects": {"high": {"pause_invites": true, "quarantine": {}, "slowmode": {"seconds": 30}},
                         "low": {"slowmode": {"seconds": 10}}}
        });
        let state = state_of(&raid, "a", &value);
        let mut errors = Errors::new();
        assert_eq!(parse(&raid, "a", &state, &mut errors), Some(value));
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn entries_need_a_valid_unique_name() {
        let counters = node("counters", &schema("counters")).unwrap();
        let mut state = State::new();
        for (n, key) in ["warns", "warns", "Bad Name"].into_iter().enumerate() {
            add_row(&mut state, "c", None);
            state.insert(format!("c.{n}.__k"), vec![key.into()]);
        }
        let mut errors = Errors::new();
        parse(&counters, "c", &state, &mut errors);
        assert_eq!(
            errors,
            [
                ("c.1.__k".to_owned(), Why::Repeated),
                ("c.2.__k".to_owned(), Why::Invalid)
            ]
        );
    }
}
