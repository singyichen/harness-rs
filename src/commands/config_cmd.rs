use crate::config::Config;

pub fn run() -> i32 {
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("error: could not determine the current directory");
        return 1;
    };
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
