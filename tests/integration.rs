/// Integration tests that fetch real pages and assert JS rendering produced output.
/// These tests require network access. Run with:
///   cargo test --test integration                        (rquickjs, default)
///   cargo test --test integration --no-default-features --features boa
use rakers::{HttpConfig, render};

fn fetch(url: &str) -> String {
    ureq::get(url).call().unwrap().into_string().unwrap()
}

/// todomvc.com/examples/react/dist/ serves a ~645-byte skeleton with an empty
/// `<section id="root">`; the React bundle renders the full todo-app UI client-side.
/// This is the canonical demo for rakers: dramatic before/after, zero JS errors.
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn todomvc_react_spa_renders_ui() {
    let raw = fetch("https://todomvc.com/examples/react/dist/");
    let out = render(
        &raw,
        false,
        Some("https://todomvc.com/examples/react/dist/"),
        &HttpConfig::default(),
        false,
        None,
        None,
    )
    .unwrap();

    // Raw skeleton has an empty <section id="root">; rendered output must contain the app.
    assert!(
        !raw.contains("<h1>todos</h1>"),
        "sanity: '<h1>todos</h1>' should be absent in the raw skeleton"
    );
    assert!(
        out.contains("<h1>todos</h1>"),
        "'<h1>todos</h1>' not found in rendered output — React may not have rendered the TodoMVC UI"
    );
    assert!(
        out.contains("class=\"new-todo\""),
        "new-todo input absent — React may not have rendered the TodoMVC UI"
    );
}

/// jsbench.me serves a ~2.7 KB skeleton with an empty React root; the JS bundle
/// renders the full benchmark UI client-side. This test confirms that the rendered
/// output is substantially larger than the raw skeleton and contains UI elements
/// ("Run") that are absent before JS executes.
///
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn jsbench_react_spa_renders_ui() {
    let raw = fetch("https://jsbench.me");
    let out = render(
        &raw,
        false,
        Some("https://jsbench.me"),
        &HttpConfig::default(),
        false,
        None,
        None,
    )
    .unwrap();

    // Skeleton is tiny; rendered output must be much larger.
    assert!(
        out.len() > raw.len() * 2,
        "rendered output ({} bytes) should be >2× raw skeleton ({} bytes) — React bundle may not have run",
        out.len(),
        raw.len()
    );

    // "Run" control is absent in the skeleton but rendered by React.
    assert!(
        !raw.to_lowercase().contains("run"),
        "sanity: 'run' should be absent in the raw skeleton"
    );
    assert!(
        out.to_lowercase().contains("run"),
        "'run' not found in rendered output — React may not have rendered the benchmark UI"
    );
}

/// v5.reactrouter.com serves a ~1.8 KB CSR-only skeleton (`<div id="app"></div>`);
/// the React Router bundle renders the full docs homepage (hero, tagline, nav).
/// This test specifically exercises two shim fixes:
///   - getElementsByTagName('script') → needed by the GA analytics snippet
///   - querySelector('head')          → needed by style-loader CSS injection
/// Without those fixes the bundle crashed before mounting and produced no output.
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn reactrouter_v5_homepage_renders() {
    let raw = fetch("https://v5.reactrouter.com/");
    let out = render(
        &raw,
        false,
        Some("https://v5.reactrouter.com/"),
        &HttpConfig::default(),
        false,
        None,
        None,
    )
    .unwrap();

    assert!(
        raw.contains("<div id=\"app\"></div>"),
        "sanity: raw HTML should have an empty app container"
    );
    assert!(
        !raw.contains("Learn once, Route anywhere"),
        "sanity: tagline should be absent in raw skeleton"
    );
    assert!(
        out.contains("React Router"),
        "'React Router' not found in rendered output"
    );
    assert!(
        out.contains("Learn once, Route anywhere"),
        "homepage tagline absent — React bundle may not have mounted"
    );
    assert!(
        out.len() > raw.len() * 5,
        "rendered ({} bytes) should be >5× raw skeleton ({} bytes) — bundle may not have run",
        out.len(),
        raw.len()
    );
}

/// babylonbee.com uses Cloudflare Rocket Loader, which rewrites script types to
/// "<hex-hash>-text/javascript". The site content is server-rendered, so this test
/// does not assert that JS added DOM — instead it asserts that our hash-type filter
/// does not break the pipeline: the rendered output must preserve all the
/// server-rendered articles that were present in the raw HTML.
///
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn babylonbee_rocket_loader_pipeline_intact() {
    let raw = fetch("https://babylonbee.com");
    let out = render(
        &raw,
        false,
        Some("https://babylonbee.com"),
        &HttpConfig::default(),
        false,
        None,
        None,
    )
    .unwrap();

    let raw_articles = raw.matches("<article").count();
    let out_articles = out.matches("<article").count();

    assert!(
        raw_articles > 0,
        "sanity: raw HTML should contain <article> elements"
    );
    assert!(
        out_articles >= raw_articles,
        "rendered output has fewer <article> elements ({}) than raw HTML ({}) — server-rendered content was lost",
        out_articles,
        raw_articles
    );
}

/// Verifies that a custom User-Agent is forwarded on all HTTP requests.
/// httpbin.org/user-agent echoes the UA back; we assert it appears in the output.
#[test]
fn custom_user_agent_is_sent() {
    let cfg = HttpConfig {
        user_agent: Some("rakers-test/1.0".to_owned()),
        headers: vec![],
        proxy: None,
        forward_headers: false,
    };
    let raw = ureq::get("https://httpbin.org/user-agent")
        .set("User-Agent", "rakers-test/1.0")
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    let out = render(&raw, false, Some("https://httpbin.org/user-agent"), &cfg, false, None, None).unwrap();

    assert!(
        out.contains("rakers-test/1.0"),
        "custom User-Agent not found in output — header may not have been sent"
    );
}
