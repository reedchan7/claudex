mod agy;
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
        /// Show Claude Code, Codex, and Gemini / Antigravity usage limits
        #[arg(long)]
        all: bool,
        /// Show the timezone name next to reset times
        #[arg(long)]
        show_timezone: bool,
    },
    /// Codex CLI commands
    Codex {
        #[command(subcommand)]
        command: CodexCommands,
    },
    /// Gemini / Antigravity CLI commands
    #[command(name = "agy", alias = "antigravity")]
    Agy {
        #[command(subcommand)]
        command: AgyCommands,
    },
    /// Update coding agents (claude, codex, agy, kimi, reasonix, pi)
    Update {
        /// Specific agent(s) to update. If omitted, checks all.
        agents: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CodexCommands {
    /// Show Codex plan usage limits
    Usage {
        /// Show the timezone name next to reset times
        #[arg(long)]
        show_timezone: bool,
    },
}

#[derive(Subcommand)]
enum AgyCommands {
    /// Show Gemini / Antigravity usage limits
    Usage {
        /// Show the timezone name next to reset times
        #[arg(long)]
        show_timezone: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Usage { all, show_timezone } => {
            if all {
                commands::usage_all::run(show_timezone).await
            } else {
                commands::usage::run(show_timezone).await
            }
        }
        Commands::Codex { command } => match command {
            CodexCommands::Usage { show_timezone } => {
                commands::codex_usage::run(show_timezone).await
            }
        },
        Commands::Agy { command } => match command {
            AgyCommands::Usage { show_timezone } => commands::agy_usage::run(show_timezone).await,
        },
        Commands::Update { agents } => commands::update::run(&agents),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "usage", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Usage { all, show_timezone } => {
                assert!(!all);
                assert!(show_timezone);
            }
            _ => panic!("expected usage command"),
        }
    }

    #[test]
    fn usage_all_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "usage", "--all", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Usage { all, show_timezone } => {
                assert!(all);
                assert!(show_timezone);
            }
            _ => panic!("expected usage command"),
        }
    }

    #[test]
    fn codex_usage_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "codex", "usage", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Codex {
                command: CodexCommands::Usage { show_timezone },
            } => assert!(show_timezone),
            _ => panic!("expected codex usage command"),
        }
    }

    #[test]
    fn agy_usage_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "agy", "usage", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Agy {
                command: AgyCommands::Usage { show_timezone },
            } => assert!(show_timezone),
            _ => panic!("expected agy usage command"),
        }
    }
}
