use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, group_digits, keys},
    session,
};

use super::shell;

#[derive(Clone, Default)]
pub(crate) struct TranscriptFilters {
    pub ticket_id: Option<String>,
    pub username: Option<String>,
    pub user_id: Option<String>,
    pub closed_by: Option<String>,
    pub panel: Option<String>,
    pub rating: Option<String>,
    pub claimed_by: Option<String>,
    pub close_reason: Option<String>,
    pub labels: Vec<String>,
    pub columns: Vec<String>,
}

pub(crate) const TRANSCRIPT_COLUMNS: [(&str, &str); 10] = [
    ("ticket_id", keys::TICKET_TRANSCRIPTS_TICKET_ID),
    ("username", keys::TICKET_TRANSCRIPTS_USERNAME),
    ("rating", keys::TICKET_TRANSCRIPTS_RATING),
    ("closed_by", keys::TICKET_TRANSCRIPTS_CLOSED_BY),
    ("claimed_by", keys::TICKET_CLAIMED_BY),
    ("panel", keys::TICKET_PANEL),
    ("closed_at", keys::TICKET_CLOSED_AT),
    ("reason", keys::TICKET_REASON),
    ("labels", keys::TICKET_TRANSCRIPTS_LABELS),
    ("actions", keys::TICKET_TRANSCRIPTS_ACTIONS),
];

#[derive(Clone, Default)]
pub(crate) struct TranscriptActionTokens {
    pub reason: String,
    pub add_label: String,
    pub remove_label: String,
}

#[derive(Default)]
pub(crate) struct TranscriptLabelTokens {
    pub create: String,
    pub delete: HashMap<String, String>,
}

// Independent view-state flags keep the list and manager notices explicit.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
pub(crate) struct TranscriptPageData {
    pub records: Vec<Value>,
    pub labels: Vec<Value>,
    pub panels: Vec<Value>,
    pub filters: TranscriptFilters,
    pub total: u64,
    pub total_complete: bool,
    pub username_truncated: bool,
    pub before: Option<String>,
    pub previous: Vec<String>,
    pub next_before: Option<String>,
    pub label_cursor: Option<String>,
    pub next_label_cursor: Option<String>,
    pub label_etag: Option<String>,
    pub syncing: bool,
    pub backfill_incomplete: bool,
    pub can_manage_labels: bool,
    pub manage_open: bool,
    pub updated: bool,
    pub conflict: bool,
    pub return_query: String,
    pub row_tokens: HashMap<String, TranscriptActionTokens>,
    pub label_tokens: TranscriptLabelTokens,
}

pub(crate) fn transcripts_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    data: &TranscriptPageData,
) -> String {
    let action_url = format!("/g/{}/transcripts/actions", escape(id));
    let labels_action_url = format!("/g/{}/transcripts/labels/actions", escape(id));
    let list_url = format!("/g/{}/transcripts", escape(id));
    let panels = transcript_panel_options(data);
    let mut body = transcript_page_heading(l, id, data);
    render_transcript_filters(&mut body, l, &list_url, &panels, data);
    render_transcript_label_manager(&mut body, l, id, &labels_action_url, data);
    column_selector(&mut body, l, id, data);
    render_transcript_records(&mut body, l, id, &action_url, data);
    render_transcript_pagination(&mut body, l, id, data);
    body.push_str("</section>");
    shell(l, headers, id, name, keys::TICKET_TRANSCRIPTS, &body, false)
}

fn transcript_page_heading(l: Locale<'_>, id: &str, data: &TranscriptPageData) -> String {
    let id_html = escape(id);
    let mut body = format!(
        r#"<header class="page-heading"><h1>{}</h1><div class="head-actions"><a class="button secondary" href="/g/{id_html}/tickets">{}</a></div></header>"#,
        l.text(keys::TICKET_TRANSCRIPTS),
        l.text(keys::TICKET_BACK)
    );
    if data.updated {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_UPDATED)
        );
    }
    if data.conflict {
        let _ = write!(
            body,
            r#"<p class="error" role="alert">{}</p>"#,
            l.text(keys::CONFLICT)
        );
    }
    let _ = write!(
        body,
        r#"<section class="card panel"><header class="panel-heading"><h2>{}</h2><span>{}</span>"#,
        l.text(keys::TICKET_TRANSCRIPTS_FILTERS),
        if data.total_complete {
            l.with_text(
                keys::TICKET_TRANSCRIPTS_TOTAL,
                "count",
                &group_digits(data.total),
            )
        } else {
            l.with_text(
                keys::TICKET_TRANSCRIPTS_TOTAL_PARTIAL,
                "count",
                &group_digits(data.total),
            )
        }
    );
    if data.can_manage_labels {
        let manage_url = query_url(
            id,
            &data.filters,
            data.before.as_deref(),
            &data.previous,
            data.label_cursor.as_deref(),
            true,
            false,
        );
        let _ = write!(
            body,
            r#"<a class="button secondary" href="{manage_url}#transcript-label-manager">{}</a>"#,
            l.text(keys::TICKET_TRANSCRIPTS_MANAGE_LABELS)
        );
    }
    body.push_str("</header>");
    if data.username_truncated {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_USERNAME_LIMIT)
        );
    }
    if data.syncing {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_SYNCING)
        );
    }
    if data.backfill_incomplete {
        let _ = write!(
            body,
            r#"<p class="error" role="status">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_BACKFILL_INCOMPLETE)
        );
    }
    body
}

fn transcript_panel_options(data: &TranscriptPageData) -> BTreeMap<String, String> {
    let mut panels = BTreeMap::<String, String>::new();
    for panel in &data.panels {
        let Some(id) = panel["id"].as_str() else {
            continue;
        };
        let name = panel["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .unwrap_or(id);
        panels.insert(id.to_owned(), name.to_owned());
    }
    for record in &data.records {
        if let Some(id) = panel_id(record) {
            let name = panel_display(record, &data.panels).unwrap_or_else(|| id.to_owned());
            panels.entry(id.to_owned()).or_insert(name);
        }
    }
    if let Some(selected) = data.filters.panel.as_ref() {
        panels
            .entry(selected.clone())
            .or_insert_with(|| selected.clone());
    }
    panels
}

fn render_transcript_filters(
    body: &mut String,
    l: Locale<'_>,
    list_url: &str,
    panels: &BTreeMap<String, String>,
    data: &TranscriptPageData,
) {
    let _ = write!(
        body,
        r#"<form method="get" action="{list_url}" data-transcript-autosubmit><div class="form-grid">"#
    );
    render_transcript_text_filters(body, l, data);
    render_transcript_select_filters(body, l, panels, data);
    for column in &data.filters.columns {
        let _ = write!(
            body,
            r#"<input type="hidden" name="column" value="{}">"#,
            escape(column)
        );
    }
    render_transcript_label_filters(body, l, data);
    let _ = write!(
        body,
        r#"<button class="button secondary" type="submit">{}</button></form>"#,
        l.text(keys::TICKET_TRANSCRIPTS_FILTER)
    );
}

fn render_transcript_text_filters(body: &mut String, l: Locale<'_>, data: &TranscriptPageData) {
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_TICKET_ID,
        "ticket_id",
        data.filters.ticket_id.as_deref(),
        "text",
    );
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_USERNAME,
        "username",
        data.filters.username.as_deref(),
        "search",
    );
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_USER_ID,
        "user_id",
        data.filters.user_id.as_deref(),
        "text",
    );
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_CLOSED_BY,
        "closed_by",
        data.filters.closed_by.as_deref(),
        "text",
    );
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_CLAIMED_BY,
        "claimed_by",
        data.filters.claimed_by.as_deref(),
        "text",
    );
    text_filter(
        body,
        l,
        keys::TICKET_TRANSCRIPTS_CLOSE_REASON,
        "close_reason",
        data.filters.close_reason.as_deref(),
        "search",
    );
}

fn render_transcript_select_filters(
    body: &mut String,
    l: Locale<'_>,
    panels: &BTreeMap<String, String>,
    data: &TranscriptPageData,
) {
    let _ = write!(
        body,
        r#"<label class="field"><span>{}</span><select name="panel"><option value="">{}</option>"#,
        l.text(keys::TICKET_TRANSCRIPTS_PANEL),
        l.text(keys::TICKET_TRANSCRIPTS_ANY)
    );
    for (panel_id, panel_name) in panels {
        let selected = if data.filters.panel.as_deref() == Some(panel_id.as_str()) {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<option value="{}"{selected}>{}</option>"#,
            escape(panel_id),
            escape(panel_name)
        );
    }
    body.push_str("</select></label>");
    let _ = write!(
        body,
        r#"<label class="field"><span>{}</span><select name="rating"><option value="">{}</option>"#,
        l.text(keys::TICKET_TRANSCRIPTS_RATING),
        l.text(keys::TICKET_TRANSCRIPTS_ANY)
    );
    for score in 1..=5 {
        let value = score.to_string();
        let selected = if data.filters.rating.as_deref() == Some(value.as_str()) {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<option value="{value}"{selected}>{} {value}/5</option>"#,
            "★".repeat(score)
        );
    }
    body.push_str("</select></label></div>");
}

fn render_transcript_label_filters(body: &mut String, l: Locale<'_>, data: &TranscriptPageData) {
    if data.labels.is_empty() && data.filters.labels.is_empty() {
        return;
    }
    let visible_labels = data
        .labels
        .iter()
        .filter_map(|label| label["name"].as_str())
        .collect::<BTreeSet<_>>();
    body.push_str(r#"<fieldset class="settings-card"><legend>"#);
    body.push_str(&l.text(keys::TICKET_TRANSCRIPTS_LABELS));
    body.push_str("</legend><div class=\"tag-list\">");
    for selected in &data.filters.labels {
        if !visible_labels.contains(selected.as_str()) {
            let _ = write!(
                body,
                r#"<input type="hidden" name="label" value="{}">"#,
                escape(selected)
            );
        }
    }
    for label in &data.labels {
        let Some(label_name) = label["name"].as_str() else {
            continue;
        };
        let checked = data
            .filters
            .labels
            .iter()
            .any(|value| value.eq_ignore_ascii_case(label_name));
        let color = label_color(label);
        let _ = write!(
            body,
            r#"<label class="pill"><input type="checkbox" name="label" value="{}"{}><input class="label-swatch" type="color" value="{}" disabled aria-hidden="true">{}</label>"#,
            escape(label_name),
            if checked { " checked" } else { "" },
            color,
            escape(label_name)
        );
    }
    body.push_str("</div></fieldset>");
}

fn render_transcript_label_manager(
    body: &mut String,
    l: Locale<'_>,
    id: &str,
    labels_action_url: &str,
    data: &TranscriptPageData,
) {
    if !data.can_manage_labels {
        return;
    }
    let open = if data.manage_open { " open" } else { "" };
    let _ = write!(
        body,
        r#"<details id="transcript-label-manager" class="settings-card"{open}><summary>{}</summary>"#,
        l.text(keys::TICKET_TRANSCRIPTS_MANAGE_LABELS)
    );
    if let Some(etag) = data.label_etag.as_deref() {
        manager_form(
            body,
            labels_action_url,
            etag,
            &data.label_tokens.create,
            &data.return_query,
            l,
        );
        render_label_definition_table(body, l, labels_action_url, etag, data);
    } else {
        let _ = write!(
            body,
            r#"<p class="form-status" role="status">{}</p>"#,
            l.text(keys::UNAVAILABLE)
        );
    }
    if let Some(cursor) = data.next_label_cursor.as_deref() {
        let next = query_url(
            id,
            &data.filters,
            data.before.as_deref(),
            &data.previous,
            Some(cursor),
            true,
            data.updated,
        );
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{next}">{}</a></p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_MORE_LABELS)
        );
    }
    body.push_str("</details>");
}

fn render_label_definition_table(
    body: &mut String,
    l: Locale<'_>,
    labels_action_url: &str,
    etag: &str,
    data: &TranscriptPageData,
) {
    body.push_str(r#"<div class="table-scroll"><table><thead><tr><th>"#);
    body.push_str(&l.text(keys::TICKET_TRANSCRIPTS_LABEL_NAME));
    body.push_str("</th><th>");
    body.push_str(&l.text(keys::TICKET_TRANSCRIPTS_LABEL_COLOR));
    body.push_str("</th><th></th></tr></thead><tbody>");
    for label in &data.labels {
        let Some(label_name) = label["name"].as_str() else {
            continue;
        };
        let color = label_color(label);
        let _ = write!(
            body,
            r#"<tr><td>{}</td><td><span class="label-color"><input type="color" value="{}" disabled aria-hidden="true"><span class="mono">{}</span></span></td><td>"#,
            escape(label_name),
            color,
            color
        );
        if let Some(key) = data.label_tokens.delete.get(label_name) {
            let _ = write!(
                body,
                r#"<form method="post" action="{labels_action_url}"><input type="hidden" name="action" value="ticket.label.delete"><input type="hidden" name="name" value="{}"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="return_query" value="{}"><button class="button ghost" type="submit">{}</button></form>"#,
                escape(label_name),
                escape(etag),
                escape(key),
                escape(&data.return_query),
                l.text(keys::TICKET_TRANSCRIPTS_DELETE_LABEL)
            );
        }
        body.push_str("</td></tr>");
    }
    body.push_str("</tbody></table></div>");
}

struct TranscriptRowDisplay {
    ticket_id: String,
    opener: String,
    rating: String,
    closing_staff: String,
    claimed: String,
    panel: String,
    closure_time: String,
    reason_text: String,
    labels: Vec<String>,
    detail_url: String,
    can_view: bool,
    channel_id: String,
    channel_safe: bool,
}

fn transcript_row_display(
    l: Locale<'_>,
    id: &str,
    data: &TranscriptPageData,
    ticket: &Value,
) -> TranscriptRowDisplay {
    let number = ticket["number"].as_u64().unwrap_or(0);
    let channel_id = ticket["channel_id"].as_str().unwrap_or("");
    let can_view =
        ticket["has_transcript"].as_bool() == Some(true) && session::snowflake(channel_id);
    let detail_url = if can_view {
        transcript_detail_url(id, channel_id, &data.return_query)
    } else {
        String::new()
    };
    let ticket_id = if can_view {
        format!(
            r#"<a href="{}">#{}</a>"#,
            escape(&detail_url),
            group_digits(number)
        )
    } else {
        format!("#{}", group_digits(number))
    };
    let opener = ticket["opener_username"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map_or_else(
            || {
                ticket["opener"]
                    .as_str()
                    .map_or_else(|| "—".to_owned(), id_text)
            },
            escape,
        );
    let rating = ticket["rating"]["score"]
        .as_u64()
        .filter(|score| (1..=5).contains(score))
        .and_then(|score| {
            usize::try_from(score)
                .ok()
                .map(|stars| format!("{} {score}/5", "★".repeat(stars)))
        })
        .unwrap_or_else(|| l.text(keys::TICKET_TRANSCRIPTS_NO_RATING));
    let closing_staff = ticket["closer"]
        .as_str()
        .map_or_else(|| "—".to_owned(), id_text);
    let claimed = ticket["claimed_by"]
        .as_str()
        .map_or_else(|| "—".to_owned(), id_text);
    let panel =
        panel_display(ticket, &data.panels).map_or_else(|| "—".to_owned(), |name| escape(&name));
    let closure_time = ticket["closed"].as_i64().map_or_else(
        || "—".to_owned(),
        |time| l.relative_time(time, session::now()),
    );
    let reason = ticket["reason"].as_str().unwrap_or("");
    let reason_text = if reason.trim().is_empty() {
        l.text(keys::TICKET_TRANSCRIPTS_NO_REASON)
    } else {
        escape(reason)
    };
    let labels = ticket["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    TranscriptRowDisplay {
        ticket_id,
        opener,
        rating,
        closing_staff,
        claimed,
        panel,
        closure_time,
        reason_text,
        labels,
        detail_url,
        can_view,
        channel_id: channel_id.to_owned(),
        channel_safe: session::snowflake(channel_id),
    }
}

struct TranscriptFormContext<'a> {
    action_url: &'a str,
    channel_id: &'a str,
    etag: &'a str,
    reason: &'a str,
    labels_json: &'a str,
    return_query: &'a str,
}

fn render_transcript_records(
    body: &mut String,
    l: Locale<'_>,
    id: &str,
    action_url: &str,
    data: &TranscriptPageData,
) {
    body.push_str(r#"<div class="table-scroll"><table><thead><tr>"#);
    for (column, key) in TRANSCRIPT_COLUMNS {
        if column_visible(data, column) {
            let _ = write!(body, "<th>{}</th>", l.text(key));
        }
    }
    body.push_str("</tr></thead><tbody>");
    for ticket in &data.records {
        render_transcript_row(body, l, id, action_url, data, ticket);
    }
    body.push_str("</tbody></table></div>");
    if data.records.is_empty() {
        let _ = write!(
            body,
            r#"<p class="empty">{}</p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_NO_RESULTS)
        );
    }
}

fn render_transcript_row(
    body: &mut String,
    l: Locale<'_>,
    id: &str,
    action_url: &str,
    data: &TranscriptPageData,
    ticket: &Value,
) {
    let display = transcript_row_display(l, id, data, ticket);
    body.push_str("<tr>");
    render_transcript_cells(body, data, &display);
    render_transcript_actions(body, l, action_url, data, ticket, &display);
    body.push_str("</tr>");
}

fn render_transcript_cells(
    body: &mut String,
    data: &TranscriptPageData,
    display: &TranscriptRowDisplay,
) {
    if column_visible(data, "ticket_id") {
        let _ = write!(body, "<td>{}</td>", display.ticket_id);
    }
    if column_visible(data, "username") {
        let _ = write!(body, "<td>{}</td>", display.opener);
    }
    if column_visible(data, "rating") {
        let _ = write!(body, "<td>{}</td>", escape(&display.rating));
    }
    if column_visible(data, "closed_by") {
        let _ = write!(body, "<td>{}</td>", display.closing_staff);
    }
    if column_visible(data, "claimed_by") {
        let _ = write!(body, "<td>{}</td>", display.claimed);
    }
    if column_visible(data, "panel") {
        let _ = write!(body, "<td>{}</td>", display.panel);
    }
    if column_visible(data, "closed_at") {
        let _ = write!(body, "<td>{}</td>", escape(&display.closure_time));
    }
    if column_visible(data, "reason") {
        let _ = write!(body, "<td>{}</td>", display.reason_text);
    }
    if column_visible(data, "labels") {
        body.push_str("<td>");
        for label in &display.labels {
            let color = data
                .labels
                .iter()
                .find(|definition| {
                    definition["name"]
                        .as_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case(label))
                })
                .map_or_else(|| "#808080".to_owned(), label_color);
            let _ = write!(
                body,
                r#"<span class="pill"><input class="label-swatch" type="color" value="{}" disabled aria-hidden="true">{}</span> "#,
                color,
                escape(label)
            );
        }
        body.push_str("</td>");
    }
}

fn render_transcript_actions(
    body: &mut String,
    l: Locale<'_>,
    action_url: &str,
    data: &TranscriptPageData,
    ticket: &Value,
    display: &TranscriptRowDisplay,
) {
    if !column_visible(data, "actions") {
        return;
    }
    body.push_str("<td>");
    if display.can_view {
        let _ = write!(
            body,
            r#"<a class="button secondary" href="{}">{}</a>"#,
            escape(&display.detail_url),
            l.text(keys::TICKET_TRANSCRIPTS_VIEW)
        );
    } else {
        let _ = write!(
            body,
            r#"<span class="settings-help">{}</span>"#,
            l.text(keys::TICKET_TRANSCRIPTS_NO_TRANSCRIPT)
        );
    }
    if display.channel_safe
        && let Some(tokens) = data.row_tokens.get(&display.channel_id)
        && let Some(revision) = ticket["revision"].as_i64().filter(|revision| *revision > 0)
    {
        let etag = format!("\"{revision}\"");
        let labels_json = serde_json::to_string(&display.labels).unwrap_or_default();
        let reason = ticket["reason"].as_str().unwrap_or("");
        let form = TranscriptFormContext {
            action_url,
            channel_id: &display.channel_id,
            etag: &etag,
            reason,
            labels_json: &labels_json,
            return_query: &data.return_query,
        };
        render_reason_form(body, l, &form, &tokens.reason);
        render_label_forms(body, l, &form, tokens, &display.labels, &data.labels);
    }
    body.push_str("</td>");
}

fn render_transcript_pagination(
    body: &mut String,
    l: Locale<'_>,
    id: &str,
    data: &TranscriptPageData,
) {
    if data.before.is_some() {
        let mut previous_cursors = data.previous.clone();
        let previous_before = previous_cursors.pop();
        let previous_cursor = previous_before
            .as_deref()
            .filter(|cursor| !cursor.is_empty());
        let previous = query_url(
            id,
            &data.filters,
            previous_cursor,
            &previous_cursors,
            data.label_cursor.as_deref(),
            false,
            false,
        );
        let key = if previous_before.is_some() {
            keys::TICKET_TRANSCRIPTS_PREVIOUS
        } else {
            keys::TICKET_TRANSCRIPTS_FIRST
        };
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{previous}">{}</a></p>"#,
            l.text(key)
        );
    }
    if let Some(before) = data.next_before.as_deref() {
        let mut previous_cursors = data.previous.clone();
        previous_cursors.push(data.before.clone().unwrap_or_default());
        if previous_cursors.len() > 100 {
            previous_cursors.remove(0);
        }
        let next = query_url(
            id,
            &data.filters,
            Some(before),
            &previous_cursors,
            data.label_cursor.as_deref(),
            false,
            false,
        );
        let _ = write!(
            body,
            r#"<p><a class="button secondary" href="{next}">{}</a></p>"#,
            l.text(keys::TICKET_TRANSCRIPTS_NEXT)
        );
    }
}

fn column_visible(data: &TranscriptPageData, column: &str) -> bool {
    data.filters.columns.is_empty()
        || data
            .filters
            .columns
            .iter()
            .any(|selected| selected == column)
}

fn column_selector(body: &mut String, l: Locale<'_>, id: &str, data: &TranscriptPageData) {
    let list_url = format!("/g/{}/transcripts", escape(id));
    let _ = write!(
        body,
        r#"<details class="settings-card"><summary>{}</summary><form method="get" action="{list_url}" data-transcript-autosubmit>"#,
        l.text(keys::TICKET_TRANSCRIPTS_COLUMNS)
    );
    hidden_query_fields(body, data);
    body.push_str(r#"<div class="tag-list">"#);
    for (column, key) in TRANSCRIPT_COLUMNS {
        let checked = if column_visible(data, column) {
            " checked"
        } else {
            ""
        };
        let _ = write!(
            body,
            r#"<label class="pill"><input type="checkbox" name="column" value="{column}"{checked}>{}</label>"#,
            l.text(key)
        );
    }
    let _ = write!(
        body,
        r#"</div><button class="button secondary" type="submit">{}</button></form></details>"#,
        l.text(keys::TICKET_TRANSCRIPTS_FILTER)
    );
}

fn hidden_query_fields(body: &mut String, data: &TranscriptPageData) {
    for (name, value) in [
        ("ticket_id", data.filters.ticket_id.as_deref()),
        ("username", data.filters.username.as_deref()),
        ("user_id", data.filters.user_id.as_deref()),
        ("closed_by", data.filters.closed_by.as_deref()),
        ("claimed_by", data.filters.claimed_by.as_deref()),
        ("panel", data.filters.panel.as_deref()),
        ("rating", data.filters.rating.as_deref()),
        ("close_reason", data.filters.close_reason.as_deref()),
    ] {
        if let Some(value) = value {
            let _ = write!(
                body,
                r#"<input type="hidden" name="{name}" value="{}">"#,
                escape(value)
            );
        }
    }
    for label in &data.filters.labels {
        let _ = write!(
            body,
            r#"<input type="hidden" name="label" value="{}">"#,
            escape(label)
        );
    }
    if let Some(before) = data.before.as_deref() {
        let _ = write!(
            body,
            r#"<input type="hidden" name="before" value="{}">"#,
            escape(before)
        );
    }
    for previous in &data.previous {
        let _ = write!(
            body,
            r#"<input type="hidden" name="previous" value="{}">"#,
            escape(previous)
        );
    }
    if let Some(cursor) = data.label_cursor.as_deref() {
        let _ = write!(
            body,
            r#"<input type="hidden" name="label_cursor" value="{}">"#,
            escape(cursor)
        );
    }
    if data.manage_open {
        body.push_str(r#"<input type="hidden" name="manage_labels" value="1">"#);
    }
}

fn text_filter(
    body: &mut String,
    l: Locale<'_>,
    label_key: &str,
    name: &str,
    value: Option<&str>,
    input_type: &str,
) {
    let value = value.map(escape).unwrap_or_default();
    let _ = write!(
        body,
        r#"<label class="field"><span>{}</span><input type="{input_type}" name="{name}" value="{value}"></label>"#,
        l.text(label_key)
    );
}

fn manager_form(
    body: &mut String,
    action_url: &str,
    etag: &str,
    key: &str,
    return_query: &str,
    l: Locale<'_>,
) {
    let _ = write!(
        body,
        r##"<form class="settings-card" method="post" action="{action_url}"><input type="hidden" name="action" value="ticket.label.create"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="return_query" value="{}"><label class="field"><span>{}</span><input name="name" type="text" maxlength="100" required></label><label class="field"><span>{}</span><input name="color" type="color" value="#808080" required></label><button class="button" type="submit">{}</button></form>"##,
        escape(etag),
        escape(key),
        escape(return_query),
        l.text(keys::TICKET_TRANSCRIPTS_LABEL_NAME),
        l.text(keys::TICKET_TRANSCRIPTS_LABEL_COLOR),
        l.text(keys::TICKET_TRANSCRIPTS_CREATE_LABEL)
    );
}

fn render_reason_form(
    body: &mut String,
    l: Locale<'_>,
    form: &TranscriptFormContext<'_>,
    key: &str,
) {
    let _ = write!(
        body,
        r#"<details class="settings-card"><summary>{}</summary><form method="post" action="{}"><input type="hidden" name="channel_id" value="{}"><input type="hidden" name="action" value="ticket.history.update"><input type="hidden" name="intent" value="reason"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="labels_json" value="{}"><input type="hidden" name="return_query" value="{}"><label class="field"><span>{}</span><textarea name="reason" maxlength="1000" rows="2">{}</textarea></label><button class="button secondary" type="submit">{}</button></form></details>"#,
        l.text(keys::TICKET_TRANSCRIPTS_EDIT_REASON),
        escape(form.action_url),
        escape(form.channel_id),
        escape(form.etag),
        escape(key),
        escape(form.labels_json),
        escape(form.return_query),
        l.text(keys::TICKET_TRANSCRIPTS_CLOSE_REASON),
        escape(form.reason),
        l.text(keys::TICKET_TRANSCRIPTS_SAVE)
    );
}

fn render_label_forms(
    body: &mut String,
    l: Locale<'_>,
    form: &TranscriptFormContext<'_>,
    tokens: &TranscriptActionTokens,
    current: &[String],
    labels: &[Value],
) {
    let _ = write!(
        body,
        r#"<details class="settings-card"><summary>{}</summary>"#,
        l.text(keys::TICKET_TRANSCRIPTS_LABELS)
    );
    let _ = write!(
        body,
        r#"<form method="post" action="{}"><input type="hidden" name="channel_id" value="{}"><input type="hidden" name="action" value="ticket.history.update"><input type="hidden" name="intent" value="label_add"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="reason" value="{}"><input type="hidden" name="labels_json" value="{}"><input type="hidden" name="return_query" value="{}"><label class="field"><span>{}</span><select name="label">"#,
        escape(form.action_url),
        escape(form.channel_id),
        escape(form.etag),
        escape(&tokens.add_label),
        escape(form.reason),
        escape(form.labels_json),
        escape(form.return_query),
        l.text(keys::TICKET_TRANSCRIPTS_ADD_LABEL)
    );
    let mut available = false;
    for label in labels {
        let Some(name) = label["name"].as_str() else {
            continue;
        };
        if current.iter().any(|value| value.eq_ignore_ascii_case(name)) {
            continue;
        }
        available = true;
        let _ = write!(
            body,
            r#"<option value="{}">{}</option>"#,
            escape(name),
            escape(name)
        );
    }
    if !available {
        let _ = write!(
            body,
            r#"<option value="" disabled selected>{}</option>"#,
            l.text(keys::TICKET_TRANSCRIPTS_NO_LABELS)
        );
    }
    let _ = write!(
        body,
        r#"</select></label><button class="button secondary" type="submit"{}>{}</button></form>"#,
        if available { "" } else { " disabled" },
        l.text(keys::TICKET_TRANSCRIPTS_ADD_LABEL)
    );
    if !current.is_empty() {
        let _ = write!(
            body,
            r#"<form method="post" action="{}"><input type="hidden" name="channel_id" value="{}"><input type="hidden" name="action" value="ticket.history.update"><input type="hidden" name="intent" value="label_remove"><input type="hidden" name="etag" value="{}"><input type="hidden" name="key" value="{}"><input type="hidden" name="reason" value="{}"><input type="hidden" name="labels_json" value="{}"><input type="hidden" name="return_query" value="{}"><label class="field"><span>{}</span><select name="label">"#,
            escape(form.action_url),
            escape(form.channel_id),
            escape(form.etag),
            escape(&tokens.remove_label),
            escape(form.reason),
            escape(form.labels_json),
            escape(form.return_query),
            l.text(keys::TICKET_TRANSCRIPTS_REMOVE_LABEL)
        );
        for label in current {
            let _ = write!(
                body,
                r#"<option value="{}">{}</option>"#,
                escape(label),
                escape(label)
            );
        }
        let _ = write!(
            body,
            r#"</select></label><button class="button secondary" type="submit">{}</button></form>"#,
            l.text(keys::TICKET_TRANSCRIPTS_REMOVE_LABEL)
        );
    }
    body.push_str("</details>");
}

fn id_text(id: &str) -> String {
    if id.is_empty() {
        "—".to_owned()
    } else {
        format!(
            r#"<span class="mono" title="{}">{}</span>"#,
            escape(id),
            escape(id)
        )
    }
}

fn panel_id(ticket: &Value) -> Option<&str> {
    ticket["panel"]
        .as_str()
        .or_else(|| ticket["panel"]["id"].as_str())
}

fn panel_name(ticket: &Value) -> Option<String> {
    ticket["panel"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| ticket["panel"]["name"].as_str().map(str::to_owned))
        .or_else(|| ticket["panel"]["id"].as_str().map(str::to_owned))
}

fn panel_display(ticket: &Value, panels: &[Value]) -> Option<String> {
    let id = panel_id(ticket)?;
    panels
        .iter()
        .find(|panel| panel["id"].as_str() == Some(id))
        .and_then(|panel| panel["name"].as_str())
        .map(str::to_owned)
        .or_else(|| panel_name(ticket))
}

fn label_color(label: &Value) -> String {
    let color = label["color"].as_str().unwrap_or("#808080");
    let valid = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        color.to_ascii_lowercase()
    } else {
        "#808080".to_owned()
    }
}

fn transcript_detail_url(id: &str, channel_id: &str, return_query: &str) -> String {
    let return_to = if return_query.is_empty() {
        format!("/g/{id}/transcripts")
    } else {
        format!("/g/{id}/transcripts?{return_query}")
    };
    let query =
        serde_urlencoded::to_string([("state", "closed"), ("return_to", return_to.as_str())])
            .unwrap_or_default();
    format!(
        "/g/{}/tickets/{}?{}",
        escape(id),
        escape(channel_id),
        escape(&query)
    )
}

fn query_url(
    id: &str,
    filters: &TranscriptFilters,
    before: Option<&str>,
    previous: &[String],
    label_cursor: Option<&str>,
    manage: bool,
    updated: bool,
) -> String {
    let query = query_string(filters, before, previous, label_cursor, manage, updated);
    if query.is_empty() {
        format!("/g/{}/transcripts", escape(id))
    } else {
        format!("/g/{}/transcripts?{}", escape(id), escape(&query))
    }
}

fn query_string(
    filters: &TranscriptFilters,
    before: Option<&str>,
    previous: &[String],
    label_cursor: Option<&str>,
    manage: bool,
    updated: bool,
) -> String {
    let mut pairs = Vec::new();
    add_filter(&mut pairs, "ticket_id", filters.ticket_id.as_deref());
    add_filter(&mut pairs, "username", filters.username.as_deref());
    add_filter(&mut pairs, "user_id", filters.user_id.as_deref());
    add_filter(&mut pairs, "closed_by", filters.closed_by.as_deref());
    add_filter(&mut pairs, "claimed_by", filters.claimed_by.as_deref());
    add_filter(&mut pairs, "panel", filters.panel.as_deref());
    add_filter(&mut pairs, "rating", filters.rating.as_deref());
    add_filter(&mut pairs, "close_reason", filters.close_reason.as_deref());
    for label in &filters.labels {
        pairs.push(("label".to_owned(), label.clone()));
    }
    for column in &filters.columns {
        pairs.push(("column".to_owned(), column.clone()));
    }
    add_filter(&mut pairs, "before", before);
    for cursor in previous {
        pairs.push(("previous".to_owned(), cursor.clone()));
    }
    add_filter(&mut pairs, "label_cursor", label_cursor);
    if manage {
        pairs.push(("manage_labels".to_owned(), "1".to_owned()));
    }
    if updated {
        pairs.push(("updated".to_owned(), "1".to_owned()));
    }
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

fn add_filter(pairs: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        pairs.push((key.to_owned(), value.to_owned()));
    }
}
