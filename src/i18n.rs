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
        HISTORY = "history", OLDER = "older", PERMISSIONS = "permissions", PERIOD = "period",
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
        SETTINGS = "settings", NO_SETTINGS = "no-settings", INHERITED = "inherited", INHERITED_LIST = "inherited-list", INHERITED_TEXT_LIST = "inherited-text-list", SAVE = "save",
        DISCARD = "discard", SAVED = "saved", UNSET = "unset", FIELD_INVALID = "field-invalid", TEXT_LIST_ITEM = "form-text-list-item",
        RELOAD = "reload", CONFLICT_RELOAD = "conflict-reload",
        SERVER_SETTINGS = "server-settings", UNKNOWN_ROLE = "unknown-role",
        SETTINGS_LANGUAGE = "settings-language",
        SETTINGS_TIMEZONE = "settings-timezone",
        SETTINGS_PREFIX = "settings-prefix", SETTINGS_LOG_CHANNEL = "settings-log-channel",
        SETTINGS_MODERATOR_ROLES = "settings-moderator-roles",
        LANG_AUTO = "lang-auto", LANG_JA = "lang-ja", LANG_EN = "lang-en",
        CAT_MODERATION = "cat-moderation", CAT_COMMUNITY = "cat-community", CAT_SYSTEM = "cat-system", CAT_OTHER = "cat-other", SEC_WARN = "sec-warn", SEC_TIMEOUT = "sec-timeout", SEC_VOICE = "sec-voice", SEC_BAN = "sec-ban", SEC_BEHAVIOR = "sec-behavior", SEC_DESTINATION = "sec-destination", SEC_EVENTS = "sec-events", SEC_AUTO_DELETE = "sec-auto-delete", SEC_AUTO_REACT = "sec-auto-react", COL_CHANNEL = "col-channel", COL_ROLE = "col-role", COL_LEVEL = "col-level", COL_PERCENT = "col-percent", COL_AFTER = "col-after", COL_SKIP_PINNED = "col-skip-pinned", COL_EMOJIS = "col-emojis", COL_SKIP_BOTS = "col-skip-bots", COL_REMOVE = "col-remove", SEC_ESCALATION = "sec-escalation", COL_WARNS = "col-warns", COL_ACTION = "col-action", COL_DURATION = "col-duration", OPT_TIMEOUT = "opt-timeout", OPT_KICK = "opt-kick", OPT_BAN = "opt-ban", SEC_TEMPLATES = "sec-templates", COL_ID = "col-id", COL_PANEL_ID = "col-panel-id",
        COL_PANEL_NAME = "col-panel-name",
        COL_PANEL_DESCRIPTION = "col-panel-description",
        COL_PANEL_BUTTON_STYLE = "col-panel-button-style",
        COL_LABEL = "col-label", COL_REASON = "col-reason", COL_CATEGORY = "col-category", COL_STAFF_ROLES = "col-staff-roles", COL_STAFF_ROLES_PICKER = "col-staff-roles-picker", COL_STAFF_ROLES_DEFAULT = "col-staff-roles-default", COL_STAFF_ROLES_SET = "col-staff-roles-set", COL_STAFF_ROLES_SELECT = "col-staff-roles-select", COL_STAFF_ROLES_MODE = "col-staff-roles-mode", COL_LOG_CHANNEL = "col-log-channel", COL_CLOSE_AFTER_DAYS = "col-close-after-days", COL_QUESTIONS = "col-questions", SEC_RULES = "sec-rules", SEC_COUNTERS = "sec-counters", SEC_ANTIRAID = "sec-antiraid", SEC_MESSAGES = "sec-messages", SEC_TEXTS = "sec-texts", COL_MESSAGE_ID = "col-message-id", COL_RULE_NAME = "col-rule-name", COL_COUNTER_NAME = "col-counter-name", COL_NAME = "col-name", COL_TEXT = "col-text", COL_JSON = "col-json", SEC_ROLES = "sec-roles", COMPOSE = "compose", CP_NOW = "composer-now", CP_EMPTY = "composer-empty", CP_COUNT = "composer-count", CP_COLOR = "composer-color", CP_NO_COLOR = "composer-no-color", CP_CUSTOM_COLOR = "composer-custom-color", CP_EMBED_N = "composer-embed-n", CP_AUTHOR = "composer-author", CP_NAME = "composer-name", CP_LINK = "composer-link", CP_ICON = "composer-icon", CP_TITLE = "composer-title", CP_TITLE_LINK = "composer-title-link", CP_DESCRIPTION = "composer-description", CP_FIELDS = "composer-fields", CP_FIELD_NAME = "composer-field-name", CP_FIELD_VALUE = "composer-field-value", CP_INLINE = "composer-inline", CP_ADD_FIELD = "composer-add-field", CP_IMAGES = "composer-images", CP_THUMBNAIL = "composer-thumbnail", CP_IMAGE = "composer-image", CP_FOOTER = "composer-footer", CP_TEXT = "composer-text", CP_TIMESTAMP = "composer-timestamp", CP_CHANNEL = "composer-channel", CP_VARIABLES = "composer-variables", CP_CONTENT = "composer-content", CP_ADD_EMBED = "composer-add-embed", CP_SEND = "composer-send", CP_PREVIEW = "composer-preview", CP_SENT = "composer-sent", CP_OPEN = "composer-open", CP_CHOOSE_CHANNEL = "composer-choose-channel", CP_NOTHING = "composer-nothing", CP_EDIT_MESSAGE = "composer-edit-message", CP_DEFAULT_MESSAGE = "composer-default-message", CP_MENTION_ROLE = "composer-mention-role", CP_MENTION_USER = "composer-mention-user", CP_MENTION_CHANNEL = "composer-mention-channel",
        CP_EDIT_TARGET = "composer-edit-target", CP_EDIT_HINT = "composer-edit-hint", CP_WHEN = "composer-when", CP_WHEN_HINT = "composer-when-hint", CP_BUTTONS = "composer-buttons", CP_BUTTONS_HINT = "composer-buttons-hint", CP_BUTTON_ROLE = "composer-button-role", CP_BUTTON_LABEL = "composer-button-label", CP_REMOVE = "composer-remove", CP_ADD_BUTTON = "composer-add-button", CP_SCHEDULE = "composer-schedule", CP_SCHEDULED = "composer-scheduled", CP_NONE_WAITING = "composer-none-waiting", CP_UNSCHEDULE = "composer-unschedule", CP_QUEUED = "composer-queued", CP_EDITED = "composer-edited", CP_UNSCHEDULED = "composer-unscheduled", CP_BAD_EDIT = "composer-bad-edit", CP_BAD_TIME = "composer-bad-time", ED_ERR_REQUIRED = "editor-err-required", ED_ERR_INVALID = "editor-err-invalid", ED_ERR_REPEATED = "editor-err-repeated", ED_DEFAULT = "editor-default", ED_ON = "editor-on", ED_OFF = "editor-off", ED_DURATION_EXAMPLE = "editor-duration-example", ED_REVERSE_CONDITION = "editor-reverse-condition", ED_ADD = "editor-add", ED_ADD_CONDITION = "editor-add-condition", ED_ADD_ACTION = "editor-add-action", ED_ADD_RULE = "editor-add-rule", ED_TPL_HEADING = "tpl-heading", ED_TPL_HINT = "tpl-hint", ED_TPL_ADD = "tpl-add", ED_ADD_COUNTER = "editor-add-counter", ED_REMOVE = "editor-remove", ED_EDIT = "editor-edit", ED_IF = "editor-if", ED_IF_HINT = "editor-if-hint", ED_THEN = "editor-then", ED_THEN_HINT = "editor-then-hint", ED_BASIC = "editor-basic", ED_SCOPE = "editor-scope", ED_MORE = "editor-more", ED_NO_RULES = "editor-no-rules", ED_NO_COUNTERS = "editor-no-counters", ED_NO_TRIGGERS = "editor-no-triggers", ED_NO_ACTIONS = "editor-no-actions", ED_BACK = "editor-back", ED_ANTIRAID_ON = "editor-antiraid-on", ED_UNSUPPORTED = "editor-unsupported", ED_NEW_RULE = "editor-new-rule", ED_NEW_COUNTER = "editor-new-counter", ED_NAME_KEY = "editor-name-key", MOD_MODERATION = "mod-moderation", MOD_AUTOMOD = "mod-automod", MOD_LOGS = "mod-logs", MOD_WELCOME = "mod-welcome", MOD_CHANNELS = "mod-channels", MOD_TAGS = "mod-tags", MOD_REMIND = "mod-remind", MOD_DIAGNOSTICS = "mod-diagnostics", MOD_DASHBOARD = "mod-dashboard",
        MOD_ROLES = "mod-roles", MOD_PERSIST = "mod-persist", MOD_UTILITY = "mod-utility", MOD_COMPANION = "mod-companion",
        MOD_LINKEXPAND = "mod-linkexpand", MOD_STARBOARD = "mod-starboard", MOD_LEVELING = "mod-leveling", MOD_TRANSLATE = "mod-translate", MOD_POST = "mod-post", MOD_GIVEAWAY = "mod-giveaway", CP_SENT_ALREADY = "cp-sent-already", MOD_TICKETS = "mod-tickets",
        OPT_DISCORD = "opt-discord", OPT_ROLE = "opt-role", OPT_PRIVATE = "opt-private", OPT_PUBLIC = "opt-public",
        HISTORY_ENABLED = "history-enabled", HISTORY_DISABLED = "history-disabled",
        HISTORY_CONFIG = "history-config", HISTORY_ADD = "history-add", HISTORY_RESET = "history-reset",
        HISTORY_LANGUAGE = "history-language", HISTORY_TIMEZONE = "history-timezone", HISTORY_PREFIX = "history-prefix",
        HISTORY_LOG_CHANNEL = "history-log-channel", HISTORY_ROLES = "history-roles", HISTORY_PRESET = "history-preset",
        HISTORY_WEBHOOK = "history-webhook", HISTORY_EMOJI = "history-emoji", HISTORY_PERMISSIONS = "history-permissions",
        TICKETS = "tickets",
        TICKETS_NONE = "tickets-none",
        TICKET_TITLE = "ticket-title",
        TICKET_STATE = "ticket-state",
        TICKET_OPEN = "ticket-open",
        TICKET_CLOSING = "ticket-closing",
        TICKET_CLOSED = "ticket-closed",
        TICKET_OPENER = "ticket-opener",
        TICKET_CLAIMED_BY = "ticket-claimed-by",
        TICKET_PANEL = "ticket-panel",
        TICKET_CHANNEL = "ticket-channel",
        TICKET_CREATED = "ticket-created",
        TICKET_CLOSED_AT = "ticket-closed-at",
        TICKET_CLOSER = "ticket-closer",
        TICKET_REASON = "ticket-close-reason",
        TICKET_TRANSCRIPT = "ticket-transcript",
        TICKET_TRANSCRIPT_TRUNCATED = "ticket-transcript-truncated",
        TICKET_TRANSCRIPT_PREVIOUS = "ticket-transcript-previous",
        TICKET_TRANSCRIPT_NEXT = "ticket-transcript-next",
        TICKET_TRANSCRIPT_SHOW_IMAGES = "ticket-transcript-show-images",
        TICKET_BACK = "ticket-back",
        TICKET_MORE = "ticket-more",
        TICKET_ACTIONS_TITLE = "ticket-actions-title",
        TICKET_ACTIONS_UPDATED = "ticket-actions-updated",
        TICKET_ACTIONS_UNAVAILABLE = "ticket-actions-unavailable",
        TICKET_ACTIONS_WAITING_FOR_REPLY = "ticket-actions-waiting-for-reply",
        TICKET_ACTIONS_CLOSE_REQUEST = "ticket-actions-close-request",
        TICKET_ACTIONS_LABELS = "ticket-actions-labels",
        TICKET_ACTIONS_REASON = "ticket-actions-reason",
        TICKET_ACTIONS_CLOSE = "ticket-actions-close",
        TICKET_ACTIONS_REQUEST_CLOSE = "ticket-actions-request-close",
        TICKET_ACTIONS_MESSAGE = "ticket-actions-message",
        TICKET_ACTIONS_REPLY = "ticket-actions-reply",
        TICKET_ACTIONS_LABEL = "ticket-actions-label",
        TICKET_ACTIONS_ADD_LABEL = "ticket-actions-add-label",
        TICKET_ACTIONS_REMOVE_LABEL = "ticket-actions-remove-label",
        TICKET_SETTINGS_BLACKLIST = "ticket-settings-blacklist-open",
        TICKET_SETTINGS_BLACKLIST_TITLE = "ticket-settings-blacklist",
        TICKET_SETTINGS_STAFF_TEAMS = "ticket-settings-staff-teams",
        TICKET_SETTINGS_MODULE = "ticket-settings-module",
        TICKET_SETTINGS_MODULE_HELP = "ticket-settings-module-help",
        TICKET_SETTINGS_MODULE_OPEN = "ticket-settings-module-open",
        TICKET_SETTINGS_KNOWLEDGE_HELP = "ticket-settings-knowledge-help",
        TICKET_SETTINGS_KNOWLEDGE_OPEN = "ticket-settings-knowledge-open",
        TICKET_BLACKLIST_GUILD = "ticket-blacklist-guild",
        TICKET_BLACKLIST_SCOPE_HELP = "ticket-blacklist-scope-help",
        TICKET_BLACKLIST_USER = "ticket-blacklist-user",
        TICKET_BLACKLIST_ROLE = "ticket-blacklist-role",
        TICKET_BLACKLIST_SAVED = "ticket-blacklist-saved",
        TICKET_BLACKLIST_REMOVE = "ticket-blacklist-remove",
        TICKET_BLACKLIST_ADD_ROLE = "ticket-blacklist-add-role",
        TICKET_BLACKLIST_USER_SEARCH = "ticket-blacklist-user-search",
        TICKET_BLACKLIST_ADD_USER = "ticket-blacklist-add-user",
        TICKET_MEMBERS_REFINE_SEARCH = "ticket-members-refine-search",
        TICKET_TEAM_EXISTING = "ticket-team-existing",
        TICKET_TEAM_NEW = "ticket-team-new",
        TICKET_TEAM_NAME = "ticket-team-name",
        TICKET_TEAM_ROLES = "ticket-team-roles",
        TICKET_TEAM_ROLES_UNAVAILABLE = "ticket-team-roles-unavailable",
        TICKET_TEAM_PERMISSIONS = "ticket-team-permissions",
        TICKET_TEAM_REMOVE = "ticket-team-remove",
        TICKET_TEAM_DEFAULT = "ticket-team-default",
        TICKET_TEAM_UNKNOWN = "ticket-team-unknown",
        TICKET_TEAM_PICKER_UNAVAILABLE = "ticket-team-picker-unavailable",
        TICKET_TEAM_USERS = "ticket-team-users",
        TICKET_TEAM_USER_SEARCH = "ticket-team-user-search",
        TICKET_TEAM_SEARCH = "ticket-team-search",
        TICKET_TEAM_USERS_NONE = "ticket-team-users-none",
        TICKET_TEAM_SEARCH_SAVE_HELP = "ticket-team-search-save-help",
        TICKET_TEAM_PERMISSION_ADD_REACTIONS = "ticket-team-permission-add_reactions",
        TICKET_TEAM_PERMISSION_SEND_MESSAGES = "ticket-team-permission-send_messages",
        TICKET_TEAM_PERMISSION_SEND_TTS_MESSAGES = "ticket-team-permission-send_tts_messages",
        TICKET_TEAM_PERMISSION_EMBED_LINKS = "ticket-team-permission-embed_links",
        TICKET_TEAM_PERMISSION_ATTACH_FILES = "ticket-team-permission-attach_files",
        TICKET_TEAM_PERMISSION_MENTION_EVERYONE = "ticket-team-permission-mention_everyone",
        TICKET_TEAM_PERMISSION_USE_EXTERNAL_EMOJIS = "ticket-team-permission-use_external_emojis",
        TICKET_TEAM_PERMISSION_USE_APPLICATION_COMMANDS = "ticket-team-permission-use_application_commands",
        TICKET_TEAM_PERMISSION_USE_EXTERNAL_STICKERS = "ticket-team-permission-use_external_stickers",
        TICKET_TEAM_PERMISSION_SEND_VOICE_MESSAGES = "ticket-team-permission-send_voice_messages",
        TICKETS_STAFF_TEAMS = "tickets-staff-teams",
        TICKETS_EXIT_SURVEY = "tickets-exit-survey",
        TICKETS_WAITING_AFTER_HOURS = "tickets-waiting-after-hours",
        TICKETS_FIRST_RESPONSE_TIMEOUT_HOURS = "tickets-first-response-timeout-hours",
        TICKETS_LAST_MESSAGE_TIMEOUT_HOURS = "tickets-last-message-timeout-hours",
        TICKETS_WAITING_CATEGORY = "tickets-waiting-category",
        HOURS_PANEL = "hours-panel",
        HOURS_PANEL_DEFAULT = "hours-panel-default",
        HOURS_PANEL_UNKNOWN = "hours-panel-unknown",
        HOURS_WEEKDAY = "hours-weekday",
        HOURS_WEEKDAY_MONDAY = "hours-weekday-monday",
        HOURS_WEEKDAY_TUESDAY = "hours-weekday-tuesday",
        HOURS_WEEKDAY_WEDNESDAY = "hours-weekday-wednesday",
        HOURS_WEEKDAY_THURSDAY = "hours-weekday-thursday",
        HOURS_WEEKDAY_FRIDAY = "hours-weekday-friday",
        HOURS_WEEKDAY_SATURDAY = "hours-weekday-saturday",
        HOURS_WEEKDAY_SUNDAY = "hours-weekday-sunday",
        HOURS_START = "hours-start",
        HOURS_END = "hours-end",
        HOURS_END_OF_DAY = "hours-end-of-day",
        HOURS_TIMEZONE_OVERRIDE = "hours-timezone-override",
        HOURS_TIMEZONE_PLACEHOLDER = "hours-timezone-placeholder",
        HOURS_TIMEZONE_INHERIT = "hours-timezone-inherit",
        HOURS_PANEL_REFERENCE_INVALID = "hours-panel-reference-invalid",
        HOURS_INVALID_TIMEZONE = "hours-invalid-timezone",
        HOURS_INVALID_INTERVAL = "hours-invalid-interval",
        TICKETS_FORM_ID = "tickets-form-id",
        TICKETS_FORM_ID_HELP = "tickets-form-id-help",
        TICKETS_FORM_INLINE = "tickets-form-inline",
        TICKETS_FORM_UNKNOWN = "tickets-form-unknown",
        FORM_QUESTION_TYPE = "form-question-type",
        FORM_QUESTION_TYPE_LEGACY = "form-question-type-legacy",
        FORM_QUESTION_TYPE_TEXT = "form-question-type-text",
        FORM_QUESTION_TYPE_STRING_SELECT = "form-question-type-string-select",
        FORM_QUESTION_TYPE_USER_SELECT = "form-question-type-user-select",
        FORM_QUESTION_TYPE_ROLE_SELECT = "form-question-type-role-select",
        FORM_QUESTION_TYPE_MENTIONABLE_SELECT = "form-question-type-mentionable-select",
        FORM_QUESTION_TYPE_CHANNEL_SELECT = "form-question-type-channel-select",
        FORM_QUESTION_TYPE_RADIO_GROUP = "form-question-type-radio-group",
        FORM_QUESTION_TYPE_CHECKBOX_GROUP = "form-question-type-checkbox-group",
        FORM_QUESTION_TYPE_DISPLAY = "form-question-type-display",
        FORM_QUESTION_LABEL = "form-question-label",
        FORM_QUESTION_DESCRIPTION = "form-question-description",
        FORM_QUESTION_REQUIRED = "form-question-required",
        FORM_QUESTION_STYLE = "form-question-style",
        FORM_QUESTION_STYLE_SHORT = "form-question-style-short",
        FORM_QUESTION_STYLE_PARAGRAPH = "form-question-style-paragraph",
        FORM_QUESTION_MIN_LENGTH = "form-question-min-length",
        FORM_QUESTION_MAX_LENGTH = "form-question-max-length",
        FORM_QUESTION_PLACEHOLDER = "form-question-placeholder",
        FORM_QUESTION_CONTENT = "form-question-content",
        FORM_QUESTION_OPTIONS = "form-question-options",
        FORM_QUESTION_OPTION_LABEL = "form-question-option-label",
        FORM_QUESTION_OPTION_VALUE = "form-question-option-value",
        FORM_QUESTION_OPTION_DESCRIPTION = "form-question-option-description",
        FORM_QUESTION_MIN_VALUES = "form-question-min-values",
        FORM_QUESTION_MAX_VALUES = "form-question-max-values",
        FORM_QUESTION_KIND_HINT = "form-question-kind-hint",
        FORM_QUESTION_MORE_SETTINGS = "form-question-more-settings",
        CONFIG_TICKETS_INTEGRATIONS = "config-tickets-integrations",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS = "config-tickets-form-dynamic-options",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS_INTEGRATION = "config-tickets-form-dynamic-options-integration",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS_ITEMS_POINTER = "config-tickets-form-dynamic-options-items_pointer",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS_LABEL_POINTER = "config-tickets-form-dynamic-options-label_pointer",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS_VALUE_POINTER = "config-tickets-form-dynamic-options-value_pointer",
        CONFIG_TICKETS_FORM_DYNAMIC_OPTIONS_DESCRIPTION_POINTER = "config-tickets-form-dynamic-options-description_pointer",
        CONFIG_TICKETS_INTEGRATION_ID = "config-tickets-integration-id",
        CONFIG_TICKETS_INTEGRATION_URL = "config-tickets-integration-url",
        CONFIG_TICKETS_INTEGRATION_AUTH = "config-tickets-integration-auth",
        CONFIG_TICKETS_INTEGRATION_SECRET_HELP = "config-tickets-integration-secret-help",
        TICKET_KNOWLEDGE_TITLE = "ticket-knowledge-title",
        TICKET_KNOWLEDGE_BACK = "ticket-knowledge-back",
        TICKET_KNOWLEDGE_SEARCH = "ticket-knowledge-search",
        TICKET_KNOWLEDGE_SEARCH_SUBMIT = "ticket-knowledge-search-submit",
        TICKET_KNOWLEDGE_CREATE = "ticket-knowledge-create",
        TICKET_KNOWLEDGE_ID = "ticket-knowledge-id",
        TICKET_KNOWLEDGE_ARTICLE_TITLE = "ticket-knowledge-article-title",
        TICKET_KNOWLEDGE_BODY = "ticket-knowledge-body",
        TICKET_KNOWLEDGE_CATEGORIES = "ticket-knowledge-categories",
        TICKET_KNOWLEDGE_KEYWORDS = "ticket-knowledge-keywords",
        TICKET_KNOWLEDGE_PUBLISHED = "ticket-knowledge-published",
        TICKET_KNOWLEDGE_CUSTOM_EMBED = "ticket-knowledge-custom-embed",
        TICKET_KNOWLEDGE_PUBLIC_LINK = "ticket-knowledge-public-link",
        TICKET_KNOWLEDGE_SAVE = "ticket-knowledge-save",
        TICKET_KNOWLEDGE_DELETE = "ticket-knowledge-delete",
        TICKET_KNOWLEDGE_NONE = "ticket-knowledge-none",
        TICKET_KNOWLEDGE_SAVED = "ticket-knowledge-saved",
        TICKET_KNOWLEDGE_PREVIOUS = "ticket-knowledge-previous",
        TICKET_KNOWLEDGE_NEXT = "ticket-knowledge-next",
        TICKET_KNOWLEDGE_UNAVAILABLE = "ticket-knowledge-unavailable",
        TICKET_KNOWLEDGE_CATEGORIES_HELP = "ticket-settings-knowledge-categories-help",
        TICKET_KNOWLEDGE_KEYWORDS_HELP = "ticket-settings-knowledge-keywords-help",
        TICKET_KNOWLEDGE_PUBLISHED_HELP = "ticket-settings-knowledge-published-help",
        TICKET_KNOWLEDGE_CUSTOM_EMBED_HELP = "ticket-settings-knowledge-custom-embed-help",
        TICKET_TAGS_TITLE = "ticket-tags-title",
        TICKET_TAGS_BACK = "ticket-tags-back",
        TICKET_TAGS_SEARCH = "ticket-tags-search",
        TICKET_TAGS_SEARCH_SUBMIT = "ticket-tags-search-submit",
        TICKET_TAGS_ARTICLE_SEARCH = "ticket-tags-article-search",
        TICKET_TAGS_ARTICLE_NEXT = "ticket-tags-article-next",
        TICKET_TAGS_ARTICLE_FIRST = "ticket-tags-article-first",
        TICKET_TAGS_ARTICLE_UNAVAILABLE = "ticket-tags-article-unavailable",
        TICKET_TAGS_CREATE = "ticket-tags-create",
        TICKET_TAGS_ID = "ticket-tags-id",
        TICKET_TAGS_BODY = "ticket-tags-body",
        TICKET_TAGS_EMBED = "ticket-tags-embed",
        TICKET_TAGS_EMBED_HELP = "ticket-tags-embed-help",
        TICKET_TAGS_ALIAS = "ticket-tags-alias",
        TICKET_TAGS_ALIAS_HELP = "ticket-tags-alias-help",
        TICKET_TAGS_ALIAS_SYNC_PENDING = "ticket-tags-alias-sync-pending",
        TICKET_TAGS_ARTICLE = "ticket-tags-article",
        TICKET_TAGS_ARTICLE_NONE = "ticket-tags-article-none",
        TICKET_TAGS_SAVE = "ticket-tags-save",
        TICKET_TAGS_EDIT = "ticket-tags-edit",
        TICKET_TAGS_CLONE = "ticket-tags-clone",
        TICKET_TAGS_CLONE_ID = "ticket-tags-clone-id",
        TICKET_TAGS_DELETE = "ticket-tags-delete",
        TICKET_TAGS_DELETE_CONFIRM = "ticket-tags-delete-confirm",
        TICKET_TAGS_EMPTY = "ticket-tags-empty",
        TICKET_TAGS_SAVED = "ticket-tags-saved",
        TICKET_TAGS_TYPE_TEXT = "ticket-tags-type-text",
        TICKET_TAGS_TYPE_EMBED = "ticket-tags-type-embed",
        TICKET_TAGS_TYPE_EMBED_TEXT = "ticket-tags-type-embed-text",
        TICKET_TAGS_UNAVAILABLE = "ticket-tags-unavailable",
        TICKET_TAGS_NEXT = "ticket-tags-next",
        TICKET_TAGS_PUBLISH = "ticket-tags-publish",
        TICKET_AUDIT_LOG = "ticket-audit-log",
        TICKET_AUDIT_USER_ID = "ticket-audit-user-id",
        TICKET_AUDIT_ACTION_TYPE = "ticket-audit-action-type",
        TICKET_AUDIT_RESOURCE_TYPE = "ticket-audit-resource-type",
        TICKET_AUDIT_RESULT = "ticket-audit-result",
        TICKET_AUDIT_DATE_FROM = "ticket-audit-date-from",
        TICKET_AUDIT_DATE_TO = "ticket-audit-date-to",
        TICKET_AUDIT_APPLY_FILTERS = "ticket-audit-apply-filters",
        TICKET_AUDIT_ALL_ACTIONS = "ticket-audit-all-actions",
        TICKET_AUDIT_ALL_RESOURCES = "ticket-audit-all-resources",
        TICKET_AUDIT_ALL_RESULTS = "ticket-audit-all-results",
        TICKET_AUDIT_COUNT = "ticket-audit-count",
        TICKET_AUDIT_COUNT_TRUNCATED = "ticket-audit-count-truncated",
        TICKET_AUDIT_ENTRIES = "ticket-audit-entries",
        TICKET_AUDIT_TIMESTAMP = "ticket-audit-timestamp",
        TICKET_AUDIT_ACTION = "ticket-audit-action",
        TICKET_AUDIT_RESOURCE = "ticket-audit-resource",
        TICKET_AUDIT_OUTCOME = "ticket-audit-outcome",
        TICKET_AUDIT_EMPTY = "ticket-audit-empty",
        TICKET_AUDIT_PRIVACY = "ticket-audit-privacy",
        TICKET_AUDIT_PAGES_LABEL = "ticket-audit-pages-label",
        TICKET_AUDIT_PREVIOUS = "ticket-audit-previous",
        TICKET_AUDIT_NEXT = "ticket-audit-next",
        TICKET_AUDIT_PAGE = "ticket-audit-page",
        TICKET_AUDIT_SYSTEM = "ticket-audit-system",
        TICKET_AUDIT_INCOMPLETE = "ticket-audit-incomplete",
        TICKET_AUDIT_ACTION_LABEL = "ticket-audit-action-label",
        TICKET_AUDIT_RESOURCE_LABEL = "ticket-audit-resource-label",
        TICKET_AUDIT_OUTCOME_LABEL = "ticket-audit-outcome-label",
        TICKET_TRANSCRIPTS = "ticket-transcripts",
        TICKET_TRANSCRIPTS_FILTERS = "ticket-transcripts-filters",
        TICKET_TRANSCRIPTS_TOTAL = "ticket-transcripts-total",
        TICKET_TRANSCRIPTS_TICKET_ID = "ticket-transcripts-ticket-id",
        TICKET_TRANSCRIPTS_USERNAME = "ticket-transcripts-username",
        TICKET_TRANSCRIPTS_USER_ID = "ticket-transcripts-user-id",
        TICKET_TRANSCRIPTS_CLOSED_BY = "ticket-transcripts-closed-by",
        TICKET_TRANSCRIPTS_PANEL = "ticket-transcripts-panel",
        TICKET_TRANSCRIPTS_RATING = "ticket-transcripts-rating",
        TICKET_TRANSCRIPTS_CLAIMED_BY = "ticket-transcripts-claimed-by",
        TICKET_TRANSCRIPTS_CLOSE_REASON = "ticket-transcripts-close-reason",
        TICKET_TRANSCRIPTS_LABELS = "ticket-transcripts-labels",
        TICKET_TRANSCRIPTS_VIEW = "ticket-transcripts-view",
        TICKET_TRANSCRIPTS_EDIT_REASON = "ticket-transcripts-edit-reason",
        TICKET_TRANSCRIPTS_NO_REASON = "ticket-transcripts-no-reason",
        TICKET_TRANSCRIPTS_NO_RATING = "ticket-transcripts-no-rating",
        TICKET_TRANSCRIPTS_NO_TRANSCRIPT = "ticket-transcripts-no-transcript",
        TICKET_TRANSCRIPTS_MANAGE_LABELS = "ticket-transcripts-manage-labels",
        TICKET_TRANSCRIPTS_LABEL_NAME = "ticket-transcripts-label-name",
        TICKET_TRANSCRIPTS_LABEL_COLOR = "ticket-transcripts-label-color",
        TICKET_TRANSCRIPTS_CREATE_LABEL = "ticket-transcripts-create-label",
        TICKET_TRANSCRIPTS_DELETE_LABEL = "ticket-transcripts-delete-label",
        TICKET_TRANSCRIPTS_SAVE = "ticket-transcripts-save",
        TICKET_TRANSCRIPTS_CANCEL = "ticket-transcripts-cancel",
        TICKET_TRANSCRIPTS_SYNCING = "ticket-transcripts-syncing",
        TICKET_TRANSCRIPTS_BACKFILL_INCOMPLETE = "ticket-transcripts-backfill-incomplete",
        TICKET_TRANSCRIPTS_FILTER = "ticket-transcripts-filter",
        TICKET_TRANSCRIPTS_NEXT = "ticket-transcripts-next",
        TICKET_TRANSCRIPTS_TOTAL_PARTIAL = "ticket-transcripts-total-partial",
        TICKET_TRANSCRIPTS_USERNAME_LIMIT = "ticket-transcripts-username-limit",
        TICKET_TRANSCRIPTS_UPDATED = "ticket-transcripts-updated",
        TICKET_TRANSCRIPTS_ACTIONS = "ticket-transcripts-actions",
        TICKET_TRANSCRIPTS_ADD_LABEL = "ticket-transcripts-add-label",
        TICKET_TRANSCRIPTS_REMOVE_LABEL = "ticket-transcripts-remove-label",
        TICKET_TRANSCRIPTS_MORE_LABELS = "ticket-transcripts-more-labels",
        TICKET_TRANSCRIPTS_NO_RESULTS = "ticket-transcripts-no-results",
        TICKET_TRANSCRIPTS_PREVIOUS = "ticket-transcripts-previous",
        TICKET_TRANSCRIPTS_ANY = "ticket-transcripts-any",
        TICKET_TRANSCRIPTS_NO_LABELS = "ticket-transcripts-no-labels",
        TICKET_TRANSCRIPTS_FIRST = "ticket-transcripts-first",
        TICKET_TRANSCRIPTS_COLUMNS = "ticket-transcripts-columns",
        TICKET_GALLERY_TITLE = "ticket-gallery-title",
        TICKET_GALLERY_SEARCH = "ticket-gallery-search",
        TICKET_GALLERY_SOURCE_SEARCH = "ticket-gallery-source-search",
        TICKET_GALLERY_SOURCE_SEARCH_SUBMIT = "ticket-gallery-source-search-submit",
        TICKET_GALLERY_SOURCE_NOTE = "ticket-gallery-source-note",
        TICKET_GALLERY_SOURCE_MORE = "ticket-gallery-source-more",
        TICKET_GALLERY_TYPE = "ticket-gallery-type",
        TICKET_GALLERY_TYPE_ALL = "ticket-gallery-type-all",
        TICKET_GALLERY_TYPE_PANEL = "ticket-gallery-type-panel",
        TICKET_GALLERY_TYPE_TAG = "ticket-gallery-type-tag",
        TICKET_GALLERY_TYPE_FORM = "ticket-gallery-type-form",
        TICKET_GALLERY_CATEGORY = "ticket-gallery-category",
        TICKET_GALLERY_CATEGORY_ALL = "ticket-gallery-category-all",
        TICKET_GALLERY_CATEGORY_SUPPORT = "ticket-gallery-category-support",
        TICKET_GALLERY_CATEGORY_MODERATION = "ticket-gallery-category-moderation",
        TICKET_GALLERY_CATEGORY_SALES = "ticket-gallery-category-sales",
        TICKET_GALLERY_CATEGORY_APPLICATIONS = "ticket-gallery-category-applications",
        TICKET_GALLERY_CATEGORY_FEEDBACK = "ticket-gallery-category-feedback",
        TICKET_GALLERY_CATEGORY_GENERAL = "ticket-gallery-category-general",
        TICKET_GALLERY_CATEGORY_OTHER = "ticket-gallery-category-other",
        TICKET_GALLERY_SORT = "ticket-gallery-sort",
        TICKET_GALLERY_SORT_NEWEST = "ticket-gallery-sort-newest",
        TICKET_GALLERY_SORT_POPULAR = "ticket-gallery-sort-popular",
        TICKET_GALLERY_CLEAR = "ticket-gallery-clear",
        TICKET_GALLERY_FEATURED = "ticket-gallery-featured",
        TICKET_GALLERY_IMPORT_COUNT = "ticket-gallery-import-count",
        TICKET_GALLERY_AUTHOR = "ticket-gallery-author",
        TICKET_GALLERY_OPEN = "ticket-gallery-open",
        TICKET_GALLERY_BACK = "ticket-gallery-back",
        TICKET_GALLERY_EMPTY = "ticket-gallery-empty",
        TICKET_GALLERY_IMPORT_PANEL = "ticket-gallery-import-panel",
        TICKET_GALLERY_IMPORT_TAG = "ticket-gallery-import-tag",
        TICKET_GALLERY_IMPORT_FORM = "ticket-gallery-import-form",
        TICKET_GALLERY_IMPORT_ID = "ticket-gallery-import-id",
        TICKET_GALLERY_PUBLISH = "ticket-gallery-publish",
        TICKET_GALLERY_NAME = "ticket-gallery-name",
        TICKET_GALLERY_DESCRIPTION = "ticket-gallery-description",
        TICKET_GALLERY_TAGS = "ticket-gallery-tags",
        TICKET_GALLERY_AUTHOR_NAME = "ticket-gallery-author-name",
        TICKET_GALLERY_SHARE_CONSENT = "ticket-gallery-share-consent",
        TICKET_GALLERY_SUBMIT = "ticket-gallery-submit",
        TICKET_GALLERY_SOURCES = "ticket-gallery-sources",
        TICKET_GALLERY_NO_SOURCES = "ticket-gallery-no-sources",
        TICKET_GALLERY_SUBMISSIONS = "ticket-gallery-submissions",
        TICKET_GALLERY_SUBMISSIONS_EMPTY = "ticket-gallery-submissions-empty",
        TICKET_GALLERY_STATUS_PENDING = "ticket-gallery-status-pending",
        TICKET_GALLERY_STATUS_APPROVED = "ticket-gallery-status-approved",
        TICKET_GALLERY_STATUS_REJECTED = "ticket-gallery-status-rejected",
        TICKET_GALLERY_STATUS_WITHDRAWN = "ticket-gallery-status-withdrawn",
        TICKET_GALLERY_STATUS_UNPUBLISHED = "ticket-gallery-status-unpublished",
        TICKET_GALLERY_REJECTION_REASON = "ticket-gallery-rejection-reason",
        TICKET_GALLERY_EDIT = "ticket-gallery-edit",
        TICKET_GALLERY_WITHDRAW = "ticket-gallery-withdraw",
        TICKET_GALLERY_UNPUBLISH = "ticket-gallery-unpublish",
        TICKET_GALLERY_SAVED = "ticket-gallery-saved",
        TICKET_GALLERY_MORE = "ticket-gallery-more",
        TICKET_GALLERY_PANEL_TITLE = "ticket-gallery-panel-title",
        TICKET_GALLERY_PANEL_CONTENT = "ticket-gallery-panel-content",
        TICKET_GALLERY_WELCOME_MESSAGE = "ticket-gallery-welcome-message",
        TICKET_GALLERY_SELECT_SOURCE = "ticket-gallery-select-source",
        TICKET_GALLERY_PUBLISHED = "ticket-gallery-published",
        TICKET_GALLERY_PAGES = "ticket-gallery-pages",
        NOTIFICATIONS_TITLE = "notifications-title",
        NOTIFICATIONS_EMPTY = "notifications-empty",
        NOTIFICATIONS_UNREAD_COUNT = "notifications-unread-count",
        NOTIFICATIONS_MARK_READ = "notifications-mark-read",
        NOTIFICATIONS_MARK_ALL_READ = "notifications-mark-all-read",
        NOTIFICATIONS_ALL = "notifications-all",
        NOTIFICATIONS_INTEGRATIONS = "notifications-integrations",
        NOTIFICATIONS_TODAY = "notifications-today",
        NOTIFICATIONS_YESTERDAY = "notifications-yesterday",
        NOTIFICATIONS_THIS_WEEK = "notifications-this-week",
        NOTIFICATIONS_EARLIER = "notifications-earlier",
        NOTIFICATIONS_PREVIOUS = "notifications-previous",
        NOTIFICATIONS_NEXT = "notifications-next",
        NOTIFICATION_SETTINGS_TITLE = "notification-settings-title",
        NOTIFICATION_SETTINGS_IN_APP = "notification-settings-in-app",
        NOTIFICATION_SETTINGS_DISCORD_DM = "notification-settings-discord-dm",
        NOTIFICATION_SETTINGS_EMAIL = "notification-settings-email",
        NOTIFICATION_SETTINGS_SAVE = "notification-settings-save",
        NOTIFICATION_SETTINGS_EMAIL_NOT_CONFIGURED = "notification-settings-email-not-configured",
        NOTIFICATION_SETTINGS_EMAIL_NEEDS_VERIFIED = "notification-settings-email-needs-verified",
        NOTIFICATION_SETTINGS_EMAIL_ADDRESS = "notification-settings-email-address",
        NOTIFICATION_SETTINGS_EMAIL_CODE = "notification-settings-email-code",
        NOTIFICATION_SETTINGS_EMAIL_SEND_CODE = "notification-settings-email-send-code",
        NOTIFICATION_SETTINGS_EMAIL_VERIFY = "notification-settings-email-verify",
        NOTIFICATION_SETTINGS_EMAIL_REMOVE = "notification-settings-email-remove",
        NOTIFICATION_SETTINGS_EMAIL_CONFIRM_REMOVE = "notification-settings-email-confirm-remove",
        NOTIFICATION_SETTINGS_EMAIL_VERIFIED = "notification-settings-email-verified",
        NOTIFICATION_SETTINGS_EMAIL_PENDING = "notification-settings-email-pending",
        NOTIFICATION_SETTINGS_EMAIL_HELP = "notification-settings-email-help",
        NOTIFICATION_SETTINGS_ACTION_ERROR = "notification-settings-action-error",
        NOTIFICATIONS_UNREAD = "notifications-unread",
        NOTIFICATIONS_OLDER = "notifications-older",
        NOTIFICATIONS_NEWEST = "notifications-newest",
        NOTIFICATIONS_CATEGORY_INTEGRATIONS = "notifications-category-integrations",
        NOTIFICATIONS_CATEGORY_GALLERY = "notifications-category-gallery",
        NOTIFICATION_IN_APP = "notification-in-app",
        NOTIFICATION_DISCORD_DM = "notification-discord-dm",
        NOTIFICATION_DISCORD_DM_HELP = "notification-discord-dm-help",
        NOTIFICATION_EMAIL = "notification-email",
        NOTIFICATION_EMAIL_HELP = "notification-email-help",
        NOTIFICATION_EMAIL_NOT_CONFIGURED = "notification-email-not-configured",
        NOTIFICATION_EMAIL_NEEDS_VERIFIED = "notification-email-needs-verified",
        NOTIFICATION_EMAIL_ADDRESS = "notification-email-address",
        NOTIFICATION_EMAIL_CODE = "notification-email-code",
        NOTIFICATION_EMAIL_SEND_CODE = "notification-email-send-code",
        NOTIFICATION_EMAIL_VERIFY = "notification-email-verify",
        NOTIFICATION_EMAIL_REMOVE = "notification-email-remove",
        NOTIFICATION_EMAIL_CONFIRM_REMOVE = "notification-email-confirm-remove",
        NOTIFICATION_EMAIL_VERIFIED = "notification-email-verified",
        NOTIFICATION_EMAIL_PENDING = "notification-email-pending",
        NOTIFICATION_GALLERY_APPROVED = "notification-gallery-approved",
        NOTIFICATION_GALLERY_REJECTED = "notification-gallery-rejected",
        NOTIFICATION_GALLERY_REASON = "notification-gallery-reason",
        NOTIFICATION_EMAIL_SAVE = "notification-email-save",
        NOTIFICATION_EMAIL_RESEND = "notification-email-resend",
        NOTIFICATION_SAVE = "notification-save",
        TICKET_ANALYTICS = "ticket-analytics",
        TICKET_ANALYTICS_TITLE = "ticket-analytics-title",
        TICKET_ANALYTICS_RANGE = "ticket-analytics-range",
        TICKET_ANALYTICS_DATA_INCOMPLETE = "ticket-analytics-data-incomplete",
        TICKET_ANALYTICS_RATING_BUCKET = "ticket-analytics-rating-bucket",
        TICKET_ANALYTICS_BACK = "ticket-analytics-back",
        TICKET_ANALYTICS_PERIOD = "ticket-analytics-period",
        TICKET_ANALYTICS_7_DAYS = "ticket-analytics-7-days",
        TICKET_ANALYTICS_30_DAYS = "ticket-analytics-30-days",
        TICKET_ANALYTICS_90_DAYS = "ticket-analytics-90-days",
        TICKET_ANALYTICS_SUBMIT = "ticket-analytics-submit",
        TICKET_ANALYTICS_CLOSED = "ticket-analytics-closed",
        TICKET_ANALYTICS_OPEN = "ticket-analytics-open",
        TICKET_ANALYTICS_DURATION = "ticket-analytics-duration",
        TICKET_ANALYTICS_DURATION_SAMPLES = "ticket-analytics-duration-samples",
        TICKET_ANALYTICS_DURATION_MISSING = "ticket-analytics-duration-missing",
        TICKET_ANALYTICS_DURATION_AVERAGE = "ticket-analytics-duration-average",
        TICKET_ANALYTICS_RATINGS = "ticket-analytics-ratings",
        TICKET_ANALYTICS_RATINGS_COUNT = "ticket-analytics-ratings-count",
        TICKET_ANALYTICS_RATINGS_MISSING = "ticket-analytics-ratings-missing",
        TICKET_ANALYTICS_RATINGS_AVERAGE = "ticket-analytics-ratings-average",
        TICKET_ANALYTICS_RATING_DISTRIBUTION = "ticket-analytics-rating-distribution",
        TICKET_ANALYTICS_RECORDS_SCANNED = "ticket-analytics-records-scanned",
        TICKET_ANALYTICS_INCOMPLETE = "ticket-analytics-incomplete",
        TICKET_ANALYTICS_COMPLETE = "ticket-analytics-complete",
        TICKET_ANALYTICS_RETAINED_SCOPE = "ticket-analytics-retained-scope",
        TICKET_ANALYTICS_NO_RATINGS = "ticket-analytics-no-ratings",
        TICKET_ANALYTICS_CONTINUE = "ticket-analytics-continue",
        TICKET_ANALYTICS_LOADING = "ticket-analytics-loading",
        TICKET_ANALYTICS_FILTERED_SCOPE = "ticket-analytics-filtered-scope",
        TICKET_BULK_SELECT = "ticket-bulk-select",
        TICKET_BULK_ACTIONS = "ticket-bulk-actions",
        TICKET_BULK_ACTION = "ticket-bulk-action",
        TICKET_BULK_RUN = "ticket-bulk-run",
        TICKET_BULK_SELECT_REQUIRED = "ticket-bulk-select-required",
        TICKET_BULK_RESULTS = "ticket-bulk-results",
        TICKET_BULK_BACK = "ticket-bulk-back",
        TICKET_BULK_RESULT = "ticket-bulk-result",
        TICKET_BULK_STATUS = "ticket-bulk-status",
        TICKET_BULK_SUCCEEDED = "ticket-bulk-succeeded",
        TICKET_BULK_FAILED = "ticket-bulk-failed",
        PUBLIC_KNOWLEDGE_TITLE = "public-knowledge-title",
        PUBLIC_KNOWLEDGE_SEARCH = "public-knowledge-search",
        PUBLIC_KNOWLEDGE_SEARCH_SUBMIT = "public-knowledge-search-submit",
        PUBLIC_KNOWLEDGE_SEARCH_HINT = "public-knowledge-search-hint",
        PUBLIC_KNOWLEDGE_CATEGORY = "public-knowledge-category",
        PUBLIC_KNOWLEDGE_ALL_CATEGORIES = "public-knowledge-all-categories",
        PUBLIC_KNOWLEDGE_KEYWORDS = "public-knowledge-keywords",
        PUBLIC_KNOWLEDGE_NO_RESULTS = "public-knowledge-no-results",
        PUBLIC_KNOWLEDGE_NEXT = "public-knowledge-next",
        PUBLIC_KNOWLEDGE_BACK = "public-knowledge-back",
        PUBLIC_KNOWLEDGE_NOT_FOUND = "public-knowledge-not-found",
        TICKET_SETTINGS_INTEGRATIONS = "ticket-settings-integrations",
        TICKET_INTEGRATION_SECRET_STATUS = "ticket-integration-secret-status",
        TICKET_INTEGRATION_SECRET_HELP = "ticket-integration-secret-help",
        CONFIG_TICKETS_INTEGRATION_HEADER_NAME = "config-tickets-integration-header-name",
        CONFIG_TICKETS_INTEGRATION_HEADER_VALUE_TEMPLATE = "config-tickets-integration-header-value-template",
        CONFIG_TICKETS_INTEGRATION_SECRET_NAME = "config-tickets-integration-secret-name",
        CONFIG_TICKETS_INTEGRATION_MAPPING_NAME = "config-tickets-integration-mapping-name",
        CONFIG_TICKETS_INTEGRATION_MAPPING_PATH = "config-tickets-integration-mapping-path",
        TICKET_INTEGRATION_SECRET_STATUS_UNAVAILABLE = "ticket-integration-secret-status-unavailable",
        TICKET_INTEGRATION_SECRET_SAVED = "ticket-integration-secret-saved",
        TICKET_INTEGRATION_ORPHANS_TITLE = "ticket-integration-orphans-title",
        TICKET_INTEGRATION_ORPHANS_HELP = "ticket-integration-orphans-help",
        TICKET_INTEGRATION_ORPHANS_EMPTY = "ticket-integration-orphans-empty",
        TICKET_INTEGRATION_ORPHAN_DELETE = "ticket-integration-orphan-delete",
        TICKET_INTEGRATION_SECRET_CONFIGURED = "ticket-integration-secret-configured",
        TICKET_INTEGRATION_SECRET_MISSING = "ticket-integration-secret-missing",
        TICKET_INTEGRATION_SECRET_VALUE = "ticket-integration-secret-value",
        TICKET_INTEGRATION_SECRET_SET = "ticket-integration-secret-set",
        TICKET_INTEGRATION_SECRET_REMOVE = "ticket-integration-secret-remove",
        TICKET_INTEGRATION_SECRETS_OWNER_ONLY = "ticket-integration-secrets-owner-only",
        TICKET_INTEGRATION_GENERATION_MISSING = "ticket-integration-generation-missing",
        TICKET_INTEGRATION_DELETE_OWNER_ONLY = "ticket-integration-delete-owner-only",
        CONFIG_TICKETS_INTEGRATION_METHOD = "config-tickets-integration-method",
        CONFIG_TICKETS_INTEGRATION_HEADERS = "config-tickets-integration-headers",
        CONFIG_TICKETS_INTEGRATION_SECRETS = "config-tickets-integration-secrets",
        CONFIG_TICKETS_INTEGRATION_RESPONSE_MAPPINGS = "config-tickets-integration-response-mappings",
        CONFIG_TICKETS_INTEGRATION_SEND_FORM_DATA = "config-tickets-integration-send-form-data",
        CONFIG_TICKETS_INTEGRATION_AUTH_NONE = "config-tickets-integration-auth-none",
        CONFIG_TICKETS_INTEGRATION_AUTH_BEARER = "config-tickets-integration-auth-bearer",
        CONFIG_TICKETS_INTEGRATION_AUTH_API_KEY = "config-tickets-integration-auth-api-key",
        CONFIG_TICKETS_INTEGRATION_METHOD_GET = "config-tickets-integration-method-get",
        CONFIG_TICKETS_INTEGRATION_METHOD_POST = "config-tickets-integration-method-post",
        TICKETS_TITLE = "tickets-title",
        TICKETS_COLOR = "tickets-color",
        TICKETS_CONTENT = "tickets-content",
        TICKETS_WELCOME_FIELDS = "tickets-welcome-fields",
        TICKETS_WELCOME_INLINE = "tickets-welcome-inline",
        TICKETS_RATING_ENABLED = "tickets-rating-enabled",
        TICKETS_CLOSE_REASONS = "tickets-close-reasons",
        TICKETS_ALLOW_CUSTOM_CLOSE_REASONS = "tickets-allow-custom-close-reasons",
        TICKETS_CHANNEL_NAME_SCHEME = "tickets-channel-name-scheme",
        TICKETS_CHANNEL_NAME_CUSTOM = "tickets-channel-name-custom",
        TICKETS_OPEN_COOLDOWN_SECONDS = "tickets-open-cooldown-seconds",
        TICKETS_ACCESS_RULES = "tickets-access-rules",
        TICKETS_ACCESS_RULES_HELP = "tickets-access-rules-help",
        TICKETS_ACCESS_RULE_ROLE = "tickets-access-role",
        TICKETS_ACCESS_RULE_ACTION = "tickets-access-action",
        TICKETS_ACCESS_RULE_ORDER = "tickets-access-rule-order",
        TICKETS_ACCESS_ALLOW = "tickets-access-allow",
        TICKETS_ACCESS_DENY = "tickets-access-deny",
        TICKETS_PANEL_TITLE = "tickets-panel-title",
        TICKETS_PANEL_COLOR = "tickets-panel-color",
        TICKETS_PANEL_CONTENT = "tickets-panel-content",
        TICKETS_WELCOME_TITLE = "tickets-welcome-title",
        TICKETS_WELCOME_TITLE_URL = "tickets-welcome-title-url",
        TICKETS_WELCOME_COLOR = "tickets-welcome-color",
        TICKETS_WELCOME_DESCRIPTION = "tickets-welcome-description",
        TICKETS_WELCOME_AUTHOR_NAME = "tickets-welcome-author-name",
        TICKETS_WELCOME_AUTHOR_URL = "tickets-welcome-author-url",
        TICKETS_WELCOME_AUTHOR_ICON_URL = "tickets-welcome-author-icon-url",
        TICKETS_WELCOME_THUMBNAIL_URL = "tickets-welcome-thumbnail-url",
        TICKETS_WELCOME_IMAGE_URL = "tickets-welcome-image-url",
        TICKETS_WELCOME_FOOTER_TEXT = "tickets-welcome-footer-text",
        TICKETS_WELCOME_FOOTER_ICON_URL = "tickets-welcome-footer-icon-url",
        TICKETS_WELCOME_TIMESTAMP = "tickets-welcome-timestamp",
        TICKETS_ACCESS_ROLE_PICKER_UNAVAILABLE = "ticket-access-role-picker-unavailable",
        CONFIG_TICKETS_PANEL_ID = "config-tickets-panel-id",
        CONFIG_TICKETS_PANEL_NAME = "config-tickets-panel-name",
        CONFIG_TICKETS_PANEL_DESCRIPTION = "config-tickets-panel-description",
        CONFIG_TICKETS_PANEL_BUTTON_STYLE = "config-tickets-panel-button_style",
        CONFIG_TICKETS_PANEL_STAFF = "config-tickets-panel-staff",
        CONFIG_TICKETS_PANEL_STAFF_TEAMS = "config-tickets-panel-staff_teams",
        CONFIG_TICKETS_PANEL_MAX_OPEN = "config-tickets-panel-max_open",
        CONFIG_TICKETS_PANEL_TIMEZONE = "config-tickets-panel-timezone",
        CONFIG_TICKETS_PANEL_WAITING_AFTER_HOURS = "config-tickets-panel-waiting_after_hours",
        CONFIG_TICKETS_PANEL_FIRST_RESPONSE_TIMEOUT_HOURS = "config-tickets-panel-first_response_timeout_hours",
        CONFIG_TICKETS_PANEL_LAST_MESSAGE_TIMEOUT_HOURS = "config-tickets-panel-last_message_timeout_hours",
        CONFIG_TICKETS_PANEL_WAITING_CATEGORY = "config-tickets-panel-waiting_category",
        CONFIG_TICKETS_PANEL_QUESTIONS = "config-tickets-panel-questions",
        CONFIG_TICKETS_PANEL_EXIT_SURVEY = "config-tickets-panel-exit_survey",
        TICKETS_PANEL_SETTINGS_INVALID = "tickets-panel-settings-invalid",
        TICKETS_PANEL_ACCESS_DENIED = "tickets-panel-access-denied",
        TICKETS_PANEL_COOLDOWN = "tickets-panel-cooldown",
        TICKETS_CLOSE_REASON_INVALID = "tickets-close-reason-invalid",
        TICKET_ACCESS_ROLE_PICKER_UNAVAILABLE = "ticket-access-role-picker-unavailable",
        OPT_ID = "opt-id",
        OPT_USERNAME = "opt-username",
        OPT_ID_USERNAME = "opt-id_username",
        OPT_NICKNAME = "opt-nickname",
        OPT_ID_PADDED = "opt-id_padded",
        OPT_CUSTOM = "opt-custom",
        OPT_ALLOW = "opt-allow",
        OPT_DENY = "opt-deny",
        CASES = "cases",
        CASES_NONE = "cases-none",
        CASE_STATE = "case-state",
        CASE_ACTIVE = "case-active",
        CASE_EXPIRED = "case-expired",
        CASE_SOURCE = "case-source",
        CASE_SEARCH = "case-search",
        CASE_ANY = "case-any",
        CASE_USER = "case-user",
        CASE_WORD = "case-word",
        CASE_HIDDEN = "case-hidden",
        CASE_SHOW_HIDDEN = "case-show-hidden",
        CASE_EXPORT = "case-export",
        CASE_TOTAL = "case-total",
        CASE_TOP_MODERATORS = "case-top-moderators",
        CASE_TOP_TARGETS = "case-top-targets",
        CASE_NOTE_ADD = "case-note-add",
        CASE_NOTE_TEXT = "case-note-text",
        CASE_REASON_EDIT = "case-reason-edit",
        CASE_HIDE = "case-hide",
        CASE_SHOW = "case-show",
        CASE_NOTE_ADDED = "case-note-added",
        CASE_UPDATED = "case-updated",
        CASE_BACK = "case-back",
        CASE_EXPIRES = "case-expires",
        CASE_REVOKED_BY = "case-revoked-by",
        CASE_MODULE = "case-module",
        CASE_TITLE = "case-title",
        CASE_LAST_DAYS = "case-last-days",
        CASE_ONLY_MODERATORS = "case-only-moderators",
        CASE_BAD_FILTER = "case-bad-filter",
        CASE_NEEDS_EDIT = "case-needs-edit",
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
