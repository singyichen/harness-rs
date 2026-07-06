use serde_json::{json, Map, Value};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const HOOK_MARKER: &str = "harness hook";

pub const HOOK_EVENTS: &[(&str, Option<&str>, &str)] = &[
    ("SessionStart", None, "harness hook session-start"),
    ("UserPromptSubmit", None, "harness hook user-prompt"),
    (
        "PostToolUse",
        Some("Edit|Write|MultiEdit|NotebookEdit|Bash"),
        "harness hook post-tool",
    ),
    ("Stop", None, "harness hook stop"),
];

fn load_settings(path: &Path) -> io::Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(path)?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is not a valid JSON object; refusing to modify it to avoid corruption",
                path.display()
            ),
        )),
    }
}

fn backup(path: &Path) -> io::Result<()> {
    if path.exists() {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let bak = path.with_file_name(format!(
            "{}.bak.{epoch}",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::copy(path, &bak)?;
    }
    Ok(())
}

fn atomic_write(path: &Path, map: &Map<String, Value>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(map.clone()))?)?;
    std::fs::rename(&tmp, path)
}

fn group_has_marker(group: &Value) -> bool {
    group["hooks"]
        .as_array()
        .map(|hs| {
            hs.iter().any(|h| {
                h["command"].as_str().map_or(false, |c| c.contains(HOOK_MARKER))
            })
        })
        .unwrap_or(false)
}

pub fn register_hooks(settings_path: &Path) -> io::Result<Vec<String>> {
    let mut map = load_settings(settings_path)?;
    backup(settings_path)?;
    let hooks = map
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "hooks field is not an object"))?;
    let mut actions = Vec::new();
    for (event, matcher, cmd) in HOOK_EVENTS {
        let arr = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]));
        let arr = arr
            .as_array_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "hook event is not an array"))?;
        if arr.iter().any(group_has_marker) {
            actions.push(format!("{event}: already registered, skipping"));
            continue;
        }
        let mut group = json!({"hooks": [{"type": "command", "command": cmd}]});
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        arr.push(group);
        actions.push(format!("{event}: registered `{cmd}`"));
    }
    atomic_write(settings_path, &map)?;
    Ok(actions)
}

pub fn unregister_hooks(settings_path: &Path) -> io::Result<Vec<String>> {
    let mut map = load_settings(settings_path)?;
    backup(settings_path)?;
    let mut actions = Vec::new();
    if let Some(hooks) = map.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let events: Vec<String> = hooks.keys().cloned().collect();
        for event in events {
            if let Some(arr) = hooks.get_mut(&event).and_then(|v| v.as_array_mut()) {
                let before = arr.len();
                arr.retain(|g| !group_has_marker(g));
                if arr.len() != before {
                    actions.push(format!("{event}: removed harness hook"));
                }
                if arr.is_empty() {
                    hooks.remove(&event);
                }
            }
        }
        if hooks.is_empty() {
            map.remove("hooks");
        }
    }
    atomic_write(settings_path, &map)?;
    Ok(actions)
}

/// Non-harness Stop hook commands (used by doctor to flag possible gate conflicts).
pub fn foreign_stop_hooks(settings_path: &Path) -> Vec<String> {
    let Ok(map) = load_settings(settings_path) else {
        return Vec::new();
    };
    map.get("hooks")
        .and_then(|h| h.get("Stop"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|g| !group_has_marker(g))
                .filter_map(|g| g["hooks"].as_array())
                .flatten()
                .filter_map(|h| h["command"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_json(p: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn register_into_missing_file_creates_all_events() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        for (event, _, cmd) in HOOK_EVENTS {
            let arr = v["hooks"][event].as_array().unwrap();
            assert!(
                arr.iter().any(|g| g["hooks"].as_array().unwrap().iter()
                    .any(|h| h["command"] == *cmd)),
                "missing {event}"
            );
        }
    }

    #[test]
    fn register_preserves_existing_content_and_backs_up() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool check"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        assert_eq!(v["model"], "opus"); // unknown fields untouched
        let stops = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stops.len(), 2); // existing entry kept, ours appended after
        assert_eq!(stops[0]["hooks"][0]["command"], "other-tool check");
        // A backup file exists
        let backups: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings.json.bak."))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn register_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        register_hooks(&p).unwrap();
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unregister_removes_only_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool check"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        unregister_hooks(&p).unwrap();
        let v = read_json(&p);
        let stops = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stops.len(), 1);
        assert_eq!(stops[0]["hooks"][0]["command"], "other-tool check");
        // Our other event keys were emptied and removed
        assert!(v["hooks"].get("SessionStart").is_none());
    }

    #[test]
    fn foreign_stop_hooks_detects_other_gates() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"python verify_gate.py"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        assert_eq!(foreign_stop_hooks(&p), vec!["python verify_gate.py".to_string()]);
    }
}
