use assert_cmd::Command;
use predicates::prelude::*;

fn run_hook(event: &str, stdin: &str) -> assert_cmd::assert::Assert {
    Command::cargo_bin("harness").unwrap()
        .args(["hook", event])
        .write_stdin(stdin.to_string())
        .assert()
}

#[test]
fn session_start_injects_protocol_and_gate_summary() {
    // Use an isolated $HOME so this test can't pick up a real, possibly
    // strict, global ~/.claude/harness/config.toml and falsely fail the
    // "advisory" (built-in default mode) assertion below.
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("harness")
        .unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "session-start"])
        .write_stdin(r#"{"cwd": "/nonexistent"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("HARNESS-PROTOCOL"))
        .stdout(predicate::str::contains("advisory")); // built-in default mode
}

#[test]
fn user_prompt_emits_one_line_nudge() {
    let assert = run_hook("user-prompt", "{}").success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.trim().lines().count() == 1, "nudge must be one line: {out}");
    assert!(out.contains("Harness"));
}

#[test]
fn hooks_fail_open_on_garbage_stdin() {
    run_hook("session-start", "not json at all").success();
    run_hook("user-prompt", "").success();
}

#[test]
fn stop_gate_end_to_end_strict_block() {
    // Arrange: use post-tool to create a "changed code, no test" state, then hit stop.
    // $HOME is isolated so session state lands in the tempdir, not the real
    // ~/.claude/harness/state, and no real global config can interfere.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("harness.toml"),
        "[gates.verify]\nmode = \"strict\"",
    )
    .unwrap();
    let sid = "e2e-strict-block";
    let cwd = tmp.path().to_str().unwrap();
    let run_hook_at_home = |event: &str, stdin: &str| {
        Command::cargo_bin("harness").unwrap()
            .env("HOME", tmp.path())
            .args(["hook", event])
            .write_stdin(stdin.to_string())
            .assert()
    };
    run_hook_at_home(
        "post-tool",
        &format!(
            r#"{{"session_id":"{sid}","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"{cwd}/src/x.rs"}}}}"#
        ),
    )
    .success();
    run_hook_at_home(
        "stop",
        &format!(r#"{{"session_id":"{sid}","cwd":"{cwd}","stop_hook_active":false}}"#),
    )
    .success()
    .stdout(predicate::str::contains("\"decision\":\"block\""));
}
