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
    run_hook("session-start", r#"{"cwd": "/nonexistent"}"#)
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
    // Arrange: use post-tool to create a "changed code, no test" state, then hit stop
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("harness.toml"),
        "[gates.verify]\nmode = \"strict\"",
    )
    .unwrap();
    let sid = "e2e-strict-block";
    let cwd = tmp.path().to_str().unwrap();
    run_hook(
        "post-tool",
        &format!(
            r#"{{"session_id":"{sid}","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"{cwd}/src/x.rs"}}}}"#
        ),
    )
    .success();
    run_hook(
        "stop",
        &format!(r#"{{"session_id":"{sid}","cwd":"{cwd}","stop_hook_active":false}}"#),
    )
    .success()
    .stdout(predicate::str::contains("\"decision\":\"block\""));
    // Clean up the state file so reruns are not polluted
    let _ = std::fs::remove_file(
        std::env::temp_dir().join("harness-state").join(format!("{sid}.json")),
    );
}
