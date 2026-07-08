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

pub fn run(project: bool) -> i32 {
    let claude_dir = match crate::commands::resolve_claude_dir(project) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return 1;
        }
    };
    let r = check(&claude_dir, project);
    for ok in &r.oks {
        println!("ok: {ok}");
    }
    let other = if project {
        dirs::home_dir().map(|h| h.join(".claude"))
    } else {
        std::env::current_dir().ok().map(|d| d.join(".claude"))
    };
    if let Some(note) = other.and_then(|o| coexistence_note(&claude_dir, &o, project)) {
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

pub fn check(claude_dir: &Path, project: bool) -> Report {
    let mut r = Report::default();
    // Repair hints must target the same scope being diagnosed: in project
    // mode, `harness install`/`update` (global) would fix `~/.claude` and
    // leave `<cwd>/.claude` broken, so the check would keep failing.
    let scope_flag = if project { " --project" } else { "" };

    // 1. Manifest and version
    match Manifest::load(claude_dir) {
        None => r
            .problems
            .push(format!("manifest not found — run harness install{scope_flag}")),
        Some(m) => {
            let bin_version = env!("CARGO_PKG_VERSION");
            if m.version == bin_version {
                r.oks.push(format!("versions match ({bin_version})"));
            } else {
                r.problems.push(format!(
                    "binary version {bin_version} differs from installed asset version {} — run harness update{scope_flag}",
                    m.version
                ));
            }
            // 3. Asset presence and customization status
            for asset in ASSETS {
                let target = claude_dir.join(asset.rel_path);
                match std::fs::read(&target) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        r.problems.push(format!(
                            "missing {} — run harness install{scope_flag}",
                            asset.rel_path
                        ))
                    }
                    Err(e) => r.problems.push(format!(
                        "could not read {} ({e})",
                        asset.rel_path
                    )),
                    Ok(current) => {
                        let recorded = m.files.get(asset.rel_path);
                        if recorded
                            .map(|h| *h == sha256_hex(&current))
                            .unwrap_or(false)
                        {
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
                .push(format!("{event} hook not registered — run harness install{scope_flag}"));
        }
    }

    // 4. Conflicting gates
    for cmd in settings::foreign_stop_hooks(&settings_path) {
        r.warnings.push(format!(
            "another Stop hook detected (`{cmd}`); it may double-block alongside the harness verify gate"
        ));
    }

    // 5. PATH reachability: cargo installs into ~/.cargo/bin which is
    //    absent from minimal shell environments (hook subprocesses). Unix-only
    //    — on Windows `cargo install` lands on a directory already on PATH, so
    //    the symlink mechanism and its `sudo ln -s` fix hint do not apply.
    #[cfg(unix)]
    if crate::path::exe_is_in_cargo_bin() {
        match crate::path::find_in_system_bin() {
            Some(p) => r.oks.push(format!(
                "harness reachable from system PATH ({})",
                p.display()
            )),
            None => r.problems.push(format!(
                "harness is only reachable via ~/.cargo/bin — Claude Code hooks may not find it; \
                 fix: {}",
                crate::path::symlink_fix_hint()
            )),
        }
    }
    r
}

/// A hint (ok-level, never a warning) when the "other" install layer is
/// also present. `claude_dir` is the layer under check; `other_claude_dir`
/// is the layer NOT being checked: the project's `.claude` when running
/// globally, `~/.claude` when running with --project. Returns `None` when the
/// two resolve to the same directory (e.g. `doctor` run from `$HOME`), which
/// is not a genuine coexistence.
pub fn coexistence_note(
    claude_dir: &Path,
    other_claude_dir: &Path,
    project_mode: bool,
) -> Option<String> {
    // When run from the home directory the two layers resolve to the same
    // `~/.claude`; that is not a genuine coexistence, so stay silent.
    if crate::commands::is_same_dir(claude_dir, other_claude_dir) {
        return None;
    }
    Manifest::load(other_claude_dir)?;
    Some(if project_mode {
        "a global install is also present; identical hook commands are de-duplicated by Claude Code, so each hook fires once".to_string()
    } else {
        "this project also has a project-scoped install — run `harness doctor --project` to check it".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::{install_to, Scope};

    #[test]
    fn coexistence_note_absent_when_other_layer_not_installed() {
        let claude = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        assert!(coexistence_note(claude.path(), other.path(), true).is_none());
        assert!(coexistence_note(claude.path(), other.path(), false).is_none());
    }

    #[test]
    fn coexistence_note_from_global_view_points_at_project() {
        let other = tempfile::tempdir().unwrap();
        install_to(other.path(), Scope::Project).unwrap();
        let claude = tempfile::tempdir().unwrap();
        let note = coexistence_note(claude.path(), other.path(), false).unwrap();
        assert!(note.contains("doctor --project"), "{note}");
    }

    #[test]
    fn coexistence_note_from_project_view_mentions_dedup() {
        let other = tempfile::tempdir().unwrap();
        install_to(other.path(), Scope::Global).unwrap();
        let claude = tempfile::tempdir().unwrap();
        let note = coexistence_note(claude.path(), other.path(), true).unwrap();
        assert!(note.contains("de-duplicated"), "{note}");
    }

    /// Regression: running `doctor` from the home directory makes `claude_dir`
    /// and `other` resolve to the same `~/.claude`; a manifest is present, but
    /// there is no *separate* other-scope install, so no coexistence note must
    /// be emitted (false positive otherwise).
    #[test]
    fn coexistence_note_absent_when_other_is_same_dir() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        assert!(coexistence_note(tmp.path(), tmp.path(), false).is_none());
        assert!(coexistence_note(tmp.path(), tmp.path(), true).is_none());
    }

    #[test]
    fn healthy_install_has_no_problems() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        let r = check(tmp.path(), false);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn missing_install_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let r = check(tmp.path(), false);
        assert!(!r.problems.is_empty());
        assert!(r.problems.iter().any(|p| p.contains("harness install")));
    }

    /// Regression: project-mode repair hints must point at `--project`, or the
    /// user's fix would target `~/.claude` and leave `<cwd>/.claude` broken.
    #[test]
    fn project_mode_repair_hints_use_project_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let r = check(tmp.path(), true);
        assert!(
            r.problems.iter().any(|p| p.contains("harness install --project")),
            "{:?}",
            r.problems
        );
        // A global-scope check must NOT carry the flag.
        let g = check(tmp.path(), false);
        assert!(
            g.problems.iter().all(|p| !p.contains("--project")),
            "{:?}",
            g.problems
        );
    }

    #[test]
    fn user_modified_asset_is_warning_not_problem() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        std::fs::write(tmp.path().join("agents/skeptic.md"), "modified").unwrap();
        let r = check(tmp.path(), false);
        assert!(r.problems.is_empty());
        assert!(r.warnings.iter().any(|w| w.contains("skeptic.md")));
    }

    #[test]
    fn non_utf8_asset_is_warning_not_missing() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        std::fs::write(tmp.path().join("agents/skeptic.md"), [0xFF, 0xFE, 0x00]).unwrap();
        let r = check(tmp.path(), false);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert!(r.warnings.iter().any(|w| w.contains("skeptic.md")));
    }

    #[test]
    fn foreign_stop_hook_is_warning() {
        let tmp = tempfile::tempdir().unwrap();
        install_to(tmp.path(), Scope::Global).unwrap();
        // Manually inject a third-party Stop hook
        let p = tmp.path().join("settings.json");
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        v["hooks"]["Stop"].as_array_mut().unwrap().push(serde_json::json!(
            {"hooks":[{"type":"command","command":"python verify_gate.py"}]}
        ));
        std::fs::write(&p, v.to_string()).unwrap();
        let r = check(tmp.path(), false);
        assert!(r.warnings.iter().any(|w| w.contains("verify_gate.py")));
    }
}
