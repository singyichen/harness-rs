use crate::config::Config;
use crate::hooks::session_start::payload_cwd;
use crate::state::{self, SessionState};
use serde_json::Value;
use std::path::Path;

pub fn run(payload: &Value) -> Option<String> {
    let session_id = payload.get("session_id")?.as_str()?;
    let tool_name = payload.get("tool_name")?.as_str()?;
    let empty = Value::Null;
    let tool_input = payload.get("tool_input").unwrap_or(&empty);
    let cwd = payload_cwd(payload);
    let config = Config::load(&cwd);

    let path = state::state_path(session_id);
    let mut st = state::load(&path);
    let before = st.seq;
    apply_event(&mut st, &config, &cwd, tool_name, tool_input);
    if st.seq != before {
        let _ = state::save(&path, &st); // write failure → fail-open, stay silent
    }
    None
}

pub fn apply_event(
    st: &mut SessionState,
    config: &Config,
    cwd: &Path,
    tool_name: &str,
    tool_input: &Value,
) {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            let file = tool_input
                .get("file_path")
                .or_else(|| tool_input.get("notebook_path"))
                .and_then(|v| v.as_str());
            if let Some(file) = file {
                if config.verify.is_code_file(file, cwd) {
                    st.record_code_change(file);
                }
            }
        }
        "Bash" => {
            if let Some(cmd) = tool_input.get("command").and_then(|v| v.as_str()) {
                if config.verify.is_test_command(cmd) {
                    st.record_test_run();
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use serde_json::json;
    use std::path::Path;

    fn apply(st: &mut SessionState, tool: &str, input: serde_json::Value) {
        apply_event(st, &Config::builtin(), Path::new("/proj"), tool, &input);
    }

    #[test]
    fn edit_code_file_records_change() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        assert!(st.unverified_changes());
        assert_eq!(st.changed_files, vec!["/proj/src/main.rs".to_string()]);
    }

    #[test]
    fn edit_markdown_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Write", json!({"file_path": "/proj/README.md"}));
        assert!(!st.unverified_changes());
    }

    #[test]
    fn notebook_edit_uses_notebook_path() {
        let mut st = SessionState::default();
        apply(&mut st, "NotebookEdit", json!({"notebook_path": "/proj/a.py"}));
        assert!(st.unverified_changes());
    }

    #[test]
    fn bash_test_command_records_test() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        apply(&mut st, "Bash", json!({"command": "cd /proj && cargo test"}));
        assert!(!st.unverified_changes());
    }

    #[test]
    fn bash_non_test_command_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        apply(&mut st, "Bash", json!({"command": "cargo build"}));
        assert!(st.unverified_changes());
    }

    #[test]
    fn unrelated_tool_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Read", json!({"file_path": "/proj/src/main.rs"}));
        assert_eq!(st, SessionState::default());
    }
}
