use crate::commands::install::release_assets;
use crate::manifest::Manifest;
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
    let install_hint = if project { "harness install --project" } else { "harness install" };
    match update_at(&claude_dir) {
        Ok(Some(actions)) => {
            if actions.is_empty() {
                println!("assets are already up to date; nothing changed.");
            } else {
                for a in &actions {
                    println!("{a}");
                }
                println!(
                    "update complete. Files marked with a warning kept your customizations; the new official copies are in the matching .new files — compare and merge as you see fit."
                );
            }
            0
        }
        Ok(None) => {
            eprintln!("error: harness is not installed — run `{install_hint}` first");
            1
        }
        Err(e) => {
            eprintln!("error: update failed: {e}");
            1
        }
    }
}

/// Re-release embedded assets for an existing install.
///
/// Returns `Ok(None)` when no prior install is found (no manifest — running
/// `harness update` before ever installing must not silently perform a
/// partial install of just the assets).
pub fn update_at(claude_dir: &Path) -> io::Result<Option<Vec<String>>> {
    if Manifest::load(claude_dir).is_none() {
        return Ok(None);
    }
    release_assets(claude_dir).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::{install_to, Scope};

    #[test]
    fn update_refreshes_unmodified_and_preserves_modified() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        // Simulate a user customization of one asset
        std::fs::write(tmp.path().join("agents/skeptic.md"), "user customized").unwrap();
        let actions = update_at(tmp.path()).unwrap().expect("install exists");
        // Customized file kept + .new copy
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("agents/skeptic.md")).unwrap(),
            "user customized"
        );
        assert!(tmp.path().join("agents/skeptic.md.new").is_file());
        assert!(actions.iter().any(|a| a.contains("skeptic.md")));
        // Manifest version refreshed to the binary version
        let m = Manifest::load(tmp.path()).unwrap();
        assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
    }

    /// Regression test: `harness update` on a machine that never ran
    /// `harness install` must refuse rather than silently half-installing
    /// (assets + manifest but no hooks/config).
    #[test]
    fn update_without_prior_install_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let result = update_at(tmp.path()).unwrap();
        assert!(result.is_none());
        assert!(!tmp.path().join("harness/manifest.json").exists());
        assert!(!tmp.path().join("agents/skeptic.md").exists());
    }
}
