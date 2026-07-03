use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};

const RULE_CHAR: char = '\u{2501}'; // ━
const PROVIDER_COUNT: usize = 5;
const PROVIDER_ORDER: [Provider; PROVIDER_COUNT] = [
    Provider::Claude,
    Provider::Codex,
    Provider::Antigravity,
    Provider::Glm,
    Provider::Kimi,
];

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

fn provider_accent(provider: Provider) -> (u8, u8, u8) {
    match provider {
        Provider::Claude => (217, 119, 87),
        Provider::Codex => (16, 163, 127),
        Provider::Antigravity => (66, 133, 244),
        Provider::Glm => (99, 102, 241),
        Provider::Kimi => (37, 190, 191),
    }
}

fn provider_gap(index: usize) -> &'static str {
    if index == 0 { "" } else { "\n\n" }
}

async fn render_provider(provider: Provider, show_timezone: bool) -> Result<(), String> {
    match provider {
        Provider::Claude => crate::commands::usage::render(show_timezone).await,
        Provider::Codex => crate::commands::codex_usage::render(show_timezone).await,
        Provider::Antigravity => crate::commands::agy_usage::render(show_timezone).await,
        Provider::Glm => crate::commands::glm_usage::render(show_timezone, None).await,
        Provider::Kimi => crate::commands::kimi_usage::render(show_timezone).await,
    }
}

pub async fn run(show_timezone: bool) {
    let mut had_error = false;
    let mut rendered = 0;

    for (index, provider) in PROVIDER_ORDER.iter().copied().enumerate() {
        print!("{}", provider_gap(index));
        print_header(provider.label(), provider_accent(provider));
        match render_provider(provider, show_timezone).await {
            Ok(()) => rendered += 1,
            Err(e) => {
                status::print_provider_error(provider, &e);
                had_error = true;
            }
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

    #[test]
    fn provider_order_places_kimi_after_glm() {
        assert_eq!(
            PROVIDER_ORDER.map(Provider::label),
            [
                "Claude Code",
                "Codex",
                "Gemini / Antigravity",
                "GLM / Z.ai",
                "Kimi Code",
            ]
        );
    }

    #[test]
    fn provider_gap_adds_a_blank_line_between_agents() {
        assert_eq!(provider_gap(0), "");
        assert_eq!(provider_gap(1), "\n\n");
    }
}
