use crate::commands::install::release_assets;

pub fn run() -> i32 {
    let Some(home) = dirs::home_dir() else {
        eprintln!("error: could not determine the home directory");
        return 1;
    };
    match release_assets(&home.join(".claude")) {
        Ok(actions) => {
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
        Err(e) => {
            eprintln!("error: update failed: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::install::{install_to, release_assets};
    use crate::manifest::Manifest;

    #[test]
    fn update_refreshes_unmodified_and_preserves_modified() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        // Simulate a user customization of one asset
        std::fs::write(tmp.path().join("agents/skeptic.md"), "user customized").unwrap();
        let actions = release_assets(tmp.path()).unwrap();
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
}
