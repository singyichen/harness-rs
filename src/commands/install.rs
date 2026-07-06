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
/// Rules: missing → write; identical to official → skip;
/// user-modified → keep, write the official copy to <path>.new.
pub fn release_assets(claude_dir: &Path) -> io::Result<Vec<String>> {
    let mut actions = Vec::new();
    let mut manifest = Manifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        files: Default::default(),
    };
    for asset in ASSETS {
        let target = claude_dir.join(asset.rel_path);
        let official_hash = sha256_hex(asset.content);
        manifest.files.insert(asset.rel_path.to_string(), official_hash.clone());
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match std::fs::read_to_string(&target) {
            Err(_) => {
                std::fs::write(&target, asset.content)?;
                actions.push(format!("released {}", asset.rel_path));
            }
            Ok(current) if sha256_hex(&current) == official_hash => {}
            Ok(_) => {
                let new_path = target.with_file_name(format!(
                    "{}.new",
                    target.file_name().unwrap().to_string_lossy()
                ));
                std::fs::write(&new_path, asset.content)?;
                actions.push(format!(
                    "warning: {} was modified by you; keeping your version, official copy at {}",
                    asset.rel_path,
                    new_path.display()
                ));
            }
        }
    }
    manifest.save(claude_dir)?;
    Ok(actions)
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
}
