use crate::manifest::{sha256_hex, Manifest};
use crate::settings;
use std::io;
use std::path::Path;

pub fn run() -> i32 {
    let Some(home) = dirs::home_dir() else {
        eprintln!("error: could not determine the home directory");
        return 1;
    };
    match uninstall_from(&home.join(".claude")) {
        Ok(actions) => {
            for a in &actions {
                println!("{a}");
            }
            println!("harness removed (global config harness/config.toml preserved).");
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
            match std::fs::read_to_string(&target) {
                Err(_) => {} // already gone
                Ok(current) if sha256_hex(current.as_bytes()) == *recorded_hash => {
                    std::fs::remove_file(&target)?;
                    actions.push(format!("deleted {rel_path}"));
                }
                Ok(_) => {
                    actions.push(format!(
                        "warning: {rel_path} was modified by you; keeping it"
                    ));
                }
            }
        }
        std::fs::remove_file(Manifest::path(claude_dir))?;
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::install_to;

    #[test]
    fn uninstall_removes_released_files_and_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
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
        install_to(tmp.path()).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, "the user's own version").unwrap();
        let actions = uninstall_from(tmp.path()).unwrap();
        assert!(target.exists());
        assert!(actions.iter().any(|a| a.contains("skeptic.md")));
    }

    #[test]
    fn uninstall_without_install_is_graceful() {
        let tmp = tempfile::tempdir().unwrap();
        uninstall_from(tmp.path()).unwrap(); // no panic, no error
    }
}
