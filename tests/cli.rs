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
        .arg("--verbose")
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
#[ignore = "live network test — flaky in CI"]
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
fn diff_flag_shows_unified_diff() {
    let out = cmd()
        .arg("--diff")
        .write_stdin(r#"<html><body><script>document.body.innerHTML="<h1>rendered</h1>"</script></body></html>"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("---"),      "missing --- header");
    assert!(s.contains("+++"),      "missing +++ header");
    assert!(s.contains("rendered"), "rendered content absent from diff");
}

#[test]
fn max_scripts_skips_remote_fetches() {
    // With --max-scripts 0, the remote script should be skipped (no [fetch] in stderr)
    // but the inline script should still run.
    let out = cmd()
        .args(["--max-scripts", "0", "--verbose"])
        .write_stdin(concat!(
            r#"<html><head><script src="https://example.com/app.js"></script></head>"#,
            r#"<body><script>document.write("<p>inline</p>")</script></body></html>"#,
        ))
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("<p>inline</p>"), "inline script should still run");
    assert!(!stderr.contains("[fetch]"),      "remote script should not be fetched");
    assert!(stderr.contains("[skip]"),        "skip message should appear in stderr");
}

#[test]
fn timeout_kills_infinite_loop() {
    cmd()
        .args(["--timeout", "1"])
        .write_stdin(concat!(
            r#"<html><body>"#,
            r#"<script>while(true){}</script>"#,
            r#"<script>document.write("<p>after</p>")</script>"#,
            r#"</body></html>"#,
        ))
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>after</p>"));
}

#[test]
fn timeout_subsecond_kills_loop() {
    cmd()
        .args(["--timeout", "0.5"])
        .write_stdin(concat!(
            r#"<html><body>"#,
            r#"<script>while(true){}</script>"#,
            r#"<script>document.write("<p>sub</p>")</script>"#,
            r#"</body></html>"#,
        ))
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>sub</p>"));
}

#[test]
fn timeout_zero_is_rejected() {
    cmd()
        .args(["--timeout", "0"])
        .write_stdin("<html></html>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("greater than zero"));
}

#[test]
fn no_timeout_flag_accepted() {
    cmd()
        .arg("--no-timeout")
        .write_stdin(r#"<script>document.write("<p>ok</p>")</script>"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>ok</p>"));
}

#[test]
fn timeout_and_no_timeout_conflict() {
    cmd()
        .args(["--timeout", "5", "--no-timeout"])
        .write_stdin("<html></html>")
        .assert()
        .failure();
}

#[test]
fn verbose_off_suppresses_console() {
    cmd()
        .write_stdin(r#"<script>console.log("should be hidden")</script>"#)
        .assert()
        .success()
        .stderr(predicate::str::contains("[console]").not());
}

#[test]
fn verbose_on_shows_console() {
    cmd()
        .arg("--verbose")
        .write_stdin(r#"<script>console.log("should appear")</script>"#)
        .assert()
        .success()
        .stderr(predicate::str::contains("[console] should appear"));
}

#[test]
fn selector_filters_rendered_output() {
    cmd()
        .args(["--selector", "h1"])
        .write_stdin("<html><body><h1>Title</h1><p>Other</p></body></html>")
        .assert()
        .success()
        .stdout(predicate::str::contains("<h1>Title</h1>"))
        .stdout(predicate::str::contains("<p>Other</p>").not());
}

#[test]
fn selector_with_js_rendered_content() {
    cmd()
        .args(["--selector", "#app"])
        .write_stdin(concat!(
            r#"<html><body><div id="app"></div>"#,
            r#"<script>document.getElementById('app').innerHTML='<p>rendered</p>';</script>"#,
            r#"</body></html>"#,
        ))
        .assert()
        .success()
        .stdout(predicate::str::contains("<p>rendered</p>"))
        .stdout(predicate::str::contains("<script>").not());
}

#[test]
fn selector_empty_when_no_match() {
    cmd()
        .args(["--selector", "h2"])
        .write_stdin("<html><body><h1>Title</h1></body></html>")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn invalid_selector_fails() {
    cmd()
        .args(["--selector", "##bad"])
        .write_stdin("<html></html>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid selector"));
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
