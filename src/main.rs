mod dom;
mod runtime;

use clap::Parser;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

#[derive(Parser)]
#[command(
    name = "rakers",
    about = "Render JavaScript into HTML using Servo's HTML parser (html5ever)"
)]
struct Cli {
    /// File path, URL (http/https), or omit to read stdin as HTML
    input: Option<String>,

    /// Write output to FILE instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn fetch(input: &str) -> anyhow::Result<(String, bool)> {
    if is_url(input) {
        let body = ureq::get(input).call()?.into_string()?;
        Ok((body, false))
    } else {
        let content = fs::read_to_string(input)?;
        let is_js = Path::new(input).extension().map(|e| e == "js").unwrap_or(false);
        Ok((content, is_js))
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let page_url = cli.input.as_deref().filter(|s| is_url(s));

    let (input, is_js) = match &cli.input {
        Some(src) => fetch(src)?,
        None => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            (s, false)
        }
    };

    let result = render(&input, is_js, page_url)?;

    match &cli.output {
        Some(path) => fs::write(path, &result)?,
        None => io::stdout().write_all(result.as_bytes())?,
    }

    Ok(())
}

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

fn fetch_script(url: &str) -> Option<String> {
    match ureq::get(url).call() {
        Ok(r) => r.into_string().ok(),
        Err(e) => { eprintln!("[fetch error] {url}: {e}"); None }
    }
}

fn load_scripts(sources: Vec<dom::ScriptSource>, page_url: Option<&str>) -> Vec<String> {
    sources.into_iter().filter_map(|s| match s {
        dom::ScriptSource::Inline(code) => Some(code),
        dom::ScriptSource::External(src) => {
            let url = resolve_url(&src, page_url)?;
            eprintln!("[fetch] {url}");
            fetch_script(&url)
        }
    }).collect()
}

fn render(input: &str, is_js: bool, page_url: Option<&str>) -> anyhow::Result<String> {
    let html = if is_js {
        format!(
            "<!DOCTYPE html><html><head></head><body><script>{input}</script></body></html>"
        )
    } else {
        input.to_owned()
    };

    let doc = dom::parse(&html);
    let scripts = load_scripts(doc.extract_scripts(), page_url);

    let rt = runtime::JsRuntime::new();
    rt.execute(&scripts, page_url)?;

    for msg in rt.logged_messages() {
        eprintln!("[console] {msg}");
    }

    Ok(doc.serialize_with_body_and_injection(&rt.body_inner_html(), &rt.written_html()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_inline_script_document_write() {
        let input = concat!(
            "<!DOCTYPE html><html><head><title>Test</title></head>",
            "<body><h1>Before</h1>",
            r#"<script>document.write("<p>Hello from JS!</p>"); console.log("done");</script>"#,
            "</body></html>"
        );
        let out = render(input, false, None).unwrap();
        assert!(out.contains("<h1>Before</h1>"), "static content preserved");
        assert!(out.contains("<p>Hello from JS!</p>"), "document.write injected");
    }

    #[test]
    fn js_file_mode_loop() {
        let js = concat!(
            r#"document.write("<ul>");"#, "\n",
            r#"for (let i = 1; i <= 3; i++) { document.write("<li>Item " + i + "</li>"); }"#, "\n",
            r#"document.write("</ul>");"#, "\n",
            r#"console.log("rendered", 3, "items");"#,
        );
        let out = render(js, true, None).unwrap();
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
        let out = render(js, true, None).unwrap();
        assert!(out.contains("line1\nline2\n"), "writeln appends newline");
    }

    // --- new tests for browser globals ---

    #[test]
    fn window_aliases_global() {
        // window.document.write must work (window === globalThis)
        let js = r#"window.document.write("<p>via window</p>");"#;
        let out = render(js, true, None).unwrap();
        assert!(out.contains("<p>via window</p>"), "window.document.write works");
    }

    #[test]
    fn script_errors_are_non_fatal() {
        // A throwing script must not prevent subsequent scripts from running.
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<script>throw new Error('deliberate');</script>",
            "<script>document.write('<p>survived</p>');</script>",
            "</body></html>"
        );
        let out = render(html, false, None).unwrap();
        assert!(out.contains("<p>survived</p>"), "rendering continues after script error");
    }

    #[test]
    fn location_href_reflects_page_url() {
        let js = r#"document.write(window.location.href);"#;
        let out = render(js, true, Some("https://example.com/page")).unwrap();
        assert!(out.contains("https://example.com/page"), "location.href set from page_url");
    }

    #[test]
    fn common_globals_accessible() {
        // navigator, setTimeout, matchMedia, MutationObserver must not throw.
        let js = r#"
            var ua = window.navigator.userAgent;
            var tid = window.setTimeout(function(){}, 100);
            var mq  = window.matchMedia('(max-width: 768px)');
            var mo  = new window.MutationObserver(function(){});
            document.write('<p>' + ua + '</p>');
        "#;
        let out = render(js, true, None).unwrap();
        assert!(out.contains("<p>rakers/"), "navigator.userAgent accessible");
    }

    #[test]
    fn document_create_element_is_accessible() {
        let js = r#"
            var el = document.createElement('div');
            el.className = 'test';
            document.write('<p>' + el.className + '</p>');
        "#;
        let out = render(js, true, None).unwrap();
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
        let out = render(html, false, None).unwrap();
        assert!(out.contains("<h1>Rendered via setTimeout</h1>"), "setTimeout callback flushed before readback");
    }

    // ── DOM-mutation rendering (the main improvement) ────────────────────────

    #[test]
    fn body_inner_html_set_directly() {
        let js = r#"document.body.innerHTML = '<h1>Set directly</h1>';"#;
        let out = render(js, true, None).unwrap();
        assert!(out.contains("<h1>Set directly</h1>"), "body.innerHTML = '...' captured");
    }

    #[test]
    fn append_child_to_body() {
        let js = r#"
            var h1 = document.createElement('h1');
            h1.innerHTML = 'Appended';
            document.body.appendChild(h1);
        "#;
        let out = render(js, true, None).unwrap();
        assert!(out.contains("<h1>Appended</h1>"), "appendChild serialized into output");
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
        let out = render(js, true, None).unwrap();
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
        let out = render(js, true, None).unwrap();
        assert!(out.contains("<p>App content</p>"), "getElementById + appendChild captured");
    }
}
