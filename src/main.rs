mod api;
mod auth;
mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "claudex")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show Claude plan usage limits
    Usage,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Usage => commands::usage::run().await,
    }
}
