use scraper::{Html, Selector};

/// Return the outer HTML of every element in `html` that matches `selector`,
/// joined by newlines.  Returns an empty string when nothing matches.
pub fn select_html(html: &str, selector: &str) -> anyhow::Result<String> {
    let sel = Selector::parse(selector)
        .map_err(|e| anyhow::anyhow!("invalid selector {:?}: {}", selector, e))?;
    let doc = Html::parse_document(html);
    let out: Vec<String> = doc.select(&sel).map(|el| el.html()).collect();
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_by_tag() {
        let html = "<html><body><h1>Title</h1><p>Para</p></body></html>";
        let out = select_html(html, "h1").unwrap();
        assert_eq!(out, "<h1>Title</h1>");
    }

    #[test]
    fn selects_multiple_matches() {
        let html = "<html><body><p>a</p><p>b</p></body></html>";
        let out = select_html(html, "p").unwrap();
        assert_eq!(out, "<p>a</p>\n<p>b</p>");
    }

    #[test]
    fn selects_by_id() {
        let html = r#"<html><body><div id="root"><span>content</span></div></body></html>"#;
        let out = select_html(html, "#root").unwrap();
        assert!(out.contains("content"), "should match #root: {out}");
    }

    #[test]
    fn selects_by_class() {
        let html = r#"<html><body><p class="note">hi</p><p>other</p></body></html>"#;
        let out = select_html(html, ".note").unwrap();
        assert_eq!(out, r#"<p class="note">hi</p>"#);
    }

    #[test]
    fn empty_string_when_no_match() {
        let html = "<html><body><p>text</p></body></html>";
        let out = select_html(html, "h1").unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn invalid_selector_returns_error() {
        let result = select_html("<html></html>", "##bad");
        assert!(result.is_err(), "invalid selector should error");
    }
}
