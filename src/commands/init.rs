use std::io;
use std::path::Path;

const TEMPLATE: &str = r#"# Harness customization layer for this project
# (overrides the global and built-in config)
# Unlisted fields inherit from upper layers; run `harness config` to see
# the merged result.

[gates.verify]
# Strict mode for this project: unverified code changes block turn endings
mode = "strict"
# This project's test commands and code scope (uncomment as needed):
# test_commands = ["cargo test"]
# code_globs = ["src/**/*.rs"]
# exempt_globs = ["**/docs/**", "**/*.md"]

[review]
panel = ["skeptic", "red-team", "simplifier"]
"#;

pub fn run() -> i32 {
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("error: could not determine the current directory");
        return 1;
    };
    match init_at(&cwd) {
        Ok(path) => {
            println!("created {path}; adjust it for this project.");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

pub fn init_at(dir: &Path) -> io::Result<String> {
    let path = dir.join("harness.toml");
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists; refusing to overwrite", path.display()),
        ));
    }
    std::fs::write(&path, TEMPLATE)?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_template() {
        let tmp = tempfile::tempdir().unwrap();
        init_at(tmp.path()).unwrap();
        let text = std::fs::read_to_string(tmp.path().join("harness.toml")).unwrap();
        assert!(text.contains("[gates.verify]"));
        // The template must be valid TOML parsable as a PartialConfig
        let _: crate::config::PartialConfig = toml::from_str(&text).unwrap();
    }

    #[test]
    fn init_refuses_to_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("harness.toml"), "existing").unwrap();
        assert!(init_at(tmp.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("harness.toml")).unwrap(),
            "existing"
        );
    }
}
