# Harness Engineer (harness-rs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `harness` — a single Rust binary that acts as an engineering-discipline engine for Claude Code: configurable verification gates, programmatic installation, and an adversarial-review system, improving on fable-harness in every dimension.

**Architecture:** One clap CLI binary with two kinds of subcommands: management commands (install/uninstall/init/doctor/update/config) and the hook engine entry point (`harness hook <event>`, invoked by Claude Code hooks: stdin JSON → stdout JSON). All prompt/agent/skill assets are embedded via `include_str!` and materialized into `~/.claude/` at install time. Configuration merges across three layers (built-in → global → project).

**Tech Stack:** Rust 2021. Dependencies: clap 4 (derive), serde 1 (derive), serde_json 1 (preserve_order), toml 0.8, dirs 5, glob 0.3, sha2 0.10. Dev-deps: assert_cmd 2, predicates 3, tempfile 3.

## Global Constraints

- **Language policy: ALL documents, assets, code comments, commit messages, and user-facing CLI copy are in English.** The only exception: README gets an additional Traditional Chinese translation as `README.zh-TW.md` (Task 13).
- **Fail-open iron rule**: `harness hook <event>` must exit 0 with no blocking output on ANY internal error (panic, bad JSON, corrupt state file, unparsable config) — never break the user's session. Management commands are the opposite: report errors clearly and return a non-zero exit code.
- **settings.json protection**: timestamped backup before modification (`settings.json.bak.<epoch-secs>`); serde_json `preserve_order` keeps field order and unknown fields; only append our own entries; temp file + rename for atomic writes.
- **Self-identification marker**: every hook command registered into settings.json contains the substring `harness hook` (constant `HOOK_MARKER`); uninstall/doctor use it to recognize our entries and never touch anything else.
- **Three gate modes**: `strict` (returns block) / `advisory` (warns but allows) / `off`. Built-in default is `advisory`.
- **stop_hook_active guard**: the Stop gate must allow unconditionally when `stop_hook_active: true`, preventing infinite block loops (fable-harness gets this right; the design doc omitted it — this is a required addition).
- **Test-command matching uses substring** (the design doc said "prefix match", but a prefix cannot cover `cd x && cargo test`; this is a confirmed spec correction).
- State file location: `std::env::temp_dir()/harness-state/<session_id>.json`, with session_id sanitized first (keep only `[A-Za-z0-9_-]`).
- Asset target paths are rooted at `~/.claude/`: `harness/protocol.md`, `harness/config.toml` (global config — written only when missing, never overwritten), `agents/*.md`, `skills/adversarial-review/SKILL.md`.
- Manifest location: `~/.claude/harness/manifest.json`, recording the version and the official-content sha256 of every released file.
- End every commit message with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: Project skeleton and CLI entry point

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `src/commands/mod.rs` (stubs; each subcommand reports "not implemented yet")
- Create: `src/hooks/mod.rs` (fail-open entry)
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: nothing (first task)
- Produces: the `harness` binary; `hooks::run(event: &str) -> i32`; `commands::{install,uninstall,init,doctor,update,config_cmd}::run() -> i32` (stubs return 1 and print "not implemented yet"; later tasks replace them one by one)

- [ ] **Step 1: Create the project and dependencies**

```bash
cargo init --name harness
```

`Cargo.toml`:

```toml
[package]
name = "harness"
version = "0.1.0"
edition = "2021"
description = "Engineering-discipline engine for Claude Code"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
toml = "0.8"
dirs = "5"
glob = "0.3"
sha2 = "0.10"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

`.gitignore`:

```
/target
.DS_Store
```

- [ ] **Step 2: Write the failing integration test**

`tests/cli.rs`:

```rust
use assert_cmd::Command;

#[test]
fn hook_with_unknown_event_fails_open() {
    // Fail-open: even an unknown event must exit 0 with no output
    Command::cargo_bin("harness").unwrap()
        .args(["hook", "no-such-event"])
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn help_lists_subcommands() {
    let out = Command::cargo_bin("harness").unwrap()
        .arg("--help").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    for sub in ["install", "uninstall", "init", "doctor", "update", "config", "hook"] {
        assert!(text.contains(sub), "help is missing subcommand {sub}");
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --test cli`
Expected: compile failure (main.rs is still hello world, no subcommands)

- [ ] **Step 4: Implement the CLI skeleton**

`src/main.rs`:

```rust
mod commands;
mod hooks;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "harness", version, about = "Engineering-discipline engine for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Release assets into ~/.claude/, register hooks, install agents/skills
    Install,
    /// Cleanly remove everything harness registered or installed
    Uninstall,
    /// Generate a harness.toml customization layer in the current project
    Init,
    /// Health check: hooks registered, versions consistent, conflicting harnesses
    Doctor,
    /// Re-release assets after an upgrade (user-modified files are preserved)
    Update,
    /// Print the merged effective config (built-in + global + project)
    Config,
    /// Hook engine entry point, invoked by Claude Code
    Hook { event: String },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Install => commands::install::run(),
        Command::Uninstall => commands::uninstall::run(),
        Command::Init => commands::init::run(),
        Command::Doctor => commands::doctor::run(),
        Command::Update => commands::update::run(),
        Command::Config => commands::config_cmd::run(),
        Command::Hook { event } => hooks::run(&event),
    };
    std::process::exit(code);
}
```

`src/commands/mod.rs`:

```rust
macro_rules! stub {
    ($name:ident) => {
        pub mod $name {
            pub fn run() -> i32 {
                eprintln!("not implemented yet");
                1
            }
        }
    };
}
stub!(install);
stub!(uninstall);
stub!(init);
stub!(doctor);
stub!(update);
stub!(config_cmd);
```

`src/hooks/mod.rs`:

```rust
use std::io::Read;

/// Hook engine entry point. Iron rule: any internal error results in
/// allowing the action (exit 0, no output).
pub fn run(event: &str) -> i32 {
    std::panic::set_hook(Box::new(|_| {})); // silence panic messages
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload: serde_json::Value =
        serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(event, &payload)
    }));
    if let Ok(Some(output)) = result {
        println!("{output}");
    }
    0
}

fn dispatch(event: &str, _payload: &serde_json::Value) -> Option<String> {
    match event {
        // Later tasks wire up session-start / user-prompt / post-tool / stop
        _ => None,
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --test cli`
Expected: 2 passed

- [ ] **Step 6: Commit (including the design doc and this plan)**

```bash
git add .gitignore Cargo.toml Cargo.lock src tests docs
git commit -m "feat: harness CLI skeleton with fail-open hook entry

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Three-layer config merge (config.rs)

**Files:**
- Create: `src/config.rs`
- Create: `assets/default-config.toml`
- Modify: `src/main.rs` (add `mod config;`)

**Interfaces:**
- Consumes: nothing
- Produces:
  - `config::Config { verify: VerifyGate, review: Review }`
  - `config::VerifyGate { mode: GateMode, test_commands: Vec<String>, code_globs: Vec<String>, exempt_globs: Vec<String> }`
  - `config::Review { panel: Vec<String> }`
  - `config::GateMode { Strict, Advisory, Off }`, `GateMode::as_str() -> &'static str`
  - `Config::builtin() -> Config`
  - `Config::apply(&mut self, layer: &PartialConfig)` (field-level override: a value present in a higher layer replaces the whole field)
  - `Config::load(cwd: &Path) -> Config` (built-in → `~/.claude/harness/config.toml` → nearest `harness.toml` walking up from cwd; any layer that fails to parse is skipped)
  - `Config::load_layered(global_path: Option<&Path>, cwd: &Path) -> Config` (testable variant; `load` is a thin wrapper)
  - `config::PartialConfig` (serde Deserialize, matching the TOML `[gates.verify]` and `[review]` tables)
  - `Config::to_toml_string(&self) -> String`
  - `VerifyGate::is_test_command(&self, cmd: &str) -> bool` (substring match)
  - `VerifyGate::is_code_file(&self, path: &str, cwd: &Path) -> bool` (matches code_globs and not exempt_globs; tries both the absolute path and the cwd-relative path)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/config.rs`:

```rust
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
    fn to_toml_roundtrip() {
        let c = Config::builtin();
        let s = c.to_toml_string();
        assert!(s.contains("[gates.verify]"));
        assert!(s.contains("mode = \"advisory\""));
        assert!(s.contains("[review]"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test config`
Expected: compile failure (config module does not exist)

- [ ] **Step 3: Implement config.rs**

```rust
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
        let hit = |globs: &[String]| {
            globs
                .iter()
                .filter_map(|g| glob::Pattern::new(g).ok())
                .any(|p| p.matches(path) || p.matches(rel))
        };
        hit(&self.code_globs) && !hit(&self.exempt_globs)
    }
}
```

Add `mod config;` at the top of `src/main.rs`.

- [ ] **Step 4: Create assets/default-config.toml (global config template released at install)**

```toml
# Harness global config (~/.claude/harness/config.toml)
# A project can override any field with a harness.toml in its root directory.
# Fields not listed here use built-in defaults; run `harness config` to see
# the merged result.

[gates.verify]
# strict = block unverified turn endings / advisory = warn but allow / off
mode = "advisory"
# What counts as "ran the tests" (substring match). Uncomment to override
# the built-in list:
# test_commands = ["cargo test", "pytest", "npm test"]
# What counts as a "code change":
# code_globs = ["**/*.rs", "**/*.py"]
# Changes to these never trigger the gate:
# exempt_globs = ["**/docs/**", "**/*.md"]

[review]
# Adversarial review panel (harness install provides all five agents)
panel = ["skeptic", "red-team", "simplifier"]
# Full panel: ["skeptic", "red-team", "simplifier", "evidence-auditor", "user-advocate"]
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test config`
Expected: 8 passed

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/main.rs assets/default-config.toml
git commit -m "feat: three-layer config merge (built-in/global/project) and gate matchers

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Per-session state tracking (state.rs)

**Files:**
- Create: `src/state.rs`
- Modify: `src/main.rs` (add `mod state;`)

**Interfaces:**
- Consumes: nothing
- Produces:
  - `state::SessionState { seq: u64, last_code_change_seq: Option<u64>, last_test_seq: Option<u64>, changed_files: Vec<String> }` (serde Serialize/Deserialize/Default)
  - `SessionState::record_code_change(&mut self, file: &str)` (dedup-append into changed_files)
  - `SessionState::record_test_run(&mut self)` (**clears changed_files** — afterwards changed_files always equals "files changed since the last test run")
  - `SessionState::unverified_changes(&self) -> bool` (last code-change seq > last test seq, or changes exist with no test ever run)
  - `state::state_path(session_id: &str) -> PathBuf` (temp_dir/harness-state/<sanitized-id>.json)
  - `state::load(path: &Path) -> SessionState` (any error → Default, fail-open)
  - `state::save(path: &Path, st: &SessionState) -> std::io::Result<()>` (creates dirs, temp+rename atomic write)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/state.rs`:

```rust
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
    fn state_path_sanitizes_session_id() {
        let p = state_path("../../evil/../id_1-2");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "evilid_1-2.json"); // only [A-Za-z0-9_-] survives
        assert!(p.parent().unwrap().ends_with("harness-state"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test state`
Expected: compile failure (state module does not exist)

- [ ] **Step 3: Implement state.rs**

```rust
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
    std::env::temp_dir()
        .join("harness-state")
        .join(format!("{safe}.json"))
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
```

Add `mod state;` to `src/main.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test state`
Expected: 8 passed

- [ ] **Step 5: Commit**

```bash
git add src/state.rs src/main.rs
git commit -m "feat: per-session state tracking (seq ordering detects unverified changes)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: SessionStart and UserPromptSubmit hooks + behavior-protocol asset

**Files:**
- Create: `assets/protocol.md`
- Create: `src/hooks/session_start.rs`
- Create: `src/hooks/prompt_nudge.rs`
- Modify: `src/hooks/mod.rs` (wire the two events into dispatch)
- Test: `tests/hooks.rs`

**Interfaces:**
- Consumes: `config::Config::load`
- Produces:
  - `hooks::session_start::run(payload: &serde_json::Value) -> Option<String>`
  - `hooks::prompt_nudge::run(payload: &serde_json::Value) -> Option<String>`
  - Payload convention (shared by all hooks): `cwd` string field; falls back to `std::env::current_dir()` when missing
  - Dispatch event names: `"session-start"`, `"user-prompt"`

- [ ] **Step 1: Create assets/protocol.md**

```markdown
# HARNESS-PROTOCOL v1 (behavior protocol, injected at SessionStart)

Operate under this protocol regardless of the current model.

## 1. OODA loop (mandatory for every task)
- **Observe**: gather evidence in parallel before answering (Glob / Grep /
  Read in one batch); never guess without reading, never recite code from
  training memory.
- **Orient**: state assumptions explicitly; when multiple interpretations
  exist, list them for the user to choose — never pick silently; when truly
  uncertain, stop and ask.
- **Decide**: turn the task into a verifiable goal (fail-then-pass); weak
  goals like "make it work" must be strengthened into checkable conditions
  before acting.
- **Act**: small change → verify → iterate; every changed line must trace
  back to a user need.

## 2. Adversarial review (mandatory before trusting major conclusions)
- **Triggers**: architecture decisions / bug root-cause verdicts /
  conclusions affecting production / security judgments.
- **Process**: follow the adversarial-review skill — dispatch the review
  panel in parallel within a single message; panel membership comes from
  the harness `[review].panel` setting.
- **Acceptance bar**: a conclusion is confirmed only if a majority of
  lenses let it survive; an unreviewed single-source conclusion may only be
  labeled an "unchallenged assumption", never stated as fact.

## 3. Reporting discipline
- First sentence = the outcome (TLDR); supporting detail comes after.
- The final message must be self-contained — conclusions, numbers, and
  risks are restated there, since mid-turn messages may go unread.
- Report failures honestly: if tests are red, paste the red output; no
  sugarcoating, no "it should work".

## 4. Definition of Done
- Changed functional logic → at least one automated test plus
  fail-then-pass evidence.
- No evidence → never claim "done"; say "modified, unverified" instead.
- console.log / eyeballing / verbal reasoning ≠ verification.
```

- [ ] **Step 2: Write the failing integration tests**

`tests/hooks.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

fn run_hook(event: &str, stdin: &str) -> assert_cmd::assert::Assert {
    Command::cargo_bin("harness").unwrap()
        .args(["hook", event])
        .write_stdin(stdin.to_string())
        .assert()
}

#[test]
fn session_start_injects_protocol_and_gate_summary() {
    run_hook("session-start", r#"{"cwd": "/nonexistent"}"#)
        .success()
        .stdout(predicate::str::contains("HARNESS-PROTOCOL"))
        .stdout(predicate::str::contains("advisory")); // built-in default mode
}

#[test]
fn user_prompt_emits_one_line_nudge() {
    let assert = run_hook("user-prompt", "{}").success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.trim().lines().count() == 1, "nudge must be one line: {out}");
    assert!(out.contains("Harness"));
}

#[test]
fn hooks_fail_open_on_garbage_stdin() {
    run_hook("session-start", "not json at all").success();
    run_hook("user-prompt", "").success();
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test hooks`
Expected: FAIL (session-start produces no output yet; contains assertions fail)

- [ ] **Step 4: Implement both hooks**

`src/hooks/session_start.rs`:

```rust
use crate::config::Config;
use serde_json::Value;
use std::path::PathBuf;

pub const PROTOCOL: &str = include_str!("../../assets/protocol.md");

pub fn payload_cwd(payload: &Value) -> PathBuf {
    payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

pub fn run(payload: &Value) -> Option<String> {
    let config = Config::load(&payload_cwd(payload));
    Some(format!(
        "{PROTOCOL}\n## 5. Active harness gates\n- verify gate: {} mode\n- review panel: {}\n",
        config.verify.mode.as_str(),
        config.review.panel.join(", "),
    ))
}
```

`src/hooks/prompt_nudge.rs`:

```rust
use serde_json::Value;

pub fn run(_payload: &Value) -> Option<String> {
    Some(
        "🧭 Harness: Observe first (gather evidence) → state assumptions; functional changes need fail-then-pass evidence; adversarially review major conclusions before trusting them."
            .to_string(),
    )
}
```

Change dispatch in `src/hooks/mod.rs` to:

```rust
pub mod prompt_nudge;
pub mod session_start;

fn dispatch(event: &str, payload: &serde_json::Value) -> Option<String> {
    match event {
        "session-start" => session_start::run(payload),
        "user-prompt" => prompt_nudge::run(payload),
        _ => None,
    }
}
```

(The existing `use std::io::Read;` and `run` at the top of the file stay unchanged.)

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test hooks`
Expected: 3 passed

- [ ] **Step 6: Commit**

```bash
git add assets/protocol.md src/hooks tests/hooks.rs
git commit -m "feat: SessionStart protocol injection and UserPromptSubmit one-line nudge

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: PostToolUse hook (silent state recording)

**Files:**
- Create: `src/hooks/post_tool_use.rs`
- Modify: `src/hooks/mod.rs` (add `"post-tool"` to dispatch)
- Test: unit tests at the bottom of `src/hooks/post_tool_use.rs`

**Interfaces:**
- Consumes: `config::Config::load`, `config::VerifyGate::{is_code_file, is_test_command}`, `state::{load, save, state_path, SessionState}`, `session_start::payload_cwd`
- Produces:
  - `hooks::post_tool_use::run(payload: &Value) -> Option<String>` (always returns None — silent)
  - `hooks::post_tool_use::apply_event(st: &mut SessionState, config: &Config, cwd: &Path, tool_name: &str, tool_input: &Value)` (pure logic, unit-testable)
  - Payload fields: `session_id`, `tool_name`, `tool_input` (Edit/Write/MultiEdit use `file_path`, NotebookEdit uses `notebook_path`, Bash uses `command`)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/hooks/post_tool_use.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use serde_json::json;
    use std::path::Path;

    fn apply(st: &mut SessionState, tool: &str, input: serde_json::Value) {
        apply_event(st, &Config::builtin(), Path::new("/proj"), tool, &input);
    }

    #[test]
    fn edit_code_file_records_change() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        assert!(st.unverified_changes());
        assert_eq!(st.changed_files, vec!["/proj/src/main.rs".to_string()]);
    }

    #[test]
    fn edit_markdown_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Write", json!({"file_path": "/proj/README.md"}));
        assert!(!st.unverified_changes());
    }

    #[test]
    fn notebook_edit_uses_notebook_path() {
        let mut st = SessionState::default();
        apply(&mut st, "NotebookEdit", json!({"notebook_path": "/proj/a.py"}));
        assert!(st.unverified_changes());
    }

    #[test]
    fn bash_test_command_records_test() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        apply(&mut st, "Bash", json!({"command": "cd /proj && cargo test"}));
        assert!(!st.unverified_changes());
    }

    #[test]
    fn bash_non_test_command_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Edit", json!({"file_path": "/proj/src/main.rs"}));
        apply(&mut st, "Bash", json!({"command": "cargo build"}));
        assert!(st.unverified_changes());
    }

    #[test]
    fn unrelated_tool_is_ignored() {
        let mut st = SessionState::default();
        apply(&mut st, "Read", json!({"file_path": "/proj/src/main.rs"}));
        assert_eq!(st, SessionState::default());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test post_tool`
Expected: compile failure (module does not exist)

- [ ] **Step 3: Implement post_tool_use.rs**

```rust
use crate::config::Config;
use crate::hooks::session_start::payload_cwd;
use crate::state::{self, SessionState};
use serde_json::Value;
use std::path::Path;

pub fn run(payload: &Value) -> Option<String> {
    let session_id = payload.get("session_id")?.as_str()?;
    let tool_name = payload.get("tool_name")?.as_str()?;
    let empty = Value::Null;
    let tool_input = payload.get("tool_input").unwrap_or(&empty);
    let cwd = payload_cwd(payload);
    let config = Config::load(&cwd);

    let path = state::state_path(session_id);
    let mut st = state::load(&path);
    let before = st.seq;
    apply_event(&mut st, &config, &cwd, tool_name, tool_input);
    if st.seq != before {
        let _ = state::save(&path, &st); // write failure → fail-open, stay silent
    }
    None
}

pub fn apply_event(
    st: &mut SessionState,
    config: &Config,
    cwd: &Path,
    tool_name: &str,
    tool_input: &Value,
) {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            let file = tool_input
                .get("file_path")
                .or_else(|| tool_input.get("notebook_path"))
                .and_then(|v| v.as_str());
            if let Some(file) = file {
                if config.verify.is_code_file(file, cwd) {
                    st.record_code_change(file);
                }
            }
        }
        "Bash" => {
            if let Some(cmd) = tool_input.get("command").and_then(|v| v.as_str()) {
                if config.verify.is_test_command(cmd) {
                    st.record_test_run();
                }
            }
        }
        _ => {}
    }
}
```

Add to `src/hooks/mod.rs`:

```rust
pub mod post_tool_use;
// inside the dispatch match:
        "post-tool" => post_tool_use::run(payload),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test post_tool`
Expected: 6 passed

- [ ] **Step 5: Commit**

```bash
git add src/hooks
git commit -m "feat: PostToolUse silently records code changes and test runs

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: Stop verification gate

**Files:**
- Create: `src/hooks/stop_gate.rs`
- Modify: `src/hooks/mod.rs` (add `"stop"` to dispatch)
- Test: unit tests at the bottom of the file + an end-to-end case appended to `tests/hooks.rs`

**Interfaces:**
- Consumes: `config::{Config, GateMode}`, `state::{SessionState, load, state_path}`, `session_start::payload_cwd`
- Produces:
  - `hooks::stop_gate::run(payload: &Value) -> Option<String>`
  - `hooks::stop_gate::verdict(st: &SessionState, mode: GateMode, stop_hook_active: bool) -> Option<String>` (pure logic)
  - strict output: `{"decision":"block","reason":"⛔ Harness verify gate: ..."}`
  - advisory output: `{"systemMessage":"⚠️ Harness notice: ..."}`
  - off / verified / stop_hook_active → None

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/hooks/stop_gate.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GateMode;
    use crate::state::SessionState;

    fn dirty_state() -> SessionState {
        let mut st = SessionState::default();
        st.record_code_change("/p/src/a.rs");
        st
    }

    #[test]
    fn strict_blocks_unverified_changes() {
        let out = verdict(&dirty_state(), GateMode::Strict, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["decision"], "block");
        assert!(v["reason"].as_str().unwrap().contains("a.rs"));
    }

    #[test]
    fn advisory_warns_but_allows() {
        let out = verdict(&dirty_state(), GateMode::Advisory, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("decision").is_none()); // does not block
        assert!(v["systemMessage"].as_str().unwrap().contains("a.rs"));
    }

    #[test]
    fn off_is_silent() {
        assert_eq!(verdict(&dirty_state(), GateMode::Off, false), None);
    }

    #[test]
    fn verified_state_is_silent() {
        let mut st = dirty_state();
        st.record_test_run();
        assert_eq!(verdict(&st, GateMode::Strict, false), None);
    }

    #[test]
    fn stop_hook_active_always_allows() {
        // Second stop always passes — prevents infinite block loops
        assert_eq!(verdict(&dirty_state(), GateMode::Strict, true), None);
    }

    #[test]
    fn clean_session_is_silent() {
        assert_eq!(verdict(&SessionState::default(), GateMode::Strict, false), None);
    }
}
```

Append an end-to-end case to `tests/hooks.rs` (reusing the file's existing `run_hook`):

```rust
#[test]
fn stop_gate_end_to_end_strict_block() {
    // Arrange: use post-tool to create a "changed code, no test" state, then hit stop
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("harness.toml"),
        "[gates.verify]\nmode = \"strict\"",
    )
    .unwrap();
    let sid = "e2e-strict-block";
    let cwd = tmp.path().to_str().unwrap();
    run_hook(
        "post-tool",
        &format!(
            r#"{{"session_id":"{sid}","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"{cwd}/src/x.rs"}}}}"#
        ),
    )
    .success();
    run_hook(
        "stop",
        &format!(r#"{{"session_id":"{sid}","cwd":"{cwd}","stop_hook_active":false}}"#),
    )
    .success()
    .stdout(predicate::str::contains("\"decision\":\"block\""));
    // Clean up the state file so reruns are not polluted
    let _ = std::fs::remove_file(
        std::env::temp_dir().join("harness-state").join(format!("{sid}.json")),
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test stop`
Expected: compile failure (stop_gate does not exist)

- [ ] **Step 3: Implement stop_gate.rs**

```rust
use crate::config::{Config, GateMode};
use crate::hooks::session_start::payload_cwd;
use crate::state::{self, SessionState};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(payload: &Value) -> Option<String> {
    let stop_hook_active = payload
        .get("stop_hook_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let session_id = payload.get("session_id")?.as_str()?;
    let config = Config::load(&payload_cwd(payload));
    let st = state::load(&state::state_path(session_id));
    verdict(&st, config.verify.mode, stop_hook_active)
}

pub fn verdict(st: &SessionState, mode: GateMode, stop_hook_active: bool) -> Option<String> {
    if stop_hook_active || !st.unverified_changes() {
        return None;
    }
    let files: Vec<&str> = st
        .changed_files
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p.as_str())
        })
        .collect();
    let files = files.join(", ");
    match mode {
        GateMode::Off => None,
        GateMode::Strict => Some(
            json!({
                "decision": "block",
                "reason": format!(
                    "⛔ Harness verify gate: code was modified this turn ({files}) but no test run was detected afterwards. \
                     Run the relevant tests and provide fail-then-pass evidence. \
                     If tests are genuinely unnecessary (mid-task pause, experimental change), explain why to the user and end the turn again to pass."
                ),
            })
            .to_string(),
        ),
        GateMode::Advisory => Some(
            json!({
                "systemMessage": format!(
                    "⚠️ Harness notice: {files} changed but no subsequent test run was detected (advisory mode, allowing stop)."
                ),
            })
            .to_string(),
        ),
    }
}
```

Add to `src/hooks/mod.rs`:

```rust
pub mod stop_gate;
// inside the dispatch match:
        "stop" => stop_gate::run(payload),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: all pass (6 stop unit tests + 1 end-to-end case included)

- [ ] **Step 5: Commit**

```bash
git add src/hooks tests/hooks.rs
git commit -m "feat: Stop verify gate (strict/advisory/off + stop_hook_active guard)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: Safe settings.json merge (settings.rs)

**Files:**
- Create: `src/settings.rs`
- Modify: `src/main.rs` (add `mod settings;`)

**Interfaces:**
- Consumes: nothing
- Produces:
  - `settings::HOOK_MARKER: &str = "harness hook"`
  - `settings::HOOK_EVENTS: &[(&str, Option<&str>, &str)]` — (Claude Code event name, matcher, our command):
    - `("SessionStart", None, "harness hook session-start")`
    - `("UserPromptSubmit", None, "harness hook user-prompt")`
    - `("PostToolUse", Some("Edit|Write|MultiEdit|NotebookEdit|Bash"), "harness hook post-tool")`
    - `("Stop", None, "harness hook stop")`
  - `settings::register_hooks(settings_path: &Path) -> std::io::Result<Vec<String>>` (returns action descriptions for install to print; does backup, append, atomic write)
  - `settings::unregister_hooks(settings_path: &Path) -> std::io::Result<Vec<String>>` (removes only entries whose command contains HOOK_MARKER)
  - `settings::foreign_stop_hooks(settings_path: &Path) -> Vec<String>` (lists non-harness Stop hook commands, used by doctor for conflict detection)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/settings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn read_json(p: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn register_into_missing_file_creates_all_events() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        for (event, _, cmd) in HOOK_EVENTS {
            let arr = v["hooks"][event].as_array().unwrap();
            assert!(
                arr.iter().any(|g| g["hooks"].as_array().unwrap().iter()
                    .any(|h| h["command"] == *cmd)),
                "missing {event}"
            );
        }
    }

    #[test]
    fn register_preserves_existing_content_and_backs_up() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool check"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        assert_eq!(v["model"], "opus"); // unknown fields untouched
        let stops = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stops.len(), 2); // existing entry kept, ours appended after
        assert_eq!(stops[0]["hooks"][0]["command"], "other-tool check");
        // A backup file exists
        let backups: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings.json.bak."))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn register_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        register_hooks(&p).unwrap();
        register_hooks(&p).unwrap();
        let v = read_json(&p);
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unregister_removes_only_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool check"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        unregister_hooks(&p).unwrap();
        let v = read_json(&p);
        let stops = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stops.len(), 1);
        assert_eq!(stops[0]["hooks"][0]["command"], "other-tool check");
        // Our other event keys were emptied and removed
        assert!(v["hooks"].get("SessionStart").is_none());
    }

    #[test]
    fn foreign_stop_hooks_detects_other_gates() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"python verify_gate.py"}]}]}}"#).unwrap();
        register_hooks(&p).unwrap();
        assert_eq!(foreign_stop_hooks(&p), vec!["python verify_gate.py".to_string()]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test settings`
Expected: compile failure (settings module does not exist)

- [ ] **Step 3: Implement settings.rs**

```rust
use serde_json::{json, Map, Value};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const HOOK_MARKER: &str = "harness hook";

pub const HOOK_EVENTS: &[(&str, Option<&str>, &str)] = &[
    ("SessionStart", None, "harness hook session-start"),
    ("UserPromptSubmit", None, "harness hook user-prompt"),
    (
        "PostToolUse",
        Some("Edit|Write|MultiEdit|NotebookEdit|Bash"),
        "harness hook post-tool",
    ),
    ("Stop", None, "harness hook stop"),
];

fn load_settings(path: &Path) -> io::Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(path)?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is not a valid JSON object; refusing to modify it to avoid corruption",
                path.display()
            ),
        )),
    }
}

fn backup(path: &Path) -> io::Result<()> {
    if path.exists() {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let bak = path.with_file_name(format!(
            "{}.bak.{epoch}",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::copy(path, &bak)?;
    }
    Ok(())
}

fn atomic_write(path: &Path, map: &Map<String, Value>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(map.clone()))?)?;
    std::fs::rename(&tmp, path)
}

fn group_has_marker(group: &Value) -> bool {
    group["hooks"]
        .as_array()
        .map(|hs| {
            hs.iter().any(|h| {
                h["command"].as_str().map_or(false, |c| c.contains(HOOK_MARKER))
            })
        })
        .unwrap_or(false)
}

pub fn register_hooks(settings_path: &Path) -> io::Result<Vec<String>> {
    let mut map = load_settings(settings_path)?;
    backup(settings_path)?;
    let hooks = map
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "hooks field is not an object"))?;
    let mut actions = Vec::new();
    for (event, matcher, cmd) in HOOK_EVENTS {
        let arr = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]));
        let arr = arr
            .as_array_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "hook event is not an array"))?;
        if arr.iter().any(group_has_marker) {
            actions.push(format!("{event}: already registered, skipping"));
            continue;
        }
        let mut group = json!({"hooks": [{"type": "command", "command": cmd}]});
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        arr.push(group);
        actions.push(format!("{event}: registered `{cmd}`"));
    }
    atomic_write(settings_path, &map)?;
    Ok(actions)
}

pub fn unregister_hooks(settings_path: &Path) -> io::Result<Vec<String>> {
    let mut map = load_settings(settings_path)?;
    backup(settings_path)?;
    let mut actions = Vec::new();
    if let Some(hooks) = map.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let events: Vec<String> = hooks.keys().cloned().collect();
        for event in events {
            if let Some(arr) = hooks.get_mut(&event).and_then(|v| v.as_array_mut()) {
                let before = arr.len();
                arr.retain(|g| !group_has_marker(g));
                if arr.len() != before {
                    actions.push(format!("{event}: removed harness hook"));
                }
                if arr.is_empty() {
                    hooks.remove(&event);
                }
            }
        }
        if hooks.is_empty() {
            map.remove("hooks");
        }
    }
    atomic_write(settings_path, &map)?;
    Ok(actions)
}

/// Non-harness Stop hook commands (used by doctor to flag possible gate conflicts).
pub fn foreign_stop_hooks(settings_path: &Path) -> Vec<String> {
    let Ok(map) = load_settings(settings_path) else {
        return Vec::new();
    };
    map.get("hooks")
        .and_then(|h| h.get("Stop"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|g| !group_has_marker(g))
                .filter_map(|g| g["hooks"].as_array())
                .flatten()
                .filter_map(|h| h["command"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
```

Add `mod settings;` to `src/main.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test settings`
Expected: 5 passed

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs src/main.rs
git commit -m "feat: safe settings.json merge (backup/append-only/atomic/self-marker)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: Asset embedding + install command

**Files:**
- Create: `assets/agents/skeptic.md`, `assets/agents/red-team.md`, `assets/agents/simplifier.md`, `assets/agents/evidence-auditor.md`, `assets/agents/user-advocate.md`
- Create: `assets/skills/adversarial-review/SKILL.md`
- Create: `src/assets.rs`
- Create: `src/manifest.rs`
- Modify: `src/commands/mod.rs` (remove the install stub, add `pub mod install;`)
- Create: `src/commands/install.rs`
- Modify: `src/main.rs` (add `mod assets; mod manifest;`)

**Interfaces:**
- Consumes: `settings::register_hooks`
- Produces:
  - `assets::Asset { pub rel_path: &'static str, pub content: &'static str }` (rel_path relative to `~/.claude/`)
  - `assets::ASSETS: &[Asset]` (protocol, 5 agents, 1 skill; **excludes** default-config, which follows a special rule)
  - `assets::DEFAULT_CONFIG: &str`
  - `manifest::Manifest { version: String, files: BTreeMap<String, String> }` (rel_path → official-content sha256)
  - `manifest::sha256_hex(text: &str) -> String`
  - `Manifest::load(claude_dir: &Path) -> Option<Manifest>` / `Manifest::save(&self, claude_dir: &Path) -> io::Result<()>` (location `<claude_dir>/harness/manifest.json`)
  - `install::release_assets(claude_dir: &Path) -> io::Result<Vec<String>>` (shared by install/update): per asset — missing → write; content == official → skip; user-modified → keep original, write `<path>.new`, report; finally save the manifest (recording official hashes)
  - `install::install_to(claude_dir: &Path) -> io::Result<Vec<String>>` (release_assets + write config.toml only if missing + register_hooks)
  - `install::run() -> i32` (claude_dir = `~/.claude`)

- [ ] **Step 1: Create the five agent assets**

`assets/agents/skeptic.md`:

```markdown
---
name: skeptic
description: Adversarial review — correctness lens. Given a conclusion/design/root-cause verdict, the default stance is "overturn it": hunt for logic holes, unverified assumptions, and counterexamples.
tools: Read, Grep, Glob, Bash
---

You are the "skeptic" in an adversarial review. Default stance: **this conclusion is wrong — prove it**.

Upon receiving a conclusion under review:
1. List every assumption the conclusion depends on (explicit and implicit).
2. Check each one: which have no evidence? Which can be verified right now with Read/Grep/Bash? Go verify them.
3. Actively construct counterexamples: what input, what timing, what environment makes the conclusion fail?
4. When uncertain, lean toward rejection (REFUTED); reasons must be specific down to file:line or reproducible steps.

Return format (raw data, no pleasantries):
```
verdict: REFUTED | SURVIVED
confidence: high | medium | low
reasons:
- <specific reason with file:line or a counterexample>
untested_assumptions:
- <assumptions the conclusion still relies on that you could not verify>
```
```

`assets/agents/red-team.md`:

```markdown
---
name: red-team
description: Adversarial review — security and failure lens. Hunts for security risks, failure modes, boundary conditions, resource exhaustion, and permission issues.
tools: Read, Grep, Glob, Bash
---

You are the "red team" in an adversarial review. Default stance: **this plan will blow up in production — find out how**.

Upon receiving a conclusion under review:
1. Map the threat surface: input sources, trust boundaries, external dependencies, concurrency points, failure paths.
2. Attack each one: malicious input, extreme values, partial failures, race conditions, missing permissions, full disk / dead network.
3. Every attack must be concrete: attack vector + trigger condition + consequence.
4. When uncertain, lean toward rejection (REFUTED).

Return format (raw data, no pleasantries):
```
verdict: REFUTED | SURVIVED
confidence: high | medium | low
reasons:
- <attack vector and consequence, with file:line>
untested_assumptions:
- <security assumptions you could not verify>
```
```

`assets/agents/simplifier.md`:

```markdown
---
name: simplifier
description: Adversarial review — simplification lens. Anti-overengineering; challenges unnecessary complexity, superfluous abstraction layers, and YAGNI violations.
tools: Read, Grep, Glob, Bash
---

You are the "simplifier" in an adversarial review. Default stance: **this plan is overcomplicated — a simpler approach exists**.

Upon receiving a conclusion under review:
1. Ask: if this component/abstraction/config option were deleted, what would break? If nothing can be named, it should be deleted.
2. Find YAGNI violations: things added for imagined future needs.
3. Find duplication: is there an existing library, language feature, or existing code that already does this?
4. Propose a concrete simpler alternative; only return SURVIVED if it truly is minimal already.

Return format (raw data, no pleasantries):
```
verdict: REFUTED | SURVIVED
confidence: high | medium | low
reasons:
- <the overcomplicated part and the simpler alternative>
untested_assumptions:
- <complexity justifications you could not verify>
```
```

`assets/agents/evidence-auditor.md`:

```markdown
---
name: evidence-auditor
description: Adversarial review — evidence lens. Audits every claim for supporting evidence — file:line, test output, measurements. Unsupported claims get flagged.
tools: Read, Grep, Glob, Bash
---

You are the "evidence auditor" in an adversarial review. Default stance: **every claim is unproven — demand evidence line by line**.

Upon receiving a conclusion under review:
1. Break the conclusion into independently checkable claims.
2. For each claim ask: where is the evidence? file:line? test output? measured numbers?
3. For cited evidence, actually Read/Bash to verify the citation is real and not taken out of context.
4. Any key claim without evidence → REFUTED, listing exactly what evidence is missing.

Return format (raw data, no pleasantries):
```
verdict: REFUTED | SURVIVED
confidence: high | medium | low
reasons:
- <claim → evidence status (present / missing / miscited)>
untested_assumptions:
- <claims that could not be audited>
```
```

`assets/agents/user-advocate.md`:

```markdown
---
name: user-advocate
description: Adversarial review — requirements lens. Checks whether the solution actually solves the user's original need; catches requirement drift, scope creep, and answering the wrong question.
tools: Read, Grep, Glob, Bash
---

You are the "user advocate" in an adversarial review. Default stance: **this solution does not solve the user's actual problem**.

Upon receiving a conclusion under review:
1. Reconstruct the original need: what did the user ask for, in their words — not the implementer's translation?
2. Map each part of the solution to a requirement; anything unmapped is scope creep.
3. Check the reverse: which requirement is covered by nothing? That is a gap.
4. Check usability: where will the user get stuck on first use?

Return format (raw data, no pleasantries):
```
verdict: REFUTED | SURVIVED
confidence: high | medium | low
reasons:
- <requirement → coverage status (covered / missing / creep)>
untested_assumptions:
- <requirement interpretations you could not confirm>
```
```

- [ ] **Step 2: Create the adversarial-review skill asset**

`assets/skills/adversarial-review/SKILL.md`:

```markdown
---
name: adversarial-review
description: Adversarial review — for major conclusions/architecture decisions/bug root-cause verdicts/security judgments, dispatch a review panel in parallel (membership from the harness [review].panel setting) for independent review; adopt only if a majority survives. Triggers: "adversarial review", "challenge this", or the trigger conditions in HARNESS-PROTOCOL section 2. Not for: trivial changes, pure Q&A.
---

# Adversarial Review

## Purpose
Prevent "sounds right but is actually wrong" conclusions from being adopted. A single model reviewing itself systematically favors its own conclusions, so multiple **independent subagents with different lenses and a default stance of overturning** cross-examine it.

## Panel
Run `harness config` to see this project's `[review].panel`; the default is `skeptic`, `red-team`, `simplifier`. The full set of lenses:

| Agent | Lens |
|---|---|
| skeptic | logic holes, unverified inference |
| red-team | security risks and failure modes |
| simplifier | anti-overengineering |
| evidence-auditor | claim-by-claim evidence audit |
| user-advocate | requirement fit |

## Steps

1. **Prepare the review package**: write the conclusion under review as one self-contained statement: the conclusion itself (one sentence), the evidence behind it (file:line, test output), and the blast radius. Multiple independent findings must each be reviewed separately — never bundled.
2. **Dispatch the whole panel in parallel within one message** (Agent tool, subagent_type set to each panel member's name, all at once — never serially). Each agent's prompt = the review package verbatim + that lens's own task instructions.
3. **Verdict (majority-survival rule)**:
   - Majority SURVIVED → confirmed; the reasons behind any REFUTED vote must be listed as risks when reporting to the user.
   - No majority → **conclusion rejected**; fix it per the REFUTED reasons and resubmit.
4. **Report format** (required in the final message to the user): a verdict table with one row per member, plus one closing line: `Review result: N/M survived → confirmed / rejected (reason)`.

## Prohibitions
- Never skip a panel member to save time.
- Never merge multiple lenses into a single agent run (independence is the point).
- Never silently swallow REFUTED reasons — report or fix them.
```

- [ ] **Step 3: Write the failing unit tests**

At the bottom of `src/commands/install.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test install`
Expected: compile failure (assets/manifest/install modules do not exist)

- [ ] **Step 5: Implement assets.rs, manifest.rs, and install.rs**

`src/assets.rs`:

```rust
pub struct Asset {
    /// Target path relative to ~/.claude/
    pub rel_path: &'static str,
    pub content: &'static str,
}

pub const ASSETS: &[Asset] = &[
    Asset {
        rel_path: "harness/protocol.md",
        content: include_str!("../assets/protocol.md"),
    },
    Asset {
        rel_path: "agents/skeptic.md",
        content: include_str!("../assets/agents/skeptic.md"),
    },
    Asset {
        rel_path: "agents/red-team.md",
        content: include_str!("../assets/agents/red-team.md"),
    },
    Asset {
        rel_path: "agents/simplifier.md",
        content: include_str!("../assets/agents/simplifier.md"),
    },
    Asset {
        rel_path: "agents/evidence-auditor.md",
        content: include_str!("../assets/agents/evidence-auditor.md"),
    },
    Asset {
        rel_path: "agents/user-advocate.md",
        content: include_str!("../assets/agents/user-advocate.md"),
    },
    Asset {
        rel_path: "skills/adversarial-review/SKILL.md",
        content: include_str!("../assets/skills/adversarial-review/SKILL.md"),
    },
];

pub const DEFAULT_CONFIG: &str = include_str!("../assets/default-config.toml");
```

`src/manifest.rs`:

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// rel_path → official-content sha256 (as of the last release)
    pub files: BTreeMap<String, String>,
}

pub fn sha256_hex(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())
}

impl Manifest {
    pub fn path(claude_dir: &Path) -> std::path::PathBuf {
        claude_dir.join("harness/manifest.json")
    }

    pub fn load(claude_dir: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(Self::path(claude_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, claude_dir: &Path) -> io::Result<()> {
        let path = Self::path(claude_dir);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}
```

`src/commands/install.rs`:

```rust
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
```

`src/commands/mod.rs`: replace `stub!(install);` with `pub mod install;`.
`src/main.rs`: add `mod assets; mod manifest;`.

Note: in the test, `target.with_extension("md.new")` on `skeptic.md` yields `skeptic.md.new` — `with_extension` replaces `md` with `md.new`, matching the implementation's `with_file_name(format!("{}.new", ...))` result.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test install`
Expected: 3 passed

- [ ] **Step 7: Commit**

```bash
git add assets src/assets.rs src/manifest.rs src/commands src/main.rs
git commit -m "feat: embedded assets (5 agents + skill + protocol) and install command

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: uninstall command

**Files:**
- Modify: `src/commands/mod.rs` (`stub!(uninstall)` → `pub mod uninstall;`)
- Create: `src/commands/uninstall.rs`

**Interfaces:**
- Consumes: `settings::unregister_hooks`, `manifest::{Manifest, sha256_hex}`
- Produces:
  - `uninstall::uninstall_from(claude_dir: &Path) -> io::Result<Vec<String>>`: unregister hooks; per manifest entry — delete only when the current hash == recorded hash (user-modified files are kept and reported); delete the manifest; **never delete** `harness/config.toml`
  - `uninstall::run() -> i32`

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/commands/uninstall.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test uninstall`
Expected: compile failure

- [ ] **Step 3: Implement uninstall.rs**

```rust
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
        }
        std::fs::remove_file(Manifest::path(claude_dir))?;
    }
    Ok(actions)
}
```

`src/commands/mod.rs`: `stub!(uninstall);` → `pub mod uninstall;`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test uninstall`
Expected: 3 passed

- [ ] **Step 5: Commit**

```bash
git add src/commands
git commit -m "feat: uninstall removes only harness-owned items, keeps user customizations

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 10: init and config commands

**Files:**
- Modify: `src/commands/mod.rs` (replace the two stubs with `pub mod init; pub mod config_cmd;`)
- Create: `src/commands/init.rs`
- Create: `src/commands/config_cmd.rs`

**Interfaces:**
- Consumes: `config::Config::{load, to_toml_string}`
- Produces:
  - `init::init_at(dir: &Path) -> io::Result<String>` (writes a `harness.toml` template; errors without overwriting if one exists)
  - `init::run() -> i32` (dir = cwd)
  - `config_cmd::run() -> i32` (prints `Config::load(cwd).to_toml_string()` plus the source-layer paths)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/commands/init.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test init`
Expected: compile failure

- [ ] **Step 3: Implement init.rs and config_cmd.rs**

`src/commands/init.rs`:

```rust
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
```

`src/commands/config_cmd.rs`:

```rust
use crate::config::Config;

pub fn run() -> i32 {
    let cwd = std::env::current_dir().unwrap_or_default();
    let global = dirs::home_dir().map(|h| h.join(".claude/harness/config.toml"));
    println!("# Merged config (built-in → global → project)");
    if let Some(g) = &global {
        println!(
            "# Global layer: {} ({})",
            g.display(),
            if g.exists() { "present" } else { "absent, using built-ins" }
        );
    }
    println!();
    print!("{}", Config::load(&cwd).to_toml_string());
    0
}
```

`src/commands/mod.rs`: replace the two stubs.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test init`
Expected: 2 passed. Also run `cargo run -- config`; Expected: prints the merged config containing `[gates.verify]`.

- [ ] **Step 5: Commit**

```bash
git add src/commands
git commit -m "feat: init generates the project layer; config prints the merged settings

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 11: doctor command

**Files:**
- Modify: `src/commands/mod.rs` (`stub!(doctor)` → `pub mod doctor;`)
- Create: `src/commands/doctor.rs`

**Interfaces:**
- Consumes: `settings::{HOOK_EVENTS, HOOK_MARKER, foreign_stop_hooks}`, `manifest::{Manifest, sha256_hex}`, `assets::ASSETS`
- Produces:
  - `doctor::Report { pub problems: Vec<String>, pub warnings: Vec<String>, pub oks: Vec<String> }`
  - `doctor::check(claude_dir: &Path) -> Report`:
    1. Manifest present? Version == binary version? (mismatch → problem "run harness update")
    2. All four events in settings.json carry an entry containing HOOK_MARKER? (missing → problem "run harness install")
    3. Every asset file present? (missing → problem) Hash differs from the manifest? (→ warning "customized")
    4. Any non-harness Stop hook? (→ warning "another verification gate detected; may double-block")
  - `doctor::run() -> i32` (problems → 1, otherwise 0)

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/commands/doctor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::install_to;

    #[test]
    fn healthy_install_has_no_problems() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        let r = check(tmp.path());
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn missing_install_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let r = check(tmp.path());
        assert!(!r.problems.is_empty());
        assert!(r.problems.iter().any(|p| p.contains("harness install")));
    }

    #[test]
    fn user_modified_asset_is_warning_not_problem() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("agents/skeptic.md"), "modified").unwrap();
        let r = check(tmp.path());
        assert!(r.problems.is_empty());
        assert!(r.warnings.iter().any(|w| w.contains("skeptic.md")));
    }

    #[test]
    fn foreign_stop_hook_is_warning() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path()).unwrap();
        // Manually inject a third-party Stop hook
        let p = tmp.path().join("settings.json");
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        v["hooks"]["Stop"].as_array_mut().unwrap().push(serde_json::json!(
            {"hooks":[{"type":"command","command":"python verify_gate.py"}]}
        ));
        std::fs::write(&p, v.to_string()).unwrap();
        let r = check(tmp.path());
        assert!(r.warnings.iter().any(|w| w.contains("verify_gate.py")));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test doctor`
Expected: compile failure

- [ ] **Step 3: Implement doctor.rs**

```rust
use crate::assets::ASSETS;
use crate::manifest::{sha256_hex, Manifest};
use crate::settings::{self, HOOK_EVENTS, HOOK_MARKER};
use std::path::Path;

#[derive(Debug, Default)]
pub struct Report {
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
    pub oks: Vec<String>,
}

pub fn run() -> i32 {
    let Some(home) = dirs::home_dir() else {
        eprintln!("error: could not determine the home directory");
        return 1;
    };
    let r = check(&home.join(".claude"));
    for ok in &r.oks {
        println!("ok: {ok}");
    }
    for w in &r.warnings {
        println!("warning: {w}");
    }
    for p in &r.problems {
        println!("problem: {p}");
    }
    if r.problems.is_empty() {
        println!("All checks passed.");
        0
    } else {
        1
    }
}

pub fn check(claude_dir: &Path) -> Report {
    let mut r = Report::default();

    // 1. Manifest and version
    match Manifest::load(claude_dir) {
        None => r
            .problems
            .push("manifest not found — run harness install".to_string()),
        Some(m) => {
            let bin_version = env!("CARGO_PKG_VERSION");
            if m.version == bin_version {
                r.oks.push(format!("versions match ({bin_version})"));
            } else {
                r.problems.push(format!(
                    "binary version {bin_version} differs from installed asset version {} — run harness update",
                    m.version
                ));
            }
            // 3. Asset presence and customization status
            for asset in ASSETS {
                let target = claude_dir.join(asset.rel_path);
                match std::fs::read_to_string(&target) {
                    Err(_) => r.problems.push(format!(
                        "missing {} — run harness install",
                        asset.rel_path
                    )),
                    Ok(current) => {
                        let recorded = m.files.get(asset.rel_path);
                        if recorded.map(|h| *h == sha256_hex(&current)).unwrap_or(false) {
                            // matches the official copy; stay silent
                        } else {
                            r.warnings.push(format!(
                                "{} is customized (update will preserve it)",
                                asset.rel_path
                            ));
                        }
                    }
                }
            }
        }
    }

    // 2. Hook registration
    let settings_path = claude_dir.join("settings.json");
    let text = std::fs::read_to_string(&settings_path).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    for (event, _, _) in HOOK_EVENTS {
        let registered = v["hooks"][event]
            .as_array()
            .map(|arr| {
                arr.iter().any(|g| {
                    g["hooks"].as_array().map_or(false, |hs| {
                        hs.iter().any(|h| {
                            h["command"].as_str().map_or(false, |c| c.contains(HOOK_MARKER))
                        })
                    })
                })
            })
            .unwrap_or(false);
        if registered {
            r.oks.push(format!("{event} hook registered"));
        } else {
            r.problems
                .push(format!("{event} hook not registered — run harness install"));
        }
    }

    // 4. Conflicting gates
    for cmd in settings::foreign_stop_hooks(&settings_path) {
        r.warnings.push(format!(
            "another Stop hook detected (`{cmd}`); it may double-block alongside the harness verify gate"
        ));
    }
    r
}
```

`src/commands/mod.rs`: `stub!(doctor);` → `pub mod doctor;`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test doctor`
Expected: 4 passed

- [ ] **Step 5: Commit**

```bash
git add src/commands
git commit -m "feat: doctor health check (version/registration/assets/gate conflicts)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 12: update command

**Files:**
- Modify: `src/commands/mod.rs` (`stub!(update)` → `pub mod update;`)
- Create: `src/commands/update.rs`

**Interfaces:**
- Consumes: `install::release_assets`
- Produces:
  - `update::run() -> i32`: runs `release_assets` against `~/.claude` (semantics: re-release assets; customized files are kept and the official copy lands in `.new`), printing actions and a summary

- [ ] **Step 1: Write the failing unit tests**

At the bottom of `src/commands/update.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test update`
Expected: compile failure (update module does not exist; the test lives inside update.rs)

- [ ] **Step 3: Implement update.rs**

```rust
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
```

`src/commands/mod.rs`: `stub!(update);` → `pub mod update;` (at this point no stubs remain; delete the `macro_rules! stub` definition).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/commands
git commit -m "feat: update re-releases assets while preserving user customizations

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 13: End-to-end tests + README (English) + README.zh-TW.md

**Files:**
- Create: `tests/e2e.rs`
- Create: `README.md` (English)
- Create: `README.zh-TW.md` (Traditional Chinese translation — the ONLY non-English document in the repo)

**Interfaces:**
- Consumes: all commands. Integration tests redirect the home directory to a tempdir via the `HOME` env var (`dirs::home_dir` reads `$HOME` on unix).
- Produces: full-lifecycle verification + user documentation.

- [ ] **Step 1: Write the end-to-end tests**

`tests/e2e.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

fn harness(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("harness").unwrap();
    c.env("HOME", home);
    c
}

#[test]
fn full_lifecycle_install_doctor_uninstall() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    harness(home).arg("install").assert().success()
        .stdout(predicate::str::contains("harness installed"));

    harness(home).arg("doctor").assert().success()
        .stdout(predicate::str::contains("All checks passed"));

    // Install is idempotent
    harness(home).arg("install").assert().success();
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert_eq!(settings.matches("harness hook stop").count(), 1);

    harness(home).arg("uninstall").assert().success();
    harness(home).arg("doctor").assert().failure()
        .stdout(predicate::str::contains("harness install"));
}

#[test]
fn hook_stop_fails_open_with_corrupt_state() {
    let tmp = tempfile::tempdir().unwrap();
    // Plant a corrupt state file
    let sid = "e2e-corrupt";
    let dir = std::env::temp_dir().join("harness-state");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{sid}.json")), "{{{broken").unwrap();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "stop"])
        .write_stdin(format!(r#"{{"session_id":"{sid}","stop_hook_active":false}}"#))
        .assert()
        .success()
        .stdout(""); // corrupt state → treated as clean → allow with no output
    let _ = std::fs::remove_file(dir.join(format!("{sid}.json")));
}

#[test]
fn advisory_mode_warns_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    // No harness.toml → built-in advisory mode
    let sid = "e2e-advisory";
    let cwd = proj.to_str().unwrap();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "post-tool"])
        .write_stdin(format!(
            r#"{{"session_id":"{sid}","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"{cwd}/src/a.rs"}}}}"#
        ))
        .assert()
        .success();
    Command::cargo_bin("harness").unwrap()
        .env("HOME", tmp.path())
        .args(["hook", "stop"])
        .write_stdin(format!(r#"{{"session_id":"{sid}","cwd":"{cwd}","stop_hook_active":false}}"#))
        .assert()
        .success()
        .stdout(predicate::str::contains("systemMessage"));
    let _ = std::fs::remove_file(
        std::env::temp_dir().join("harness-state").join(format!("{sid}.json")),
    );
}
```

- [ ] **Step 2: Run the tests to verify everything is green**

Run: `cargo test`
Expected: all pass (unit + integration + e2e)

- [ ] **Step 3: Write README.md (English)**

Full content (write it out completely):

```markdown
# harness — Engineering-discipline engine for Claude Code

A single Rust binary that establishes behavioral baselines for Claude Code:
evidence before claims, explicit assumptions, adversarial review for major
decisions, and test-verified code changes. Install once, effective across
all your projects; each project can layer its own customizations on top.

[繁體中文說明](README.zh-TW.md)

## Install

```bash
cargo install --path .
harness install    # release assets + register hooks + install agents/skills
harness doctor     # health check
```

## Commands

| Command | Purpose |
|---|---|
| `harness install` | Release assets into ~/.claude/ and register hooks |
| `harness uninstall` | Clean removal (only harness-owned items; your customizations and global config are kept) |
| `harness init` | Generate a harness.toml customization layer in the current project |
| `harness doctor` | Health check: hook registration, version consistency, gate conflicts |
| `harness update` | Re-release assets after an upgrade (your modified files are kept; official copies land in .new files) |
| `harness config` | Print the merged effective config (built-in → global → project) |
| `harness hook <event>` | Hook engine entry point (invoked by Claude Code; not for manual use) |

## The verify gate

Changed code without running tests afterwards? At the end of the turn,
the gate reacts according to its mode:

- `strict`: blocks, demanding tests (or an explanation to the user; the
  second stop attempt passes)
- `advisory` (default): warns but allows
- `off`: no check

Drop a `harness.toml` in a project root to override anything
(`harness init` generates the template):

```toml
[gates.verify]
mode = "strict"
test_commands = ["cargo test"]
```

## Adversarial review

Five agent lenses (skeptic / red-team / simplifier / evidence-auditor /
user-advocate) plus an `adversarial-review` skill: major conclusions are
adopted only when a majority of the panel lets them survive. Panel
membership is set via `[review].panel`.

## Design principles

- **Fail-open**: any internal error in the hook engine allows the action —
  it never breaks your session.
- **settings.json safety**: timestamped backups, append-only merging,
  atomic writes, unknown fields preserved.
- **Your customizations win**: install/update/uninstall never overwrite or
  delete files you have modified.
```

- [ ] **Step 4: Write README.zh-TW.md (Traditional Chinese translation)**

Full content (this is the only non-English document in the repo):

```markdown
# harness — Claude Code 工程紀律引擎

一個 Rust 單一 binary,為 Claude Code 建立行為底線:證據先行、假設明示、
重大決策經對抗審查、程式碼變更必經測試驗證。安裝一次,所有專案生效;
每個專案可疊加自己的客製層。

[English README](README.md)

## 安裝

```bash
cargo install --path .
harness install    # 釋出 assets + 註冊 hooks + 安裝 agents/skills
harness doctor     # 體檢
```

## 指令

| 指令 | 作用 |
|---|---|
| `harness install` | 釋出 assets 到 ~/.claude/、註冊 hooks |
| `harness uninstall` | 乾淨移除(只刪自己的東西,保留你的客製與全域設定) |
| `harness init` | 在當前專案產生 harness.toml 客製層 |
| `harness doctor` | 體檢:hooks 註冊、版本一致、閘門衝突 |
| `harness update` | 升版後重釋 assets(你改過的檔案保留,官方新版存 .new) |
| `harness config` | 顯示合併後設定(內建 → 全域 → 專案) |
| `harness hook <event>` | hook 引擎入口(Claude Code 呼叫,不需手動使用) |

## 驗證閘門

改了程式碼卻沒在其後跑測試?回合結束時依模式處理:

- `strict`:擋下,要求補測試(或向使用者說明原因後,第二次結束放行)
- `advisory`(預設):附警告放行
- `off`:不檢查

專案根目錄放 `harness.toml` 即可覆寫(`harness init` 產生範本):

```toml
[gates.verify]
mode = "strict"
test_commands = ["cargo test"]
```

## 對抗審查

五個 agent 鏡頭(skeptic / red-team / simplifier / evidence-auditor /
user-advocate)+ `adversarial-review` skill,重大結論過半存活才採信。
小組成員由 `[review].panel` 設定。

## 設計原則

- **Fail-open**:hook 引擎任何內部錯誤一律放行,絕不弄壞你的 session。
- **settings.json 安全**:時間戳備份、只增不覆、原子寫入、保留未知欄位。
- **你的客製優先**:install/update/uninstall 都不會覆蓋或刪除你改過的檔案。
```

- [ ] **Step 5: Manual smoke test of the install flow**

```bash
cargo build
HOME=$(mktemp -d) ./target/debug/harness install
```

Expected: prints release/registration actions + "harness installed". Then:

```bash
echo '{}' | ./target/debug/harness hook session-start
```

Expected: prints the full HARNESS-PROTOCOL text plus the gate summary.

- [ ] **Step 6: Commit**

```bash
git add tests/e2e.rs README.md README.zh-TW.md
git commit -m "test: end-to-end lifecycle tests; docs: README (en) + README.zh-TW

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Acceptance cross-check (plan coverage of the spec)

| Spec section | Covering task(s) |
|---|---|
| §2 seven CLI commands | Task 1 (skeleton), 8 (install), 9 (uninstall), 10 (init/config), 11 (doctor), 12 (update) |
| §3 four hooks + fail-open | Task 1 (fail-open entry), 4 (SessionStart/UserPromptSubmit), 5 (PostToolUse), 6 (Stop) |
| §4 three-layer config | Task 2 |
| §5 five agents + skill | Task 8 |
| §6 repo structure | file layout across tasks |
| §7 settings.json protection / uninstall touches only its own / update preserves customizations | Tasks 7, 8, 9, 12 |
| §8 test strategy | TDD steps in every task + Task 13 e2e |
| §9 YAGNI (no plugin, no PreToolUse) | intentionally absent from this plan |
