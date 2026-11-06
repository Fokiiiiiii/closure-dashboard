use std::collections::HashSet;
use std::sync::Mutex;

use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use unic_langid::LanguageIdentifier;

use crate::html::escape;

pub mod keys {
    macro_rules! keys {
        ($($name:ident = $value:literal),* $(,)?) => {
            $(pub const $name: &str = $value;)*
            pub const ALL: &[&str] = &[$($name),*];
        };
    }
    keys! {
        TITLE = "title", LOGIN = "login", LOGOUT = "logout",
        SERVERS = "servers", EMPTY = "empty", OVERVIEW = "overview", MODULES = "modules",
        NONE = "none", ENABLED = "enabled", DISABLED = "disabled",
        DENIED = "denied", UNAVAILABLE = "unavailable", NOTFOUND = "notfound",
        INVALID = "invalid", CONFLICT = "conflict", OAUTH = "oauth",
        TIME_NOW = "time-now", TIME_YESTERDAY = "time-yesterday",
        TIME_MINUTE = "time-minute", TIME_HOUR = "time-hour", TIME_DAY = "time-day",
        NAME = "name", NAVIGATION = "navigation",
        HISTORY = "history", PERMISSIONS = "permissions", PERIOD = "period",
        DAYS = "days", SUMMARY = "summary", MEMBERS = "members", JOINS = "joins",
        LEAVES = "leaves", MESSAGES = "messages", MODERATION = "moderation",
        CHARTS = "charts", JOIN_LEAVE = "join-leave",
        NO_DATA = "no-data", STATS_FAILED = "stats-failed",
        OTHER = "other", TABLE_VIEW = "table-view", DATE = "date", COUNT = "count",
        LOG = "log", NUMBER = "number", KIND = "kind",
        TARGET = "target", MODERATOR = "moderator", REASON = "reason", TIME = "time",
        REVOKED = "revoked", INITIAL = "initial", NO_CHANGE = "no-change",
        MORE = "more", CHANGE = "change", ACTOR = "actor", REVISION = "revision",
        ROLES = "roles",
        LEVEL = "level", VALUE = "value", SOURCE = "source", ROLE_EMPTY = "role-empty",
        USER_ASSIGNMENTS = "user-assignments", KEY_OVERRIDES = "key-overrides",
        WARN_KIND = "warn-kind", TIMEOUT_KIND = "timeout-kind", KICK_KIND = "kick-kind",
        BAN_KIND = "ban-kind", VCMUTE_KIND = "vcmute-kind",
        SETTINGS = "settings", NO_SETTINGS = "no-settings", SAVE = "save",
        DISCARD = "discard", SAVED = "saved", UNSET = "unset", FIELD_INVALID = "field-invalid",
        RELOAD = "reload", CONFLICT_RELOAD = "conflict-reload",
        SERVER_SETTINGS = "server-settings", UNKNOWN_ROLE = "unknown-role",
        SETTINGS_LANGUAGE = "settings-language",
        SETTINGS_TIMEZONE = "settings-timezone",
        SETTINGS_PREFIX = "settings-prefix", SETTINGS_LOG_CHANNEL = "settings-log-channel",
        SETTINGS_MODERATOR_ROLES = "settings-moderator-roles",
        LANG_AUTO = "lang-auto", LANG_JA = "lang-ja", LANG_EN = "lang-en",
        CAT_MODERATION = "cat-moderation", CAT_COMMUNITY = "cat-community", CAT_SYSTEM = "cat-system", CAT_OTHER = "cat-other", SEC_WARN = "sec-warn", SEC_TIMEOUT = "sec-timeout", SEC_VOICE = "sec-voice", SEC_BAN = "sec-ban", SEC_BEHAVIOR = "sec-behavior", SEC_DESTINATION = "sec-destination", SEC_EVENTS = "sec-events", SEC_AUTO_DELETE = "sec-auto-delete", SEC_AUTO_REACT = "sec-auto-react", COL_CHANNEL = "col-channel", COL_AFTER = "col-after", COL_SKIP_PINNED = "col-skip-pinned", COL_EMOJIS = "col-emojis", COL_SKIP_BOTS = "col-skip-bots", COL_REMOVE = "col-remove", SEC_ESCALATION = "sec-escalation", COL_WARNS = "col-warns", COL_ACTION = "col-action", COL_DURATION = "col-duration", OPT_TIMEOUT = "opt-timeout", OPT_KICK = "opt-kick", OPT_BAN = "opt-ban", SEC_TEMPLATES = "sec-templates", COL_ID = "col-id", COL_LABEL = "col-label", COL_REASON = "col-reason", SEC_RULES = "sec-rules", SEC_COUNTERS = "sec-counters", SEC_ANTIRAID = "sec-antiraid", SEC_MESSAGES = "sec-messages", SEC_TEXTS = "sec-texts", COL_MESSAGE_ID = "col-message-id", COL_RULE_NAME = "col-rule-name", COL_COUNTER_NAME = "col-counter-name", COL_NAME = "col-name", COL_TEXT = "col-text", COL_JSON = "col-json", SEC_ROLES = "sec-roles", COMPOSE = "compose", CP_NOW = "composer-now", CP_EMPTY = "composer-empty", CP_COUNT = "composer-count", CP_COLOR = "composer-color", CP_NO_COLOR = "composer-no-color", CP_CUSTOM_COLOR = "composer-custom-color", CP_EMBED_N = "composer-embed-n", CP_AUTHOR = "composer-author", CP_NAME = "composer-name", CP_LINK = "composer-link", CP_ICON = "composer-icon", CP_TITLE = "composer-title", CP_TITLE_LINK = "composer-title-link", CP_DESCRIPTION = "composer-description", CP_FIELDS = "composer-fields", CP_FIELD_NAME = "composer-field-name", CP_FIELD_VALUE = "composer-field-value", CP_INLINE = "composer-inline", CP_ADD_FIELD = "composer-add-field", CP_IMAGES = "composer-images", CP_THUMBNAIL = "composer-thumbnail", CP_IMAGE = "composer-image", CP_FOOTER = "composer-footer", CP_TEXT = "composer-text", CP_TIMESTAMP = "composer-timestamp", CP_CHANNEL = "composer-channel", CP_VARIABLES = "composer-variables", CP_CONTENT = "composer-content", CP_ADD_EMBED = "composer-add-embed", CP_SEND = "composer-send", CP_PREVIEW = "composer-preview", CP_SENT = "composer-sent", CP_OPEN = "composer-open", CP_CHOOSE_CHANNEL = "composer-choose-channel", CP_NOTHING = "composer-nothing", CP_EDIT_MESSAGE = "composer-edit-message", CP_DEFAULT_MESSAGE = "composer-default-message", ED_ERR_REQUIRED = "editor-err-required", ED_ERR_INVALID = "editor-err-invalid", ED_ERR_REPEATED = "editor-err-repeated", ED_DEFAULT = "editor-default", ED_ON = "editor-on", ED_OFF = "editor-off", ED_DURATION_EXAMPLE = "editor-duration-example", ED_REVERSE_CONDITION = "editor-reverse-condition", ED_ADD = "editor-add", ED_ADD_CONDITION = "editor-add-condition", ED_ADD_ACTION = "editor-add-action", ED_ADD_RULE = "editor-add-rule", ED_ADD_COUNTER = "editor-add-counter", ED_REMOVE = "editor-remove", ED_EDIT = "editor-edit", ED_IF = "editor-if", ED_IF_HINT = "editor-if-hint", ED_THEN = "editor-then", ED_THEN_HINT = "editor-then-hint", ED_BASIC = "editor-basic", ED_SCOPE = "editor-scope", ED_MORE = "editor-more", ED_NO_RULES = "editor-no-rules", ED_NO_COUNTERS = "editor-no-counters", ED_NO_TRIGGERS = "editor-no-triggers", ED_NO_ACTIONS = "editor-no-actions", ED_BACK = "editor-back", ED_ANTIRAID_ON = "editor-antiraid-on", ED_UNSUPPORTED = "editor-unsupported", ED_NEW_RULE = "editor-new-rule", ED_NEW_COUNTER = "editor-new-counter", ED_NAME_KEY = "editor-name-key", MOD_MODERATION = "mod-moderation", MOD_AUTOMOD = "mod-automod", MOD_LOGS = "mod-logs", MOD_WELCOME = "mod-welcome", MOD_CHANNELS = "mod-channels", MOD_TAGS = "mod-tags", MOD_REMIND = "mod-remind", MOD_DIAGNOSTICS = "mod-diagnostics", MOD_DASHBOARD = "mod-dashboard",
        MOD_ROLES = "mod-roles", MOD_PERSIST = "mod-persist", MOD_UTILITY = "mod-utility", MOD_COMPANION = "mod-companion",
        OPT_DISCORD = "opt-discord", OPT_ROLE = "opt-role", OPT_PRIVATE = "opt-private", OPT_PUBLIC = "opt-public",
        HISTORY_ENABLED = "history-enabled", HISTORY_DISABLED = "history-disabled",
        HISTORY_CONFIG = "history-config", HISTORY_ADD = "history-add", HISTORY_RESET = "history-reset",
        HISTORY_LANGUAGE = "history-language", HISTORY_TIMEZONE = "history-timezone", HISTORY_PREFIX = "history-prefix",
        HISTORY_LOG_CHANNEL = "history-log-channel", HISTORY_ROLES = "history-roles", HISTORY_PRESET = "history-preset",
        HISTORY_WEBHOOK = "history-webhook", HISTORY_EMOJI = "history-emoji", HISTORY_PERMISSIONS = "history-permissions",
        HISTORY_GENERIC = "history-generic"
    }
}

#[derive(Clone, Copy)]
pub struct Definition {
    pub tag: &'static str,
    pub name: &'static str,
    pub dir: &'static str,
    pub source: &'static str,
}

/// Groups digits by thousands.
// shortcut: a comma for every locale (ja and en agree); when a locale needs another separator, move it into `Definition`.
#[must_use]
pub fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub const REGISTRY: &[Definition] = &[
    Definition {
        tag: "ja",
        name: "日本語",
        dir: "ltr",
        source: include_str!("../locales/ja.ftl"),
    },
    Definition {
        tag: "en",
        name: "English",
        dir: "ltr",
        source: include_str!("../locales/en.ftl"),
    },
];

pub struct Catalogs {
    entries: Vec<(Definition, FluentBundle<FluentResource>)>,
    warned: Mutex<HashSet<(String, String)>>,
}

impl Catalogs {
    /// Constructs and validates the registered catalogs.
    ///
    /// # Errors
    /// Returns an error if a tag or Fluent source is invalid, or if a message is duplicated.
    pub fn new(definitions: &[Definition]) -> Result<Self, String> {
        let mut entries = Vec::new();
        for definition in definitions {
            let lang: LanguageIdentifier = definition
                .tag
                .parse()
                .map_err(|_| format!("invalid locale: {}", definition.tag))?;
            if entries
                .iter()
                .any(|(d, _): &(Definition, FluentBundle<FluentResource>)| {
                    d.tag.eq_ignore_ascii_case(definition.tag)
                })
            {
                return Err(format!("duplicate locale: {}", definition.tag));
            }
            let resource = FluentResource::try_new(definition.source.to_owned())
                .map_err(|(_, e)| format!("invalid catalog {}: {e:?}", definition.tag))?;
            let mut bundle = FluentBundle::new_concurrent(vec![lang]);
            bundle.set_use_isolating(false);
            bundle
                .add_resource(resource)
                .map_err(|e| format!("duplicate messages: {e:?}"))?;
            entries.push((*definition, bundle));
        }
        if !entries.iter().any(|(d, _)| d.tag == "ja") {
            return Err("missing default locale".into());
        }
        Ok(Self {
            entries,
            warned: Mutex::new(HashSet::new()),
        })
    }

    pub fn definitions(&self) -> impl Iterator<Item = Definition> + '_ {
        self.entries.iter().map(|(d, _)| *d)
    }

    fn exact(&self, tag: &str) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(d, _)| d.tag.eq_ignore_ascii_case(tag))
            .map(|(d, _)| d.tag)
    }

    #[must_use]
    pub fn locale<'a>(&'a self, tag: &str) -> Locale<'a> {
        let exact = self.exact(tag);
        let parent = tag.split_once('-').and_then(|(base, _)| self.exact(base));
        Locale {
            catalogs: self,
            tag: exact.or(parent).unwrap_or("ja"),
        }
    }

    #[must_use]
    pub fn negotiate<'a>(&'a self, cookie: Option<&str>, accept: Option<&str>) -> Locale<'a> {
        if let Some(tag) = cookie.and_then(|v| self.exact(v)) {
            return self.locale(tag);
        }
        let mut best = (0_u16, "ja");
        for part in accept.unwrap_or("").split(',') {
            let mut parts = part.trim().split(';');
            let tag = parts.next().unwrap_or("").trim();
            if tag == "*" || tag.parse::<LanguageIdentifier>().is_err() {
                continue;
            }
            let mut q = 1000;
            let mut valid = true;
            let mut seen_q = false;
            for parameter in parts {
                if seen_q {
                    valid = false;
                    break;
                }
                let Some(value) = parameter.trim().strip_prefix("q=") else {
                    valid = false;
                    break;
                };
                let Some(parsed) = quality(value) else {
                    valid = false;
                    break;
                };
                q = parsed;
                seen_q = true;
            }
            let matched = self
                .exact(tag)
                .or_else(|| tag.split_once('-').and_then(|(base, _)| self.exact(base)));
            if valid
                && q > best.0
                && let Some(matched) = matched
            {
                best = (q, matched);
            }
        }
        self.locale(best.1)
    }
}

fn quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match whole {
        "0" => Some(
            fraction.parse::<u16>().unwrap_or(0)
                * 10_u16.pow(u32::try_from(3 - fraction.len()).ok()?),
        ),
        "1" if fraction.bytes().all(|b| b == b'0') => Some(1000),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub struct Locale<'a> {
    catalogs: &'a Catalogs,
    tag: &'a str,
}

impl<'a> Locale<'a> {
    #[must_use]
    pub fn catalogs(self) -> &'a Catalogs {
        self.catalogs
    }
    #[must_use]
    pub fn tag(self) -> &'static str {
        self.catalogs.exact(self.tag).unwrap_or("ja")
    }
    #[must_use]
    pub fn dir(self) -> &'static str {
        self.catalogs
            .entries
            .iter()
            .find(|(d, _)| d.tag == self.tag)
            .map_or("ltr", |(d, _)| d.dir)
    }
    #[must_use]
    pub fn bot_language(self) -> &'static str {
        if self.tag == "ja" { "ja" } else { "en" }
    }
    #[must_use]
    pub fn text(self, key: &str) -> String {
        self.format(key, None)
    }
    #[must_use]
    pub fn number(self, key: &str, n: u64) -> String {
        let mut args = FluentArgs::new();
        args.set("n", n);
        self.format(key, Some(&args))
    }
    #[must_use]
    pub fn with_text(self, key: &str, name: &str, value: &str) -> String {
        let mut args = FluentArgs::new();
        args.set(name, value);
        self.format(key, Some(&args))
    }
    #[must_use]
    pub fn with_texts(self, key: &str, values: &[(&str, &str)]) -> String {
        let mut args = FluentArgs::new();
        for (name, value) in values {
            args.set(*name, *value);
        }
        self.format(key, Some(&args))
    }
    /// The text of a key, or `None` when no catalog has it: for the labels of the structured
    /// editor, whose keys follow the names in the bot's schema and may be missing for a new one.
    #[must_use]
    pub fn find(self, key: &str) -> Option<String> {
        let parent = self.tag.split_once('-').map_or(self.tag, |(tag, _)| tag);
        for tag in [self.tag, parent, "ja"] {
            if let Some((_, bundle)) = self.catalogs.entries.iter().find(|(d, _)| d.tag == tag)
                && let Some(pattern) = bundle.get_message(key).and_then(|m| m.value())
            {
                let mut errors = Vec::new();
                let result = bundle.format_pattern(pattern, None, &mut errors);
                if errors.is_empty() {
                    return Some(escape(&result));
                }
            }
        }
        None
    }
    fn format(self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        let parent = self.tag.split_once('-').map_or(self.tag, |(tag, _)| tag);
        for tag in [self.tag, parent, "ja"] {
            if let Some((_, bundle)) = self.catalogs.entries.iter().find(|(d, _)| d.tag == tag)
                && let Some(pattern) = bundle.get_message(key).and_then(|m| m.value())
            {
                let mut errors = Vec::new();
                let result = bundle.format_pattern(pattern, args, &mut errors);
                if errors.is_empty() {
                    return escape(&result);
                }
            }
            if let Ok(mut warned) = self.catalogs.warned.lock()
                && warned.insert((tag.to_owned(), key.to_owned()))
            {
                tracing::warn!(locale = tag, key, "missing or invalid translation");
            }
        }
        escape(key)
    }
    #[must_use]
    pub fn relative_time(self, timestamp: i64, now: u64) -> String {
        let elapsed = now.saturating_sub(u64::try_from(timestamp).unwrap_or(0));
        if elapsed < 60 {
            self.text(keys::TIME_NOW)
        } else if elapsed < 3600 {
            self.number(keys::TIME_MINUTE, elapsed / 60)
        } else if elapsed < 86_400 {
            self.number(keys::TIME_HOUR, elapsed / 3600)
        } else if elapsed < 172_800 {
            self.text(keys::TIME_YESTERDAY)
        } else {
            self.number(keys::TIME_DAY, elapsed / 86_400)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    const AR: &str = "title = عربي\nplural = { $n ->\n [zero] zero\n [one] one\n [two] two\n [few] few\n [many] many\n *[other] other\n }\nescape = Hello { $name }";
    const PT: &str = "title = Português";
    const PT_BR: &str = "other = Brasil";
    fn catalogs() -> Catalogs {
        Catalogs::new(&[
            REGISTRY[0],
            REGISTRY[1],
            Definition {
                tag: "ar",
                name: "العربية",
                dir: "rtl",
                source: AR,
            },
            Definition {
                tag: "pt",
                name: "Português",
                dir: "ltr",
                source: PT,
            },
            Definition {
                tag: "pt-BR",
                name: "Brasil",
                dir: "ltr",
                source: PT_BR,
            },
            Definition {
                tag: "fr",
                name: "Français",
                dir: "ltr",
                source: "title = Français",
            },
        ])
        .unwrap()
    }
    fn signatures(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut result = BTreeMap::<String, BTreeSet<String>>::new();
        let mut id = String::new();
        for line in source.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if !line.starts_with(char::is_whitespace)
                && let Some((name, _)) = line.split_once('=')
            {
                id = name.trim().to_owned();
                result.entry(id.clone()).or_default();
            }
            for rest in line.split('$').skip(1) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !name.is_empty() {
                    result.entry(id.clone()).or_default().insert(name);
                }
            }
        }
        result
    }
    #[test]
    fn catalog_files_keys_and_variables_match() {
        let catalog = Catalogs::new(REGISTRY).unwrap();
        let expected = signatures(REGISTRY[0].source);
        // The labels of the structured editor (`ed-...`) follow the names in the bot's schema, so
        // they are looked up by name and not declared as constants.
        assert_eq!(
            expected
                .keys()
                .map(String::as_str)
                .filter(|key| !key.starts_with("ed-"))
                .collect::<BTreeSet<_>>(),
            keys::ALL.iter().copied().collect()
        );
        let files: BTreeSet<_> = std::fs::read_dir("locales")
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|s| {
                std::path::Path::new(s)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ftl"))
            })
            .collect();
        assert_eq!(
            files,
            catalog
                .definitions()
                .map(|d| format!("{}.ftl", d.tag))
                .collect()
        );
        for definition in REGISTRY {
            assert_eq!(
                signatures(definition.source),
                expected,
                "{}",
                definition.tag
            );
        }
    }
    #[test]
    fn digits_are_grouped_by_thousands() {
        for (n, expected) in [
            (0, "0"),
            (999, "999"),
            (1000, "1,000"),
            (1284, "1,284"),
            (1_234_567, "1,234,567"),
        ] {
            assert_eq!(group_digits(n), expected);
        }
    }
    #[test]
    fn removed_display_keys_stay_removed() {
        for key in [
            "average",
            "retention",
            "newest",
            "history-note",
            "read-only",
            "emoji-default",
            "bot",
            "connected",
            "active",
            "last",
            "recent",
            "status",
        ] {
            assert!(!keys::ALL.contains(&key), "{key}");
            for definition in REGISTRY {
                assert!(
                    !definition
                        .source
                        .lines()
                        .any(|line| line.starts_with(&format!("{key} ="))),
                    "{key}"
                );
            }
        }
    }
    #[test]
    fn fallback_and_bot_mapping() {
        let catalogs = catalogs();
        assert_eq!(catalogs.locale("pt-BR").text(keys::TITLE), "Português");
        assert_eq!(
            catalogs.locale("pt").text(keys::LOGIN),
            "Discord でログイン"
        );
        assert_eq!(
            catalogs.locale("unknown").text(keys::TITLE),
            "ダッシュボード"
        );
        assert_eq!(catalogs.locale("ar").text("missing"), "missing");
        assert_eq!(catalogs.locale("ja").bot_language(), "ja");
        assert_eq!(catalogs.locale("en").bot_language(), "en");
        assert_eq!(catalogs.locale("fr").bot_language(), "en");
        assert_eq!(catalogs.locale("ar").dir(), "rtl");
        assert_eq!(
            catalogs
                .locale("ar")
                .with_text("escape", "name", "<script>"),
            "Hello &lt;script&gt;"
        );
    }
    #[test]
    fn negotiation() {
        let catalogs = catalogs();
        for (cookie, accept, expected) in [
            (Some("ja"), "en", "ja"),
            (Some("bogus"), "en", "en"),
            (None, "ja;q=0.2,en;q=0.9", "en"),
            (None, "en-GB,ja", "en"),
            (None, "*,en;q=bogus,ja;q=2", "ja"),
            (None, "en;q=0.5;q=1,ja;q=0.1", "ja"),
            (None, "ja;q=0.5,en;q=0.5", "ja"),
        ] {
            assert_eq!(catalogs.negotiate(cookie, Some(accept)).tag(), expected);
        }
    }
    #[test]
    fn plural_and_relative_time() {
        let catalogs = catalogs();
        for (n, expected) in [
            (0, "zero"),
            (1, "one"),
            (2, "two"),
            (3, "few"),
            (11, "many"),
            (100, "other"),
        ] {
            assert_eq!(catalogs.locale("ar").number("plural", n), expected);
        }
        for (elapsed, ja, en) in [
            (0, "たった今", "just now"),
            (60, "1 分前", "1 minute ago"),
            (3540, "59 分前", "59 minutes ago"),
            (3600, "1 時間前", "1 hour ago"),
            (7200, "2 時間前", "2 hours ago"),
            (86400, "昨日", "yesterday"),
            (259_200, "3 日前", "3 days ago"),
            (2_592_000, "30 日前", "30 days ago"),
        ] {
            assert_eq!(
                catalogs
                    .locale("ja")
                    .relative_time(3_000_000 - elapsed, 3_000_000),
                ja
            );
            assert_eq!(
                catalogs
                    .locale("en")
                    .relative_time(3_000_000 - elapsed, 3_000_000),
                en
            );
        }
        assert_eq!(
            catalogs.locale("en").relative_time(4_000_000, 3_000_000),
            "just now"
        );
    }
}
