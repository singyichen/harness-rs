pub mod config_cmd;
pub mod doctor;
pub mod init;
pub mod install;
pub mod uninstall;
pub mod update;

/// Compare two `.claude` directories for equality, resolving symlinks first.
/// Plain `PathBuf` equality is not enough: on macOS `$TMPDIR` (and thus a temp
/// `$HOME` in tests) lives under `/var/...`, a symlink to `/private/var/...`;
/// `std::env::current_dir()` returns the resolved path while a raw `$HOME` join
/// does not, so the two never compare equal even when they name the same
/// directory. Fall back to the raw comparison if canonicalization fails (e.g.
/// a directory that does not exist yet — in which case two distinct paths are
/// correctly reported as different).
pub fn is_same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    let resolve =
        |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    resolve(a) == resolve(b)
}

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

    #[test]
    fn is_same_dir_true_for_identical_path() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(is_same_dir(tmp.path(), tmp.path()));
    }

    #[test]
    fn is_same_dir_false_for_distinct_dirs() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert!(!is_same_dir(a.path(), b.path()));
    }
}
