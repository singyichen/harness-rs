use assert_cmd::Command;

#[test]
fn hook_with_unknown_event_fails_open() {
    // Fail-open: even an unknown event must exit 0 with no output
    Command::cargo_bin("harness").unwrap()
        .args(["hook", "no-such-event"])
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn help_lists_subcommands() {
    let out = Command::cargo_bin("harness").unwrap()
        .arg("--help").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    for sub in ["install", "uninstall", "init", "doctor", "update", "config", "hook"] {
        assert!(text.contains(sub), "help is missing subcommand {sub}");
    }
}
