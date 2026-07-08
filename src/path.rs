use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Well-known system bin directories that are virtually always on PATH,
/// even in minimal shell environments (e.g. Claude Code hook subprocesses
/// that don't source `~/.cargo/env`).
const SYSTEM_BIN_CANDIDATES: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// Returns true if the running binary lives under the cargo bin directory.
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

/// Look for a `harness` on the system PATH that resolves to *this* binary.
/// A missing, non-executable, dangling, or *different* `harness` at those
/// paths does not count as reachable — a hook that ran it would invoke the
/// wrong binary, so `install` should still create the symlink and `doctor`
/// should still report the problem.
pub fn find_in_system_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe()
        .and_then(|p| std::fs::canonicalize(p))
        .ok()?;
    find_matching_in(SYSTEM_BIN_CANDIDATES, &exe)
}

/// Try to create a symlink to the current exe in the first writable
/// system bin directory. Returns the created symlink path, or None.
#[cfg(unix)]
pub fn try_create_symlink() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resolved = std::fs::canonicalize(&exe).unwrap_or(exe);
    try_create_symlink_in(SYSTEM_BIN_CANDIDATES, &resolved, cargo_bin_dir().as_deref())
}

/// Remove a harness symlink from system bin directories. Only removes
/// entries that are symlinks pointing into the cargo bin directory (i.e.,
/// ones that `install` would have created).
#[cfg(unix)]
pub fn remove_system_symlink() -> Option<PathBuf> {
    remove_symlink_in(SYSTEM_BIN_CANDIDATES, cargo_bin_dir().as_deref())
}

/// A manual fix command for when automatic symlinking fails. Unix-only:
/// the symlink mechanism (and the whole PATH-reachability check) does not
/// apply on Windows, where `cargo install` targets a directory already on
/// PATH via PATHEXT.
#[cfg(unix)]
pub fn symlink_fix_hint() -> String {
    let exe = std::env::current_exe()
        .and_then(|p| std::fs::canonicalize(p))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "$HOME/.cargo/bin/harness".to_string());
    format!("sudo ln -s {exe} /usr/local/bin/harness")
}

/// The cargo bin directory: `$CARGO_HOME/bin` when `CARGO_HOME` is set,
/// otherwise `~/.cargo/bin`. Honors a custom Cargo home so installs done
/// with e.g. `CARGO_HOME=… cargo install` are still detected.
fn cargo_bin_dir() -> Option<PathBuf> {
    resolve_cargo_bin(std::env::var_os("CARGO_HOME"), dirs::home_dir())
}

fn resolve_cargo_bin(cargo_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    match cargo_home {
        Some(h) if !h.is_empty() => Some(PathBuf::from(h).join("bin")),
        _ => home.map(|h| h.join(".cargo/bin")),
    }
}

/// Find a `harness` entry in `candidates` that canonicalizes to `target`.
/// Canonicalization fails for missing paths and dangling symlinks, and a
/// stale/foreign binary resolves to a different path — both correctly yield
/// no match.
fn find_matching_in(candidates: &[&str], target: &Path) -> Option<PathBuf> {
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        if std::fs::canonicalize(&candidate).is_ok_and(|resolved| resolved == target) {
            return Some(candidate);
        }
    }
    None
}

/// True when `link` is a symlink whose target resolves into `cargo_bin` — a
/// harness symlink that `install` created (possibly now dangling).
#[cfg(unix)]
fn symlink_points_into(link: &Path, cargo_bin: &Path) -> bool {
    let cargo = std::fs::canonicalize(cargo_bin).unwrap_or_else(|_| cargo_bin.to_path_buf());
    match std::fs::read_link(link) {
        Ok(target) => std::fs::canonicalize(&target).unwrap_or(target).starts_with(&cargo),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn try_create_symlink_in(
    candidates: &[&str],
    link_target: &Path,
    cargo_bin: Option<&Path>,
) -> Option<PathBuf> {
    for dir in candidates {
        let dir = Path::new(dir);
        if !dir.is_dir() {
            continue;
        }
        let link = dir.join("harness");
        if link.symlink_metadata().is_ok() {
            // Something already occupies the path. Only replace a symlink we
            // own (points into cargo bin) — including a dangling one, so a
            // reinstall can self-heal. Never clobber a real file or a foreign
            // symlink.
            let ours = cargo_bin.is_some_and(|cb| symlink_points_into(&link, cb));
            if !ours || std::fs::remove_file(&link).is_err() {
                continue;
            }
        }
        if std::os::unix::fs::symlink(link_target, &link).is_ok() {
            return Some(link);
        }
    }
    None
}

#[cfg(unix)]
fn remove_symlink_in(candidates: &[&str], cargo_bin: Option<&Path>) -> Option<PathBuf> {
    let cargo_bin = cargo_bin?;
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        if candidate.is_symlink()
            && symlink_points_into(&candidate, cargo_bin)
            && std::fs::remove_file(&candidate).is_ok()
        {
            return Some(candidate);
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
    fn resolve_cargo_bin_honors_cargo_home() {
        let got = resolve_cargo_bin(Some("/custom/cargo".into()), Some(PathBuf::from("/home/u")));
        assert_eq!(got, Some(PathBuf::from("/custom/cargo/bin")));
    }

    #[test]
    fn resolve_cargo_bin_falls_back_to_home_when_unset() {
        let got = resolve_cargo_bin(None, Some(PathBuf::from("/home/u")));
        assert_eq!(got, Some(PathBuf::from("/home/u/.cargo/bin")));
    }

    #[test]
    fn resolve_cargo_bin_ignores_empty_cargo_home() {
        let got = resolve_cargo_bin(Some(OsString::new()), Some(PathBuf::from("/home/u")));
        assert_eq!(got, Some(PathBuf::from("/home/u/.cargo/bin")));
    }

    #[cfg(unix)]
    #[test]
    fn find_matching_in_returns_entry_that_is_this_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("real-harness");
        std::fs::write(&exe, "bin").unwrap();
        let target = std::fs::canonicalize(&exe).unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&exe, bin.join("harness")).unwrap();
        let s = bin.to_str().unwrap();
        assert_eq!(find_matching_in(&[s], &target), Some(bin.join("harness")));
    }

    #[test]
    fn find_matching_in_skips_a_different_binary() {
        // A stale or non-executable placeholder that is not our binary must
        // not count as reachable.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("harness"), "someone-elses-harness").unwrap();
        let s = tmp.path().to_str().unwrap();
        let other_target = tmp.path().join("not-this-binary");
        assert!(find_matching_in(&[s], &other_target).is_none());
    }

    #[test]
    fn find_matching_in_skips_missing_paths() {
        assert!(find_matching_in(&["/no/such/dir/ever"], Path::new("/whatever")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_creates_link_in_writable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let target = tmp.path().join("harness-exe");
        std::fs::write(&target, "exe").unwrap();
        let link = try_create_symlink_in(&[bin.to_str().unwrap()], &target, None).unwrap();
        assert_eq!(link, bin.join("harness"));
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_skips_dir_with_existing_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("harness"), "existing").unwrap();
        let target = tmp.path().join("exe");
        std::fs::write(&target, "x").unwrap();
        assert!(try_create_symlink_in(&[bin.to_str().unwrap()], &target, None).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_skips_nonexistent_dir() {
        let target = PathBuf::from("/tmp/whatever");
        assert!(try_create_symlink_in(&["/no/such/dir/ever"], &target, None).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_repairs_dangling_symlink_it_owns() {
        // A dangling harness symlink into cargo bin must be replaced, not
        // skipped — otherwise `install` can never self-heal a broken link.
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        // Dangling: target under cargo bin that does not exist.
        std::os::unix::fs::symlink(cargo_bin.join("harness"), bin.join("harness")).unwrap();
        let new_exe = root.join("new-harness");
        std::fs::write(&new_exe, "exe").unwrap();
        let link =
            try_create_symlink_in(&[bin.to_str().unwrap()], &new_exe, Some(&cargo_bin)).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), new_exe);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_does_not_clobber_foreign_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let foreign = root.join("foreign");
        std::fs::write(&foreign, "x").unwrap();
        std::os::unix::fs::symlink(&foreign, bin.join("harness")).unwrap();
        let new_exe = root.join("new-harness");
        std::fs::write(&new_exe, "exe").unwrap();
        assert!(
            try_create_symlink_in(&[bin.to_str().unwrap()], &new_exe, Some(&cargo_bin)).is_none()
        );
        assert_eq!(std::fs::read_link(bin.join("harness")).unwrap(), foreign);
    }

    #[cfg(unix)]
    #[test]
    fn remove_symlink_removes_only_links_into_cargo_bin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let real = cargo_bin.join("harness");
        std::fs::write(&real, "exe").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&real, bin.join("harness")).unwrap();
        let removed = remove_symlink_in(&[bin.to_str().unwrap()], Some(&cargo_bin)).unwrap();
        assert_eq!(removed, bin.join("harness"));
        assert!(!bin.join("harness").symlink_metadata().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn remove_symlink_keeps_foreign_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let foreign = root.join("foreign");
        std::fs::write(&foreign, "x").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&foreign, bin.join("harness")).unwrap();
        assert!(remove_symlink_in(&[bin.to_str().unwrap()], Some(&cargo_bin)).is_none());
        assert!(bin.join("harness").symlink_metadata().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn hint_contains_ln_and_target() {
        let hint = symlink_fix_hint();
        assert!(hint.contains("ln -s"), "{hint}");
        assert!(hint.contains("/usr/local/bin/harness"), "{hint}");
    }
}
