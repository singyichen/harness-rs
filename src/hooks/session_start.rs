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

/// The protocol text to inject: the released (possibly user-customized)
/// copy in `<claude_dir>/harness/protocol.md`, falling back to the embedded
/// version when it is missing or unreadable (fail-open). Install promises
/// "your customizations win" — the injected protocol must honor that too.
pub fn protocol_from(claude_dir: &Path) -> String {
    std::fs::read_to_string(claude_dir.join("harness").join("protocol.md"))
        .unwrap_or_else(|_| PROTOCOL.to_string())
}

pub fn run(payload: &Value) -> Option<String> {
    let config = Config::load(&payload_cwd(payload));
    let protocol = dirs::home_dir()
        .map(|h| protocol_from(&h.join(".claude")))
        .unwrap_or_else(|| PROTOCOL.to_string());
    Some(format!(
        "{protocol}\n## 5. Active harness gates\n- verify gate: {} mode\n- review panel: {}\n",
        config.verify.mode.as_str(),
        config.review.panel.join(", "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customized_protocol_on_disk_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("protocol.md"), "# my customized protocol").unwrap();
        assert_eq!(protocol_from(tmp.path()), "# my customized protocol");
    }

    #[test]
    fn missing_protocol_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(protocol_from(tmp.path()), PROTOCOL);
    }

    #[test]
    fn non_utf8_protocol_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("protocol.md"), [0xFF, 0xFE]).unwrap();
        assert_eq!(protocol_from(tmp.path()), PROTOCOL);
    }
}
