mod api;
mod auth;
mod codex;
mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "claudex", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show Claude plan usage limits
    Usage {
        /// Show both Claude Code and Codex usage limits
        #[arg(long)]
        all: bool,
    },
    /// Codex CLI commands
    Codex {
        #[command(subcommand)]
        command: CodexCommands,
    },
}

#[derive(Subcommand)]
enum CodexCommands {
    /// Show Codex plan usage limits
    Usage,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Usage { all } => {
            if all {
                commands::usage_all::run().await
            } else {
                commands::usage::run().await
            }
        }
        Commands::Codex { command } => match command {
            CodexCommands::Usage => commands::codex_usage::run().await,
        },
    }
}
