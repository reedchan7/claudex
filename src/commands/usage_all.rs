use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};

const RULE_CHAR: char = '\u{2501}'; // ━

// Widest possible progress-bar line suffix: " 100.00% used".
const PCT_SUFFIX_WIDTH: usize = 18;

// Matches `bar_width()` in provider renderers so the rule lines up with the
// bars rendered underneath each header.
fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

// Span the rule across the longest line a section can print: a full bar
// plus its percentage suffix. Both sections share this width, so they align.
fn rule_width() -> usize {
    bar_width() + PCT_SUFFIX_WIDTH
}

fn print_header(title: &str, accent: (u8, u8, u8)) {
    let (r, g, b) = accent;
    let rule = RULE_CHAR.to_string().repeat(rule_width());
    println!("{}", title.bold().truecolor(r, g, b));
    println!("{}", rule.truecolor(r, g, b));
    println!();
}

pub async fn run(show_timezone: bool) {
    let mut had_error = false;
    let mut rendered = 0;

    print_header("Claude Code", (217, 119, 87));
    match crate::commands::usage::render(show_timezone).await {
        Ok(()) => rendered += 1,
        Err(e) => {
            status::print_provider_error(Provider::Claude, &e);
            had_error = true;
        }
    }

    println!();
    print_header("Codex", (16, 163, 127));
    match crate::commands::codex_usage::render(show_timezone).await {
        Ok(()) => rendered += 1,
        Err(e) => {
            status::print_provider_error(Provider::Codex, &e);
            had_error = true;
        }
    }

    println!();
    print_header(Provider::Antigravity.label(), (66, 133, 244));
    match crate::commands::agy_usage::render(show_timezone).await {
        Ok(()) => rendered += 1,
        Err(e) => {
            status::print_provider_error(Provider::Antigravity, &e);
            had_error = true;
        }
    }

    if should_exit_failure(rendered, had_error) {
        std::process::exit(1);
    }
}

fn should_exit_failure(rendered: usize, had_error: bool) -> bool {
    had_error && rendered == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_all_succeeds_when_at_least_one_provider_renders() {
        assert!(!should_exit_failure(1, true));
    }

    #[test]
    fn usage_all_fails_when_every_provider_is_unavailable() {
        assert!(should_exit_failure(0, true));
    }
}
