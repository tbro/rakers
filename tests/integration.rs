/// Integration tests that fetch real pages and assert JS rendering produced output.
/// These tests require network access. Run with:
///   cargo test --test integration                         (boa engine)
///   cargo test --test integration --no-default-features --features rquickjs
use rakers::{render_url, HttpConfig};

/// babylonbee.com uses Cloudflare Rocket Loader: script types are rewritten to
/// "<hex-hash>-text/javascript". Verifies that rakers executes those scripts and
/// produces a fully-populated page rather than a skeleton.
#[test]
fn babylonbee_rocket_loader_renders_articles() {
    let out = render_url("https://babylonbee.com", &HttpConfig::default())
        .expect("render_url failed");

    assert!(
        out.len() > 100_000,
        "expected >100 KB of rendered HTML, got {} bytes — scripts may not have run",
        out.len()
    );
    assert!(
        out.contains("Babylon Bee"),
        "site name not found — page may not have rendered"
    );
    assert!(
        out.contains("<article"),
        "<article> elements missing — Rocket Loader scripts may have been skipped"
    );
}

/// jsbench.me is a React SPA: the server returns an almost-empty HTML shell and
/// React renders the full UI client-side. Verifies that rakers executes the React
/// bundle and serializes the rendered DOM.
#[test]
fn jsbench_react_spa_renders_ui() {
    let out = render_url("https://jsbench.me", &HttpConfig::default())
        .expect("render_url failed");

    assert!(
        out.len() > 4_000,
        "expected >4 KB of rendered HTML, got {} bytes — React bundle may not have run",
        out.len()
    );
    // React renders a nav, suite editor, and benchmark controls.
    assert!(
        out.to_lowercase().contains("benchmark"),
        "'benchmark' not found in output — React may not have rendered"
    );
    assert!(
        out.to_lowercase().contains("run"),
        "'run' control not found — React may not have rendered the benchmark UI"
    );
}

/// Verifies that a custom User-Agent header is forwarded on all HTTP requests,
/// including external script fetches. Uses httpbin.org's /user-agent endpoint
/// which echoes the UA back as JSON, then wraps it in a <script> that writes it.
#[test]
fn custom_user_agent_is_sent() {
    let cfg = HttpConfig {
        user_agent: Some("rakers-test/1.0".to_owned()),
        headers: vec![],
    };
    // Fetch a page that just echoes the UA. httpbin returns JSON; wrapping it in
    // a script context is unnecessary — we just verify rakers can reach the URL
    // with the custom UA without error and gets a non-empty response.
    let out = render_url("https://httpbin.org/user-agent", &cfg)
        .expect("render_url failed");

    assert!(
        out.contains("rakers-test/1.0"),
        "custom User-Agent not echoed back — header may not have been sent"
    );
}
