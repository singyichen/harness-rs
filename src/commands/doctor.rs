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
                        if recorded
                            .map(|h| *h == sha256_hex(current.as_bytes()))
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
