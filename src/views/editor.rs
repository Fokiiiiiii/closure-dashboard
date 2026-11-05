//! The pages of the structured editor: rules, counters and anti-raid as cards, not as text.

use super::{form::part_tabs, modules::module_icon, modules::part_name, shell};
use crate::{
    editor::{self, Errors, Node, Prop, State, Variant, Why, join, one, rows},
    form::Field,
    html::escape,
    i18n::{Locale, keys},
};
use axum::http::HeaderMap;
use serde_json::Value;
use std::fmt::Write;

/// What the controls of a page need besides the form.
pub(crate) struct Ctx<'a> {
    pub l: Locale<'a>,
    pub directory: Option<&'a Value>,
    /// Counter names and anti-raid levels, offered as suggestions where one is named.
    pub counters: &'a [String],
    pub levels: &'a [String],
    pub errors: &'a Errors,
}

fn dom_id(path: &str) -> String {
    format!("e-{}", path.replace('.', "-"))
}

/// The caption of a field: its own for the variant, else the shared one, else the name made readable.
fn label(l: Locale<'_>, tag: Option<&str>, name: &str) -> String {
    tag.and_then(|t| l.find(&format!("ed-x-{t}-{name}")))
        .or_else(|| l.find(&format!("ed-p-{name}")))
        .unwrap_or_else(|| escape(&name.replace('_', " ")))
}

fn hint(l: Locale<'_>, tag: Option<&str>, name: &str) -> Option<String> {
    tag.and_then(|t| l.find(&format!("ed-h-{t}-{name}")))
        .or_else(|| l.find(&format!("ed-h-{name}")))
}

fn variant_name(l: Locale<'_>, tag: &str) -> String {
    l.find(&format!("ed-v-{tag}"))
        .unwrap_or_else(|| escape(tag))
}

fn choice_label(l: Locale<'_>, value: &str) -> String {
    l.find(&format!("ed-o-{value}"))
        .unwrap_or_else(|| escape(value))
}

fn problems(ctx: &Ctx<'_>, path: &str) -> String {
    let mut out = String::new();
    for (at, why) in ctx.errors {
        if at == path {
            let key = match why {
                Why::Required => keys::ED_ERR_REQUIRED,
                Why::Invalid => keys::ED_ERR_INVALID,
                Why::Repeated => keys::ED_ERR_REPEATED,
            };
            let _ = write!(
                out,
                "<p class=\"ed-error\" role=\"alert\">⚠ {}</p>",
                ctx.l.text(key)
            );
        }
    }
    out
}

fn field(
    ctx: &Ctx<'_>,
    path: &str,
    name: &str,
    tag: Option<&str>,
    required: bool,
    control: &str,
) -> String {
    let note = hint(ctx.l, tag, name)
        .map_or_else(String::new, |h| format!("<p class=\"ed-hint\">{h}</p>"));
    format!(
        "<div class=\"ed-field\"><label class=\"ed-label\" for=\"{}\">{}{}</label><div class=\"ed-control\">{control}{note}{}</div></div>",
        dom_id(path),
        label(ctx.l, tag, name),
        if required {
            "<span class=\"ed-req\" aria-hidden=\"true\">*</span>"
        } else {
            ""
        },
        problems(ctx, path)
    )
}

fn flag(l: Locale<'_>, path: &str, state: &State) -> String {
    let current = one(state, path);
    let mut out = format!(
        "<div class=\"form-segments\" role=\"radiogroup\" id=\"{}\">",
        dom_id(path)
    );
    for (n, (value, key)) in [
        ("", keys::ED_DEFAULT),
        ("true", keys::ED_ON),
        ("false", keys::ED_OFF),
    ]
    .into_iter()
    .enumerate()
    {
        let _ = write!(
            out,
            "<input type=\"radio\" id=\"{id}-{n}\" name=\"{}\" value=\"{value}\"{}><label for=\"{id}-{n}\">{}</label>",
            escape(path),
            if current == value { " checked" } else { "" },
            l.text(key),
            id = dom_id(path)
        );
    }
    out.push_str("</div>");
    out
}

/// A switch with two positions, for a setting that is on unless it says otherwise.
fn switch(l: Locale<'_>, path: &str, state: &State) -> String {
    let off = one(state, path) == "false";
    let id = dom_id(path);
    let mut out = format!("<div class=\"form-segments\" role=\"radiogroup\" id=\"{id}\">");
    for (n, (value, key)) in [("true", keys::ED_ON), ("false", keys::ED_OFF)]
        .into_iter()
        .enumerate()
    {
        let _ = write!(
            out,
            "<input type=\"radio\" id=\"{id}-{n}\" name=\"{}\" value=\"{value}\"{}><label for=\"{id}-{n}\">{}</label>",
            escape(path),
            if (value == "false") == off {
                " checked"
            } else {
                ""
            },
            l.text(key)
        );
    }
    out.push_str("</div>");
    out
}

fn bounded(n: i64) -> bool {
    n > i64::from(i32::MIN) && n < i64::from(i32::MAX)
}

/// The ids and names of a kind of thing on the server: its text channels, or its roles.
fn directory_items<'a>(ctx: &'a Ctx<'_>, roles: bool) -> Vec<(&'a str, &'a str, u64)> {
    let list = ctx.directory.map_or(&Value::Null, |d| {
        if roles { &d["roles"] } else { &d["channels"] }
    });
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            if roles {
                item["managed"] != true && item["name"] != "@everyone"
            } else {
                item["kind"] == "text"
            }
        })
        .filter_map(|item| {
            Some((
                item["id"].as_str()?,
                item["name"].as_str()?,
                item["color"].as_u64().unwrap_or(0),
            ))
        })
        .collect()
}

fn select(ctx: &Ctx<'_>, path: &str, state: &State, roles: bool) -> String {
    let current = one(state, path);
    let mut out = format!(
        "<select id=\"{}\" name=\"{}\"><option value=\"\">{}</option>",
        dom_id(path),
        escape(path),
        ctx.l.text(keys::UNSET)
    );
    let mut found = current.is_empty();
    for (id, name, _) in directory_items(ctx, roles) {
        found |= id == current;
        let _ = write!(
            out,
            "<option value=\"{}\"{}>{}{}</option>",
            escape(id),
            if id == current { " selected" } else { "" },
            if roles { "" } else { "# " },
            escape(name)
        );
    }
    if !found {
        let _ = write!(
            out,
            "<option value=\"{0}\" selected>{0}</option>",
            escape(current)
        );
    }
    out.push_str("</select>");
    out
}

fn chips(ctx: &Ctx<'_>, path: &str, state: &State, roles: bool) -> String {
    let chosen: Vec<&String> = state.get(path).into_iter().flatten().collect();
    let items = directory_items(ctx, roles);
    let mut out = String::from("<div class=\"role-list ed-chips\" role=\"group\">");
    for (id, name, color) in &items {
        let dot = if roles {
            format!(
                "<svg aria-hidden=\"true\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><circle cx=\"6\" cy=\"6\" r=\"5\" fill=\"{}\"/></svg>",
                super::role_fill(*color)
            )
        } else {
            String::from("#")
        };
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{}\" value=\"{}\"{}>{dot}{}</label>",
            escape(path),
            escape(id),
            if chosen.iter().any(|c| c.as_str() == *id) {
                " checked"
            } else {
                ""
            },
            escape(name)
        );
    }
    for id in chosen
        .iter()
        .filter(|c| !items.iter().any(|(i, _, _)| i == &c.as_str()))
    {
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{name}\" value=\"{id}\" checked>{id}</label>",
            name = escape(path),
            id = escape(id)
        );
    }
    if items.is_empty() && chosen.is_empty() {
        let _ = write!(
            out,
            "<span class=\"muted\">{}</span>",
            ctx.l.text(keys::NONE)
        );
    }
    out.push_str("</div>");
    out
}

fn choices(l: Locale<'_>, path: &str, state: &State, values: &[String]) -> String {
    let chosen: Vec<&String> = state.get(path).into_iter().flatten().collect();
    let mut out = String::from("<div class=\"role-list ed-chips\" role=\"group\">");
    for value in values {
        let _ = write!(
            out,
            "<label class=\"role-option\"><input type=\"checkbox\" name=\"{}\" value=\"{}\"{}>{}</label>",
            escape(path),
            escape(value),
            if chosen.contains(&value) {
                " checked"
            } else {
                ""
            },
            choice_label(l, value)
        );
    }
    out.push_str("</div>");
    out
}

fn leaf(
    ctx: &Ctx<'_>,
    node: &Node,
    path: &str,
    name: &str,
    state: &State,
    required: bool,
) -> String {
    let value = escape(one(state, path));
    let (id, field_name) = (dom_id(path), escape(path));
    match node {
        Node::Flag => flag(ctx.l, path, state),
        Node::Int { min, max } => {
            let range = format!(
                "{}{}",
                if bounded(*min) {
                    format!(" min=\"{min}\"")
                } else {
                    String::new()
                },
                if bounded(*max) {
                    format!(" max=\"{max}\"")
                } else {
                    String::new()
                }
            );
            format!(
                "<input type=\"number\" id=\"{id}\" name=\"{field_name}\" value=\"{value}\"{range} inputmode=\"numeric\">"
            )
        }
        Node::Num { min, max } => format!(
            "<input type=\"number\" id=\"{id}\" name=\"{field_name}\" value=\"{value}\" min=\"{min}\" max=\"{max}\" step=\"0.05\">"
        ),
        Node::Text { max, .. } if (201..10_000).contains(max) => format!(
            "<textarea id=\"{id}\" name=\"{field_name}\" rows=\"3\" maxlength=\"{max}\">{value}</textarea>"
        ),
        Node::Text { pattern, max, .. } => {
            let duration = pattern.as_deref().is_some_and(|p| p.contains("[smhdw]"));
            let attrs = format!(
                "{}{}{}",
                if *max < 10_000 {
                    format!(" maxlength=\"{max}\"")
                } else {
                    String::new()
                },
                pattern.as_deref().map_or_else(String::new, |p| format!(
                    " pattern=\"{}\"",
                    escape(&p.replace("-]", "\\-]"))
                )),
                if duration {
                    format!(" placeholder=\"{}\"", ctx.l.text(keys::ED_DURATION_EXAMPLE))
                } else {
                    String::new()
                }
            );
            let list = match name {
                "counter" => " list=\"e-counters\"",
                "level" => " list=\"e-levels\"",
                _ => "",
            };
            format!(
                "<input type=\"text\" id=\"{id}\" name=\"{field_name}\" value=\"{value}\"{attrs}{list}>"
            )
        }
        Node::Pick(values) => {
            let mut out = format!("<select id=\"{id}\" name=\"{field_name}\">");
            if !required {
                let _ = write!(
                    out,
                    "<option value=\"\">{}</option>",
                    ctx.l.text(keys::ED_DEFAULT)
                );
            }
            for v in values {
                let _ = write!(
                    out,
                    "<option value=\"{}\"{}>{}</option>",
                    escape(v),
                    if one(state, path) == v {
                        " selected"
                    } else {
                        ""
                    },
                    choice_label(ctx.l, v)
                );
            }
            out + "</select>"
        }
        Node::Many(values) => choices(ctx.l, path, state, values),
        Node::Words => format!(
            "<textarea id=\"{id}\" name=\"{field_name}\" rows=\"4\" spellcheck=\"false\">{value}</textarea>"
        ),
        Node::Roles => chips(ctx, path, state, true),
        Node::Channels => chips(ctx, path, state, false),
        Node::Channel => select(ctx, path, state, false),
        Node::Role => select(ctx, path, state, true),
        Node::Condition => format!(
            "<input type=\"text\" id=\"{id}\" name=\"{field_name}\" value=\"{value}\" placeholder=\">= 5\"><input type=\"text\" name=\"{}\" value=\"{}\" placeholder=\"{}\">",
            escape(&join(path, "rev")),
            escape(one(state, &join(path, "rev"))),
            ctx.l.text(keys::ED_REVERSE_CONDITION)
        ),
        _ => String::new(),
    }
}

fn group(
    ctx: &Ctx<'_>,
    name: &str,
    tag: Option<&str>,
    members: &[Prop],
    path: &str,
    state: &State,
) -> String {
    let on = one(state, &join(path, "__on")) == "1";
    let note = hint(ctx.l, tag, name)
        .map_or_else(String::new, |h| format!("<p class=\"ed-hint\">{h}</p>"));
    format!(
        "<fieldset class=\"ed-group\"><legend><label class=\"ed-switch-label\"><input type=\"checkbox\" name=\"{}\" value=\"1\"{}>{}</label></legend>{note}{}</fieldset>",
        escape(&join(path, "__on")),
        if on { " checked" } else { "" },
        label(ctx.l, tag, name),
        object_body(ctx, members, path, state, Some(name))
    )
}

fn object_body(
    ctx: &Ctx<'_>,
    members: &[Prop],
    path: &str,
    state: &State,
    tag: Option<&str>,
) -> String {
    members
        .iter()
        .map(|p| prop(ctx, p, path, state, tag))
        .collect()
}

/// One property: a field, a group that can be switched on, or a list.
fn prop(ctx: &Ctx<'_>, p: &Prop, path: &str, state: &State, tag: Option<&str>) -> String {
    let child = join(path, &p.name);
    match &p.node {
        Node::Object(members) => group(ctx, &p.name, tag, members, &child, state),
        Node::Entries { key_pattern, value } => format!(
            "<div class=\"ed-list\"><h3 class=\"ed-subtitle\">{}</h3>{}</div>",
            label(ctx.l, tag, &p.name),
            entries(ctx, key_pattern.as_deref(), value, &child, state, &p.name)
        ),
        node => field(
            ctx,
            &child,
            &p.name,
            tag,
            p.required,
            &leaf(ctx, node, &child, &p.name, state, p.required),
        ),
    }
}

fn remove_button(l: Locale<'_>, row: &str) -> String {
    format!(
        "<button class=\"icon-button\" type=\"submit\" name=\"op\" value=\"del:{row}\" aria-label=\"{text}\" title=\"{text}\">✕</button>",
        row = escape(row),
        text = l.text(keys::ED_REMOVE)
    )
}

/// The conditions or the actions of a rule: one card each, and a choice of what to add.
fn variants(
    ctx: &Ctx<'_>,
    all: &[Variant],
    path: &str,
    state: &State,
    add: &str,
    empty: &str,
) -> String {
    let mut out = String::from("<div class=\"ed-list\">");
    let numbers = rows(state, path, "__t");
    if numbers.is_empty() {
        let _ = write!(out, "<p class=\"muted ed-empty\">{empty}</p>");
    }
    out.push_str(&problems(ctx, path));
    for n in numbers {
        let row = join(path, &n.to_string());
        let tag = one(state, &join(&row, "__t"));
        let Some(variant) = all.iter().find(|v| v.tag == tag) else {
            continue;
        };
        let (required, optional): (Vec<&Prop>, Vec<&Prop>) =
            variant.props.iter().partition(|p| p.required);
        let show = |props: &[&Prop]| -> String {
            props
                .iter()
                .map(|p| prop(ctx, p, &row, state, Some(tag)))
                .collect()
        };
        let body = if required.is_empty() || optional.is_empty() {
            show(&variant.props.iter().collect::<Vec<_>>())
        } else {
            format!(
                "{}<details class=\"ed-more\"><summary>{}</summary>{}</details>",
                show(&required),
                ctx.l.text(keys::ED_MORE),
                show(&optional)
            )
        };
        let note = ctx
            .l
            .find(&format!("ed-d-{tag}"))
            .map_or_else(String::new, |d| {
                format!("<p class=\"muted ed-card-note\">{d}</p>")
            });
        let _ = write!(
            out,
            "<section class=\"ed-card\"><header><div><h4>{}</h4>{note}</div>{}</header><input type=\"hidden\" name=\"{}\" value=\"{}\">{body}</section>",
            variant_name(ctx.l, tag),
            remove_button(ctx.l, &row),
            escape(&join(&row, "__t")),
            escape(tag)
        );
    }
    let mut options = String::new();
    for v in all {
        let _ = write!(
            options,
            "<option value=\"{}\">{}</option>",
            escape(&v.tag),
            variant_name(ctx.l, &v.tag)
        );
    }
    let _ = write!(
        out,
        "<div class=\"ed-add\"><select name=\"new.{}\" aria-label=\"{add}\">{options}</select><button class=\"button secondary\" type=\"submit\" name=\"op\" value=\"add:{}\">＋ {add}</button></div></div>",
        escape(path),
        escape(path)
    );
    out
}

/// Named entries, each a card with its name on top.
fn entries(
    ctx: &Ctx<'_>,
    key_pattern: Option<&str>,
    value: &Node,
    path: &str,
    state: &State,
    name: &str,
) -> String {
    let mut out = String::new();
    let pattern = key_pattern.map_or_else(String::new, |p| {
        format!(" pattern=\"{}\"", escape(&p.replace("-]", "\\-]")))
    });
    for n in rows(state, path, "__k") {
        let row = join(path, &n.to_string());
        let key = join(&row, "__k");
        let body = match value {
            Node::Object(members) => object_body(ctx, members, &row, state, None),
            node => leaf(ctx, node, &row, name, state, false),
        };
        let _ = write!(
            out,
            "<section class=\"ed-card\"><header><input class=\"ed-key\" type=\"text\" name=\"{key_name}\" value=\"{key_value}\" maxlength=\"32\"{pattern} placeholder=\"{caption}\" aria-label=\"{caption}\">{remove}</header>{errors}{body}</section>",
            key_name = escape(&key),
            key_value = escape(one(state, &key)),
            caption = ctx.l.text(keys::ED_NAME_KEY),
            remove = remove_button(ctx.l, &row),
            errors = problems(ctx, &key)
        );
    }
    let _ = write!(
        out,
        "<div class=\"ed-add\"><button class=\"button secondary\" type=\"submit\" name=\"op\" value=\"add:{}\">＋ {}</button></div>",
        escape(path),
        ctx.l.text(keys::ED_ADD)
    );
    out
}

fn datalists(ctx: &Ctx<'_>) -> String {
    let mut out = String::new();
    for (id, names) in [("e-counters", ctx.counters), ("e-levels", ctx.levels)] {
        let _ = write!(out, "<datalist id=\"{id}\">");
        for name in names {
            let _ = write!(out, "<option value=\"{}\">", escape(name));
        }
        out.push_str("</datalist>");
    }
    out
}

fn member<'a>(members: &'a [Prop], name: &str) -> Option<&'a Prop> {
    members.iter().find(|p| p.name == name)
}

fn section(title: &str, note: &str, body: &str) -> String {
    let note = if note.is_empty() {
        String::new()
    } else {
        format!("<p class=\"muted\">{note}</p>")
    };
    format!("<section class=\"card ed-section\"><h2>{title}</h2>{note}{body}</section>")
}

/// The form of a rule: what it is called, when it acts, and what it does.
pub(crate) fn rule_body(ctx: &Ctx<'_>, node: &Node, state: &State, new: bool) -> String {
    let Node::Object(members) = node else {
        return String::new();
    };
    let l = ctx.l;
    let mut basic = String::new();
    if new {
        basic.push_str(&field(
            ctx,
            "__id",
            "rule_id",
            None,
            true,
            &format!(
                "<input type=\"text\" id=\"e-__id\" name=\"__id\" value=\"{}\" maxlength=\"32\" pattern=\"^[a-z0-9_\\-]{{1,32}}$\">",
                escape(one(state, "__id"))
            ),
        ));
    }
    if let Some(p) = member(members, "name") {
        basic.push_str(&prop(ctx, p, "r", state, None));
    }
    if member(members, "enabled").is_some() {
        basic.push_str(&field(
            ctx,
            "r.enabled",
            "enabled",
            None,
            false,
            &switch(l, "r.enabled", state),
        ));
    }
    let mut out = section(&l.text(keys::ED_BASIC), "", &basic);
    for (name, title, note, add, empty) in [
        (
            "triggers",
            keys::ED_IF,
            keys::ED_IF_HINT,
            keys::ED_ADD_CONDITION,
            keys::ED_NO_TRIGGERS,
        ),
        (
            "actions",
            keys::ED_THEN,
            keys::ED_THEN_HINT,
            keys::ED_ADD_ACTION,
            keys::ED_NO_ACTIONS,
        ),
    ] {
        if let Some(Node::Variants { variants: all, .. }) = member(members, name).map(|p| &p.node) {
            out.push_str(&section(
                &l.text(title),
                &l.text(note),
                &variants(
                    ctx,
                    all,
                    &join("r", name),
                    state,
                    &l.text(add),
                    &l.text(empty),
                ),
            ));
        }
    }
    for (title, names) in [
        (keys::ED_SCOPE, &["scope"][..]),
        (keys::ED_MORE, &["cooldown", "continue_after_match"][..]),
    ] {
        let body: String = names
            .iter()
            .filter_map(|n| member(members, n))
            .map(|p| prop(ctx, p, "r", state, None))
            .collect();
        if !body.is_empty() {
            let open = state.keys().any(|k| {
                k.starts_with("r.scope.") || k == "r.cooldown" || k == "r.continue_after_match"
            });
            let _ = write!(
                out,
                "<details class=\"card ed-section ed-fold\"{}><summary>{}</summary>{body}</details>",
                if open { " open" } else { "" },
                l.text(title)
            );
        }
    }
    out
}

/// The form of a counter or of the anti-raid: its fields, as cards.
pub(crate) fn object_card(
    ctx: &Ctx<'_>,
    node: &Node,
    path: &str,
    state: &State,
    new: bool,
) -> String {
    let Node::Object(members) = node else {
        return String::new();
    };
    let mut body = String::new();
    if new {
        body.push_str(&field(
            ctx,
            "__id",
            "counter_id",
            None,
            true,
            &format!(
                "<input type=\"text\" id=\"e-__id\" name=\"__id\" value=\"{}\" maxlength=\"32\" pattern=\"^[a-z0-9_\\-]{{1,32}}$\">",
                escape(one(state, "__id"))
            ),
        ));
    }
    body.push_str(&object_body(ctx, members, path, state, None));
    section(&ctx.l.text(keys::ED_BASIC), "", &body)
}

pub(crate) fn datalist_html(ctx: &Ctx<'_>) -> String {
    datalists(ctx)
}

/// The page around an editor form.
#[allow(clippy::too_many_arguments)]
pub(crate) fn editor_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    guild: &str,
    module: &str,
    part: &str,
    fields: &[Field],
    title: &str,
    action: &str,
    back: &str,
    etag: &str,
    inner: &str,
    banner: &[String],
    saved: bool,
) -> String {
    let mut body = format!(
        "<header class=\"page-heading\"><h1 class=\"with-icon\">{}{}</h1></header>{}",
        module_icon(module),
        title,
        part_tabs(l, id, module, part, fields)
    );
    if saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::SAVED)
        );
    }
    for message in banner {
        let _ = write!(
            body,
            "<p class=\"form-error\" role=\"alert\">⚠ {}</p>",
            escape(message)
        );
    }
    let _ = write!(
        body,
        "<form method=\"post\" action=\"{}\" class=\"ed-form\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><button class=\"sr-only\" type=\"submit\" name=\"op\" value=\"save\" tabindex=\"-1\" aria-hidden=\"true\"></button>{inner}<div class=\"save-bar\"><a class=\"button ghost\" href=\"{}\">{}</a><button class=\"button primary\" type=\"submit\" name=\"op\" value=\"save\">{}</button></div></form>",
        escape(action),
        escape(etag),
        escape(back),
        l.text(keys::ED_BACK),
        l.text(keys::SAVE)
    );
    shell(l, headers, id, guild, keys::MODULES, &body, true)
}

/// What a list of rules or counters shows of each one.
pub(crate) struct Item {
    pub key: String,
    pub title: String,
    pub summary: String,
    /// The switch of the item, when it has one.
    pub enabled: Option<bool>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collection_page(
    l: Locale<'_>,
    headers: &HeaderMap,
    id: &str,
    guild: &str,
    module: &str,
    part: &str,
    fields: &[Field],
    items: &[Item],
    etag: &str,
    saved: bool,
) -> String {
    let base = super::form::part_path(id, module, part);
    let mut body = format!(
        "<header class=\"page-heading\"><h1 class=\"with-icon\">{}{}</h1><a class=\"button primary\" href=\"{}/new\">＋ {}</a></header>{}",
        module_icon(module),
        part_name(l, module, part),
        escape(&base),
        l.text(if part == "rules" {
            keys::ED_ADD_RULE
        } else {
            keys::ED_ADD_COUNTER
        }),
        part_tabs(l, id, module, part, fields)
    );
    if saved {
        let _ = write!(
            body,
            "<p role=\"status\" class=\"form-status\">{}</p>",
            l.text(keys::SAVED)
        );
    }
    if items.is_empty() {
        let _ = write!(
            body,
            "<div class=\"card ed-blank\"><p>{}</p><a class=\"button primary\" href=\"{}/new\">＋ {}</a></div>",
            l.text(if part == "rules" {
                keys::ED_NO_RULES
            } else {
                keys::ED_NO_COUNTERS
            }),
            escape(&base),
            l.text(if part == "rules" {
                keys::ED_ADD_RULE
            } else {
                keys::ED_ADD_COUNTER
            })
        );
    }
    body.push_str("<ul class=\"ed-items\">");
    for item in items {
        let key = escape(&item.key);
        let switch = item.enabled.map_or_else(String::new, |on| {
            format!(
                "<form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><button class=\"switch\" type=\"submit\" name=\"op\" value=\"toggle:{key}:{}\" role=\"switch\" aria-checked=\"{on}\" aria-label=\"{}\"></button></form>",
                escape(&base),
                escape(etag),
                !on,
                l.text(if on { keys::ENABLED } else { keys::DISABLED })
            )
        });
        let _ = write!(
            body,
            "<li class=\"card ed-item{}\"><div class=\"ed-item-main\"><a class=\"ed-item-title\" href=\"{}/{key}\">{}</a><span class=\"muted mono\">{key}</span><p class=\"muted\">{}</p></div><div class=\"ed-item-actions\">{switch}<a class=\"button secondary\" href=\"{}/{key}\">{}</a><form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"etag\" value=\"{}\"><button class=\"button ghost danger\" type=\"submit\" name=\"op\" value=\"delete:{key}\">{}</button></form></div></li>",
            if item.enabled == Some(false) {
                " off"
            } else {
                ""
            },
            escape(&base),
            escape(&item.title),
            escape(&item.summary),
            escape(&base),
            l.text(keys::ED_EDIT),
            escape(&base),
            escape(etag),
            l.text(keys::ED_REMOVE)
        );
    }
    body.push_str("</ul>");
    shell(l, headers, id, guild, keys::MODULES, &body, true)
}

/// The summary line of a rule: its conditions and its actions, by name.
pub(crate) fn rule_summary(l: Locale<'_>, value: &Value) -> String {
    let names = |key: &str, tag: &str| -> String {
        value[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v[tag].as_str())
            .map(|t| variant_name(l, t))
            .collect::<Vec<_>>()
            .join(" / ")
    };
    let (when, then) = (names("triggers", "type"), names("actions", "do"));
    if when.is_empty() && then.is_empty() {
        String::new()
    } else {
        format!("{when} → {then}")
    }
}

/// Which of the editor parts a field belongs to; the editor pages show only these.
#[must_use]
pub(crate) fn editor_field<'a>(fields: &'a [Field], part: &str) -> Option<&'a Field> {
    fields.iter().find(|f| f.name == part)
}

#[must_use]
pub(crate) fn editor_node(field: &Field) -> Option<Node> {
    editor::node(&field.name, &field.schema)
}
