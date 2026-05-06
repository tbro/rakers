/// Fixture-based rendering tests.
///
/// Each test loads an HTML file from tests/fixtures/, renders it with rakers,
/// and asserts that the JavaScript produced the expected DOM output.
use rakers::{HttpConfig, render};
use std::fs;

fn render_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}");
    let html = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"));
    render(&html, false, None, &HttpConfig::default(), false, None, None)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"))
}

#[test]
fn document_write_renders_list() {
    let out = render_fixture("document_write.html");
    assert!(out.contains("<li>Item 1</li>"), "Item 1 missing: {out}");
    assert!(out.contains("<li>Item 2</li>"), "Item 2 missing: {out}");
    assert!(out.contains("<li>Item 3</li>"), "Item 3 missing: {out}");
}

#[test]
fn inner_html_renders_spa_content() {
    let out = render_fixture("inner_html.html");
    assert!(
        out.contains("<h1>Rendered by JS</h1>"),
        "h1 missing: {out}"
    );
    assert!(
        out.contains("SPA-style rendering"),
        "paragraph text missing: {out}"
    );
}

#[test]
fn settimeout_content_is_flushed() {
    let out = render_fixture("settimeout.html");
    assert!(
        out.contains("Async content loaded"),
        "setTimeout content not flushed: {out}"
    );
}

#[test]
fn dom_api_creates_elements() {
    let out = render_fixture("dom_api.html");
    assert!(
        out.contains("Built with DOM API"),
        "h2 text content missing: {out}"
    );
    assert!(
        out.contains("Paragraph via createElement"),
        "paragraph text missing: {out}"
    );
}
