//! Core rendering pipeline for rakers.
//!
//! Parses HTML, collects and executes scripts in a sandboxed JS context,
//! then serializes the post-execution DOM back to HTML.

mod dom;
mod pretty;
mod runtime;

pub use pretty::pretty_print;

/// Serialize render results as a JSON object with three fields:
/// `raw_bytes`, `rendered_bytes`, and `html`.
///
/// The `html` string is JSON-escaped; no external dependency is required.
pub fn to_json(raw_bytes: usize, html: &str) -> String {
    format!(
        "{{\n  \"raw_bytes\": {},\n  \"rendered_bytes\": {},\n  \"html\": \"{}\"\n}}\n",
        raw_bytes,
        html.len(),
        json_escape(html)
    )
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c    => out.push(c),
        }
    }
    out
}

/// HTTP options applied to every outbound request made by rakers.
#[derive(Default)]
pub struct HttpConfig {
    /// Value for the `User-Agent` header. `None` sends no `User-Agent`.
    pub user_agent: Option<String>,
    /// Additional headers sent with every request, in `(name, value)` form.
    pub headers: Vec<(String, String)>,
}

impl HttpConfig {
    /// Apply the configured user-agent and headers to `req`, returning the modified request.
    pub fn apply(&self, req: ureq::Request) -> ureq::Request {
        let mut req = req;
        if let Some(ua) = &self.user_agent {
            req = req.set("User-Agent", ua);
        }
        for (name, value) in &self.headers {
            req = req.set(name, value);
        }
        req
    }
}

/// Resolve `src` against an optional `base` URL, returning an absolute `http`/`https` URL.
///
/// Returns `None` for `data:` and `blob:` URLs (not fetchable), and when `src` is relative
/// but no base is available.
fn resolve_url(src: &str, base: Option<&str>) -> Option<String> {
    if src.starts_with("data:") || src.starts_with("blob:") {
        return None;
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return Some(src.to_owned());
    }
    if src.starts_with("//") {
        return Some(format!("https:{src}"));
    }
    let base_url = url::Url::parse(base?).ok()?;
    let resolved = base_url.join(src).ok()?;
    Some(resolved.to_string())
}

/// Fetch the script at `url` and return its source text.
///
/// Returns `None` on network error or if the response body is not valid UTF-8.
/// Files that open with `import`/`export` are skipped — they are ES module entry
/// points that require a full module loader with relative specifier resolution.
fn fetch_script(url: &str, cfg: &HttpConfig) -> Option<String> {
    let body = match cfg.apply(ureq::get(url)).call() {
        Ok(r) => r.into_string().ok()?,
        Err(e) => {
            eprintln!("[fetch error] {url}: {e}");
            return None;
        }
    };
    // Skip ES module files that use static import/export — they require a full
    // module loader with relative specifier resolution that we can't provide.
    // Self-contained bundles tagged type="module" by their bundler are fine.
    let trimmed = body.trim_start();
    if trimmed.starts_with("import ") || trimmed.starts_with("import{") || trimmed.starts_with("export ") {
        // Narrow exception: a file whose entire content is a single bare side-effect
        // import (`import './bundle.js'`) is a Vite/Rollup entry-point shim that
        // just loads one self-contained bundle.  Follow that one hop.
        if let Some(target) = single_reexport_target(trimmed) {
            if let Some(resolved) = resolve_url(target, Some(url)) {
                eprintln!("[module-shim] {url} → {resolved}");
                return fetch_script(&resolved, cfg);
            }
        }
        eprintln!("[skip] {url}: ES module syntax requires a module loader");
        return None;
    }
    Some(body)
}

/// If `src` is a JS module whose only statement is a single side-effect import
/// (`import './bundle.js'` or `import "../path/to/bundle.js"`), return the
/// specifier string.  Returns `None` for anything more complex.
///
/// This handles the common Vite/Rollup entry-point shim pattern where the HTML
/// `<script type="module">` points at a tiny file that just re-exports a bundle.
fn single_reexport_target(src: &str) -> Option<&str> {
    // Strip block comments and collapse whitespace just enough to check structure.
    let s = src.trim();
    // Must start with `import ` and contain exactly one statement.
    if !s.starts_with("import ") {
        return None;
    }
    // A bare side-effect import looks like: import 'specifier' or import "specifier"
    // optionally followed by a semicolon and nothing else (modulo whitespace).
    let after_import = s["import".len()..].trim_start();
    let (quote, rest) = match after_import.chars().next()? {
        '\'' => ('\'', &after_import[1..]),
        '"'  => ('"',  &after_import[1..]),
        _    => return None, // not a bare side-effect import
    };
    let specifier_end = rest.find(quote)?;
    let specifier = &rest[..specifier_end];
    // Verify there is nothing meaningful after the closing quote.
    let tail = rest[specifier_end + 1..].trim().trim_start_matches(';').trim();
    if !tail.is_empty() {
        return None; // more than one statement
    }
    // Only follow relative or absolute-path specifiers; skip bare specifiers
    // (npm package names) that require a module resolver.
    if specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/') {
        Some(specifier)
    } else {
        None
    }
}

/// Resolve and fetch all script sources, returning a list of executable JS strings.
///
/// Inline scripts are returned as-is. External scripts are resolved against `page_url`
/// and fetched; any that fail or are skipped (e.g. ES module files) are omitted.
fn load_scripts(
    sources: Vec<dom::ScriptSource>,
    page_url: Option<&str>,
    cfg: &HttpConfig,
) -> Vec<String> {
    sources
        .into_iter()
        .filter_map(|s| match s {
            dom::ScriptSource::Inline(code) => Some(code),
            dom::ScriptSource::External(src) => {
                let url = resolve_url(&src, page_url)?;
                eprintln!("[fetch] {url}");
                fetch_script(&url, cfg)
            }
        })
        .collect()
}

/// Parse `input`, execute its scripts, and return the rendered HTML.
///
/// `is_js` — when `true`, `input` is treated as a bare JS snippet and wrapped in a
/// minimal HTML document before processing (used for `.js` file inputs).
///
/// `page_url` — the URL the page was fetched from, used for resolving relative script
/// `src` attributes and populating `window.location`.
///
/// Script errors are non-fatal; execution continues with the next script.
/// `console.log/warn/error` output is printed to stderr with a `[console]` prefix.
///
/// When `clean` is `true` a post-processing pass is applied (see [`clean_document`]).
pub fn render(
    input: &str,
    is_js: bool,
    page_url: Option<&str>,
    cfg: &HttpConfig,
    clean: bool,
) -> anyhow::Result<String> {
    let html = if is_js {
        format!("<!DOCTYPE html><html><head></head><body><script>{input}</script></body></html>")
    } else {
        input.to_owned()
    };

    let doc = dom::parse(&html);
    let scripts = load_scripts(doc.extract_scripts(), page_url, cfg);

    let rt = runtime::JsRuntime::new();
    rt.execute(&scripts, page_url)?;

    for msg in rt.logged_messages() {
        eprintln!("[console] {msg}");
    }

    let body_html = rt.body_inner_html();

    // Avoid clobbering large server-rendered bodies (SSR sites) with a tiny JS DOM
    // result (e.g. a measurement div appended for scrollbar detection).
    // Only substitute the body when either:
    //   a) the raw HTML body was small (SPA skeleton, unit-test wrapper, bare JS mode), or
    //   b) the JS body is at least half the size of the server body (JS rendered real content).
    let raw_body_len = raw_body_content_len(&html);
    let effective_body = if raw_body_len < 512 || body_html.len() * 2 >= raw_body_len {
        body_html.as_str()
    } else {
        ""
    };

    let out = doc.serialize_with_body_and_injection(effective_body, &rt.written_html());
    Ok(if clean { clean_document(out) } else { out })
}

/// Strip scripts and unwrap `<noscript>` elements from rendered HTML.
///
/// Intended to produce a static, crawlable snapshot similar to what
/// prerendering services (Prerender.io, rendertron) deliver to bots:
///
/// - `<script>` elements (both inline and `src=`) are removed entirely.
/// - `<link rel="modulepreload">` and `<link rel="preload" as="script">` are removed.
/// - `<noscript>` wrappers are removed but their inner content is kept, so
///   crawlers see any fallback markup (e.g. `<meta>` redirects, image links).
pub fn clean_document(mut html: String) -> String {
    html = remove_script_elements(html);
    html = remove_preload_links(html);
    html = unwrap_noscript(html);
    html
}

/// Remove all `<script>…</script>` elements.
fn remove_script_elements(mut html: String) -> String {
    const OPEN: &str = "<script";
    const CLOSE: &str = "</script>";
    while let Some(start) = html.find(OPEN) {
        // Guard against false matches like a hypothetical <scriptures> tag.
        let next = html.as_bytes().get(start + OPEN.len()).copied();
        if !matches!(next, Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>') | Some(b'/') | None) {
            break;
        }
        let end = html[start..]
            .find(CLOSE)
            .map(|p| start + p + CLOSE.len())
            .unwrap_or(html.len());
        html.drain(start..end);
    }
    html
}

/// Remove `<link rel="modulepreload">` and `<link rel="preload" as="script">` elements.
fn remove_preload_links(mut html: String) -> String {
    const OPEN: &str = "<link";
    let mut pos = 0;
    while let Some(rel) = html[pos..].find(OPEN).map(|p| p + pos) {
        let tag_end = match html[rel..].find('>') {
            Some(p) => rel + p + 1,
            None => break,
        };
        let tag = &html[rel..tag_end];
        let is_modulepreload = tag.contains("modulepreload");
        let is_preload_script = tag.contains("preload") && tag.contains("as=\"script\"");
        if is_modulepreload || is_preload_script {
            html.drain(rel..tag_end);
        } else {
            pos = tag_end;
        }
    }
    html
}

/// Remove `<noscript>` and `</noscript>` tags, keeping the content between them.
fn unwrap_noscript(mut html: String) -> String {
    // html5ever always lowercases tag names; no attributes appear on <noscript>.
    loop {
        // Remove opening tag (may have no attributes, so just "<noscript>")
        let Some(open_start) = html.find("<noscript") else { break };
        let Some(open_end) = html[open_start..].find('>').map(|p| open_start + p + 1) else { break };
        html.drain(open_start..open_end);
        // Remove the matching closing tag (now starts searching from open_start).
        if let Some(close) = html[open_start..].find("</noscript>").map(|p| open_start + p) {
            html.drain(close..close + "</noscript>".len());
        }
    }
    html
}

/// Return the byte length of the content inside `<body>...</body>`, excluding the tags.
///
/// Used by [`render`] to decide whether the JS-rendered body is substantial enough to
/// replace the server-rendered body (SSR heuristic).
fn raw_body_content_len(html: &str) -> usize {
    let body_start = html.find("<body").unwrap_or(0);
    let content_start = html[body_start..].find('>').map(|i| i + body_start + 1).unwrap_or(0);
    let body_end = html.rfind("</body>").unwrap_or(html.len());
    body_end.saturating_sub(content_start)
}

/// Fetch `url`, execute its scripts, and return the rendered HTML.
///
/// Convenience wrapper around [`render`] that handles the HTTP fetch.
pub fn render_url(url: &str, cfg: &HttpConfig, clean: bool) -> anyhow::Result<String> {
    let body = cfg.apply(ureq::get(url)).call()?.into_string()?;
    render(&body, false, Some(url), cfg, clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_simple(input: &str, is_js: bool, page_url: Option<&str>) -> anyhow::Result<String> {
        render(input, is_js, page_url, &HttpConfig::default(), false)
    }

    #[test]
    fn html_inline_script_document_write() {
        let input = concat!(
            "<!DOCTYPE html><html><head><title>Test</title></head>",
            "<body><h1>Before</h1>",
            r#"<script>document.write("<p>Hello from JS!</p>"); console.log("done");</script>"#,
            "</body></html>"
        );
        let out = render_simple(input, false, None).unwrap();
        assert!(out.contains("<h1>Before</h1>"), "static content preserved");
        assert!(
            out.contains("<p>Hello from JS!</p>"),
            "document.write injected"
        );
    }

    #[test]
    fn js_file_mode_loop() {
        let js = concat!(
            r#"document.write("<ul>");"#,
            "\n",
            r#"for (let i = 1; i <= 3; i++) { document.write("<li>Item " + i + "</li>"); }"#,
            "\n",
            r#"document.write("</ul>");"#,
            "\n",
            r#"console.log("rendered", 3, "items");"#,
        );
        let out = render_simple(js, true, None).unwrap();
        assert!(out.contains("<li>Item 1</li>"), "first item");
        assert!(out.contains("<li>Item 2</li>"), "second item");
        assert!(out.contains("<li>Item 3</li>"), "third item");
    }

    #[test]
    fn console_messages_captured() {
        let js = r#"console.log("hello", "world"); console.warn("oops");"#;
        let rt = runtime::JsRuntime::new();
        rt.execute(&[js.to_owned()], None).unwrap();
        let msgs = rt.logged_messages();
        assert_eq!(msgs[0], "hello world");
        assert_eq!(msgs[1], "oops");
    }

    #[test]
    fn document_writeln_adds_newline() {
        let js = r#"document.writeln("line1"); document.writeln("line2");"#;
        let out = render_simple(js, true, None).unwrap();
        assert!(out.contains("line1\nline2\n"), "writeln appends newline");
    }

    #[test]
    fn window_aliases_global() {
        let js = r#"window.document.write("<p>via window</p>");"#;
        let out = render_simple(js, true, None).unwrap();
        assert!(
            out.contains("<p>via window</p>"),
            "window.document.write works"
        );
    }

    #[test]
    fn script_errors_are_non_fatal() {
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<script>throw new Error('deliberate');</script>",
            "<script>document.write('<p>survived</p>');</script>",
            "</body></html>"
        );
        let out = render_simple(html, false, None).unwrap();
        assert!(
            out.contains("<p>survived</p>"),
            "rendering continues after script error"
        );
    }

    #[test]
    fn location_href_reflects_page_url() {
        let js = r#"document.write(window.location.href);"#;
        let out = render_simple(js, true, Some("https://example.com/page")).unwrap();
        assert!(
            out.contains("https://example.com/page"),
            "location.href set from page_url"
        );
    }

    #[test]
    fn common_globals_accessible() {
        let js = r#"
            var ua = window.navigator.userAgent;
            var tid = window.setTimeout(function(){}, 100);
            var mq  = window.matchMedia('(max-width: 768px)');
            var mo  = new window.MutationObserver(function(){});
            document.write('<p>' + ua + '</p>');
        "#;
        let out = render_simple(js, true, None).unwrap();
        assert!(out.contains("<p>rakers/"), "navigator.userAgent accessible");
    }

    #[test]
    fn document_create_element_is_accessible() {
        let js = r#"
            var el = document.createElement('div');
            el.className = 'test';
            document.write('<p>' + el.className + '</p>');
        "#;
        let out = render_simple(js, true, None).unwrap();
        assert!(out.contains("<p>test</p>"), "createElement stub works");
    }

    #[test]
    fn settimeout_callback_flushed() {
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            r#"<div id="app"></div>"#,
            "<script>setTimeout(function() {",
            r#"document.getElementById('app').innerHTML = '<h1>Rendered via setTimeout</h1>';"#,
            "}, 0);</script>",
            "</body></html>"
        );
        let out = render_simple(html, false, None).unwrap();
        assert!(
            out.contains("<h1>Rendered via setTimeout</h1>"),
            "setTimeout callback flushed before readback"
        );
    }

    #[test]
    fn body_inner_html_set_directly() {
        let js = r#"document.body.innerHTML = '<h1>Set directly</h1>';"#;
        let out = render_simple(js, true, None).unwrap();
        assert!(
            out.contains("<h1>Set directly</h1>"),
            "body.innerHTML = '...' captured"
        );
    }

    #[test]
    fn append_child_to_body() {
        let js = r#"
            var h1 = document.createElement('h1');
            h1.innerHTML = 'Appended';
            document.body.appendChild(h1);
        "#;
        let out = render_simple(js, true, None).unwrap();
        assert!(
            out.contains("<h1>Appended</h1>"),
            "appendChild serialized into output"
        );
    }

    #[test]
    fn nested_elements_serialized() {
        let js = r#"
            var ul = document.createElement('ul');
            for (var i = 1; i <= 3; i++) {
                var li = document.createElement('li');
                li.innerHTML = 'Item ' + i;
                ul.appendChild(li);
            }
            document.body.appendChild(ul);
        "#;
        let out = render_simple(js, true, None).unwrap();
        assert!(out.contains("<li>Item 1</li>"), "nested li 1");
        assert!(out.contains("<li>Item 3</li>"), "nested li 3");
    }

    #[test]
    fn get_element_by_id_content_with_append() {
        let js = r#"
            var app = document.getElementById('app');
            app.innerHTML = '<p>App content</p>';
            document.body.appendChild(app);
        "#;
        let out = render_simple(js, true, None).unwrap();
        assert!(
            out.contains("<p>App content</p>"),
            "getElementById + appendChild captured"
        );
    }

    #[test]
    fn clean_removes_scripts_and_unwraps_noscript() {
        let html = concat!(
            "<!DOCTYPE html><html><head>",
            r#"<link rel="modulepreload" href="/bundle.js">"#,
            r#"<link rel="preload" as="script" href="/chunk.js">"#,
            r#"<link rel="stylesheet" href="/style.css">"#, // must be kept
            "</head><body>",
            "<h1>Hello</h1>",
            r#"<script src="/app.js"></script>"#,
            "<script>var x = 1;</script>",
            "<noscript><p>JS required</p></noscript>",
            "</body></html>",
        );
        let out = render(html, false, None, &HttpConfig::default(), true).unwrap();
        assert!(!out.contains("<script"),      "script tags removed");
        assert!(!out.contains("modulepreload"),"modulepreload link removed");
        assert!(!out.contains(r#"as="script""#), "preload-script link removed");
        assert!( out.contains(r#"rel="stylesheet""#), "stylesheet link preserved");
        assert!(!out.contains("<noscript"),    "noscript tags removed");
        assert!( out.contains("<p>JS required</p>"), "noscript content preserved");
        assert!( out.contains("<h1>Hello</h1>"),     "regular content preserved");
    }

    #[test]
    fn to_json_fields() {
        let out = to_json(100, "<h1>hi</h1>");
        assert!(out.contains("\"raw_bytes\": 100"), "raw_bytes field");
        assert!(out.contains("\"rendered_bytes\": 11"), "rendered_bytes field");
        assert!(out.contains("\"html\""), "html field present");
        assert!(out.contains("<h1>hi</h1>"), "html content");
    }

    #[test]
    fn to_json_escapes_special_chars() {
        let out = to_json(0, "say \"hello\"\nline2\\end");
        assert!(out.contains(r#"say \"hello\"\nline2\\end"#), "quotes, newline, backslash escaped: {out}");
    }

    #[test]
    fn single_reexport_target_detects_shim() {
        assert_eq!(single_reexport_target("import './bundle.js'"), Some("./bundle.js"));
        assert_eq!(single_reexport_target("import \"../dist/app.js\";"), Some("../dist/app.js"));
        assert_eq!(single_reexport_target("import '/assets/main.js'\n"), Some("/assets/main.js"));
        // Multiple statements — not a shim
        assert_eq!(single_reexport_target("import './a.js'\nimport './b.js'"), None);
        // Named import — not a bare side-effect import
        assert_eq!(single_reexport_target("import { foo } from './lib.js'"), None);
        // Bare specifier (npm package) — don't follow
        assert_eq!(single_reexport_target("import 'react'"), None);
        // Regular IIFE bundle — not a module
        assert_eq!(single_reexport_target("(function(){ var x = 1; })()"), None);
    }
}
