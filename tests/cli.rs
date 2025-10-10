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
    assert!(content.contains("<p>written</p>"), "output file missing rendered content");
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
