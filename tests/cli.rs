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

/// React SPA: server returns a ~2.7 KB skeleton; the bundle renders the full UI.
/// Requires rquickjs — boa overflows the native stack on the React bundle.
#[test]
#[cfg_attr(not(feature = "rquickjs"), ignore = "requires --features rquickjs")]
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
fn invalid_header_format_fails() {
    cmd()
        .args(["-H", "no-colon-here"])
        .write_stdin("<html></html>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid header"));
}
