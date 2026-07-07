use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub seq: u64,
    pub last_code_change_seq: Option<u64>,
    pub last_test_seq: Option<u64>,
    pub changed_files: Vec<String>,
}

impl SessionState {
    pub fn record_code_change(&mut self, file: &str) {
        self.seq += 1;
        self.last_code_change_seq = Some(self.seq);
        if !self.changed_files.iter().any(|f| f == file) {
            self.changed_files.push(file.to_string());
        }
    }

    pub fn record_test_run(&mut self) {
        self.seq += 1;
        self.last_test_seq = Some(self.seq);
        self.changed_files.clear(); // list = files changed since the last test
    }

    pub fn unverified_changes(&self) -> bool {
        match (self.last_code_change_seq, self.last_test_seq) {
            (Some(c), Some(t)) => c > t,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

pub fn state_path(session_id: &str) -> PathBuf {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    // Per-user location: a world-shared dir like /tmp/harness-state invites
    // symlink attacks and first-user-owns-it permission conflicts.
    dirs::home_dir()
        .map(|h| h.join(".claude").join("harness").join("state"))
        .unwrap_or_else(|| std::env::temp_dir().join("harness-state"))
        .join(format!("{safe}.json"))
}

/// Delete state files older than `max_age` (best-effort, errors ignored:
/// pruning runs inside the fail-open hook path).
pub fn prune_older_than(dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let expired = entry
            .metadata()
            .and_then(|m| m.modified())
            .and_then(|t| t.elapsed().map_err(|e| std::io::Error::other(e)))
            .map(|age| age >= max_age)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn load(path: &Path) -> SessionState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, st: &SessionState) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(st).expect("state is serializable"))?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_without_test_is_unverified() {
        let mut st = SessionState::default();
        st.record_code_change("/p/src/a.rs");
        assert!(st.unverified_changes());
    }

    #[test]
    fn test_after_change_is_verified() {
        let mut st = SessionState::default();
        st.record_code_change("/p/src/a.rs");
        st.record_test_run();
        assert!(!st.unverified_changes());
        assert!(st.changed_files.is_empty()); // list resets after a test run
    }

    #[test]
    fn change_after_test_is_unverified_again() {
        let mut st = SessionState::default();
        st.record_code_change("/p/src/a.rs");
        st.record_test_run();
        st.record_code_change("/p/src/b.rs");
        assert!(st.unverified_changes());
        assert_eq!(st.changed_files, vec!["/p/src/b.rs".to_string()]);
    }

    #[test]
    fn changed_files_dedup() {
        let mut st = SessionState::default();
        st.record_code_change("/p/a.rs");
        st.record_code_change("/p/a.rs");
        assert_eq!(st.changed_files.len(), 1);
    }

    #[test]
    fn no_changes_is_verified() {
        assert!(!SessionState::default().unverified_changes());
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/s1.json");
        let mut st = SessionState::default();
        st.record_code_change("/p/a.rs");
        save(&path, &st).unwrap();
        assert_eq!(load(&path), st);
    }

    #[test]
    fn corrupt_state_loads_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("s.json");
        std::fs::write(&path, "{{{ not json").unwrap();
        assert_eq!(load(&path), SessionState::default());
    }

    #[test]
    fn prune_removes_old_state_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("old.json"), "{}").unwrap();
        prune_older_than(tmp.path(), std::time::Duration::ZERO);
        assert!(!tmp.path().join("old.json").exists());
    }

    #[test]
    fn prune_keeps_recent_state_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("fresh.json"), "{}").unwrap();
        prune_older_than(tmp.path(), std::time::Duration::from_secs(3600));
        assert!(tmp.path().join("fresh.json").exists());
    }

    #[test]
    fn prune_missing_dir_is_silent() {
        prune_older_than(Path::new("/nonexistent/harness-state"), std::time::Duration::ZERO);
    }

    #[test]
    fn state_path_sanitizes_session_id() {
        let p = state_path("../../evil/../id_1-2");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "evilid_1-2.json"); // only [A-Za-z0-9_-] survives
    }

    #[test]
    fn state_path_is_per_user() {
        let p = state_path("s1");
        let home = dirs::home_dir().unwrap();
        assert!(p.starts_with(home.join(".claude").join("harness").join("state")));
    }
}
