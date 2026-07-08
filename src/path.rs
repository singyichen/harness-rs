use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Well-known system bin directories that are virtually always on PATH,
/// even in minimal shell environments (e.g. Claude Code hook subprocesses
/// that don't source `~/.cargo/env`). Ordered by typical PATH priority.
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

/// What a bare `harness` resolves to on the system PATH — the minimal PATH a
/// Claude Code hook subprocess runs with.
#[cfg(unix)]
pub enum SystemPath {
    /// The winning entry on PATH is this binary.
    Reachable(PathBuf),
    /// The first resolvable entry on PATH is a *different* binary; a bare
    /// `harness` would run it instead of us.
    Shadowed(PathBuf),
    /// No `harness` on the system PATH.
    Missing,
}

/// Probe the system bin directories for a reachable `harness`. Candidates are
/// checked in PATH-priority order and the first *resolvable* entry wins: a
/// stale/different binary in an earlier directory shadows a valid one later,
/// which is what a bare `harness` would actually execute.
#[cfg(unix)]
pub fn system_path_status() -> SystemPath {
    let exe = match std::env::current_exe().and_then(|p| std::fs::canonicalize(p)) {
        Ok(p) => p,
        Err(_) => return SystemPath::Missing,
    };
    probe_system_bin(SYSTEM_BIN_CANDIDATES, &exe)
}

/// Try to create a symlink to the current exe in the first writable
/// system bin directory. Returns the created symlink path, or None.
#[cfg(unix)]
pub fn try_create_symlink() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resolved = std::fs::canonicalize(&exe).unwrap_or(exe);
    try_create_symlink_in(SYSTEM_BIN_CANDIDATES, &resolved, cargo_bin_dir().as_deref())
}

/// Find a harness symlink in the system bin dirs that `install` created (a
/// symlink pointing into the cargo bin). Does *not* remove it — the cargo
/// binary it points to outlives `harness uninstall`, and the link may be
/// shared with other installs, so uninstall only reports it.
#[cfg(unix)]
pub fn find_owned_system_symlink() -> Option<PathBuf> {
    find_owned_symlink_in(SYSTEM_BIN_CANDIDATES, cargo_bin_dir().as_deref())
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

/// The first resolvable `harness` on PATH decides reachability. A missing or
/// dangling entry does not shadow (a shell skips it and searches on); a
/// resolvable entry that is not our binary does.
#[cfg(unix)]
fn probe_system_bin(candidates: &[&str], target: &Path) -> SystemPath {
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        match std::fs::canonicalize(&candidate) {
            Ok(resolved) if resolved == *target => return SystemPath::Reachable(candidate),
            Ok(_) => return SystemPath::Shadowed(candidate),
            Err(_) => continue,
        }
    }
    SystemPath::Missing
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
fn find_owned_symlink_in(candidates: &[&str], cargo_bin: Option<&Path>) -> Option<PathBuf> {
    let cargo_bin = cargo_bin?;
    for dir in candidates {
        let candidate = Path::new(dir).join("harness");
        if candidate.is_symlink() && symlink_points_into(&candidate, cargo_bin) {
            return Some(candidate);
        }
    }
    None
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
    fn is_reachable(s: &SystemPath) -> bool {
        matches!(s, SystemPath::Reachable(_))
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_reachable_when_entry_is_this_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("real-harness");
        std::fs::write(&exe, "bin").unwrap();
        let target = std::fs::canonicalize(&exe).unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&exe, bin.join("harness")).unwrap();
        let s = bin.to_str().unwrap();
        match probe_system_bin(&[s], &target) {
            SystemPath::Reachable(p) => assert_eq!(p, bin.join("harness")),
            _ => panic!("expected Reachable"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_shadowed_for_a_different_binary() {
        // A stale/foreign `harness` must not count as reachable.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("harness"), "someone-elses-harness").unwrap();
        let s = tmp.path().to_str().unwrap();
        let other_target = tmp.path().join("not-this-binary");
        match probe_system_bin(&[s], &other_target) {
            SystemPath::Shadowed(_) => {}
            _ => panic!("expected Shadowed"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_missing_when_absent() {
        assert!(matches!(
            probe_system_bin(&["/no/such/dir/ever"], Path::new("/whatever")),
            SystemPath::Missing
        ));
    }

    #[cfg(unix)]
    #[test]
    fn probe_earlier_different_binary_shadows_a_valid_later_one() {
        // Regression: a stale binary in an earlier (higher-priority) PATH dir
        // must shadow our valid binary in a later dir — otherwise `doctor`
        // reports OK while a bare `harness` runs the stale one.
        let tmp = tempfile::tempdir().unwrap();
        let ours = tmp.path().join("real-harness");
        std::fs::write(&ours, "us").unwrap();
        let target = std::fs::canonicalize(&ours).unwrap();

        let early = tmp.path().join("early");
        std::fs::create_dir(&early).unwrap();
        std::fs::write(early.join("harness"), "stale-other").unwrap();

        let late = tmp.path().join("late");
        std::fs::create_dir(&late).unwrap();
        std::os::unix::fs::symlink(&ours, late.join("harness")).unwrap();

        let cands = [early.to_str().unwrap(), late.to_str().unwrap()];
        match probe_system_bin(&cands, &target) {
            SystemPath::Shadowed(p) => assert_eq!(p, early.join("harness")),
            _ => panic!("expected Shadowed by the earlier entry"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_skips_dangling_earlier_entry_and_reaches_later() {
        // A dangling (broken) symlink earlier on PATH does not shadow — a
        // shell skips it — so a valid later entry is still Reachable.
        let tmp = tempfile::tempdir().unwrap();
        let ours = tmp.path().join("real-harness");
        std::fs::write(&ours, "us").unwrap();
        let target = std::fs::canonicalize(&ours).unwrap();

        let early = tmp.path().join("early");
        std::fs::create_dir(&early).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("does-not-exist"), early.join("harness")).unwrap();

        let late = tmp.path().join("late");
        std::fs::create_dir(&late).unwrap();
        std::os::unix::fs::symlink(&ours, late.join("harness")).unwrap();

        let cands = [early.to_str().unwrap(), late.to_str().unwrap()];
        assert!(is_reachable(&probe_system_bin(&cands, &target)));
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
    fn find_owned_returns_symlink_into_cargo_bin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let real = cargo_bin.join("harness");
        std::fs::write(&real, "exe").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&real, bin.join("harness")).unwrap();
        assert_eq!(
            find_owned_symlink_in(&[bin.to_str().unwrap()], Some(&cargo_bin)),
            Some(bin.join("harness"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn find_owned_ignores_foreign_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cargo_bin = root.join("cargo-bin");
        std::fs::create_dir(&cargo_bin).unwrap();
        let foreign = root.join("foreign");
        std::fs::write(&foreign, "x").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(&foreign, bin.join("harness")).unwrap();
        assert!(find_owned_symlink_in(&[bin.to_str().unwrap()], Some(&cargo_bin)).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn hint_contains_ln_and_target() {
        let hint = symlink_fix_hint();
        assert!(hint.contains("ln -s"), "{hint}");
        assert!(hint.contains("/usr/local/bin/harness"), "{hint}");
    }
}
