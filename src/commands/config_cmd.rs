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
