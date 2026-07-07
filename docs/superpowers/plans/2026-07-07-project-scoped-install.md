# Project-Scoped Install（--project flag）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 讓 `install` / `uninstall` / `doctor` / `update` 支援 `--project` flag，把 harness 裝進（或管理）`<cwd>/.claude/` 而非 `~/.claude/`，全域安裝行為不變、兩者可共存。

**Architecture:** 四個生命週期指令的核心函式（`install_to` / `uninstall_from` / `check` / `update_at`）已參數化接受 `claude_dir: &Path`，本擴充只在 CLI 層新增路徑解析（`resolve_claude_dir`）與少量行為分支（專案安裝不建 config、doctor 共存提示、protocol 三層讀取）。

**Tech Stack:** Rust、clap（derive）、tempfile / assert_cmd / predicates（測試）。

**Spec:** `docs/superpowers/specs/2026-07-07-project-scoped-install-design.md`

## Global Constraints

- 執行任何 cargo 指令前先 `export PATH="$HOME/.cargo/bin:$PATH"`（此機器 cargo 不在預設 PATH）。
- 全域（無 flag）行為必須逐字元不變：既有訊息字串、檔案佈局、退出碼都不得改動。
- 全域與專案註冊的 hook 指令字串必須逐字元相同（皆來自 `settings::HOOK_EVENTS` 常數）——Claude Code 靠這個去重。
- 專案安裝**不建立** `harness/config.toml`。
- `init` / `config` / `hook` 三個子指令不改動。
- 每個 task 內：先寫失敗測試 → 確認失敗 → 最小實作 → 確認通過 → commit。
- commit 訊息結尾加 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`。

---

### Task 1: `resolve_claude_dir` 路徑解析 helper

**Files:**
- Modify: `src/commands/mod.rs`（目前只有 6 行 `pub mod` 宣告）

**Interfaces:**
- Produces: `pub fn resolve_claude_dir(project: bool) -> Result<std::path::PathBuf, String>` —— `false` → `~/.claude`；`true` → `<cwd>/.claude`。後續 Task 2–5 的 `run(project)` 都呼叫它。

- [ ] **Step 1: 在 `src/commands/mod.rs` 底部加入失敗測試**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_scope_resolves_to_home_claude() {
        let d = resolve_claude_dir(false).unwrap();
        assert_eq!(d, dirs::home_dir().unwrap().join(".claude"));
    }

    #[test]
    fn project_scope_resolves_to_cwd_claude() {
        let d = resolve_claude_dir(true).unwrap();
        assert_eq!(d, std::env::current_dir().unwrap().join(".claude"));
    }
}
```

- [ ] **Step 2: 確認測試失敗（編譯錯誤：函式不存在）**

Run: `cargo test --lib commands::tests`
Expected: 編譯失敗，`cannot find function resolve_claude_dir`

- [ ] **Step 3: 在 `pub mod` 宣告之後加入實作**

```rust
/// Resolve the target `.claude` directory for lifecycle commands:
/// global (`~/.claude`) or the current project's (`<cwd>/.claude`).
pub fn resolve_claude_dir(project: bool) -> Result<std::path::PathBuf, String> {
    if project {
        std::env::current_dir()
            .map(|d| d.join(".claude"))
            .map_err(|e| format!("could not determine the current directory ({e})"))
    } else {
        dirs::home_dir()
            .map(|h| h.join(".claude"))
            .ok_or_else(|| "could not determine the home directory".to_string())
    }
}
```

- [ ] **Step 4: 確認測試通過**

Run: `cargo test --lib commands::tests`
Expected: 2 passed

- [ ] **Step 5: Commit**

```bash
git add src/commands/mod.rs
git commit -m "feat: add resolve_claude_dir helper for install scope

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `install --project`（Scope enum、跳過 config、CLI 佈線與提示）

**Files:**
- Modify: `src/commands/install.rs`
- Modify: `src/main.rs:20-21`（Install variant）、`src/main.rs:39`（match arm）
- Modify: 其他呼叫 `install_to` 的測試（`src/commands/uninstall.rs`、`src/commands/doctor.rs`、`src/commands/update.rs` 的 `#[cfg(test)]`）

**Interfaces:**
- Consumes: `resolve_claude_dir(project)`（Task 1）
- Produces: `pub enum Scope { Global, Project }`、`pub fn install_to(claude_dir: &Path, scope: Scope) -> io::Result<Vec<String>>`、`pub fn run(project: bool) -> i32`。Task 3–5 與 7 依賴這些簽名；所有既有測試呼叫改為 `install_to(dir, Scope::Global)`。

- [ ] **Step 1: 在 `src/commands/install.rs` 的 tests 模組加入失敗測試**

```rust
    #[test]
    fn project_install_skips_global_config() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Project).unwrap();
        // Assets and hooks are released as usual…
        assert!(tmp.path().join("agents/skeptic.md").is_file());
        assert!(tmp.path().join("harness/manifest.json").is_file());
        let settings = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(settings.contains("harness hook stop"));
        // …but no config layer is created (the project layer is harness.toml).
        assert!(!tmp.path().join("harness/config.toml").exists());
    }
```

- [ ] **Step 2: 確認失敗（`Scope` 不存在、`install_to` 參數數量錯誤）**

Run: `cargo test --lib commands::install`
Expected: 編譯失敗

- [ ] **Step 3: 修改 `src/commands/install.rs`**

在檔案頂部（`use` 之後）加入：

```rust
/// Where an install lives: the user-wide `~/.claude` or a single project's
/// `.claude/`. A project install never creates `harness/config.toml` — the
/// engine's project config layer is `harness.toml` (see `harness init`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
}
```

`run` 與 `install_to` 改為：

```rust
pub fn run(project: bool) -> i32 {
    let claude_dir = match crate::commands::resolve_claude_dir(project) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return 1;
        }
    };
    let scope = if project { Scope::Project } else { Scope::Global };
    match install_to(&claude_dir, scope) {
        Ok(actions) => {
            for a in &actions {
                println!("{a}");
            }
            if project {
                if dirs::home_dir().is_some_and(|h| h.join(".claude") == claude_dir) {
                    println!("note: --project in your home directory targets ~/.claude — this is effectively a global install (without config.toml)");
                }
                println!("note: no config file was created — to customize this project's gates, run `harness init`");
                println!("note: the installed files appear in git status; commit them to share with your team, or add them to .gitignore");
                println!("harness installed for this project. Run `harness doctor --project` anytime for a health check.");
            } else {
                println!("harness installed. Run `harness doctor` anytime for a health check.");
            }
            0
        }
        Err(e) => {
            eprintln!("error: install failed: {e}");
            1
        }
    }
}

pub fn install_to(claude_dir: &Path, scope: Scope) -> io::Result<Vec<String>> {
    let mut actions = release_assets(claude_dir)?;

    // Global config: written only when missing, never overwritten.
    // A project install skips it — the engine only reads the global
    // config.toml plus the nearest harness.toml, so a project-local
    // config.toml would be dead weight.
    if scope == Scope::Global {
        let config_path = claude_dir.join("harness/config.toml");
        if !config_path.exists() {
            if let Some(dir) = config_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&config_path, DEFAULT_CONFIG)?;
            actions.push(format!("created global config {}", config_path.display()));
        }
    }

    let mut hook_actions = settings::register_hooks(&claude_dir.join("settings.json"))?;
    actions.append(&mut hook_actions);
    Ok(actions)
}
```

- [ ] **Step 4: 更新所有既有呼叫點**

`src/main.rs`：

```rust
    /// Release assets into ~/.claude/ (or ./.claude with --project), register hooks
    Install {
        /// Install into the current project's .claude/ instead of ~/.claude/
        #[arg(long)]
        project: bool,
    },
```

match arm：`Command::Install { project } => commands::install::run(project),`

`install.rs` 自己的 4 個既有測試、`uninstall.rs` tests（8 處）、`doctor.rs` tests（5 處）、`update.rs` tests（1 處）中的 `install_to(tmp.path())` 全部改為 `install_to(tmp.path(), Scope::Global)`，並在各檔 tests 模組的 `use` 加上 `Scope`（如 `use crate::commands::install::{install_to, Scope};`）。

- [ ] **Step 5: 全套測試通過**

Run: `cargo test`
Expected: 全部通過（含新測試 `project_install_skips_global_config`）

- [ ] **Step 6: Commit**

```bash
git add src/commands/install.rs src/commands/uninstall.rs src/commands/doctor.rs src/commands/update.rs src/main.rs
git commit -m "feat: harness install --project releases into <cwd>/.claude

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: `uninstall --project`

**Files:**
- Modify: `src/commands/uninstall.rs:6-24`（`run`）
- Modify: `src/main.rs`（Uninstall variant + match arm）

**Interfaces:**
- Consumes: `resolve_claude_dir(project)`
- Produces: `pub fn run(project: bool) -> i32`。`uninstall_from` 簽名不變（已通用）。

- [ ] **Step 1: 修改 `run`（此 task 為 CLI 佈線，核心邏輯已有單元測試覆蓋；行為驗證在 Task 7 的 E2E）**

```rust
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
```

`src/main.rs`：

```rust
    /// Cleanly remove everything harness registered or installed
    Uninstall {
        /// Remove the current project's install instead of the global one
        #[arg(long)]
        project: bool,
    },
```

match arm：`Command::Uninstall { project } => commands::uninstall::run(project),`

- [ ] **Step 2: 編譯與既有測試通過**

Run: `cargo test`
Expected: 全部通過

- [ ] **Step 3: Commit**

```bash
git add src/commands/uninstall.rs src/main.rs
git commit -m "feat: harness uninstall --project

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: `update --project`

**Files:**
- Modify: `src/commands/update.rs:6-34`（`run`）
- Modify: `src/main.rs`（Update variant + match arm）

**Interfaces:**
- Consumes: `resolve_claude_dir(project)`
- Produces: `pub fn run(project: bool) -> i32`。`update_at` 簽名不變。

- [ ] **Step 1: 修改 `run`**

```rust
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
```

`src/main.rs`：

```rust
    /// Re-release assets after an upgrade (user-modified files are preserved)
    Update {
        /// Update the current project's install instead of the global one
        #[arg(long)]
        project: bool,
    },
```

match arm：`Command::Update { project } => commands::update::run(project),`

- [ ] **Step 2: 編譯與既有測試通過**

Run: `cargo test`
Expected: 全部通過

- [ ] **Step 3: Commit**

```bash
git add src/commands/update.rs src/main.rs
git commit -m "feat: harness update --project

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: `doctor --project` 與共存提示

**Files:**
- Modify: `src/commands/doctor.rs`
- Modify: `src/main.rs`（Doctor variant + match arm）

**Interfaces:**
- Consumes: `resolve_claude_dir(project)`、`Manifest::load`
- Produces: `pub fn coexistence_note(other_claude_dir: &Path, project_mode: bool) -> Option<String>`、`pub fn run(project: bool) -> i32`。`check` 簽名不變（settings.json 與 foreign Stop hook 檢查已相對於傳入的 `claude_dir`，`--project` 模式自然檢查專案的 settings.json）。

- [ ] **Step 1: 在 `src/commands/doctor.rs` tests 加入失敗測試**

```rust
    #[test]
    fn coexistence_note_absent_when_other_layer_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(coexistence_note(tmp.path(), true).is_none());
        assert!(coexistence_note(tmp.path(), false).is_none());
    }

    #[test]
    fn coexistence_note_from_global_view_points_at_project() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Project).unwrap();
        let note = coexistence_note(tmp.path(), false).unwrap();
        assert!(note.contains("doctor --project"), "{note}");
    }

    #[test]
    fn coexistence_note_from_project_view_mentions_dedup() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let note = coexistence_note(tmp.path(), true).unwrap();
        assert!(note.contains("de-duplicated"), "{note}");
    }
```

（tests 模組的 `use` 改為 `use crate::commands::install::{install_to, Scope};`）

- [ ] **Step 2: 確認失敗**

Run: `cargo test --lib commands::doctor`
Expected: 編譯失敗，`cannot find function coexistence_note`

- [ ] **Step 3: 實作**

在 `check` 之後加入：

```rust
/// A hint (ok-level, never a warning) when the "other" install layer is
/// also present. `other_claude_dir` is the layer NOT being checked:
/// the project's `.claude` when running globally, `~/.claude` when
/// running with --project.
pub fn coexistence_note(other_claude_dir: &Path, project_mode: bool) -> Option<String> {
    Manifest::load(other_claude_dir)?;
    Some(if project_mode {
        "a global install is also present; identical hook commands are de-duplicated by Claude Code, so each hook fires once".to_string()
    } else {
        "this project also has a project-scoped install — run `harness doctor --project` to check it".to_string()
    })
}
```

`run` 改為：

```rust
pub fn run(project: bool) -> i32 {
    let claude_dir = match crate::commands::resolve_claude_dir(project) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return 1;
        }
    };
    let r = check(&claude_dir);
    for ok in &r.oks {
        println!("ok: {ok}");
    }
    let other = if project {
        dirs::home_dir().map(|h| h.join(".claude"))
    } else {
        std::env::current_dir().ok().map(|d| d.join(".claude"))
    };
    if let Some(note) = other.and_then(|o| coexistence_note(&o, project)) {
        println!("note: {note}");
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
```

注意：`Manifest::load(other_claude_dir)?` 中 `?` 作用於 `Option`，manifest 不存在即回傳 `None`。全域模式下 `other` 是 `<cwd>/.claude`——若 cwd 恰好是家目錄，`other == claude_dir`，note 會指向同一份安裝；可接受（提示無害且此情境罕見）。

`src/main.rs`：

```rust
    /// Health check: hooks registered, versions consistent, conflicting harnesses
    Doctor {
        /// Check the current project's install instead of the global one
        #[arg(long)]
        project: bool,
    },
```

match arm：`Command::Doctor { project } => commands::doctor::run(project),`

- [ ] **Step 4: 全套測試通過**

Run: `cargo test`
Expected: 全部通過

- [ ] **Step 5: Commit**

```bash
git add src/commands/doctor.rs src/main.rs
git commit -m "feat: harness doctor --project with coexistence notes

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: protocol.md 三層讀取（專案 → 全域 → 內嵌）

**Files:**
- Modify: `src/hooks/session_start.rs`

**Interfaces:**
- Produces: `pub fn protocol_layered(project_claude_dir: Option<&Path>, global_claude_dir: Option<&Path>) -> String`（取代 `protocol_from`；repo 內無其他呼叫者）。

- [ ] **Step 1: 改寫 tests 模組為分層語意（先寫、先跑、先看它失敗）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn write_protocol(claude_dir: &Path, content: &[u8]) {
        let dir = claude_dir.join("harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("protocol.md"), content).unwrap();
    }

    #[test]
    fn project_layer_wins_over_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&proj, b"# project protocol");
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# project protocol"
        );
    }

    #[test]
    fn missing_project_layer_falls_back_to_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# global protocol"
        );
    }

    #[test]
    fn missing_both_layers_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(protocol_layered(Some(tmp.path()), None), PROTOCOL);
    }

    #[test]
    fn non_utf8_project_layer_falls_through_to_global() {
        let tmp = tempfile::tempdir().unwrap();
        let (proj, global) = (tmp.path().join("p"), tmp.path().join("g"));
        write_protocol(&proj, &[0xFF, 0xFE]);
        write_protocol(&global, b"# global protocol");
        assert_eq!(
            protocol_layered(Some(&proj), Some(&global)),
            "# global protocol"
        );
    }

    #[test]
    fn non_utf8_everywhere_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("g");
        write_protocol(&global, &[0xFF, 0xFE]);
        assert_eq!(protocol_layered(None, Some(&global)), PROTOCOL);
    }
}
```

- [ ] **Step 2: 確認失敗**

Run: `cargo test --lib hooks::session_start`
Expected: 編譯失敗，`cannot find function protocol_layered`

- [ ] **Step 3: 以 `protocol_layered` 取代 `protocol_from` 並更新 `run`**

```rust
/// The protocol text to inject, layered nearest-wins like the config:
/// the project's released copy (`<cwd>/.claude/harness/protocol.md`),
/// then the global copy, then the embedded version (fail-open). Install
/// promises "your customizations win" — the injected protocol honors
/// that at whichever layer the customization lives.
pub fn protocol_layered(
    project_claude_dir: Option<&Path>,
    global_claude_dir: Option<&Path>,
) -> String {
    project_claude_dir
        .and_then(read_protocol)
        .or_else(|| global_claude_dir.and_then(read_protocol))
        .unwrap_or_else(|| PROTOCOL.to_string())
}

fn read_protocol(claude_dir: &Path) -> Option<String> {
    std::fs::read_to_string(claude_dir.join("harness").join("protocol.md")).ok()
}
```

`run` 中原本的

```rust
    let protocol = dirs::home_dir()
        .map(|h| protocol_from(&h.join(".claude")))
        .unwrap_or_else(|| PROTOCOL.to_string());
```

改為（`payload_cwd` 已在 `run` 開頭為 config 取得 cwd，提到共用變數）：

```rust
    let cwd = payload_cwd(payload);
    let config = Config::load(&cwd);
    // …（state prune 不變）…
    let global = dirs::home_dir().map(|h| h.join(".claude"));
    let protocol = protocol_layered(Some(&cwd.join(".claude")), global.as_deref());
```

刪除 `protocol_from`（唯一外部呼叫者就是 `run`；舊測試已在 Step 1 改寫）。

- [ ] **Step 4: 全套測試通過（含 `tests/hooks.rs` 的 session-start E2E 不受影響）**

Run: `cargo test`
Expected: 全部通過

- [ ] **Step 5: Commit**

```bash
git add src/hooks/session_start.rs
git commit -m "feat: layered protocol injection (project > global > embedded)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: E2E — 專案生命週期、共存、hook 字串不變量

**Files:**
- Modify: `tests/e2e.rs`（沿用既有 `harness(home)` helper：`Command::cargo_bin("harness")` + `env("HOME", …)`；專案模式再加 `.current_dir(proj)`）

**Interfaces:**
- Consumes: Task 2–5 的 CLI 行為與訊息字串。

- [ ] **Step 1: 加入三個 E2E 測試**

```rust
#[test]
fn project_scoped_full_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();

    harness(&home).current_dir(&proj).args(["install", "--project"]).assert().success()
        .stdout(predicate::str::contains("harness installed for this project"))
        .stdout(predicate::str::contains("harness init"));
    // Released into the project, not the global home
    assert!(proj.join(".claude/harness/manifest.json").is_file());
    assert!(proj.join(".claude/agents/skeptic.md").is_file());
    assert!(!proj.join(".claude/harness/config.toml").exists());
    assert!(!home.join(".claude/harness/manifest.json").exists());

    harness(&home).current_dir(&proj).args(["doctor", "--project"]).assert().success()
        .stdout(predicate::str::contains("All checks passed"));

    harness(&home).current_dir(&proj).args(["update", "--project"]).assert().success()
        .stdout(predicate::str::contains("already up to date"));

    harness(&home).current_dir(&proj).args(["uninstall", "--project"]).assert().success()
        .stdout(predicate::str::contains("harness removed from this project"));
    assert!(!proj.join(".claude/harness/manifest.json").exists());
    assert!(!proj.join(".claude/agents/skeptic.md").exists());
    let settings = std::fs::read_to_string(proj.join(".claude/settings.json")).unwrap();
    assert!(!settings.contains("harness hook"));
}

#[test]
fn global_and_project_installs_coexist() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();

    harness(&home).current_dir(&proj).arg("install").assert().success();
    harness(&home).current_dir(&proj).args(["install", "--project"]).assert().success();

    // Hook command strings must be character-for-character identical in
    // both layers — Claude Code's native de-duplication depends on it.
    let read_cmds = |p: &std::path::Path| -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let mut cmds: Vec<String> = v["hooks"].as_object().unwrap().values()
            .flat_map(|groups| groups.as_array().unwrap().iter())
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .filter_map(|h| h["command"].as_str().map(String::from))
            .filter(|c| c.contains("harness hook"))
            .collect();
        cmds.sort();
        cmds
    };
    assert_eq!(
        read_cmds(&home.join(".claude/settings.json")),
        read_cmds(&proj.join(".claude/settings.json"))
    );

    // Doctor points out the coexistence from both viewpoints.
    harness(&home).current_dir(&proj).arg("doctor").assert().success()
        .stdout(predicate::str::contains("doctor --project"));
    harness(&home).current_dir(&proj).args(["doctor", "--project"]).assert().success()
        .stdout(predicate::str::contains("de-duplicated"));

    // Removing the project install leaves the global one untouched.
    harness(&home).current_dir(&proj).args(["uninstall", "--project"]).assert().success();
    assert!(home.join(".claude/harness/manifest.json").is_file());
    assert!(home.join(".claude/agents/skeptic.md").is_file());
}

#[test]
fn project_install_in_home_directory_prints_overlap_note() {
    let tmp = tempfile::tempdir().unwrap();
    // cwd == $HOME → <cwd>/.claude is the global location
    harness(tmp.path()).current_dir(tmp.path()).args(["install", "--project"]).assert().success()
        .stdout(predicate::str::contains("effectively a global install"));
}
```

需要 `tests/e2e.rs` 之 dev-dependency `serde_json`——`Cargo.toml` 的 `[dev-dependencies]` 若尚無 `serde_json` 則加入（主依賴已有，直接 `serde_json = "1"`）。

- [ ] **Step 2: 執行 E2E 確認通過**

Run: `cargo test --test e2e`
Expected: 全部通過（若失敗，回頭修 Task 2–5 的實作，不改測試斷言）

- [ ] **Step 3: Commit**

```bash
git add tests/e2e.rs Cargo.toml
git commit -m "test: e2e coverage for project-scoped lifecycle and coexistence

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: README 文件更新（英文＋繁中）

**Files:**
- Modify: `README.md`（Commands 表格、Quick start 之後）
- Modify: `README.zh-TW.md`（對應段落）

- [ ] **Step 1: `README.md` Commands 表格更新兩列並補一段**

表格中 install / uninstall / doctor / update 四列的 Purpose 各補 `--project` 說明，例如：

```markdown
| `harness install [--project]` | Release assets into ~/.claude/ and register hooks; `--project` targets the current project's .claude/ instead |
| `harness uninstall [--project]` | Clean removal (only harness-owned items; your customizations and global config are kept) |
| `harness doctor [--project]` | Health check: hook registration, version consistency, gate conflicts, install coexistence |
| `harness update [--project]` | Re-release assets after an upgrade (your modified files are kept; official copies land in .new files) |
```

表格之後加一節：

```markdown
## Project-scoped install

`harness install --project` (run from the project root) installs into that
project's `.claude/` instead of `~/.claude/` — hooks, agents, and skills
then apply to that project only. It never creates a config file: the
project config layer is `harness.toml` (run `harness init`). A global and
a project install can coexist; hook commands are identical strings, so
Claude Code de-duplicates them and each hook fires once. `doctor`,
`update`, and `uninstall` accept the same flag to manage the project
install. The released files show up in git status — commit them to share
the setup with your team, or add them to `.gitignore`.

The injected protocol resolves nearest-wins, like the config layers:
`<project>/.claude/harness/protocol.md` → `~/.claude/harness/protocol.md`
→ the embedded copy.
```

- [ ] **Step 2: `README.zh-TW.md` 加入對應繁中內容**

指令表格四列同步補 `[--project]` 與說明，並在對應位置加入：

```markdown
## 專案範圍安裝

在專案根目錄執行 `harness install --project`，會把 harness 裝進該專案的
`.claude/` 而非 `~/.claude/`——hooks、agents、skills 只對這個專案生效。
專案安裝不會建立任何設定檔：專案層的設定就是 `harness.toml`（執行
`harness init` 生成）。全域與專案安裝可以共存；兩邊註冊的 hook 指令
字串完全相同，Claude Code 會自動去重，每個 hook 只觸發一次。`doctor`、
`update`、`uninstall` 也接受同樣的 flag 來管理專案安裝。釋出的檔案會
出現在 git status——commit 進 repo 可與團隊共享這套設定，不想共享就
加進 `.gitignore`。

注入的行為協議與設定分層一樣採「最近的贏」：
`<專案>/.claude/harness/protocol.md` → `~/.claude/harness/protocol.md`
→ 內嵌版。
```

- [ ] **Step 3: 確認文件與實際輸出一致**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run -q -- --help`
Expected: help 顯示四個指令的 `--project`；README 描述與之相符

- [ ] **Step 4: Commit**

```bash
git add README.md README.zh-TW.md
git commit -m "docs: document project-scoped install (--project)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
