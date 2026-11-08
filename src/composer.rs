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
pub const MAX_BUTTONS: usize = 25;

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
    for (n, button) in value["buttons"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let b = format!("b.{n}");
        form.insert(format!("{b}.__"), "1".into());
        put(&mut form, format!("{b}.label"), text(button, "label"));
        put(&mut form, format!("{b}.role"), text(button, "role"));
    }
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

/// Adds an empty role button, up to the most Discord takes.
pub fn add_button(form: &mut Form) {
    if numbers(form, "b").len() < MAX_BUTTONS {
        form.insert(format!("b.{}.__", next(form, "b")), "1".into());
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
    let buttons: Vec<Value> = numbers(form, "b")
        .into_iter()
        .filter_map(|n| {
            let label = value(form, &format!("b.{n}.label"));
            let role = value(form, &format!("b.{n}.role"));
            (!label.is_empty() || !role.is_empty()).then(|| json!({"label": label, "role": role}))
        })
        .collect();
    if !buttons.is_empty() {
        message.insert("buttons".into(), Value::Array(buttons));
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

/// The message id in what was typed: the id itself, or the link Discord gives a message
/// (`https://discord.com/channels/<server>/<channel>/<message>`), which also names the channel.
#[must_use]
pub fn message_ref(text: &str) -> Option<(Option<String>, String)> {
    let text = text.trim();
    let digits = |s: &str| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit());
    if digits(text) {
        return Some((None, text.to_owned()));
    }
    let path = text
        .strip_prefix("https://discord.com/channels/")
        .or_else(|| text.strip_prefix("https://canary.discord.com/channels/"))
        .or_else(|| text.strip_prefix("https://ptb.discord.com/channels/"))?;
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    match parts.as_slice() {
        [guild, channel, message] if digits(guild) && digits(channel) && digits(message) => {
            Some((Some((*channel).to_owned()), (*message).to_owned()))
        }
        _ => None,
    }
}

/// The days since 1970-01-01 of a calendar date (proleptic Gregorian).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The calendar date of a count of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Where a browser is: how many minutes it is behind UTC (what `Date.getTimezoneOffset` says,
/// so Tokyo is `-540`). Anything unreadable is Tokyo, where the dashboard's users are.
#[must_use]
pub fn zone_minutes(text: &str) -> i64 {
    text.trim()
        .parse::<i64>()
        .ok()
        .filter(|m| (-840..=720).contains(m))
        .unwrap_or(-540)
}

/// The Unix second of a `YYYY-MM-DDTHH:MM` the writer typed, in the zone `behind_utc` minutes behind UTC.
#[must_use]
pub fn parse_local(text: &str, behind_utc: i64) -> Option<i64> {
    let (date, time) = text.trim().split_once('T')?;
    let mut d = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(str::parse::<i64>);
    let (hour, minute) = (t.next()?.ok()?, t.next()?.ok()?);
    if d.next().is_some()
        || t.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(1970..=9999).contains(&year)
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    // A day the month has: the 31st of February comes back as the 3rd of March, and is no date.
    if civil_from_days(days) != (year, month, day) {
        return None;
    }
    let local = days * 86_400 + hour * 3600 + minute * 60;
    Some(local + behind_utc * 60)
}

/// A Unix second as `YYYY-MM-DD HH:MM` in the zone `behind_utc` minutes behind UTC.
#[must_use]
pub fn show_local(unix: i64, behind_utc: i64) -> String {
    let local = unix - behind_utc * 60;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let seconds = local.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_that_does_not_exist_or_has_more_than_the_minute_is_refused() {
        assert!(parse_local("2026-02-28T10:00", 0).is_some());
        assert!(parse_local("2028-02-29T10:00", 0).is_some());
        for bad in [
            "2026-02-31T10:00",
            "2026-02-29T10:00",
            "2026-04-31T10:00",
            "2026-11-06T10:00:30",
        ] {
            assert_eq!(parse_local(bad, 0), None, "{bad}");
        }
    }

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
    fn role_buttons_are_read_from_their_rows_and_empty_rows_are_left_out() {
        let mut form = Form::new();
        add_button(&mut form);
        add_button(&mut form);
        add_button(&mut form);
        form.insert("c".into(), "pick".into());
        form.insert("b.0.label".into(), "News".into());
        form.insert("b.0.role".into(), "9".into());
        form.insert("b.2.label".into(), "Games".into());
        form.insert("b.2.role".into(), "10".into());
        assert_eq!(
            parse(&form)["buttons"],
            json!([{"label": "News", "role": "9"}, {"label": "Games", "role": "10"}])
        );
        remove(&mut form, "b.0");
        assert_eq!(parse(&form)["buttons"].as_array().unwrap().len(), 1);
        assert_eq!(flatten(&parse(&form))["b.0.role"], "10");
    }

    #[test]
    fn a_message_is_named_by_its_id_or_its_link() {
        assert_eq!(message_ref(" 123 "), Some((None, "123".into())));
        assert_eq!(
            message_ref("https://discord.com/channels/1/22/333"),
            Some((Some("22".into()), "333".into()))
        );
        assert_eq!(message_ref("https://example.com/channels/1/22/333"), None);
        assert_eq!(message_ref("12x"), None);
        assert_eq!(message_ref("https://discord.com/channels/1/22"), None);
    }

    #[test]
    fn a_local_time_goes_to_unix_seconds_and_back() {
        let unix = parse_local("2026-11-06T09:30", -540).unwrap();
        assert_eq!(unix, 1_793_925_000);
        assert_eq!(show_local(unix, -540), "2026-11-06 09:30");
        assert_eq!(show_local(unix, 0), "2026-11-06 00:30");
        assert_eq!(parse_local("1969-12-31T23:59", 0), None);
        assert_eq!(parse_local("2026-13-01T00:00", 0), None);
        assert_eq!(parse_local("soon", 0), None);
        assert_eq!(zone_minutes("x"), -540);
        assert_eq!(zone_minutes("300"), 300);
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
