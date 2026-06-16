use colored::Colorize;
use terminal_size::{Width, terminal_size};

const EMPTY_CHAR: char = '░';

#[derive(Clone, Copy)]
pub enum Provider {
    Claude,
    Codex,
    Antigravity,
}

pub struct ProviderStatus {
    pub heading: String,
    pub detail: String,
    pub next_step: String,
    pub details: Option<String>,
}

impl Provider {
    fn label(self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
            Provider::Antigravity => "Antigravity",
        }
    }

    fn connect_action(self) -> &'static str {
        match self {
            Provider::Claude => "Run `claude` and sign in, or set `CLAUDE_CODE_OAUTH_TOKEN`.",
            Provider::Codex => "Run `codex` and sign in with ChatGPT.",
            Provider::Antigravity => "Run `agy` and sign in with Google.",
        }
    }

    fn refresh_action(self) -> &'static str {
        match self {
            Provider::Claude => "Run `claude` and sign in again, then retry.",
            Provider::Codex => "Run `codex` and sign in again, then retry.",
            Provider::Antigravity => "Run `agy` and sign in with Google.",
        }
    }
}

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

fn empty_bar(width: usize) -> String {
    EMPTY_CHAR.to_string().repeat(width)
}

pub fn status_for_error(provider: Provider, error: &str) -> ProviderStatus {
    let lower = error.to_ascii_lowercase();
    let label = provider.label();

    if is_not_connected(provider, &lower) {
        return ProviderStatus {
            heading: format!("{label} is not connected"),
            detail: format!("No local {label} session was found on this machine."),
            next_step: provider.connect_action().to_string(),
            details: None,
        };
    }

    if provider_is_unsupported(provider, &lower) {
        return ProviderStatus {
            heading: "Antigravity credentials are unavailable".to_string(),
            detail: "claudex currently reads Antigravity sessions from macOS Keychain.".to_string(),
            next_step: "Use this command on macOS after signing in with `agy`.".to_string(),
            details: None,
        };
    }

    if lower.contains("authentication failed") {
        return ProviderStatus {
            heading: format!("{label} session needs refresh"),
            detail: format!("The saved {label} token was rejected."),
            next_step: provider.refresh_action().to_string(),
            details: None,
        };
    }

    if looks_like_invalid_credentials(&lower) {
        return ProviderStatus {
            heading: format!("{label} credentials need repair"),
            detail: format!("The local {label} credentials could not be read."),
            next_step: provider.refresh_action().to_string(),
            details: Some(error.to_string()),
        };
    }

    ProviderStatus {
        heading: format!("{label} usage is temporarily unavailable"),
        detail: format!("claudex could not fetch {label} usage data right now."),
        next_step: format!("Retry later. If it keeps failing, refresh your local {label} session."),
        details: Some(error.to_string()),
    }
}

fn is_not_connected(provider: Provider, lower: &str) -> bool {
    match provider {
        Provider::Claude => {
            lower.contains("could not find claude credentials")
                || lower.contains("keychain entry not found")
        }
        Provider::Codex => {
            lower.contains("could not find codex credentials")
                || lower.contains("auth.json has no tokens")
        }
        Provider::Antigravity => lower.contains("could not find antigravity credentials"),
    }
}

fn provider_is_unsupported(provider: Provider, lower: &str) -> bool {
    matches!(provider, Provider::Antigravity) && lower.contains("currently supports macos keychain")
}

fn looks_like_invalid_credentials(lower: &str) -> bool {
    lower.contains("could not parse")
        || lower.contains("invalid keychain")
        || lower.contains("could not decode")
        || lower.contains("not utf-8")
        || lower.contains("no usable access token")
        || lower.contains("missing field")
}

pub fn print_provider_error(provider: Provider, error: &str) {
    let status = status_for_error(provider, error);

    println!("{}", status.heading.bold().truecolor(245, 198, 106));
    println!(
        "{} {}",
        empty_bar(bar_width()).truecolor(100, 100, 100),
        "unavailable".dimmed()
    );
    println!("{}", status.detail.dimmed());
    println!("{} {}", "Next:".bold(), status.next_step);

    if let Some(details) = status.details {
        println!("{}", format!("Details: {details}").dimmed());
    }
}

#[cfg(test)]
fn plain_status_block(status: &ProviderStatus, width: usize) -> String {
    let mut lines = vec![
        status.heading.clone(),
        format!("{} unavailable", empty_bar(width)),
        status.detail.clone(),
        format!("Next: {}", status.next_step),
    ];

    if let Some(details) = &status.details {
        lines.push(format!("Details: {details}"));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_missing_codex_credentials_as_not_connected() {
        let status = status_for_error(
            Provider::Codex,
            "could not find Codex credentials at /tmp/auth.json — sign in with Codex (run `codex`)",
        );

        assert_eq!(status.heading, "Codex is not connected");
        assert_eq!(
            status.detail,
            "No local Codex session was found on this machine."
        );
        assert_eq!(status.next_step, "Run `codex` and sign in with ChatGPT.");
        assert!(status.details.is_none());
    }

    #[test]
    fn classifies_antigravity_auth_failure_as_session_refresh() {
        let status = status_for_error(
            Provider::Antigravity,
            "authentication failed — try restarting Antigravity to refresh your Google login",
        );

        assert_eq!(status.heading, "Antigravity session needs refresh");
        assert_eq!(status.detail, "The saved Antigravity token was rejected.");
        assert_eq!(status.next_step, "Run `agy` and sign in with Google.");
        assert!(status.details.is_none());
    }

    #[test]
    fn status_block_has_empty_bar_and_action() {
        let status = status_for_error(
            Provider::Claude,
            "could not find Claude credentials — sign in with Claude Code (run `claude`)",
        );
        let block = plain_status_block(&status, 8);

        assert!(block.contains("Claude Code is not connected"));
        assert!(block.contains("░░░░░░░░ unavailable"));
        assert!(
            block.contains("Next: Run `claude` and sign in, or set `CLAUDE_CODE_OAUTH_TOKEN`.")
        );
    }
}
