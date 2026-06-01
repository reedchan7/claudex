use colored::Colorize;
use terminal_size::{Width, terminal_size};

const RULE_CHAR: char = '\u{2501}'; // ━

fn rule_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).min(50))
        .unwrap_or(50)
}

fn print_header(title: &str, accent: (u8, u8, u8)) {
    let (r, g, b) = accent;
    let rule = RULE_CHAR.to_string().repeat(rule_width());
    println!("{}", title.bold().truecolor(r, g, b));
    println!("{}", rule.truecolor(r, g, b));
    println!();
}

pub async fn run() {
    let mut had_error = false;

    print_header("Claude Code", (217, 119, 87));
    if let Err(e) = crate::commands::usage::render().await {
        eprintln!("{} {e}", "Error:".red());
        had_error = true;
    }

    println!();
    print_header("Codex", (16, 163, 127));
    if let Err(e) = crate::commands::codex_usage::render().await {
        eprintln!("{} {e}", "Error:".red());
        had_error = true;
    }

    if had_error {
        std::process::exit(1);
    }
}
