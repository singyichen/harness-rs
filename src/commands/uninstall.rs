use crate::manifest::{sha256_hex, Manifest};
use crate::settings;
use std::io;
use std::path::Path;

pub fn run(project: bool) -> i32 {
    let claude_dir = match crate::commands::resolve_claude_dir(project) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return 1;
        }
    };
    match uninstall_from(&claude_dir) {
        Ok(actions) => {
            for a in &actions {
                println!("{a}");
            }
            remove_shared_symlink_if_orphaned(&claude_dir, project);
            if project {
                println!("harness removed from this project.");
            } else {
                println!("harness removed (global config harness/config.toml preserved).");
            }
            0
        }
        Err(e) => {
            eprintln!("error: uninstall failed: {e}");
            1
        }
    }
}

/// The system-bin symlink is shared infrastructure: a global and a project
/// install reuse the same `~/.cargo/bin` binary, and whichever installs first
/// creates the symlink. Removing it on *every* uninstall breaks the surviving
/// install's hooks — a `harness uninstall --project` (or a global uninstall
/// while a project install remains) would strand the other layer. So remove it
/// only when the other layer has no harness install still relying on it.
#[cfg(unix)]
fn remove_shared_symlink_if_orphaned(claude_dir: &Path, project: bool) {
    let other = if project {
        dirs::home_dir().map(|h| h.join(".claude"))
    } else {
        std::env::current_dir().ok().map(|d| d.join(".claude"))
    };
    if !should_remove_shared_symlink(claude_dir, other.as_deref()) {
        return;
    }
    if let Some(removed) = crate::path::remove_system_symlink() {
        println!("removed symlink {}", removed.display());
    }
}

#[cfg(not(unix))]
fn remove_shared_symlink_if_orphaned(_claude_dir: &Path, _project: bool) {}

/// True when the shared system symlink is safe to remove: the `other` layer
/// (the `.claude` not being uninstalled) has no harness install, or is the
/// same directory as the one being uninstalled (e.g. `--project` run from
/// `$HOME`), or is unknown.
#[cfg(unix)]
fn should_remove_shared_symlink(claude_dir: &Path, other: Option<&Path>) -> bool {
    match other {
        Some(o) if !crate::commands::is_same_dir(claude_dir, o) => Manifest::load(o).is_none(),
        _ => true,
    }
}

pub fn uninstall_from(claude_dir: &Path) -> io::Result<Vec<String>> {
    let mut actions = Vec::new();
    let settings_path = claude_dir.join("settings.json");
    if settings_path.exists() {
        let mut hook_actions = settings::unregister_hooks(&settings_path)?;
        actions.append(&mut hook_actions);
    }
    if let Some(manifest) = Manifest::load(claude_dir) {
        for (rel_path, recorded_hash) in &manifest.files {
            let target = claude_dir.join(rel_path);
            match std::fs::read(&target) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {} // already gone
                Err(e) => {
                    actions.push(format!(
                        "warning: could not read {rel_path} ({e}); keeping it"
                    ));
                }
                Ok(current) if sha256_hex(&current) == *recorded_hash => {
                    std::fs::remove_file(&target)?;
                    actions.push(format!("deleted {rel_path}"));
                }
                Ok(_) => {
                    actions.push(format!(
                        "warning: {rel_path} was modified by you; keeping it"
                    ));
                }
            }
            // `<name>.new` official copies are harness artifacts (released
            // alongside customized files); clean removal deletes them too —
            // but only while they still hold the recorded official content,
            // since the user may have edited one while merging. A corrupt
            // manifest entry ("..") may have no file name — skip it.
            if let Some(file_name) = target.file_name() {
                let new_copy = target
                    .with_file_name(format!("{}.new", file_name.to_string_lossy()));
                match std::fs::read(&new_copy) {
                    Err(_) => {} // absent or unreadable: nothing to delete safely
                    Ok(bytes) if sha256_hex(&bytes) == *recorded_hash => {
                        std::fs::remove_file(&new_copy)?;
                        actions.push(format!("deleted {rel_path}.new"));
                    }
                    Ok(_) => {
                        actions.push(format!(
                            "warning: {rel_path}.new was modified by you; keeping it"
                        ));
                    }
                }
            }
        }
        std::fs::remove_file(Manifest::path(claude_dir))?;
    }

    // Per-session verify-gate state is a harness artifact as well.
    let state_dir = claude_dir.join("harness").join("state");
    match std::fs::remove_dir_all(&state_dir) {
        Ok(()) => actions.push("deleted harness/state".to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => actions.push(format!("warning: could not remove harness/state ({e})")),
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::{install_to, Scope};

    #[cfg(unix)]
    #[test]
    fn keeps_shared_symlink_when_other_layer_still_installed() {
        // Regression: `uninstall --project` (or a global uninstall) must not
        // remove the shared system symlink while the other layer still has an
        // install that depends on it.
        let this = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        install_to(other.path(), Scope::Global).unwrap(); // writes a manifest
        assert!(!should_remove_shared_symlink(this.path(), Some(other.path())));
    }

    #[cfg(unix)]
    #[test]
    fn removes_shared_symlink_when_no_other_install() {
        let this = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap(); // no manifest → not installed
        assert!(should_remove_shared_symlink(this.path(), Some(other.path())));
    }

    #[cfg(unix)]
    #[test]
    fn removes_shared_symlink_when_other_is_same_dir() {
        // `--project` run from $HOME makes both layers resolve to ~/.claude;
        // that is not a genuine coexistence, so removal is safe.
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        assert!(should_remove_shared_symlink(tmp.path(), Some(tmp.path())));
    }

    #[cfg(unix)]
    #[test]
    fn removes_shared_symlink_when_other_unknown() {
        let this = tempfile::tempdir().unwrap();
        assert!(should_remove_shared_symlink(this.path(), None));
    }

    #[test]
    fn uninstall_removes_released_files_and_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        uninstall_from(tmp.path()).unwrap();
        assert!(!tmp.path().join("agents/skeptic.md").exists());
        assert!(!tmp.path().join("harness/manifest.json").exists());
        assert!(tmp.path().join("harness/config.toml").exists()); // user config kept
        let settings = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(!settings.contains("harness hook"));
    }

    #[test]
    fn uninstall_keeps_user_modified_files() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, "the user's own version").unwrap();
        let actions = uninstall_from(tmp.path()).unwrap();
        assert!(target.exists());
        assert!(actions.iter().any(|a| a.contains("skeptic.md")));
    }

    #[test]
    fn uninstall_keeps_non_utf8_modified_files() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, [0xFF, 0xFE, 0x00]).unwrap();
        let actions = uninstall_from(tmp.path()).unwrap();
        assert!(target.exists());
        assert!(actions.iter().any(|a| a.contains("skeptic.md")));
    }

    #[test]
    fn uninstall_removes_session_state_dir() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let state_dir = tmp.path().join("harness/state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("s1.json"), "{}").unwrap();
        uninstall_from(tmp.path()).unwrap();
        assert!(!state_dir.exists());
    }

    #[test]
    fn uninstall_removes_official_new_copies() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, "customized").unwrap();
        install_to(tmp.path(), Scope::Global).unwrap(); // customization → skeptic.md.new released
        let new_copy = tmp.path().join("agents/skeptic.md.new");
        assert!(new_copy.is_file(), "precondition: .new copy exists");
        uninstall_from(tmp.path()).unwrap();
        assert!(!new_copy.exists(), ".new copies are harness artifacts");
        assert!(target.exists(), "customized file still kept");
    }

    #[test]
    fn uninstall_keeps_user_edited_new_copies() {
        // A `.new` copy the user edited (e.g. while merging customizations)
        // no longer matches the recorded official hash and must be kept.
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, "customized").unwrap();
        install_to(tmp.path(), Scope::Global).unwrap(); // customization → skeptic.md.new released
        let new_copy = tmp.path().join("agents/skeptic.md.new");
        std::fs::write(&new_copy, "official copy, edited by the user").unwrap();
        let actions = uninstall_from(tmp.path()).unwrap();
        assert!(new_copy.exists(), "edited .new copy must survive uninstall");
        assert!(actions.iter().any(|a| a.contains("skeptic.md.new")));
    }

    #[test]
    fn uninstall_survives_manifest_entry_without_file_name() {
        // A hand-edited/corrupt manifest can contain entries like ".." whose
        // join has no file name; uninstall must warn, not panic.
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let mut m = crate::manifest::Manifest::load(tmp.path()).unwrap();
        m.files.insert("..".to_string(), "not-a-real-hash".to_string());
        m.save(tmp.path()).unwrap();
        uninstall_from(tmp.path()).unwrap();
    }

    #[test]
    fn uninstall_without_install_is_graceful() {
        let tmp = tempfile::tempdir().unwrap();
        uninstall_from(tmp.path()).unwrap(); // no panic, no error
    }
}
