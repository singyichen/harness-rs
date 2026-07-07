pub mod config_cmd;
pub mod doctor;
pub mod init;
pub mod install;
pub mod uninstall;
pub mod update;

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
}
