mod agy;
mod api;
mod auth;
mod codex;
mod commands;
mod glm;
mod kimi;

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
        /// Show Claude Code, Codex, Kimi Code, Gemini / Antigravity, and GLM usage limits
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
    /// GLM (Z.ai / BigModel) commands
    #[command(name = "glm", alias = "zai")]
    Glm {
        #[command(subcommand)]
        command: GlmCommands,
    },
    /// Kimi Code commands
    Kimi {
        #[command(subcommand)]
        command: KimiCommands,
    },
    /// Update coding agents (claude, codex, agy, kimi, reasonix, pi)
    Update {
        /// Only run update commands; skip the post-update version check.
        #[arg(long)]
        no_post_check: bool,
        /// Specific agent(s) to update. If omitted, checks all.
        agents: Vec<String>,
    },
    /// Update claudex itself to the latest release
    SelfUpdate {
        /// Only check whether a newer version is available; don't install
        #[arg(long)]
        check: bool,
        /// Reinstall even if already on the latest version
        #[arg(long)]
        force: bool,
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

#[derive(Subcommand)]
enum GlmCommands {
    /// Show GLM Coding Plan usage limits
    Usage {
        /// Show the timezone name next to reset times
        #[arg(long)]
        show_timezone: bool,
        /// Use the domestic BigModel edition (open.bigmodel.cn)
        #[arg(long, conflicts_with = "global")]
        cn: bool,
        /// Use the overseas Z.ai edition (api.z.ai)
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand)]
enum KimiCommands {
    /// Show Kimi Code plan usage limits
    Usage {
        /// Accepted for consistency with other usage commands
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
        Commands::Glm { command } => match command {
            GlmCommands::Usage {
                show_timezone,
                cn,
                global,
            } => commands::glm_usage::run(show_timezone, region_override(cn, global)).await,
        },
        Commands::Kimi { command } => match command {
            KimiCommands::Usage { show_timezone } => commands::kimi_usage::run(show_timezone).await,
        },
        Commands::Update {
            no_post_check,
            agents,
        } => commands::update::run(&agents, !no_post_check),
        Commands::SelfUpdate { check, force } => commands::self_update::run(check, force).await,
    }
}

fn region_override(cn: bool, global: bool) -> Option<&'static str> {
    if cn {
        Some("cn")
    } else if global {
        Some("global")
    } else {
        None
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

    #[test]
    fn glm_usage_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "glm", "usage", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Glm {
                command:
                    GlmCommands::Usage {
                        show_timezone,
                        cn,
                        global,
                    },
            } => {
                assert!(show_timezone);
                assert!(!cn);
                assert!(!global);
            }
            _ => panic!("expected glm usage command"),
        }
    }

    #[test]
    fn glm_usage_alias_zai_parses_region_flag() {
        let cli = Cli::try_parse_from(["claudex", "zai", "usage", "--cn"]).unwrap();

        match cli.command {
            Commands::Glm {
                command: GlmCommands::Usage { cn, global, .. },
            } => {
                assert!(cn);
                assert!(!global);
            }
            _ => panic!("expected glm usage command via zai alias"),
        }
    }

    #[test]
    fn kimi_usage_parses_show_timezone() {
        let cli = Cli::try_parse_from(["claudex", "kimi", "usage", "--show-timezone"]).unwrap();

        match cli.command {
            Commands::Kimi {
                command: KimiCommands::Usage { show_timezone },
            } => assert!(show_timezone),
            _ => panic!("expected kimi usage command"),
        }
    }

    #[test]
    fn update_parses_no_post_check() {
        let cli = Cli::try_parse_from(["claudex", "update", "--no-post-check", "kimi"]).unwrap();

        match cli.command {
            Commands::Update {
                no_post_check,
                agents,
            } => {
                assert!(no_post_check);
                assert_eq!(agents, ["kimi"]);
            }
            _ => panic!("expected update command"),
        }
    }

    #[test]
    fn glm_region_override_maps_flags() {
        assert_eq!(region_override(true, false), Some("cn"));
        assert_eq!(region_override(false, true), Some("global"));
        assert_eq!(region_override(false, false), None);
    }
}
