use similar::TextDiff;

/// Produce a unified diff between `before` and `after`, split on lines.
///
/// Both inputs are pretty-printed before diffing so the output is
/// human-readable regardless of whether the caller already formatted them.
/// Returns an empty string when the inputs are identical after formatting.
#[must_use] 
pub fn diff_html(before: &str, after: &str) -> String {
    let a = crate::pretty_print(before);
    let b = crate::pretty_print(after);
    TextDiff::from_lines(a.as_str(), b.as_str())
        .unified_diff()
        .header("raw", "rendered")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::diff_html;

    #[test]
    fn shows_added_content() {
        let before = "<html><body></body></html>";
        let after = "<html><body><h1>hello</h1></body></html>";
        let d = diff_html(before, after);
        assert!(d.contains("---"), "missing --- header");
        assert!(d.contains("+++"), "missing +++ header");
        assert!(
            d.lines().any(|l| l.starts_with('+') && l.contains("<h1>")),
            "added h1 not in diff: {d}"
        );
    }

    #[test]
    fn empty_when_identical() {
        let html = "<html><body><p>same</p></body></html>";
        assert!(
            diff_html(html, html).is_empty(),
            "identical inputs should produce empty diff"
        );
    }

    #[test]
    fn shows_removed_content() {
        let before = "<html><body><p>gone</p></body></html>";
        let after = "<html><body></body></html>";
        let d = diff_html(before, after);
        assert!(
            d.lines().any(|l| l.starts_with('-') && l.contains("<p>")),
            "removed p not in diff: {d}"
        );
    }
}
