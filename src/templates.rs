//! Rules the dashboard offers ready-made on the rules page of automod.
//!
//! A template is a whole rule in the shape the bot reads; adding one stores a copy under a free id.
//! The bot's own tests check these rules against its schema, so a template cannot drift into one it refuses.

use serde_json::{Value, json};

/// The templates, in the order shown. Their names and hints are `ed-tpl-<key>` and
/// `ed-tpl-<key>-hint` in the catalogs.
pub const KEYS: [&str; 7] = [
    "invites",
    "mention_spam",
    "duplicates",
    "flood",
    "files",
    "caps",
    "words",
];

fn delete_then_timeout(duration: &str) -> Value {
    json!([{"do": "delete"}, {"do": "timeout", "duration": duration}])
}

/// The rule of a template, named `name`; `None` for a key that is not one.
#[must_use]
pub fn rule(key: &str, name: &str) -> Option<Value> {
    let (triggers, actions, enabled) = match key {
        "invites" => (
            json!([{"type": "invites"}]),
            delete_then_timeout("10m"),
            true,
        ),
        "mention_spam" => (
            json!([{"type": "spam", "measure": "mentions", "amount": 8, "within": "30s"}]),
            delete_then_timeout("1h"),
            true,
        ),
        "duplicates" => (
            json!([{"type": "spam", "measure": "duplicates", "amount": 4, "within": "30s"}]),
            delete_then_timeout("10m"),
            true,
        ),
        "flood" => (
            json!([{"type": "spam", "measure": "messages", "amount": 7, "within": "10s"}]),
            delete_then_timeout("10m"),
            true,
        ),
        "files" => (
            json!([{"type": "attachments",
                "blocked_extensions": ["exe", "scr", "bat", "cmd", "msi", "jar", "vbs", "ps1"]}]),
            json!([{"do": "delete"}]),
            true,
        ),
        "caps" => (
            json!([{"type": "caps", "max_ratio": 0.8, "min_letters": 12}]),
            json!([{"do": "delete"}]),
            true,
        ),
        // It needs the writer's own words, so it starts switched off.
        "words" => (
            json!([{"type": "words", "words": ["example"], "whole_words": false,
                "normalize": true, "loose_matching": true}]),
            json!([{"do": "delete"}]),
            false,
        ),
        _ => return None,
    };
    Some(json!({"name": name, "enabled": enabled, "triggers": triggers, "actions": actions}))
}

/// The id a new rule takes: the key of the template, or the first of `key_2`, `key_3`... that no
/// rule has.
#[must_use]
pub fn free_id(key: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(key) {
        return key.to_owned();
    }
    (2..1000)
        .map(|n| format!("{key}_{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_makes_a_rule_with_a_trigger_and_an_action() {
        for key in KEYS {
            let rule = rule(key, "name").unwrap();
            assert_eq!(rule["name"], "name");
            assert!(!rule["triggers"].as_array().unwrap().is_empty(), "{key}");
            assert!(!rule["actions"].as_array().unwrap().is_empty(), "{key}");
        }
        assert!(rule("nothing", "x").is_none());
    }

    #[test]
    fn an_id_already_taken_gets_a_number() {
        let taken = ["invites", "invites_2"];
        assert_eq!(free_id("flood", |id| taken.contains(&id)), "flood");
        assert_eq!(free_id("invites", |id| taken.contains(&id)), "invites_3");
    }
}
