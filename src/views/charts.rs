use std::fmt::Write;

use serde_json::Value;

use crate::{
    html::escape,
    i18n::{Locale, keys},
};

pub(super) const KINDS: [(&str, &str); 6] = [
    ("warn", keys::WARN_KIND),
    ("timeout", keys::TIMEOUT_KIND),
    ("kick", keys::KICK_KIND),
    ("ban", keys::BAN_KIND),
    ("vcmute", keys::VCMUTE_KIND),
    ("other", keys::OTHER),
];

pub(super) fn kind_index(kind: &str) -> usize {
    KINDS
        .iter()
        .position(|(name, _)| *name == kind)
        .unwrap_or(5)
}
#[allow(clippy::cast_precision_loss)]
fn height(value: u64, max: u64) -> f64 {
    if max == 0 {
        0.0
    } else {
        (value as f64 / max as f64) * 120.0
    }
}
fn ceiling(value: u64) -> u64 {
    let mut step = 1_u64;
    while value / step > 40 {
        step = step.saturating_mul(10);
    }
    let step = [1, 2, 5, 10]
        .into_iter()
        .map(|n| step.saturating_mul(n))
        .find(|n| value <= n.saturating_mul(4))
        .unwrap_or(u64::MAX / 4);
    step.saturating_mul(4)
}
fn date(value: &str) -> String {
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() == 3 {
        format!("{}/{}", parts[1].trim_start_matches('0'), parts[2])
    } else {
        value.to_owned()
    }
}
fn cell(v: Option<u64>) -> String {
    v.map_or_else(|| "—".into(), |n| n.to_string())
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub(super) fn chart(
    l: Locale<'_>,
    title: &str,
    data: &Value,
    fields: &[(&str, &str)],
    stacked: bool,
) -> String {
    let days = data.as_array().map_or(&[][..], Vec::as_slice);
    let mut values = Vec::new();
    for day in days {
        let row: Vec<Option<u64>> = if stacked {
            if day["kinds"].is_null() {
                values.push(vec![None; fields.len()]);
                continue;
            }
            let mut totals = vec![Some(0_u64); 6];
            if let Some(kinds) = day["kinds"].as_object() {
                for (kind, value) in kinds {
                    let i = kind_index(kind);
                    totals[i] = Some(
                        totals[i]
                            .unwrap_or(0)
                            .saturating_add(value.as_u64().unwrap_or(0)),
                    );
                }
            }
            totals
        } else {
            fields.iter().map(|(name, _)| day[*name].as_u64()).collect()
        };
        values.push(row);
    }
    let max = values
        .iter()
        .map(|row| {
            if stacked {
                row.iter()
                    .flatten()
                    .copied()
                    .fold(0_u64, u64::saturating_add)
            } else {
                row.iter().flatten().copied().max().unwrap_or(0)
            }
        })
        .max()
        .unwrap_or(0);
    let scale = ceiling(max);
    let mut svg = format!(
        "<svg viewBox=\"0 0 360 160\" role=\"img\" aria-labelledby=\"{title}-title {title}-desc\"><title id=\"{title}-title\">{}</title><desc id=\"{title}-desc\">{}</desc>",
        l.text(title),
        format_args!(
            "{} · {} · {}",
            l.text(title),
            l.number(keys::DAYS, days.len() as u64),
            l.text(keys::TABLE_VIEW)
        )
    );
    for tick in 0..=4 {
        let y = 128 - tick * 30;
        let n = scale / 4 * tick;
        let label = if n >= 1000 {
            if n.is_multiple_of(1000) {
                format!("{}K", n / 1000)
            } else {
                format!("{}.{:01}K", n / 1000, n % 1000 / 100)
            }
        } else {
            n.to_string()
        };
        let _ = write!(
            svg,
            "<line class=\"gridline\" x1=\"32\" x2=\"356\" y1=\"{y}\" y2=\"{y}\"/><text x=\"28\" y=\"{}\" text-anchor=\"end\">{label}</text>",
            y + 3
        );
    }
    let slot = 324.0 / days.len().max(1) as f64;
    for (i, (day, row)) in days.iter().zip(&values).enumerate() {
        let x = 32.0 + i as f64 * slot;
        let label = escape(&date(day["date"].as_str().unwrap_or("")));
        let mut base = 128.0;
        for (j, v) in row.iter().enumerate() {
            let bar = (if stacked {
                slot * 0.65
            } else {
                (slot * 0.7 / row.len().max(1) as f64) - 2.0
            })
            .clamp(0.5, 24.0);
            let bx = if stacked {
                x + (slot - bar) / 2.0
            } else {
                x + (slot - (bar * row.len() as f64 + 2.0 * (row.len() - 1) as f64)) / 2.0
                    + j as f64 * (bar + 2.0)
            };
            if let Some(n) = v {
                let h = height(*n, scale);
                if h > 0.0 {
                    let y = if stacked { base - h } else { 128.0 - h };
                    let visible = if stacked && base < 128.0 {
                        (h - 2.0).max(0.0)
                    } else {
                        h
                    };
                    let top = y + (h - visible);
                    let radius = visible.min(4.0).min(bar / 2.0);
                    let _ = write!(
                        svg,
                        "<path class=\"series-{}\" d=\"M {bx:.2} {:.2} v -{:.2} q 0 -{radius:.2} {radius:.2} -{radius:.2} h {:.2} q {radius:.2} 0 {radius:.2} {radius:.2} v {:.2} z\"><title>{label}: {}: {}</title></path>",
                        j + 1,
                        top + visible,
                        (visible - radius).max(0.0),
                        (bar - 2.0 * radius).max(0.0),
                        (visible - radius).max(0.0),
                        l.text(fields[j].1),
                        l.number(keys::COUNT, *n)
                    );
                    if stacked {
                        base = y;
                    }
                }
            } else {
                let _ = write!(
                    svg,
                    "<line class=\"missing\" x1=\"{bx:.2}\" x2=\"{:.2}\" y1=\"127\" y2=\"127\"/>",
                    bx + bar
                );
            }
        }
        if days.len() <= 4 || i % days.len().div_ceil(5) == 0 || i + 1 == days.len() {
            let _ = write!(
                svg,
                "<text x=\"{:.2}\" y=\"149\" text-anchor=\"middle\">{label}</text>",
                x + slot / 2.0
            );
        }
        for (j, v) in row.iter().enumerate() {
            let label_kind = l.text(fields[j].1);
            let value = v.map_or_else(|| l.text(keys::NO_DATA), |n| l.number(keys::COUNT, n));
            let hx = x + j as f64 * slot / row.len().max(1) as f64;
            let _ = write!(
                svg,
                "<rect class=\"hover-target\" x=\"{hx:.2}\" y=\"8\" width=\"{:.2}\" height=\"120\"><title>{label}: {label_kind}: {value}</title></rect>",
                slot / row.len().max(1) as f64
            );
        }
    }
    svg.push_str("</svg>");
    let mut legend = String::new();
    let mut table = format!(
        "<details><summary>{}</summary><div class=\"table-scroll\"><table><thead><tr><th>{}</th>",
        l.text(keys::TABLE_VIEW),
        l.text(keys::DATE)
    );
    for (j, (_, key)) in fields.iter().enumerate() {
        let total: u64 = values
            .iter()
            .filter_map(|r| r[j])
            .fold(0, u64::saturating_add);
        if fields.len() > 1 && (total > 0 || !stacked) {
            let _ = write!(
                legend,
                "<span><i class=\"legend-dot series-{}\"></i>{} {total}</span>",
                j + 1,
                l.text(key)
            );
        }
        let _ = write!(table, "<th>{}</th>", l.text(key));
    }
    table.push_str("</tr></thead><tbody>");
    for (day, row) in days.iter().zip(&values) {
        let _ = write!(
            table,
            "<tr><th>{}</th>",
            escape(day["date"].as_str().unwrap_or(""))
        );
        for v in row {
            let _ = write!(table, "<td>{}</td>", cell(*v));
        }
        table.push_str("</tr>");
    }
    table.push_str("</tbody></table></div></details>");
    let legend = if legend.is_empty() {
        String::new()
    } else {
        format!("<div class=\"legend\">{legend}</div>")
    };
    format!(
        "<figure class=\"card chart\"><figcaption>{}</figcaption>{svg}{legend}{table}</figure>",
        l.text(title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Catalogs, REGISTRY};
    use serde_json::json;
    #[test]
    fn scales_and_kinds() {
        assert_eq!(height(0, 4), 0.0);
        assert_eq!(height(4, 4), 120.0);
        assert_eq!(ceiling(8), 8);
        assert_eq!(kind_index("warn"), 0);
        assert_eq!(kind_index("unrecognized"), 5);
    }
    #[test]
    fn missing_and_stable_series() {
        let catalogs = Catalogs::new(REGISTRY).unwrap();
        let l = catalogs.locale("ja");
        let message = chart(
            l,
            keys::MESSAGES,
            &json!([{"date":"2026-11-01","count":null}]),
            &[("count", keys::MESSAGES)],
            false,
        );
        assert!(message.contains("class=\"missing\""));
        assert!(message.contains("<details>"));
        let missing_cases = chart(
            l,
            keys::MODERATION,
            &json!([{"date":"2026-11-01","kinds":null}]),
            &KINDS,
            true,
        );
        assert!(missing_cases.contains("class=\"missing\""));
        assert!(missing_cases.contains("<td>—</td>"));
        let first = chart(
            l,
            keys::MODERATION,
            &json!([{"date":"2026-11-01","kinds":{"warn":1,"zzz":9}}]),
            &KINDS,
            true,
        );
        let second = chart(
            l,
            keys::MODERATION,
            &json!([{"date":"2026-11-01","kinds":{"warn":9,"zzz":1}}]),
            &KINDS,
            true,
        );
        for result in [first, second] {
            assert!(result.contains("class=\"series-1\""));
            assert!(result.contains("class=\"series-6\""));
            assert!(result.contains("その他"));
            assert!(result.contains("<details>"));
        }
    }
}
