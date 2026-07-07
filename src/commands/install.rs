use crate::assets::{ASSETS, DEFAULT_CONFIG};
use crate::manifest::{sha256_hex, Manifest};
use crate::settings;
use std::io;
use std::path::Path;

pub fn run() -> i32 {
    let Some(home) = dirs::home_dir() else {
        eprintln!("error: could not determine the home directory");
        return 1;
    };
    match install_to(&home.join(".claude")) {
        Ok(actions) => {
            for a in &actions {
                println!("{a}");
            }
            println!("harness installed. Run `harness doctor` anytime for a health check.");
            0
        }
        Err(e) => {
            eprintln!("error: install failed: {e}");
            1
        }
    }
}

pub fn install_to(claude_dir: &Path) -> io::Result<Vec<String>> {
    let mut actions = release_assets(claude_dir)?;

    // Global config: written only when missing, never overwritten
    let config_path = claude_dir.join("harness/config.toml");
    if !config_path.exists() {
        if let Some(dir) = config_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&config_path, DEFAULT_CONFIG)?;
        actions.push(format!("created global config {}", config_path.display()));
    }

    let mut hook_actions = settings::register_hooks(&claude_dir.join("settings.json"))?;
    actions.append(&mut hook_actions);
    Ok(actions)
}

/// Release embedded assets (shared by install and update).
///
/// Rules:
/// - missing → write the embedded content.
/// - unreadable (permission error, non-UTF-8 content) → treat as
///   user-modified: keep the file, write the official copy to `<name>.new`.
/// - identical to the currently-embedded content → already up to date, skip.
/// - identical to the hash RECORDED in the previous manifest (i.e.
///   unmodified official content from an older release) → overwrite with
///   the new embedded content.
/// - anything else → user-modified: keep the file, write the official copy
///   to `<name>.new`.
pub fn release_assets(claude_dir: &Path) -> io::Result<Vec<String>> {
    let previous_manifest = Manifest::load(claude_dir);
    let mut actions = Vec::new();
    let mut manifest = Manifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        files: Default::default(),
    };
    for asset in ASSETS {
        let target = claude_dir.join(asset.rel_path);
        let official_hash = sha256_hex(asset.content.as_bytes());
        manifest.files.insert(asset.rel_path.to_string(), official_hash.clone());
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let recorded_hash = previous_manifest
            .as_ref()
            .and_then(|m| m.files.get(asset.rel_path));
        match std::fs::read(&target) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                std::fs::write(&target, asset.content)?;
                actions.push(format!("released {}", asset.rel_path));
            }
            Err(_) => {
                write_new_copy(&target, asset)?;
                actions.push(user_modified_warning(asset.rel_path, &target));
            }
            Ok(bytes) => {
                let current_hash = sha256_hex(&bytes);
                let is_valid_utf8 = std::str::from_utf8(&bytes).is_ok();
                if !is_valid_utf8 {
                    write_new_copy(&target, asset)?;
                    actions.push(user_modified_warning(asset.rel_path, &target));
                } else if current_hash == official_hash {
                    // already up to date; nothing to do
                } else if recorded_hash.is_some_and(|h| *h == current_hash) {
                    // unmodified official content from an older release
                    std::fs::write(&target, asset.content)?;
                    actions.push(format!("updated {}", asset.rel_path));
                } else {
                    write_new_copy(&target, asset)?;
                    actions.push(user_modified_warning(asset.rel_path, &target));
                }
            }
        }
    }
    manifest.save(claude_dir)?;
    Ok(actions)
}

/// Write the embedded (official) content of `asset` alongside `target` as
/// `<name>.new`, without touching the user's own file.
fn write_new_copy(target: &Path, asset: &crate::assets::Asset) -> io::Result<()> {
    let new_path = target.with_file_name(format!(
        "{}.new",
        target.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(&new_path, asset.content)
}

fn user_modified_warning(rel_path: &str, target: &Path) -> String {
    let new_path = target.with_file_name(format!(
        "{}.new",
        target.file_name().unwrap().to_string_lossy()
    ));
    format!(
        "warning: {rel_path} was modified by you; keeping your version, official copy at {}",
        new_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::ASSETS;
    use crate::manifest::Manifest;

    #[test]
    fn install_releases_all_assets_and_registers_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        for a in ASSETS {
            assert!(tmp.path().join(a.rel_path).is_file(), "missing {}", a.rel_path);
        }
        assert!(tmp.path().join("harness/config.toml").is_file());
        assert!(tmp.path().join("harness/manifest.json").is_file());
        let settings = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(settings.contains("harness hook stop"));
        let m = Manifest::load(tmp.path()).unwrap();
        assert_eq!(m.files.len(), ASSETS.len());
    }

    #[test]
    fn install_preserves_user_modified_asset() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        let target = tmp.path().join("agents/skeptic.md");
        std::fs::write(&target, "the user's own version").unwrap();
        install_to(tmp.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "the user's own version"
        );
        assert!(target.with_extension("md.new").is_file()); // official copy alongside
    }

    #[test]
    fn install_never_overwrites_global_config() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        let cfg = tmp.path().join("harness/config.toml");
        std::fs::write(&cfg, "[gates.verify]\nmode = \"strict\"").unwrap();
        install_to(tmp.path()).unwrap();
        assert!(std::fs::read_to_string(&cfg).unwrap().contains("strict"));
    }

    /// Regression test for the "compare against the RECORDED manifest hash"
    /// fix: a file whose disk content matches an OLDER release's recorded
    /// hash (not the currently-embedded one) is unmodified official content
    /// and must be refreshed, not flagged as user-modified.
    #[test]
    fn release_assets_refreshes_unmodified_content_from_older_release() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        let rel_path = "agents/skeptic.md";
        let target = tmp.path().join(rel_path);
        let old_content = "an older official release of this file";
        std::fs::write(&target, old_content).unwrap();
        // Simulate an older release by rewriting the manifest so the
        // recorded hash for this file matches `old_content` (disk ==
        // recorded, both != the currently-embedded content).
        let mut manifest = Manifest::load(tmp.path()).unwrap();
        manifest
            .files
            .insert(rel_path.to_string(), sha256_hex(old_content.as_bytes()));
        manifest.save(tmp.path()).unwrap();

        let embedded = ASSETS
            .iter()
            .find(|a| a.rel_path == rel_path)
            .unwrap()
            .content;
        release_assets(tmp.path()).unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), embedded);
        assert!(!target.with_extension("md.new").is_file());
    }

    /// Regression test: non-UTF-8 (unreadable-as-text) content on disk must
    /// never be silently clobbered — it is treated as user-modified.
    #[test]
    fn release_assets_does_not_clobber_non_utf8_content() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        let rel_path = "agents/skeptic.md";
        let target = tmp.path().join(rel_path);
        let invalid_utf8: &[u8] = b"\xFF\xFE";
        std::fs::write(&target, invalid_utf8).unwrap();

        release_assets(tmp.path()).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), invalid_utf8);
        assert!(target.with_extension("md.new").is_file());
    }
}
