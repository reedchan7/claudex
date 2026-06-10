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
        /// Show the timezone name next to reset times
        #[arg(long)]
        show_timezone: bool,
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
}
