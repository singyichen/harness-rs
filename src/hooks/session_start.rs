use crate::config::Config;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const PROTOCOL: &str = include_str!("../../assets/protocol.md");

pub fn payload_cwd(payload: &Value) -> PathBuf {
    payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// The protocol text to inject, layered nearest-wins like the config:
/// the project's released copy (`<cwd>/.claude/harness/protocol.md`),
/// then the global copy, then the embedded version (fail-open). Install
/// promises "your customizations win" — the injected protocol honors
/// that at whichever layer the customization lives.
pub fn protocol_layered(
    project_claude_dir: Option<&Path>,
    global_claude_dir: Option<&Path>,
) -> String {
    project_claude_dir
        .and_then(read_protocol)
        .or_else(|| global_claude_dir.and_then(read_protocol))
        .unwrap_or_else(|| PROTOCOL.to_string())
}

fn read_protocol(claude_dir: &Path) -> Option<String> {
    std::fs::read_to_string(claude_dir.join("harness").join("protocol.md")).ok()
}

/// State files older than this are leftovers from long-dead sessions.
const STATE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

pub fn run(payload: &Value) -> Option<String> {
    let cwd = payload_cwd(payload);
    let config = Config::load(&cwd);
    // Housekeeping: once per session, drop state files from dead sessions
    // (nothing else ever deletes them; best-effort, fail-open).
    if let Some(h) = dirs::home_dir() {
        crate::state::prune_older_than(
            &h.join(".claude").join("harness").join("state"),
            STATE_MAX_AGE,
        );
    }
    let global = dirs::home_dir().map(|h| h.join(".claude"));
    let protocol = protocol_layered(Some(&cwd.join(".claude")), global.as_deref());
    Some(format!(
        "{protocol}\n## 5. Active harness gates\n- verify gate: {} mode\n- review panel: {}\n",
        config.verify.mode.as_str(),
        config.review.panel.join(", "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_protocol(claude_dir: &Path, content: &[u8]) {
        let dir = claude_dir.join("harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("protocol.md"), content).unwrap();
    }

    #[test]
    fn project_layer_wins_over_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&proj, b"# project protocol");
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# project protocol"
        );
    }

    #[test]
    fn missing_project_layer_falls_back_to_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# global protocol"
        );
    }

    #[test]
    fn missing_both_layers_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(protocol_layered(Some(tmp.path()), None), PROTOCOL);
    }

    #[test]
    fn non_utf8_project_layer_falls_through_to_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&proj, &[0xFF, 0xFE]);
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# global protocol"
        );
    }

    #[test]
    fn non_utf8_everywhere_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("g");
        write_protocol(&global, &[0xFF, 0xFE]);
        assert_eq!(protocol_layered(None, Some(&global)), PROTOCOL);
    }
}
