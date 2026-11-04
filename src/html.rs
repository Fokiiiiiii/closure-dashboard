#[must_use]
pub fn escape(value: &str) -> String {
    let mut result = String::new();
    for c in value.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}
#[cfg(test)]
mod tests {
    #[test]
    fn escapes_html() {
        assert_eq!(super::escape("<a & \"'"), "&lt;a &amp; &quot;&#39;");
    }
}
