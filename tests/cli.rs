use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

fn cmd() -> Command {
    Command::cargo_bin("rakers").unwrap()
}

#[test]
fn help_exits_cleanly() {
    cmd().arg("--help").assert().success();
}

#[test]
fn stdin_document_write() {
    cmd()
        .write_stdin(r#"<script>document.write("<p>hello</p>")</script>"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>hello</p>"));
}

#[test]
fn static_html_passthrough() {
    cmd()
        .write_stdin("<html><body><h1>Static</h1></body></html>")
        .assert()
        .success()
        .stdout(predicate::str::contains("<h1>Static</h1>"));
}

#[test]
fn console_log_goes_to_stderr() {
    cmd()
        .write_stdin(r#"<script>console.log("test message")</script>"#)
        .assert()
        .success()
        .stderr(predicate::str::contains("[console] test message"));
}

#[test]
fn script_error_is_non_fatal() {
    cmd()
        .write_stdin(concat!(
            r#"<script>throw new Error("oops")</script>"#,
            r#"<script>document.write("<p>survived</p>")</script>"#,
        ))
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>survived</p>"));
}

#[test]
fn html_file_arg() {
    let mut f = tempfile::Builder::new().suffix(".html").tempfile().unwrap();
    write!(
        f,
        r#"<html><body><script>document.write("<p>from file</p>")</script></body></html>"#
    )
    .unwrap();
    cmd()
        .arg(f.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>from file</p>"));
}

#[test]
fn js_file_arg_wraps_in_html() {
    let mut f = tempfile::Builder::new().suffix(".js").tempfile().unwrap();
    write!(f, r#"document.write("<p>from js</p>")"#).unwrap();
    cmd()
        .arg(f.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>from js</p>"));
}

#[test]
fn output_flag_writes_file_not_stdout() {
    let out = tempfile::NamedTempFile::new().unwrap();
    cmd()
        .write_stdin(r#"<script>document.write("<p>written</p>")</script>"#)
        .args(["-o", out.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    let content = std::fs::read_to_string(out.path()).unwrap();
    assert!(
        content.contains("<p>written</p>"),
        "output file missing rendered content"
    );
}

/// TodoMVC React SPA: server returns a ~645-byte skeleton with an empty `<section id="root">`;
/// the React bundle renders the full todo-app UI into it.
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn todomvc_react_renders_ui() {
    let output = cmd()
        .arg("https://todomvc.com/examples/react/dist/")
        .output()
        .unwrap();

    assert!(output.status.success(), "rakers exited with non-zero status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Raw skeleton has an empty <section id="root">; React fills it in.
    assert!(
        stdout.contains("<h1>todos</h1>"),
        "'<h1>todos</h1>' absent — React may not have rendered the TodoMVC UI"
    );
    assert!(
        stdout.contains("class=\"new-todo\""),
        "new-todo input absent — React may not have rendered the TodoMVC UI"
    );
}

/// React SPA: server returns a ~2.7 KB skeleton; the bundle renders the full UI.
/// Ignored under boa — it overflows the native stack on the React bundle.
#[test]
#[cfg_attr(feature = "boa", ignore = "boa overflows on large React bundles")]
fn jsbench_url_renders_react_ui() {
    let output = cmd()
        .arg("https://jsbench.me")
        .output()
        .unwrap();

    assert!(output.status.success(), "rakers exited with non-zero status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.len() > 4_000, "output too small ({} bytes) — React may not have run", stdout.len());
    assert!(stdout.to_lowercase().contains("run"), "'run' absent — React UI may not have rendered");
}

#[test]
fn pretty_flag_indents_output() {
    cmd()
        .arg("--pretty")
        .write_stdin("<html><body><div><p>hello</p></div></body></html>")
        .assert()
        .success()
        // Block elements each start on their own indented line.
        .stdout(predicate::str::contains("\n  <body>"))
        .stdout(predicate::str::contains("\n    <div>"))
        .stdout(predicate::str::contains("\n      <p>"))
        // Inline content stays on the same line as the text.
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn pretty_flag_script_content_verbatim() {
    // '<' inside a <script> body must not be parsed as a tag start.
    cmd()
        .arg("--pretty")
        .write_stdin("<html><body><script>var x = 1 < 2;</script></body></html>")
        .assert()
        .success()
        .stdout(predicate::str::contains("var x = 1 < 2;"));
}

#[test]
fn json_flag_emits_json_object() {
    let out = cmd()
        .arg("--json")
        .write_stdin(r#"<script>document.write("<p>hi</p>")</script>"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\"raw_bytes\""),      "raw_bytes field absent");
    assert!(s.contains("\"rendered_bytes\""), "rendered_bytes field absent");
    assert!(s.contains("\"html\""),           "html field absent");
    assert!(s.contains("<p>hi</p>"),          "rendered content absent");
}

#[test]
fn json_and_pretty_combined() {
    let out = cmd()
        .args(["--json", "--pretty"])
        .write_stdin("<html><body><div><p>test</p></div></body></html>")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    // Pretty-printed HTML is embedded inside the JSON html field (newlines escaped).
    assert!(s.contains("\\n"), "pretty newlines should be JSON-escaped in html field");
    assert!(s.contains("\"rendered_bytes\""), "rendered_bytes field absent");
}

#[test]
fn invalid_header_format_fails() {
    cmd()
        .args(["-H", "no-colon-here"])
        .write_stdin("<html></html>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid header"));
}
