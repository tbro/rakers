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

    let (input, is_js) = match &cli.input {
        Some(src) => fetch(src)?,
        None => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            (s, false)
        }
    };

    let result = render(&input, is_js)?;

    match &cli.output {
        Some(path) => fs::write(path, &result)?,
        None => io::stdout().write_all(result.as_bytes())?,
    }


    Ok(())
}

fn render(input: &str, is_js: bool) -> anyhow::Result<String> {
    let html = if is_js {
        format!(
            "<!DOCTYPE html><html><head></head><body><script>{input}</script></body></html>"
        )
    } else {
        input.to_owned()
    };

    let doc = dom::parse(&html);
    let scripts = doc.extract_scripts();

    let rt = runtime::JsRuntime::new();
    rt.execute(&scripts)?;

    for msg in rt.logged_messages() {
        eprintln!("[console] {msg}");
    }

    Ok(doc.serialize_with_injection(&rt.written_html()))
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
        let out = render(input, false).unwrap();
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
        let out = render(js, true).unwrap();
        assert!(out.contains("<li>Item 1</li>"), "first item");
        assert!(out.contains("<li>Item 2</li>"), "second item");
        assert!(out.contains("<li>Item 3</li>"), "third item");
    }

    #[test]
    fn console_messages_captured() {
        let js = r#"console.log("hello", "world"); console.warn("oops");"#;
        let rt = runtime::JsRuntime::new();
        rt.execute(&[js.to_owned()]).unwrap();
        let msgs = rt.logged_messages();
        assert_eq!(msgs[0], "hello world");
        assert_eq!(msgs[1], "oops");
    }

    #[test]
    fn document_writeln_adds_newline() {
        let js = r#"document.writeln("line1"); document.writeln("line2");"#;
        let out = render(js, true).unwrap();
        assert!(out.contains("line1\nline2\n"), "writeln appends newline");
    }
}
