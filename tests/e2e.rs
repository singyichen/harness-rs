use assert_cmd::Command;
use predicates::prelude::*;

fn harness(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("harness").unwrap();
    c.env("HOME", home);
    c
}

#[test]
fn full_lifecycle_install_doctor_uninstall() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    harness(home).arg("install").assert().success()
        .stdout(predicate::str::contains("harness installed"));

    harness(home).arg("doctor").assert().success()
        .stdout(predicate::str::contains("All checks passed"));

    // Install is idempotent
    harness(home).arg("install").assert().success();
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert_eq!(settings.matches("harness hook stop").count(), 1);

    harness(home).arg("uninstall").assert().success();
    harness(home).arg("doctor").assert().failure()
        .stdout(predicate::str::contains("harness install"));
}

#[test]
fn hook_stop_fails_open_with_corrupt_state() {
    let tmp = tempfile::tempdir().unwrap();
    // Plant a corrupt state file
    let sid = "e2e-corrupt";
    let dir = std::env::temp_dir().join("harness-state");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{sid}.json")), "{{{broken").unwrap();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "stop"])
        .write_stdin(format!(r#"{{"session_id":"{sid}","stop_hook_active":false}}"#))
        .assert()
        .success()
        .stdout(""); // corrupt state → treated as clean → allow with no output
    let _ = std::fs::remove_file(dir.join(format!("{sid}.json")));
}

#[test]
fn advisory_mode_warns_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    // No harness.toml → built-in advisory mode
    let sid = "e2e-advisory";
    let cwd = proj.to_str().unwrap();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "post-tool"])
        .write_stdin(format!(
            r#"{{"session_id":"{sid}","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"{cwd}/src/a.rs"}}}}"#
        ))
        .assert()
        .success();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "stop"])
        .write_stdin(format!(r#"{{"session_id":"{sid}","cwd":"{cwd}","stop_hook_active":false}}"#))
        .assert()
        .success()
        .stdout(predicate::str::contains("systemMessage"));
    let _ = std::fs::remove_file(
        std::env::temp_dir().join("harness-state").join(format!("{sid}.json")),
    );
}
