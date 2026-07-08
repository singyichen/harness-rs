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
    // Plant a corrupt state file in the isolated $HOME's state dir
    let sid = "e2e-corrupt";
    let dir = tmp.path().join(".claude/harness/state");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{sid}.json")), "{{{broken").unwrap();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "stop"])
        .write_stdin(format!(r#"{{"session_id":"{sid}","stop_hook_active":false}}"#))
        .assert()
        .success()
        .stdout(""); // corrupt state → treated as clean → allow with no output
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
}

#[test]
fn project_scoped_full_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();

    harness(&home).current_dir(&proj).args(["install", "--project"]).assert().success()
        .stdout(predicate::str::contains("harness installed for this project"))
        .stdout(predicate::str::contains("harness init"));
    // Released into the project, not the global home
    assert!(proj.join(".claude/harness/manifest.json").is_file());
    assert!(proj.join(".claude/agents/skeptic.md").is_file());
    assert!(!proj.join(".claude/harness/config.toml").exists());
    assert!(!home.join(".claude/harness/manifest.json").exists());

    harness(&home).current_dir(&proj).args(["doctor", "--project"]).assert().success()
        .stdout(predicate::str::contains("All checks passed"));

    harness(&home).current_dir(&proj).args(["update", "--project"]).assert().success()
        .stdout(predicate::str::contains("already up to date"));

    harness(&home).current_dir(&proj).args(["uninstall", "--project"]).assert().success()
        .stdout(predicate::str::contains("harness removed from this project"));
    assert!(!proj.join(".claude/harness/manifest.json").exists());
    assert!(!proj.join(".claude/agents/skeptic.md").exists());
    let settings = std::fs::read_to_string(proj.join(".claude/settings.json")).unwrap();
    assert!(!settings.contains("harness hook"));
}

#[test]
fn global_and_project_installs_coexist() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();

    harness(&home).current_dir(&proj).arg("install").assert().success();
    harness(&home).current_dir(&proj).args(["install", "--project"]).assert().success();

    // Hook command strings must be character-for-character identical in
    // both layers — Claude Code's native de-duplication depends on it.
    let read_cmds = |p: &std::path::Path| -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let mut cmds: Vec<String> = v["hooks"].as_object().unwrap().values()
            .flat_map(|groups| groups.as_array().unwrap().iter())
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .filter_map(|h| h["command"].as_str().map(String::from))
            .filter(|c| c.contains("harness hook"))
            .collect();
        cmds.sort();
        cmds
    };
    assert_eq!(
        read_cmds(&home.join(".claude/settings.json")),
        read_cmds(&proj.join(".claude/settings.json"))
    );

    // Doctor points out the coexistence from both viewpoints.
    harness(&home).current_dir(&proj).arg("doctor").assert().success()
        .stdout(predicate::str::contains("doctor --project"));
    harness(&home).current_dir(&proj).args(["doctor", "--project"]).assert().success()
        .stdout(predicate::str::contains("de-duplicated"));

    // Removing the project install leaves the global one untouched.
    harness(&home).current_dir(&proj).args(["uninstall", "--project"]).assert().success();
    assert!(home.join(".claude/harness/manifest.json").is_file());
    assert!(home.join(".claude/agents/skeptic.md").is_file());
}

#[test]
fn project_install_in_home_directory_prints_overlap_note() {
    let tmp = tempfile::tempdir().unwrap();
    // cwd == $HOME → <cwd>/.claude is the global location
    harness(tmp.path()).current_dir(tmp.path()).args(["install", "--project"]).assert().success()
        .stdout(predicate::str::contains("effectively a global install"));
}
