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
    // Remove system-path symlink if install created one.
    #[cfg(unix)]
    if let Some(removed) = crate::path::remove_system_symlink() {
        actions.push(format!("removed symlink {}", removed.display()));
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
