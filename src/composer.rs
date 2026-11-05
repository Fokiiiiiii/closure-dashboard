//! The composer: a message with embeds, written as a form and read back into the message the bot
//! understands (`content` and `embeds`, as in the bot's `MessageSpec`).
//!
//! The form is flat. `c` is the text; `e.<n>` is an embed (`e.<n>.__` marks it), `e.<n>.f.<m>` one of
//! its fields. A row that was taken out leaves no inputs behind.

use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};

pub type Form = HashMap<String, String>;

pub const MAX_EMBEDS: usize = 10;
pub const MAX_FIELDS: usize = 25;

/// The colours offered as swatches: the value, and the name of its tooltip.
pub const COLORS: [(&str, &str); 6] = [
    ("#ed4245", "red"),
    ("#f57c3b", "orange"),
    ("#fee75c", "yellow"),
    ("#57f287", "green"),
    ("#3ba7f0", "blue"),
    ("#5865f2", "indigo"),
];

fn put(form: &mut Form, name: String, value: &str) {
    if !value.is_empty() {
        form.insert(name, value.to_owned());
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

/// The inputs that show `value`: a plain text, or an object with `content` and `embeds`.
#[must_use]
pub fn flatten(value: &Value) -> Form {
    let mut form = Form::new();
    if let Some(plain) = value.as_str() {
        put(&mut form, "c".into(), plain);
        return form;
    }
    put(&mut form, "c".into(), text(value, "content"));
    for (n, embed) in value["embeds"].as_array().into_iter().flatten().enumerate() {
        let e = format!("e.{n}");
        form.insert(format!("{e}.__"), "1".into());
        for key in ["title", "description", "url", "thumbnail", "image"] {
            put(&mut form, format!("{e}.{key}"), text(embed, key));
        }
        let color = match &embed["color"] {
            Value::Number(n) => n.as_u64().map(|c| format!("#{c:06x}")),
            Value::String(s) => Some(s.clone()),
            _ => None,
        };
        put(&mut form, format!("{e}.color"), &color.unwrap_or_default());
        put(
            &mut form,
            format!("{e}.author_name"),
            text(&embed["author"], "name"),
        );
        put(
            &mut form,
            format!("{e}.author_url"),
            text(&embed["author"], "url"),
        );
        put(
            &mut form,
            format!("{e}.author_icon"),
            text(&embed["author"], "icon_url"),
        );
        put(
            &mut form,
            format!("{e}.footer_text"),
            text(&embed["footer"], "text"),
        );
        put(
            &mut form,
            format!("{e}.footer_icon"),
            text(&embed["footer"], "icon_url"),
        );
        if embed["timestamp"] == true {
            form.insert(format!("{e}.timestamp"), "1".into());
        }
        for (m, field) in embed["fields"].as_array().into_iter().flatten().enumerate() {
            let f = format!("{e}.f.{m}");
            form.insert(format!("{f}.__"), "1".into());
            put(&mut form, format!("{f}.name"), text(field, "name"));
            put(&mut form, format!("{f}.value"), text(field, "value"));
            if field["inline"] == true {
                form.insert(format!("{f}.inline"), "1".into());
            }
        }
    }
    form
}

/// The numbers of the rows under `prefix`, found by their marker.
#[must_use]
pub fn numbers(form: &Form, prefix: &str) -> Vec<usize> {
    let start = format!("{prefix}.");
    let found: BTreeSet<usize> = form
        .keys()
        .filter_map(|k| k.strip_prefix(&start)?.strip_suffix(".__")?.parse().ok())
        .collect();
    found.into_iter().collect()
}

fn next(form: &Form, prefix: &str) -> usize {
    numbers(form, prefix).last().map_or(0, |n| n + 1)
}

/// Adds an empty embed, up to the most Discord takes.
pub fn add_embed(form: &mut Form) {
    if numbers(form, "e").len() < MAX_EMBEDS {
        form.insert(format!("e.{}.__", next(form, "e")), "1".into());
    }
}

/// Adds an empty field to an embed, up to the most Discord takes.
pub fn add_field(form: &mut Form, embed: usize) {
    let prefix = format!("e.{embed}.f");
    if form.contains_key(&format!("e.{embed}.__")) && numbers(form, &prefix).len() < MAX_FIELDS {
        form.insert(format!("{prefix}.{}.__", next(form, &prefix)), "1".into());
    }
}

/// Takes a row, and what is in it, out of the form.
pub fn remove(form: &mut Form, row: &str) {
    let inner = format!("{row}.");
    form.retain(|name, _| name != row && !name.starts_with(&inner));
}

fn value<'a>(form: &'a Form, name: &str) -> &'a str {
    form.get(name).map_or("", |v| v.trim())
}

fn set(map: &mut Map<String, Value>, key: &str, text: &str) {
    if !text.is_empty() {
        map.insert(key.to_owned(), json!(text));
    }
}

fn pair(a: (&str, &str), b: (&str, &str), c: (&str, &str)) -> Option<Value> {
    let mut map = Map::new();
    for (key, text) in [a, b, c] {
        set(&mut map, key, text);
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// The message the form holds, as the bot reads it; what is left empty is left out.
#[must_use]
pub fn parse(form: &Form) -> Value {
    let mut message = Map::new();
    let content = form
        .get("c")
        .map_or(String::new(), |c| c.replace("\r\n", "\n"));
    if !content.trim().is_empty() {
        message.insert("content".into(), json!(content));
    }
    let mut embeds = Vec::new();
    for n in numbers(form, "e") {
        let e = format!("e.{n}");
        let mut embed = Map::new();
        for key in ["title", "description", "url", "thumbnail", "image"] {
            let raw = form
                .get(&format!("{e}.{key}"))
                .map_or(String::new(), |v| v.replace("\r\n", "\n"));
            set(&mut embed, key, raw.trim());
        }
        let color = match value(form, &format!("{e}.color")) {
            "custom" => value(form, &format!("{e}.color_custom")),
            other => other,
        };
        set(&mut embed, "color", color);
        if let Some(author) = pair(
            ("name", value(form, &format!("{e}.author_name"))),
            ("url", value(form, &format!("{e}.author_url"))),
            ("icon_url", value(form, &format!("{e}.author_icon"))),
        ) {
            embed.insert("author".into(), author);
        }
        if let Some(footer) = pair(
            ("text", value(form, &format!("{e}.footer_text"))),
            ("icon_url", value(form, &format!("{e}.footer_icon"))),
            ("", ""),
        ) {
            embed.insert("footer".into(), footer);
        }
        if form.contains_key(&format!("{e}.timestamp")) {
            embed.insert("timestamp".into(), json!(true));
        }
        let fields: Vec<Value> = numbers(form, &format!("{e}.f"))
            .into_iter()
            .filter_map(|m| {
                let f = format!("{e}.f.{m}");
                let (name, text) = (value(form, &format!("{f}.name")), value(form, &format!("{f}.value")));
                (!name.is_empty() || !text.is_empty()).then(|| {
                    json!({"name": name, "value": text, "inline": form.contains_key(&format!("{f}.inline"))})
                })
            })
            .collect();
        if !fields.is_empty() {
            embed.insert("fields".into(), Value::Array(fields));
        }
        // An embed with nothing in it is a row that was never filled in.
        if !embed.is_empty() {
            embeds.push(Value::Object(embed));
        }
    }
    if !embeds.is_empty() {
        message.insert("embeds".into(), Value::Array(embeds));
    }
    Value::Object(message)
}

/// The message as a setting stores it: a plain text when that is all there is, `null` when it is
/// empty (so that the default comes back), else the whole message.
#[must_use]
pub fn setting(message: Value) -> Value {
    let only_text = message
        .as_object()
        .is_some_and(|m| m.len() == 1 && m.contains_key("content"));
    if only_text {
        return message["content"].clone();
    }
    if message.as_object().is_some_and(Map::is_empty) {
        return Value::Null;
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "content": "Hello",
            "embeds": [{
                "title": "T", "description": "D", "url": "https://x.example", "color": 5_793_266,
                "author": {"name": "A", "icon_url": "https://a.example/i.png"},
                "footer": {"text": "F"}, "timestamp": true, "thumbnail": "https://t.example/t.png",
                "fields": [{"name": "n", "value": "v", "inline": true}, {"name": "m", "value": "w", "inline": false}]
            }]
        })
    }

    #[test]
    fn a_message_comes_back_as_it_went_in_with_its_colour_as_hex() {
        let form = flatten(&sample());
        assert_eq!(form["e.0.color"], "#5865f2");
        let back = parse(&form);
        let mut expected = sample();
        expected["embeds"][0]["color"] = json!("#5865f2");
        assert_eq!(back, expected);
    }

    #[test]
    fn a_custom_colour_wins_and_empty_rows_are_left_out() {
        let mut form = Form::new();
        add_embed(&mut form);
        add_embed(&mut form);
        form.insert("e.1.title".into(), "x".into());
        form.insert("e.1.color".into(), "custom".into());
        form.insert("e.1.color_custom".into(), "#112233".into());
        add_field(&mut form, 1);
        add_field(&mut form, 1);
        form.insert("e.1.f.1.name".into(), "a".into());
        form.insert("e.1.f.1.value".into(), "b".into());
        let message = parse(&form);
        assert_eq!(message["embeds"].as_array().unwrap().len(), 1);
        assert_eq!(message["embeds"][0]["color"], "#112233");
        assert_eq!(
            message["embeds"][0]["fields"],
            json!([{"name": "a", "value": "b", "inline": false}])
        );
    }

    #[test]
    fn rows_are_added_up_to_the_limit_and_taken_out_with_their_inputs() {
        let mut form = Form::new();
        for _ in 0..12 {
            add_embed(&mut form);
        }
        assert_eq!(numbers(&form, "e").len(), MAX_EMBEDS);
        form.insert("e.3.title".into(), "t".into());
        add_field(&mut form, 3);
        remove(&mut form, "e.3");
        assert!(form.keys().all(|k| !k.starts_with("e.3.")));
        assert_eq!(numbers(&form, "e").len(), MAX_EMBEDS - 1);
        add_field(&mut form, 3);
        assert!(!form.contains_key("e.3.f.0.__"));
    }

    #[test]
    fn a_setting_is_a_plain_text_when_that_is_all_it_has() {
        assert_eq!(setting(json!({"content": "hi"})), json!("hi"));
        assert_eq!(setting(json!({})), Value::Null);
        assert_eq!(setting(sample()), sample());
        assert_eq!(flatten(&json!("plain"))["c"], "plain");
    }
}
