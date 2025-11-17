//! Core rendering pipeline for rakers.
//!
//! Parses HTML, collects and executes scripts in a sandboxed JS context,
//! then serializes the post-execution DOM back to HTML.

mod dom;
mod runtime;

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
        eprintln!("[skip] {url}: ES module syntax requires a module loader");
        return None;
    }
    Some(body)
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
pub fn render(
    input: &str,
    is_js: bool,
    page_url: Option<&str>,
    cfg: &HttpConfig,
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

    Ok(doc.serialize_with_body_and_injection(effective_body, &rt.written_html()))
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
pub fn render_url(url: &str, cfg: &HttpConfig) -> anyhow::Result<String> {
    let body = cfg.apply(ureq::get(url)).call()?.into_string()?;
    render(&body, false, Some(url), cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_simple(input: &str, is_js: bool, page_url: Option<&str>) -> anyhow::Result<String> {
        render(input, is_js, page_url, &HttpConfig::default())
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
}
