use crate::config::Config;
use serde_json::Value;
use std::path::PathBuf;

pub const PROTOCOL: &str = include_str!("../../assets/protocol.md");

pub fn payload_cwd(payload: &Value) -> PathBuf {
    payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

pub fn run(payload: &Value) -> Option<String> {
    let config = Config::load(&payload_cwd(payload));
    Some(format!(
        "{PROTOCOL}\n## 5. Active harness gates\n- verify gate: {} mode\n- review panel: {}\n",
        config.verify.mode.as_str(),
        config.review.panel.join(", "),
    ))
}
