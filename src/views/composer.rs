//! The composer: a form for a message with embeds, and the preview that shows it as Discord will.

use super::shell;
use crate::{
    composer::{self, COLORS, Form, MAX_EMBEDS, MAX_FIELDS},
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

/// A link that may be shown: only the web.
fn web(url: &str) -> Option<&str> {
    let url = url.trim();
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

fn get<'a>(form: &'a Form, name: &str) -> &'a str {
    form.get(name).map_or("", String::as_str)
}

/// The text with the variables of the setting replaced by an example, as the preview shows them.
fn fill(text: &str, samples: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (name, sample) in samples {
        out = out.replace(&format!("{{{name}}}"), sample);
    }
    out
}

/// Wraps what stands between two marks, as Discord does for `**bold**`.
fn wrap(text: &str, mark: &str, open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(mark) {
        let after = &rest[start + mark.len()..];
        match after.find(mark) {
            Some(end) if end > 0 => {
                out.push_str(&rest[..start]);
                out.push_str(open);
                out.push_str(&after[..end]);
                out.push_str(close);
                rest = &after[end + mark.len()..];
            }
            _ => {
                out.push_str(&rest[..start + mark.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Mentions and links, in text that is already escaped.
fn mentions_and_links(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("&lt;") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 4..];
        let kind = if tail.starts_with("@&amp;") {
            Some(("@&amp;", "ロール"))
        } else if tail.starts_with('@') {
            Some(("@", "ユーザー"))
        } else if tail.starts_with('#') {
            Some(("#", "チャンネル"))
        } else {
            None
        };
        let parsed = kind.and_then(|(sign, name)| {
            let digits = &tail[sign.len()..];
            let end = digits.find("&gt;")?;
            (end > 0 && digits[..end].bytes().all(|b| b.is_ascii_digit())).then_some((
                sign.chars().next().unwrap_or('@'),
                name,
                start + 4 + sign.len() + end + 4,
            ))
        });
        if let Some((sign, name, consumed)) = parsed {
            let _ = write!(out, "<span class=\"dc-mention\">{sign}{name}</span>");
            rest = &rest[consumed..];
        } else {
            out.push_str("&lt;");
            rest = tail;
        }
    }
    out.push_str(rest);
    link(&out)
}

fn link(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("https://").or_else(|| rest.find("http://")) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '<')
            .unwrap_or(tail.len());
        let _ = write!(
            out,
            "<a class=\"dc-link\" href=\"{0}\">{0}</a>",
            &tail[..end]
        );
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Discord's text styling, for the preview: bold, italic, underline, strike, code, links.
fn markdown(source: &str) -> String {
    let escaped = escape(source);
    let mut out = String::new();
    for (n, part) in escaped.split('`').enumerate() {
        if n % 2 == 1 {
            let _ = write!(out, "<code class=\"dc-code\">{part}</code>");
            continue;
        }
        let styled = wrap(part, "~~", "<s>", "</s>");
        let styled = wrap(&styled, "**", "<strong>", "</strong>");
        let styled = wrap(&styled, "__", "<u>", "</u>");
        let styled = wrap(&styled, "*", "<em>", "</em>");
        out.push_str(&mentions_and_links(&styled));
    }
    out.replace('\n', "<br>")
}

/// Whether an embed has something to show; its colour alone is not enough.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn has_content(form: &Form, n: usize) -> bool {
    let prefix = format!("e.{n}.");
    form.iter().any(|(name, value)| {
        name.starts_with(&prefix)
            && !value.trim().is_empty()
            && !name.ends_with(".__")
            && !name.ends_with(".color")
            && !name.ends_with(".color_custom")
            && !name.ends_with(".inline")
    })
}

#[allow(clippy::too_many_lines)]
fn embed_preview(form: &Form, n: usize, samples: &[(String, String)], now: &str) -> String {
    let e = format!("e.{n}");
    let text = |key: &str| fill(get(form, &format!("{e}.{key}")), samples);
    let color = match get(form, &format!("{e}.color")) {
        "custom" => get(form, &format!("{e}.color_custom")),
        other => other,
    };
    let color = if color.len() == 7 && color.starts_with('#') {
        color
    } else {
        "#4e5058"
    };
    let mut main = String::new();
    let author = text("author_name");
    if !author.is_empty() {
        let icon = web(get(form, &format!("{e}.author_icon"))).map_or_else(String::new, |u| {
            format!("<img class=\"dc-icon\" alt=\"\" src=\"{}\">", escape(u))
        });
        let name = match web(get(form, &format!("{e}.author_url"))) {
            Some(u) => format!(
                "<a class=\"dc-link\" href=\"{}\">{}</a>",
                escape(u),
                escape(&author)
            ),
            None => escape(&author),
        };
        let _ = write!(main, "<div class=\"dc-author\">{icon}{name}</div>");
    }
    let title = text("title");
    if !title.is_empty() {
        let inner = match web(get(form, &format!("{e}.url"))) {
            Some(u) => format!(
                "<a class=\"dc-link\" href=\"{}\">{}</a>",
                escape(u),
                escape(&title)
            ),
            None => escape(&title),
        };
        let _ = write!(main, "<div class=\"dc-title\">{inner}</div>");
    }
    let description = text("description");
    if !description.is_empty() {
        let _ = write!(
            main,
            "<div class=\"dc-description\">{}</div>",
            markdown(&description)
        );
    }
    let mut fields = String::new();
    for m in composer::numbers(form, &format!("{e}.f")) {
        let f = format!("{e}.f.{m}");
        let (name, value) = (
            fill(get(form, &format!("{f}.name")), samples),
            fill(get(form, &format!("{f}.value")), samples),
        );
        if name.is_empty() && value.is_empty() {
            continue;
        }
        let _ = write!(
            fields,
            "<div class=\"dc-field{}\"><div class=\"dc-field-name\">{}</div><div>{}</div></div>",
            if form.contains_key(&format!("{f}.inline")) {
                " inline"
            } else {
                ""
            },
            escape(&name),
            markdown(&value)
        );
    }
    if !fields.is_empty() {
        let _ = write!(main, "<div class=\"dc-fields\">{fields}</div>");
    }
    if let Some(image) = web(get(form, &format!("{e}.image"))) {
        let _ = write!(
            main,
            "<img class=\"dc-image\" alt=\"\" src=\"{}\">",
            escape(image)
        );
    }
    let footer = text("footer_text");
    let stamp = form.contains_key(&format!("{e}.timestamp"));
    if !footer.is_empty() || stamp {
        let icon = web(get(form, &format!("{e}.footer_icon")))
            .filter(|_| !footer.is_empty())
            .map_or_else(String::new, |u| {
                format!(
                    "<img class=\"dc-icon small\" alt=\"\" src=\"{}\">",
                    escape(u)
                )
            });
        let when = if stamp {
            format!("{}{now}", if footer.is_empty() { "" } else { " • " })
        } else {
            String::new()
        };
        let _ = write!(
            main,
            "<div class=\"dc-footer\">{icon}<span>{}{}</span></div>",
            escape(&footer),
            escape(&when)
        );
    }
    let thumb = web(get(form, &format!("{e}.thumbnail"))).map_or_else(String::new, |u| {
        format!("<img class=\"dc-thumb\" alt=\"\" src=\"{}\">", escape(u))
    });
    format!(
        "<div class=\"dc-embed\"><svg class=\"dc-bar\" aria-hidden=\"true\" viewBox=\"0 0 4 10\" preserveAspectRatio=\"none\"><rect width=\"4\" height=\"10\" fill=\"{}\"/></svg><div class=\"dc-embed-body\"><div class=\"dc-embed-main\">{main}</div>{thumb}</div></div>",
        escape(color)
    )
}

/// The message as Discord shows it. `samples` are the examples the variables stand for.
pub(crate) fn preview(l: Locale<'_>, form: &Form, samples: &[(String, String)]) -> String {
    let now = l.text(keys::CP_NOW);
    let content = fill(get(form, "c"), samples);
    let mut out = format!(
        "<div class=\"dc-message\"><div class=\"dc-avatar\" aria-hidden=\"true\">C</div><div class=\"dc-body\"><div class=\"dc-head\"><span class=\"dc-name\">Closure</span><span class=\"dc-tag\">BOT</span><span class=\"dc-time\">{now}</span></div>"
    );
    if !content.trim().is_empty() {
        let _ = write!(
            out,
            "<div class=\"dc-content\">{}</div>",
            markdown(&content)
        );
    }
    let mut shown = 0;
    for n in composer::numbers(form, "e") {
        if has_content(form, n) {
            shown += 1;
            out.push_str(&embed_preview(form, n, samples, &now));
        }
    }
    if content.trim().is_empty() && shown == 0 {
        let _ = write!(
            out,
            "<div class=\"dc-empty\">{}</div>",
            l.text(keys::CP_EMPTY)
        );
    }
    let buttons = composer::parse(form)["buttons"].clone();
    if let Some(buttons) = buttons.as_array() {
        out.push_str("<div class=\"dc-buttons\">");
        for button in buttons {
            let _ = write!(
                out,
                "<span class=\"dc-button\">{}</span>",
                escape(button["label"].as_str().unwrap_or(""))
            );
        }
        out.push_str("</div>");
    }
    out.push_str("</div></div>");
    let size = composer::parse(form)["embeds"]
        .as_array()
        .into_iter()
        .flatten()
        .map(embed_size)
        .sum::<usize>();
    let _ = write!(
        out,
        "<p class=\"cp-count{}\">{}</p>",
        if size > 6000 { " over" } else { "" },
        l.with_texts(keys::CP_COUNT, &[("n", &size.to_string()), ("max", "6000")])
    );
    out
}

/// How many characters of an embed Discord counts toward its limit of 6000.
fn embed_size(embed: &Value) -> usize {
    let count = |v: &Value| v.as_str().map_or(0, |s| s.chars().count());
    count(&embed["title"])
        + count(&embed["description"])
        + count(&embed["author"]["name"])
        + count(&embed["footer"]["text"])
        + embed["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| count(&f["name"]) + count(&f["value"]))
            .sum::<usize>()
}

fn input(name: &str, form: &Form, label: &str, kind: &str, extra: &str) -> String {
    format!(
        "<label class=\"cp-input\"><span>{label}</span><input type=\"{kind}\" name=\"{}\" value=\"{}\"{extra}></label>",
        escape(name),
        escape(get(form, name))
    )
}

/// The rows of role buttons, and the button that adds one.
fn buttons_card(l: Locale<'_>, form: &Form, roles: &[(String, String)]) -> String {
    let mut out = format!(
        "<section class=\"card cp-section\"><h3>{}</h3><p class=\"cp-hint\">{}</p>",
        l.text(keys::CP_BUTTONS),
        l.text(keys::CP_BUTTONS_HINT)
    );
    let rows = composer::numbers(form, "b");
    for n in &rows {
        let chosen = get(form, &format!("b.{n}.role"));
        let mut select = format!(
            "<select name=\"b.{n}.role\" aria-label=\"{}\"><option value=\"\">{}</option>",
            l.text(keys::CP_BUTTON_ROLE),
            l.text(keys::UNSET)
        );
        for (rid, name) in roles {
            let _ = write!(
                select,
                "<option value=\"{}\"{}>{}</option>",
                escape(rid),
                if rid == chosen { " selected" } else { "" },
                escape(name)
            );
        }
        select.push_str("</select>");
        let _ = write!(
            out,
            "<div class=\"cp-row\"><input type=\"hidden\" name=\"b.{n}.__\" value=\"1\"><input type=\"text\" name=\"b.{n}.label\" value=\"{}\" maxlength=\"80\" placeholder=\"{}\" aria-label=\"{}\">{select}<button class=\"button ghost\" type=\"submit\" name=\"op\" value=\"del:b.{n}\" aria-label=\"{}\">✕</button></div>",
            escape(get(form, &format!("b.{n}.label"))),
            l.text(keys::CP_BUTTON_LABEL),
            l.text(keys::CP_BUTTON_LABEL),
            l.text(keys::CP_REMOVE)
        );
    }
    if rows.len() < composer::MAX_BUTTONS {
        let _ = write!(
            out,
            "<button class=\"button secondary cp-add\" type=\"submit\" name=\"op\" value=\"add_button\">＋ {}</button>",
            l.text(keys::CP_ADD_BUTTON)
        );
    }
    out.push_str("</section>");
    out
}

/// The posts waiting for their time, each with a button that takes it back.
fn waiting_card(l: Locale<'_>, waiting: &[Waiting]) -> String {
    let mut out = format!(
        "<section class=\"card cp-section\"><h3>{}</h3>",
        l.text(keys::CP_SCHEDULED)
    );
    if waiting.is_empty() {
        let _ = write!(
            out,
            "<p class=\"cp-hint\">{}</p>",
            l.text(keys::CP_NONE_WAITING)
        );
    }
    for post in waiting {
        let _ = write!(
            out,
            "<div class=\"cp-row cp-waiting\"><span class=\"cp-when\">{}</span><span class=\"cp-where\"># {}</span><span class=\"cp-what\">{}</span><button class=\"button ghost\" type=\"submit\" name=\"op\" value=\"unschedule:{}\">{}</button></div>",
            escape(&post.when),
            escape(&post.channel),
            escape(&post.preview),
            escape(&post.key),
            l.text(keys::CP_UNSCHEDULE)
        );
    }
    out.push_str("</section>");
    out
}

fn swatches(l: Locale<'_>, form: &Form, e: &str) -> String {
    let name = format!("{e}.color");
    let current = get(form, &name);
    let custom_value = get(form, &format!("{e}.color_custom"));
    let preset = COLORS.iter().any(|(hex, _)| *hex == current);
    let custom = current == "custom" || (!current.is_empty() && !preset);
    let custom_hex = if current == "custom" {
        custom_value
    } else {
        current
    };
    let custom_hex = if custom_hex.len() == 7 {
        custom_hex
    } else {
        "#5865f2"
    };
    let checked = |on: bool| if on { " checked" } else { "" };
    let mut out = format!(
        "<div class=\"cp-colors\" role=\"radiogroup\" aria-label=\"{}\"><label class=\"cp-swatch none\" title=\"{}\"><input type=\"radio\" name=\"{}\" value=\"\"{}><svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><circle cx=\"10\" cy=\"10\" r=\"8\"/><path d=\"M5 15L15 5\"/></svg></label>",
        l.text(keys::CP_COLOR),
        l.text(keys::CP_NO_COLOR),
        escape(&name),
        checked(current.is_empty())
    );
    for (hex, key) in COLORS {
        let _ = write!(
            out,
            "<label class=\"cp-swatch\" title=\"{}\"><input type=\"radio\" name=\"{}\" value=\"{hex}\"{}><svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><circle cx=\"10\" cy=\"10\" r=\"9\" fill=\"{hex}\"/></svg></label>",
            l.text(&format!("ed-color-{key}")),
            escape(&name),
            checked(current == hex)
        );
    }
    let _ = write!(
        out,
        "<label class=\"cp-swatch custom\" title=\"{}\"><input type=\"radio\" name=\"{}\" value=\"custom\"{}><input type=\"color\" name=\"{}\" value=\"{}\"></label></div>",
        l.text(keys::CP_CUSTOM_COLOR),
        escape(&name),
        checked(custom),
        escape(&format!("{e}.color_custom")),
        escape(custom_hex)
    );
    out
}

#[allow(clippy::too_many_lines)]
fn embed_card(l: Locale<'_>, form: &Form, n: usize) -> String {
    let e = format!("e.{n}");
    let field = |key: &str| format!("{e}.{key}");
    let mut out = format!(
        "<section class=\"card cp-embed\"><header><h3>{}</h3><button class=\"icon-button\" type=\"submit\" name=\"op\" value=\"del:{}\" aria-label=\"{}\" title=\"{0}\">✕</button></header><input type=\"hidden\" name=\"{}\" value=\"1\">",
        l.with_texts(keys::CP_EMBED_N, &[("n", &(n + 1).to_string())]),
        escape(&e),
        l.text(keys::ED_REMOVE),
        escape(&field("__"))
    );
    out.push_str(&swatches(l, form, &e));
    let open = |names: &[&str]| {
        if names
            .iter()
            .any(|k| !get(form, &field(k)).is_empty() || form.contains_key(&field(k)))
        {
            " open"
        } else {
            ""
        }
    };
    let _ = write!(
        out,
        "<details class=\"cp-fold\"{}><summary>{}</summary><div class=\"cp-row\">{}{}{}</div></details>",
        open(&["author_name", "author_url", "author_icon"]),
        l.text(keys::CP_AUTHOR),
        input(
            &field("author_name"),
            form,
            &l.text(keys::CP_NAME),
            "text",
            " maxlength=\"256\""
        ),
        input(
            &field("author_url"),
            form,
            &l.text(keys::CP_LINK),
            "url",
            ""
        ),
        input(
            &field("author_icon"),
            form,
            &l.text(keys::CP_ICON),
            "url",
            ""
        )
    );
    let _ = write!(
        out,
        "<div class=\"cp-row\">{}{}</div>",
        input(
            &field("title"),
            form,
            &l.text(keys::CP_TITLE),
            "text",
            " maxlength=\"256\""
        ),
        input(&field("url"), form, &l.text(keys::CP_TITLE_LINK), "url", "")
    );
    let _ = write!(
        out,
        "<label class=\"cp-input\"><span>{}</span><textarea name=\"{}\" rows=\"5\" maxlength=\"4096\">{}</textarea></label>",
        l.text(keys::CP_DESCRIPTION),
        escape(&field("description")),
        escape(get(form, &field("description")))
    );
    let rows = composer::numbers(form, &format!("{e}.f"));
    let _ = write!(
        out,
        "<div class=\"cp-fields\"><h4>{}</h4>",
        l.text(keys::CP_FIELDS)
    );
    for m in &rows {
        let f = format!("{e}.f.{m}");
        let _ = write!(
            out,
            "<div class=\"cp-field\"><input type=\"hidden\" name=\"{marker}\" value=\"1\"><input type=\"text\" name=\"{name}\" value=\"{name_v}\" placeholder=\"{p_name}\" maxlength=\"256\"><textarea name=\"{value}\" rows=\"2\" placeholder=\"{p_value}\" maxlength=\"1024\">{value_v}</textarea><label class=\"cp-inline\"><input type=\"checkbox\" name=\"{inline}\" value=\"1\"{checked}>{inline_label}</label><button class=\"icon-button\" type=\"submit\" name=\"op\" value=\"del:{row}\" aria-label=\"{remove}\" title=\"{remove}\">✕</button></div>",
            marker = escape(&format!("{f}.__")),
            name = escape(&format!("{f}.name")),
            name_v = escape(get(form, &format!("{f}.name"))),
            p_name = l.text(keys::CP_FIELD_NAME),
            value = escape(&format!("{f}.value")),
            p_value = l.text(keys::CP_FIELD_VALUE),
            value_v = escape(get(form, &format!("{f}.value"))),
            inline = escape(&format!("{f}.inline")),
            checked = if form.contains_key(&format!("{f}.inline")) {
                " checked"
            } else {
                ""
            },
            inline_label = l.text(keys::CP_INLINE),
            row = escape(&f),
            remove = l.text(keys::ED_REMOVE)
        );
    }
    if rows.len() < MAX_FIELDS {
        let _ = write!(
            out,
            "<button class=\"button secondary\" type=\"submit\" name=\"op\" value=\"addf:{n}\">＋ {}</button>",
            l.text(keys::CP_ADD_FIELD)
        );
    }
    out.push_str("</div>");
    let _ = write!(
        out,
        "<details class=\"cp-fold\"{}><summary>{}</summary><div class=\"cp-row\">{}{}</div></details>",
        open(&["thumbnail", "image"]),
        l.text(keys::CP_IMAGES),
        input(
            &field("thumbnail"),
            form,
            &l.text(keys::CP_THUMBNAIL),
            "url",
            ""
        ),
        input(&field("image"), form, &l.text(keys::CP_IMAGE), "url", "")
    );
    let _ = write!(
        out,
        "<details class=\"cp-fold\"{}><summary>{}</summary><div class=\"cp-row\">{}{}<label class=\"cp-inline\"><input type=\"checkbox\" name=\"{}\" value=\"1\"{}>{}</label></div></details></section>",
        open(&["footer_text", "footer_icon", "timestamp"]),
        l.text(keys::CP_FOOTER),
        input(
            &field("footer_text"),
            form,
            &l.text(keys::CP_TEXT),
            "text",
            " maxlength=\"2048\""
        ),
        input(
            &field("footer_icon"),
            form,
            &l.text(keys::CP_ICON),
            "url",
            ""
        ),
        escape(&field("timestamp")),
        if form.contains_key(&field("timestamp")) {
            " checked"
        } else {
            ""
        },
        l.text(keys::CP_TIMESTAMP)
    );
    out
}

/// A post that waits for its time, as the list shows it.
pub(crate) struct Waiting {
    pub key: String,
    pub when: String,
    pub channel: String,
    pub preview: String,
}

/// What the composer shows apart from the message: where it goes, or what its variables are.
pub(crate) enum Side<'a> {
    /// A message to send now: the text channels to choose from, and the one chosen.
    Send {
        channels: Vec<(String, String)>,
        roles: Vec<(String, String)>,
        chosen: &'a str,
        /// What was typed to name the message to replace; empty to send a new one.
        edit: &'a str,
        /// The `datetime-local` value of the time to send at.
        when: &'a str,
        /// How many minutes the browser is behind UTC, as it said.
        zone: &'a str,
        /// The one-time token of this form: a second post with the same token is not sent again.
        nonce: &'a str,
        waiting: Vec<Waiting>,
    },
    /// A setting that holds a message: the variables it may use, with their examples.
    Setting { variables: &'a [String] },
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn composer_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    guild: &str,
    title: &str,
    back: Option<&str>,
    action: &str,
    form: &Form,
    side: &Side<'_>,
    samples: &[(String, String)],
    banners: &[(bool, String)],
) -> String {
    let mut body = format!("<header class=\"page-heading\"><h1>{title}</h1></header>");
    for (good, text) in banners {
        let _ = write!(
            body,
            "<p class=\"{}\" role=\"{}\">{}</p>",
            if *good { "form-status" } else { "form-error" },
            if *good { "status" } else { "alert" },
            text
        );
    }
    let mut left = String::new();
    let mut pending = None;
    match side {
        Side::Send {
            channels,
            roles,
            chosen,
            edit,
            when,
            zone,
            nonce,
            waiting,
        } => {
            let mut select = format!(
                "<label class=\"cp-input\"><span>{}</span><select name=\"channel\">",
                l.text(keys::CP_CHANNEL)
            );
            let _ = write!(
                select,
                "<option value=\"\">{}</option>",
                l.text(keys::UNSET)
            );
            for (cid, name) in channels {
                let _ = write!(
                    select,
                    "<option value=\"{}\"{}># {}</option>",
                    escape(cid),
                    if cid == chosen { " selected" } else { "" },
                    escape(name)
                );
            }
            select.push_str("</select></label>");
            let _ = write!(
                left,
                "<section class=\"card cp-section\">{select}<label class=\"cp-input\"><span>{}</span><input type=\"text\" name=\"edit\" value=\"{}\" placeholder=\"https://discord.com/channels/…\" autocomplete=\"off\"><small class=\"cp-hint\">{}</small></label><label class=\"cp-input\"><span>{}</span><input type=\"datetime-local\" name=\"when\" value=\"{}\"><small class=\"cp-hint\">{}</small></label><input type=\"hidden\" name=\"tz\" value=\"{}\"><input type=\"hidden\" name=\"nonce\" value=\"{}\"></section>",
                l.text(keys::CP_EDIT_TARGET),
                escape(edit),
                l.text(keys::CP_EDIT_HINT),
                l.text(keys::CP_WHEN),
                escape(when),
                l.text(keys::CP_WHEN_HINT),
                escape(zone),
                escape(nonce)
            );
            pending = Some((roles, waiting));
        }
        Side::Setting { variables } => {
            let mut chips = String::new();
            for name in *variables {
                let _ = write!(
                    chips,
                    "<span class=\"cp-var\"><code>{{{}}}</code> {}</span>",
                    escape(name),
                    l.find(&format!("ed-var-{name}")).unwrap_or_default()
                );
            }
            if !chips.is_empty() {
                let _ = write!(
                    left,
                    "<section class=\"card cp-section\"><h3>{}</h3><div class=\"cp-vars\">{chips}</div></section>",
                    l.text(keys::CP_VARIABLES)
                );
            }
        }
    }
    let _ = write!(
        left,
        "<section class=\"card cp-section\"><label class=\"cp-input\"><span>{}</span><textarea name=\"c\" rows=\"4\" maxlength=\"2000\">{}</textarea></label></section>",
        l.text(keys::CP_CONTENT),
        escape(get(form, "c"))
    );
    let embeds = composer::numbers(form, "e");
    for n in &embeds {
        left.push_str(&embed_card(l, form, *n));
    }
    if embeds.len() < MAX_EMBEDS {
        let _ = write!(
            left,
            "<button class=\"button secondary cp-add\" type=\"submit\" name=\"op\" value=\"add_embed\">＋ {}</button>",
            l.text(keys::CP_ADD_EMBED)
        );
    }
    if let Some((roles, waiting)) = pending {
        left.push_str(&buttons_card(l, form, roles));
        left.push_str(&waiting_card(l, waiting));
    }
    let schedule = if matches!(side, Side::Send { .. }) {
        format!(
            "<button class=\"button secondary\" type=\"submit\" name=\"op\" value=\"schedule\">{}</button>",
            l.text(keys::CP_SCHEDULE)
        )
    } else {
        String::new()
    };
    let (done_key, done_op) = match side {
        Side::Send { .. } => (keys::CP_SEND, "send"),
        Side::Setting { .. } => (keys::SAVE, "save"),
    };
    let back_link = back.map_or_else(String::new, |b| {
        format!(
            "<a class=\"button ghost\" href=\"{}\">{}</a>",
            escape(b),
            l.text(keys::ED_BACK)
        )
    });
    let _ = write!(
        body,
        "<form method=\"post\" action=\"{action}\" class=\"composer\" data-preview=\"{action}?preview=1\"><button class=\"sr-only\" type=\"submit\" name=\"op\" value=\"{done_op}\" tabindex=\"-1\" aria-hidden=\"true\"></button><div class=\"composer-edit\">{left}</div><aside class=\"composer-side\"><div class=\"cp-preview-label\">{}</div><div id=\"preview\">{}</div></aside><div class=\"save-bar\">{back_link}{schedule}<button class=\"button primary\" type=\"submit\" name=\"op\" value=\"{done_op}\">{}</button></div></form>",
        l.text(keys::CP_PREVIEW),
        preview(l, form, samples),
        l.text(done_key),
        action = escape(action)
    );
    shell(l, headers, id, guild, keys::COMPOSE, &body, false)
}
