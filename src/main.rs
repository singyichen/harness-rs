mod commands;
mod config;
mod hooks;
mod settings;
mod state;
mod assets;
mod manifest;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "harness", version, about = "Engineering-discipline engine for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Release assets into ~/.claude/ (or ./.claude with --project), register hooks
    Install {
        /// Install into the current project's .claude/ instead of ~/.claude/
        #[arg(long)]
        project: bool,
    },
    /// Cleanly remove everything harness registered or installed
    Uninstall {
        /// Remove the current project's install instead of the global one
        #[arg(long)]
        project: bool,
    },
    /// Generate a harness.toml customization layer in the current project
    Init,
    /// Health check: hooks registered, versions consistent, conflicting harnesses
    Doctor,
    /// Re-release assets after an upgrade (user-modified files are preserved)
    Update {
        /// Update the current project's install instead of the global one
        #[arg(long)]
        project: bool,
    },
    /// Print the merged effective config (built-in + global + project)
    Config,
    /// Hook engine entry point, invoked by Claude Code
    Hook { event: String },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Install { project } => commands::install::run(project),
        Command::Uninstall { project } => commands::uninstall::run(project),
        Command::Init => commands::init::run(),
        Command::Doctor => commands::doctor::run(),
        Command::Update { project } => commands::update::run(project),
        Command::Config => commands::config_cmd::run(),
        Command::Hook { event } => hooks::run(&event),
    };
    std::process::exit(code);
}
