use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    Strict,
    Advisory,
    Off,
}

impl GateMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GateMode::Strict => "strict",
            GateMode::Advisory => "advisory",
            GateMode::Off => "off",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "strict" => Some(GateMode::Strict),
            "advisory" => Some(GateMode::Advisory),
            "off" => Some(GateMode::Off),
            _ => None, // fail-open: invalid values take no effect
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifyGate {
    pub mode: GateMode,
    pub test_commands: Vec<String>,
    pub code_globs: Vec<String>,
    pub exempt_globs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub panel: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub verify: VerifyGate,
    pub review: Review,
}

#[derive(Debug, Default, Deserialize)]
pub struct PartialConfig {
    #[serde(default)]
    pub gates: PartialGates,
    #[serde(default)]
    pub review: PartialReview,
}

#[derive(Debug, Default, Deserialize)]
pub struct PartialGates {
    #[serde(default)]
    pub verify: PartialVerify,
}

#[derive(Debug, Default, Deserialize)]
pub struct PartialVerify {
    pub mode: Option<String>,
    pub test_commands: Option<Vec<String>>,
    pub code_globs: Option<Vec<String>>,
    pub exempt_globs: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PartialReview {
    pub panel: Option<Vec<String>>,
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Config {
    pub fn builtin() -> Self {
        Config {
            verify: VerifyGate {
                mode: GateMode::Advisory,
                test_commands: strs(&[
                    "cargo test", "pytest", "python -m pytest", "python -m unittest",
                    "npm test", "npm run test", "pnpm test", "yarn test", "bun test",
                    "node --test", "vitest", "jest", "go test", "make test", "ctest",
                    "dotnet test", "mix test", "rspec", "phpunit", "deno test",
                    "rake test", "mvn test", "gradle test", "tox", "nox",
                ]),
                code_globs: strs(&[
                    "**/*.rs", "**/*.py", "**/*.js", "**/*.ts", "**/*.tsx", "**/*.jsx",
                    "**/*.mjs", "**/*.cjs", "**/*.go", "**/*.java", "**/*.c", "**/*.cpp",
                    "**/*.h", "**/*.hpp", "**/*.cs", "**/*.rb", "**/*.php", "**/*.sh",
                    "**/*.sql", "**/*.swift", "**/*.kt",
                ]),
                exempt_globs: strs(&["**/docs/**", "**/*.md", "**/*.txt"]),
            },
            review: Review {
                panel: strs(&["skeptic", "red-team", "simplifier"]),
            },
        }
    }

    pub fn apply(&mut self, layer: &PartialConfig) {
        let v = &layer.gates.verify;
        if let Some(mode) = v.mode.as_deref().and_then(GateMode::parse) {
            self.verify.mode = mode;
        }
        if let Some(x) = &v.test_commands {
            self.verify.test_commands = x.clone();
        }
        if let Some(x) = &v.code_globs {
            self.verify.code_globs = x.clone();
        }
        if let Some(x) = &v.exempt_globs {
            self.verify.exempt_globs = x.clone();
        }
        if let Some(x) = &layer.review.panel {
            self.review.panel = x.clone();
        }
    }

    /// Called from hooks/commands: built-in → global → project.
    pub fn load(cwd: &Path) -> Self {
        let global = dirs::home_dir().map(|h| h.join(".claude/harness/config.toml"));
        Self::load_layered(global.as_deref(), cwd)
    }

    /// Testable variant with an explicit global-config path.
    /// Any layer that fails to read or parse is skipped.
    pub fn load_layered(global_path: Option<&Path>, cwd: &Path) -> Self {
        let mut config = Config::builtin();
        if let Some(p) = global_path {
            if let Some(layer) = read_partial(p) {
                config.apply(&layer);
            }
        }
        if let Some(p) = find_project_config(cwd) {
            if let Some(layer) = read_partial(&p) {
                config.apply(&layer);
            }
        }
        config
    }

    pub fn to_toml_string(&self) -> String {
        let list = |v: &[String]| {
            v.iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "[gates.verify]\nmode = \"{}\"\ntest_commands = [{}]\ncode_globs = [{}]\nexempt_globs = [{}]\n\n[review]\npanel = [{}]\n",
            self.verify.mode.as_str(),
            list(&self.verify.test_commands),
            list(&self.verify.code_globs),
            list(&self.verify.exempt_globs),
            list(&self.review.panel),
        )
    }
}

fn read_partial(path: &Path) -> Option<PartialConfig> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

/// Walk up from cwd to find the nearest harness.toml.
fn find_project_config(cwd: &Path) -> Option<PathBuf> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let candidate = d.join("harness.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

impl VerifyGate {
    pub fn is_test_command(&self, cmd: &str) -> bool {
        self.test_commands.iter().any(|t| cmd.contains(t.as_str()))
    }

    pub fn is_code_file(&self, path: &str, cwd: &Path) -> bool {
        let rel = Path::new(path)
            .strip_prefix(cwd)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or(path);
        // Globs use `/` separators; Windows paths arrive with `\`.
        let path = path.replace('\\', "/");
        let rel = rel.replace('\\', "/");
        let hit = |globs: &[String]| {
            globs
                .iter()
                .filter_map(|g| glob::Pattern::new(g).ok())
                .any(|p| p.matches(&path) || p.matches(&rel))
        };
        hit(&self.code_globs) && !hit(&self.exempt_globs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn builtin_defaults() {
        let c = Config::builtin();
        assert_eq!(c.verify.mode, GateMode::Advisory);
        assert!(c.verify.test_commands.iter().any(|t| t == "cargo test"));
        assert!(c.review.panel.contains(&"skeptic".to_string()));
    }

    #[test]
    fn apply_overrides_only_present_fields() {
        let mut c = Config::builtin();
        let layer: PartialConfig =
            toml::from_str("[gates.verify]\nmode = \"strict\"").unwrap();
        c.apply(&layer);
        assert_eq!(c.verify.mode, GateMode::Strict);
        // Unspecified fields keep their built-in values
        assert!(!c.verify.test_commands.is_empty());
    }

    #[test]
    fn unknown_mode_is_ignored() {
        let mut c = Config::builtin();
        let layer: PartialConfig =
            toml::from_str("[gates.verify]\nmode = \"banana\"").unwrap();
        c.apply(&layer); // fail-open: invalid values take no effect, no panic
        assert_eq!(c.verify.mode, GateMode::Advisory);
    }

    #[test]
    fn layered_load_project_overrides_global() {
        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("config.toml");
        std::fs::write(&global, "[gates.verify]\nmode = \"off\"").unwrap();
        let proj = tmp.path().join("proj/sub");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            tmp.path().join("proj/harness.toml"),
            "[gates.verify]\nmode = \"strict\"\n[review]\npanel = [\"skeptic\"]",
        )
        .unwrap();
        // Project file lives in a parent dir; found by walking up from the subdir
        let c = Config::load_layered(Some(&global), &proj);
        assert_eq!(c.verify.mode, GateMode::Strict);
        assert_eq!(c.review.panel, vec!["skeptic".to_string()]);
    }

    #[test]
    fn corrupt_layer_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("config.toml");
        std::fs::write(&global, "not [ valid toml").unwrap();
        let c = Config::load_layered(Some(&global), tmp.path());
        assert_eq!(c.verify.mode, GateMode::Advisory); // broken layer skipped
    }

    #[test]
    fn test_command_substring_match() {
        let v = Config::builtin().verify;
        assert!(v.is_test_command("cd backend && cargo test --all"));
        assert!(v.is_test_command("pytest tests/ -v"));
        assert!(!v.is_test_command("cargo build --release"));
    }

    #[test]
    fn code_file_matching_with_exempt() {
        let v = Config::builtin().verify;
        let cwd = Path::new("/proj");
        assert!(v.is_code_file("/proj/src/main.rs", cwd));
        assert!(v.is_code_file("src/lib.py", cwd)); // relative paths work too
        assert!(!v.is_code_file("/proj/docs/guide.md", cwd)); // exempt
        assert!(!v.is_code_file("/proj/notes.txt", cwd)); // not code
    }

    #[test]
    fn code_file_matching_with_backslash_separators() {
        let v = Config::builtin().verify;
        let cwd = Path::new("/proj");
        // Windows-style separators are normalized before glob matching
        assert!(v.is_code_file("src\\main.rs", cwd));
        assert!(!v.is_code_file("docs\\guide.md", cwd));
    }

    #[test]
    fn to_toml_roundtrip() {
        let c = Config::builtin();
        let s = c.to_toml_string();
        assert!(s.contains("[gates.verify]"));
        assert!(s.contains("mode = \"advisory\""));
        assert!(s.contains("[review]"));
    }
}
