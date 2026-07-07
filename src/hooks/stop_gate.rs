use crate::config::{Config, GateMode};
use crate::hooks::session_start::payload_cwd;
use crate::state::{self, SessionState};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(payload: &Value) -> Option<String> {
    let stop_hook_active = payload
        .get("stop_hook_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let session_id = payload.get("session_id")?.as_str()?;
    let config = Config::load(&payload_cwd(payload));
    let st = state::load(&state::state_path(session_id));
    verdict(&st, config.verify.mode, stop_hook_active)
}

pub fn verdict(st: &SessionState, mode: GateMode, stop_hook_active: bool) -> Option<String> {
    if stop_hook_active || !st.unverified_changes() {
        return None;
    }
    let files: Vec<&str> = st
        .changed_files
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p.as_str())
        })
        .collect();
    let files = files.join(", ");
    match mode {
        GateMode::Off => None,
        GateMode::Strict => Some(
            json!({
                "decision": "block",
                "reason": format!(
                    "⛔ Harness verify gate: code was modified this turn ({files}) but no test run was detected afterwards. \
                     Run the relevant tests and provide fail-then-pass evidence. \
                     If tests are genuinely unnecessary (mid-task pause, experimental change), explain why to the user and end the turn again to pass."
                ),
            })
            .to_string(),
        ),
        GateMode::Advisory => Some(
            json!({
                "systemMessage": format!(
                    "⚠️ Harness notice: {files} changed but no subsequent test run was detected (advisory mode, allowing stop)."
                ),
            })
            .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GateMode;
    use crate::state::SessionState;

    fn dirty_state() -> SessionState {
        let mut st = SessionState::default();
        st.record_code_change("/p/src/a.rs");
        st
    }

    #[test]
    fn strict_blocks_unverified_changes() {
        let out = verdict(&dirty_state(), GateMode::Strict, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["decision"], "block");
        assert!(v["reason"].as_str().unwrap().contains("a.rs"));
    }

    #[test]
    fn advisory_warns_but_allows() {
        let out = verdict(&dirty_state(), GateMode::Advisory, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("decision").is_none()); // does not block
        assert!(v["systemMessage"].as_str().unwrap().contains("a.rs"));
    }

    #[test]
    fn off_is_silent() {
        assert_eq!(verdict(&dirty_state(), GateMode::Off, false), None);
    }

    #[test]
    fn verified_state_is_silent() {
        let mut st = dirty_state();
        st.record_test_run();
        assert_eq!(verdict(&st, GateMode::Strict, false), None);
    }

    #[test]
    fn stop_hook_active_always_allows() {
        // Second stop always passes — prevents infinite block loops
        assert_eq!(verdict(&dirty_state(), GateMode::Strict, true), None);
    }

    #[test]
    fn clean_session_is_silent() {
        assert_eq!(verdict(&SessionState::default(), GateMode::Strict, false), None);
    }
}
