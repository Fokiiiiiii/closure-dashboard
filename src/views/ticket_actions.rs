use std::fmt::Write;

use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

pub(crate) struct ActionTokens {
    pub close: String,
    pub request_close: String,
    pub reply: String,
    pub add_label: String,
    pub remove_label: String,
}

pub(crate) fn actions(
    l: Locale<'_>,
    guild_id: &str,
    channel_id: &str,
    state: &Value,
    etag: &str,
    tokens: &ActionTokens,
) -> String {
    if !matches!(state["status"].as_str(), Some("open" | "closing")) {
        return String::new();
    }

    let guild = escape(guild_id);
    let channel = escape(channel_id);
    let etag = escape(etag);
    let action_url = format!("/g/{guild}/tickets/{channel}/actions");
    let mut body = String::from(r#"<section class="card panel"><div class="panel-heading"><h2>"#);
    let _ = write!(body, "{}</h2></div>", l.text(keys::TICKET_ACTIONS_TITLE));
    if state["waiting_for_reply"] == Value::Bool(true) {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ACTIONS_WAITING_FOR_REPLY)
        );
    }
    if state["close_request"].is_object() {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_ACTIONS_CLOSE_REQUEST)
        );
    }
    if let Some(labels) = state["labels"]
        .as_array()
        .filter(|labels| !labels.is_empty())
    {
        let _ = write!(
            body,
            r#"<h3>{}</h3><ul class="ticket-labels">"#,
            l.text(keys::TICKET_ACTIONS_LABELS)
        );
        for label in labels.iter().filter_map(Value::as_str) {
            let _ = write!(body, "<li>{}</li>", escape(label));
        }
        body.push_str("</ul>");
    }

    let _ = write!(
        body,
        r#"<form method="post" action="{action_url}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><textarea name="reason"></textarea></label><button class="button secondary" name="action" value="ticket.close">{}</button></form>"#,
        escape(&tokens.close),
        l.text(keys::TICKET_ACTIONS_REASON),
        l.text(keys::TICKET_ACTIONS_CLOSE)
    );
    if state["close_request"].is_null() {
        let _ = write!(
            body,
            r#"<form method="post" action="{action_url}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><textarea name="reason"></textarea></label><button class="button secondary" name="action" value="ticket.request_close">{}</button></form>"#,
            escape(&tokens.request_close),
            l.text(keys::TICKET_ACTIONS_REASON),
            l.text(keys::TICKET_ACTIONS_REQUEST_CLOSE)
        );
    }
    let _ = write!(
        body,
        r#"<form method="post" action="{action_url}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><textarea name="content" required></textarea></label><button class="button" name="action" value="ticket.reply">{}</button></form>"#,
        escape(&tokens.reply),
        l.text(keys::TICKET_ACTIONS_MESSAGE),
        l.text(keys::TICKET_ACTIONS_REPLY)
    );
    let _ = write!(
        body,
        r#"<form method="post" action="{action_url}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><input name="label" type="text" required></label><button class="button secondary" name="action" value="ticket.add_label">{}</button></form>"#,
        escape(&tokens.add_label),
        l.text(keys::TICKET_ACTIONS_LABEL),
        l.text(keys::TICKET_ACTIONS_ADD_LABEL)
    );
    let _ = write!(
        body,
        r#"<form method="post" action="{action_url}"><input type="hidden" name="etag" value="{etag}"><input type="hidden" name="key" value="{}"><label class="field"><span>{}</span><input name="label" type="text" required></label><button class="button secondary" name="action" value="ticket.remove_label">{}</button></form></section>"#,
        escape(&tokens.remove_label),
        l.text(keys::TICKET_ACTIONS_LABEL),
        l.text(keys::TICKET_ACTIONS_REMOVE_LABEL)
    );
    body
}
