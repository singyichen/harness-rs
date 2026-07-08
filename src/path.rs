use std::path::{Path, PathBuf};

/// Well-known system bin directories that are virtually always on PATH,
/// even in minimal shell environments (e.g. Claude Code hook subprocesses
/// that don't source `~/.cargo/env`).
const SYSTEM_BIN_CANDIDATES: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// Returns true if the running binary lives under `~/.cargo/bin/`.
pub fn exe_is_in_cargo_bin() -> bool {
    let exe = match std::env::current_exe().and_then(|p| std::fs::canonicalize(p)) {
        Ok(p) => p,
        Err(_) => return false,
    };
    match cargo_bin_dir() {
        Some(cb) => {
            let cb = std::fs::canonicalize(&cb).unwrap_or(cb);
            exe.starts_with(&cb)
        }
        None => false,
    }
}

/// Look for a `harness` binary in well-known system bin directories.
pub fn find_in_system_bin() -> Option<PathBuf> {
    find_harness_in(SYSTEM_BIN_CANDIDATES)
}

/// Try to create a symlink to the current exe in the first writable
/// system bin directory. Returns the created symlink path, or None.
#[cfg(unix)]
pub fn try_create_symlink() -> Option<PathBuf> {
    try_create_symlink_in(SYSTEM_BIN_CANDIDATES)
}

/// Remove a harness symlink from system bin directories. Only removes
/// entries that are symlinks pointing into `~/.cargo/bin/` (i.e., ones
/// that `install` would have created).
#[cfg(unix)]
pub fn remove_system_symlink() -> Option<PathBuf> {
    remove_symlink_in(SYSTEM_BIN_CANDIDATES)
}

/// A manual fix command for when automatic symlinking fails.
pub fn symlink_fix_hint() -> String {
    let exe = std::env::current_exe()
        .and_then(|p| std::fs::canonicalize(p))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "$HOME/.cargo/bin/harness".to_string());
    format!("sudo ln -s {exe} /usr/local/bin/harness")
}

fn cargo_bin_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cargo/bin"))
}

fn find_harness_in(candidates: &[&str]) -> Option<PathBuf> {
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn try_create_symlink_in(candidates: &[&str]) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resolved = std::fs::canonicalize(&exe).unwrap_or(exe);
    for dir in candidates {
        let dir = Path::new(dir);
        if !dir.is_dir() {
            continue;
        }
        let link = dir.join("harness");
        if link.exists() || link.symlink_metadata().is_ok() {
            continue;
        }
        if std::os::unix::fs::symlink(&resolved, &link).is_ok() {
            return Some(link);
        }
    }
    None
}

#[cfg(unix)]
fn remove_symlink_in(candidates: &[&str]) -> Option<PathBuf> {
    let cargo_bin = cargo_bin_dir()?;
    let cargo_resolved = std::fs::canonicalize(&cargo_bin).unwrap_or(cargo_bin);
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        if !candidate.is_symlink() {
            continue;
        }
        if let Ok(target) = std::fs::read_link(&candidate) {
            let resolved = std::fs::canonicalize(&target).unwrap_or(target);
            if resolved.starts_with(&cargo_resolved)
                && std::fs::remove_file(&candidate).is_ok()
            {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_not_in_cargo_bin_during_tests() {
        assert!(!exe_is_in_cargo_bin());
    }

    #[test]
    fn find_harness_in_returns_existing_binary() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("harness"), "fake").unwrap();
        let s = tmp.path().to_str().unwrap();
        assert!(find_harness_in(&[s]).is_some());
    }

    #[test]
    fn find_harness_in_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let s = tmp.path().to_str().unwrap();
        assert!(find_harness_in(&[s]).is_none());
    }

    #[test]
    fn find_harness_in_skips_nonexistent_dirs() {
        assert!(find_harness_in(&["/no/such/dir/ever"]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_creates_link_in_writable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let bin_str = bin.to_str().unwrap();
        let link = try_create_symlink_in(&[bin_str]).unwrap();
        assert_eq!(link, bin.join("harness"));
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_skips_dir_with_existing_harness() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("harness"), "existing").unwrap();
        let bin_str = bin.to_str().unwrap();
        assert!(try_create_symlink_in(&[bin_str]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_skips_nonexistent_dir() {
        assert!(try_create_symlink_in(&["/no/such/dir/ever"]).is_none());
    }

    #[test]
    fn hint_contains_ln_and_target() {
        let hint = symlink_fix_hint();
        assert!(hint.contains("ln -s"), "{hint}");
        assert!(hint.contains("/usr/local/bin/harness"), "{hint}");
    }
}
