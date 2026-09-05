use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::codex::api::{
    Credits, RateLimitResetCredit, RateLimitResetCreditsDetails, UsageResponse, WindowSnapshot,
};
use crate::commands::status::{self, Provider};

const FILL_CHAR: char = '\u{2588}';
const EMPTY_CHAR: char = '\u{2591}';

fn window_label(seconds: i64) -> &'static str {
    let minutes = seconds / 60;
    match minutes {
        ..=59 => "Current session",
        60..=359 => "Current session (5h)",
        360..=1499 => "Current day",
        1500..=14399 => "Current week",
        14400..=129599 => "Current month",
        _ => "Current year",
    }
}

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

fn progress_bar(used_percent: f64, width: usize) -> String {
    let filled = ((used_percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);

    let fill_str = FILL_CHAR.to_string().repeat(filled);
    let empty_str = EMPTY_CHAR.to_string().repeat(empty);

    if used_percent < 50.0 {
        format!(
            "{}{}",
            fill_str.truecolor(142, 192, 124),
            empty_str.truecolor(100, 100, 100)
        )
    } else if used_percent < 80.0 {
        format!(
            "{}{}",
            fill_str.yellow(),
            empty_str.truecolor(100, 100, 100)
        )
    } else {
        format!("{}{}", fill_str.red(), empty_str.truecolor(100, 100, 100))
    }
}

fn format_duration_short(seconds: i64) -> String {
    if seconds <= 0 {
        return "now".to_string();
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let days = hours / 24;
    let rem_hours = hours % 24;

    if days > 0 {
        if rem_hours > 0 {
            format!("{days}d {rem_hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else {
        format!("{}m", minutes.max(1))
    }
}

fn format_reset_from_unix_with_options(reset_at: i64, show_timezone: bool) -> String {
    let Some(dt) = DateTime::from_timestamp(reset_at, 0) else {
        return String::new();
    };
    let local_dt = dt.with_timezone(&Local);
    let today = Local::now().date_naive();
    format_local(local_dt, today, show_timezone)
}

fn time_remaining_from_unix(reset_at: i64) -> Option<String> {
    let now = Local::now().timestamp();
    let secs = reset_at - now;
    if secs <= 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

fn format_local(local_dt: DateTime<Local>, today: NaiveDate, show_timezone: bool) -> String {
    let time_str = if local_dt.minute() == 0 {
        local_dt.format("%-I%P").to_string()
    } else {
        local_dt.format("%-I:%M%P").to_string()
    };
    let time_str = if show_timezone {
        let tz = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string());
        format!("{time_str} ({tz})")
    } else {
        time_str
    };

    if local_dt.date_naive() == today {
        time_str
    } else {
        let date_str = local_dt.format("%b %-d").to_string();
        format!("{date_str} at {time_str}")
    }
}

fn print_window(label: &str, window: &WindowSnapshot, show_timezone: bool) {
    let bar = progress_bar(window.used_percent, bar_width());
    println!("{}", label.bold());
    println!("{} {:.0}% used", bar, window.used_percent);
    if let Some(reset_at) = window.reset_at {
        let reset_str = format_reset_from_unix_with_options(reset_at, show_timezone);
        if !reset_str.is_empty() {
            let line = match time_remaining_from_unix(reset_at) {
                Some(rem) => format!("Resets {reset_str}, {rem} left"),
                None => format!("Resets {reset_str}"),
            };
            println!("{}", line.dimmed());
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

pub async fn run(show_timezone: bool) {
    if let Err(e) = render(show_timezone).await {
        status::print_provider_error(Provider::Codex, &e);
        std::process::exit(1);
    }
}

pub async fn run_json(show_timezone: bool) {
    let snapshot = crate::snapshot::ProviderSnapshot::from_result(
        Provider::Codex,
        snapshot(show_timezone).await,
    );
    crate::commands::usage_all::print_json(vec![snapshot]);
}

// --- Snapshot support (JSON output / claudex-bar). Keep in sync with the
// print_* functions above; see src/snapshot.rs.

fn window_bar_row(window: &WindowSnapshot, show_timezone: bool) -> crate::snapshot::Row {
    let used = window.used_percent;
    let detail = window.reset_at.and_then(|reset_at| {
        let reset_str = format_reset_from_unix_with_options(reset_at, show_timezone);
        if reset_str.is_empty() {
            return None;
        }
        Some(match time_remaining_from_unix(reset_at) {
            Some(rem) => format!("Resets {reset_str}, {rem} left"),
            None => format!("Resets {reset_str}"),
        })
    });
    let resets_at = window
        .reset_at
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .map(|dt| dt.to_rfc3339());
    crate::snapshot::Row::bar(used, format!("{used:.0}% used"), detail, resets_at)
}

fn window_block(
    title: &str,
    window: &WindowSnapshot,
    show_timezone: bool,
) -> crate::snapshot::Block {
    crate::snapshot::Block::titled(title, vec![window_bar_row(window, show_timezone)])
}

fn format_expiry_line(expires_at: i64, show_timezone: bool) -> String {
    let when = format_reset_from_unix_with_options(expires_at, show_timezone);
    if when.is_empty() {
        return String::new();
    }
    match time_remaining_from_unix(expires_at) {
        Some(rem) => format!("Expires {when}, {rem} left"),
        None => format!("Expires {when}"),
    }
}

fn reset_credits_count(
    usage: &UsageResponse,
    details: Option<&RateLimitResetCreditsDetails>,
) -> Option<i64> {
    usage
        .rate_limit_reset_credits
        .as_ref()
        .map(|summary| summary.available_count)
        .or_else(|| details.map(|details| details.available_count))
}

fn available_reset_credits(
    details: Option<&RateLimitResetCreditsDetails>,
) -> Vec<&RateLimitResetCredit> {
    let mut credits: Vec<&RateLimitResetCredit> = details
        .map(|details| {
            details
                .credits
                .iter()
                .filter(|credit| credit.is_available())
                .collect()
        })
        .unwrap_or_default();
    credits.sort_by_key(|credit| credit.expires_at.unwrap_or(i64::MAX));
    credits
}

fn reset_credits_block(
    usage: &UsageResponse,
    details: Option<&RateLimitResetCreditsDetails>,
    show_timezone: bool,
) -> Option<crate::snapshot::Block> {
    let count = reset_credits_count(usage, details)?;
    let mut rows = vec![crate::snapshot::Row::text(format!("{count} available"))];
    if count > 0 {
        for credit in available_reset_credits(details) {
            if let Some(expires_at) = credit.expires_at {
                let line = format_expiry_line(expires_at, show_timezone);
                if !line.is_empty() {
                    rows.push(crate::snapshot::Row::text(line));
                }
            }
        }
    }
    Some(crate::snapshot::Block::titled("Reset credits", rows))
}

fn print_reset_credits(
    usage: &UsageResponse,
    details: Option<&RateLimitResetCreditsDetails>,
    show_timezone: bool,
) {
    let Some(block) = reset_credits_block(usage, details, show_timezone) else {
        return;
    };
    println!();
    if let Some(title) = &block.title {
        println!("{}", title.bold());
    }
    for row in &block.rows {
        match row {
            crate::snapshot::Row::Text { text } if text.starts_with("Expires ") => {
                println!("{}", text.dimmed());
            }
            crate::snapshot::Row::Text { text } => println!("{text}"),
            _ => {}
        }
    }
}

fn credits_block(credits: &Credits) -> Option<crate::snapshot::Block> {
    if credits.unlimited.unwrap_or(false) {
        return Some(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text("Credits: Unlimited"),
        ]));
    }
    if credits.has_credits.unwrap_or(false) {
        let balance = credits
            .balance
            .as_deref()
            .and_then(|b| b.parse::<f64>().ok())
            .unwrap_or(0.0);
        if balance > 0.0 {
            return Some(crate::snapshot::Block::untitled(vec![
                crate::snapshot::Row::text(format!("Credits: ${balance:.2}")),
            ]));
        }
    }
    None
}

fn build_blocks(
    usage: &UsageResponse,
    details: Option<&RateLimitResetCreditsDetails>,
    show_timezone: bool,
) -> Vec<crate::snapshot::Block> {
    let has_limits = usage.rate_limit.is_some()
        || usage
            .additional_rate_limits
            .as_ref()
            .is_some_and(|a| !a.is_empty());

    if !has_limits {
        return vec![crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text("Codex usage data is not available for your plan."),
        ])];
    }

    let mut blocks = Vec::new();

    if let Some(plan) = &usage.plan_type {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text(format!("Subscription: {}", capitalize(plan))),
        ]));
    }

    if let Some(rl) = &usage.rate_limit {
        for window in [&rl.primary_window, &rl.secondary_window]
            .into_iter()
            .flatten()
        {
            blocks.push(window_block(
                window_label(window.limit_window_seconds),
                window,
                show_timezone,
            ));
        }
    }

    if let Some(additional) = &usage.additional_rate_limits {
        for extra in additional {
            if let Some(rl) = &extra.rate_limit {
                let name = extra.limit_name.as_deref().unwrap_or("Other");
                for window in [&rl.primary_window, &rl.secondary_window]
                    .into_iter()
                    .flatten()
                {
                    let title = format!("{name} — {}", window_label(window.limit_window_seconds));
                    blocks.push(window_block(&title, window, show_timezone));
                }
            }
        }
    }

    if let Some(block) = reset_credits_block(usage, details, show_timezone) {
        blocks.push(block);
    }

    if let Some(credits) = &usage.credits
        && let Some(block) = credits_block(credits)
    {
        blocks.push(block);
    }

    blocks
}

pub async fn snapshot(show_timezone: bool) -> Result<crate::snapshot::ProviderSnapshot, String> {
    let creds = crate::codex::auth::read_credentials()?;

    let (usage, details) = crate::codex::api::fetch_usage_bundle(&creds).await?;

    Ok(crate::snapshot::ProviderSnapshot::ok(
        Provider::Codex,
        build_blocks(&usage, details.as_ref(), show_timezone),
    ))
}

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let creds = crate::codex::auth::read_credentials()?;

    let (usage, details) = crate::codex::api::fetch_usage_bundle(&creds).await?;

    let has_limits = usage.rate_limit.is_some()
        || usage
            .additional_rate_limits
            .as_ref()
            .is_some_and(|a| !a.is_empty());

    if !has_limits {
        println!("Codex usage data is not available for your plan.");
        return Ok(());
    }

    if let Some(plan) = &usage.plan_type {
        println!("{} {}\n", "Subscription:".bold(), capitalize(plan));
    }

    if let Some(rl) = &usage.rate_limit {
        let mut first = true;
        for window in [&rl.primary_window, &rl.secondary_window]
            .into_iter()
            .flatten()
        {
            if !first {
                println!();
            }
            print_window(
                window_label(window.limit_window_seconds),
                window,
                show_timezone,
            );
            first = false;
        }
    }

    if let Some(additional) = &usage.additional_rate_limits {
        for extra in additional {
            if let Some(rl) = &extra.rate_limit {
                let name = extra.limit_name.as_deref().unwrap_or("Other");
                for window in [&rl.primary_window, &rl.secondary_window]
                    .into_iter()
                    .flatten()
                {
                    println!();
                    let label = format!("{name} — {}", window_label(window.limit_window_seconds));
                    print_window(&label, window, show_timezone);
                }
            }
        }
    }

    print_reset_credits(&usage, details.as_ref(), show_timezone);

    if let Some(credits) = &usage.credits {
        let unlimited = credits.unlimited.unwrap_or(false);
        let has_credits = credits.has_credits.unwrap_or(false);
        if unlimited {
            println!("\n{}", "Credits: Unlimited".bold());
        } else if has_credits {
            let balance = credits
                .balance
                .as_deref()
                .and_then(|b| b.parse::<f64>().ok())
                .unwrap_or(0.0);
            if balance > 0.0 {
                println!("\nCredits: ${:.2}", balance);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_label() {
        assert_eq!(window_label(18000), "Current session (5h)");
        assert_eq!(window_label(604800), "Current week");
        assert_eq!(window_label(3600), "Current session (5h)");
        assert_eq!(window_label(86400), "Current day");
        assert_eq!(window_label(2592000), "Current month");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("pro"), "Pro");
        assert_eq!(capitalize("free"), "Free");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn test_format_reset_from_unix_valid() {
        let result = format_reset_from_unix_with_options(1779972641, false);
        assert!(!result.is_empty());
        assert!(!result.contains('('));
    }

    #[test]
    fn test_format_local_hides_timezone_by_default() {
        let dt = DateTime::from_timestamp(1779972641, 0)
            .unwrap()
            .with_timezone(&Local);
        let today = dt.date_naive();

        assert!(!format_local(dt, today, false).contains('('));
        assert!(format_local(dt, today, true).contains('('));
    }

    #[test]
    fn test_format_duration_short() {
        assert_eq!(format_duration_short(0), "now");
        assert_eq!(format_duration_short(-10), "now");
        assert_eq!(format_duration_short(90), "1m");
        assert_eq!(format_duration_short(3600), "1h");
        assert_eq!(format_duration_short(3660), "1h 1m");
        assert_eq!(format_duration_short(86400), "1d");
        assert_eq!(format_duration_short(90000), "1d 1h");
    }

    #[test]
    fn test_progress_bar_not_empty() {
        let bar = progress_bar(50.0, 20);
        assert!(bar.contains('\u{2588}'));
        assert!(bar.contains('\u{2591}'));
    }

    #[test]
    fn test_time_remaining_from_unix_future() {
        let future = Local::now().timestamp() + 7200;
        let result = time_remaining_from_unix(future);
        assert!(result.is_some());
        assert!(result.unwrap().contains('h'));
    }

    #[test]
    fn test_time_remaining_from_unix_past() {
        let past = Local::now().timestamp() - 100;
        assert!(time_remaining_from_unix(past).is_none());
    }

    fn usage_from_json(json: &str) -> UsageResponse {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn build_blocks_full_response_has_all_sections() {
        let usage = usage_from_json(
            r#"{
                "plan_type": "pro",
                "rate_limit": {
                    "primary_window": {"used_percent": 9, "limit_window_seconds": 18000, "reset_at": 1779972641},
                    "secondary_window": {"used_percent": 36, "limit_window_seconds": 604800, "reset_at": 1780210528}
                },
                "additional_rate_limits": [
                    {
                        "limit_name": "GPT-5.3-Codex-Spark",
                        "rate_limit": {
                            "primary_window": {"used_percent": 0, "limit_window_seconds": 18000, "reset_at": 1779975302},
                            "secondary_window": {"used_percent": 0, "limit_window_seconds": 604800, "reset_at": 1780562102}
                        }
                    }
                ],
                "credits": {"has_credits": false, "unlimited": false, "balance": "0"}
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);

        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].title, None);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Subscription: Pro"),
            _ => panic!("expected text row"),
        }
        assert_eq!(blocks[1].title.as_deref(), Some("Current session (5h)"));
        match &blocks[1].rows[0] {
            crate::snapshot::Row::Bar {
                percent,
                text,
                detail,
                resets_at,
            } => {
                assert_eq!(*percent, 9.0);
                assert_eq!(text, "9% used");
                assert!(detail.as_deref().unwrap().starts_with("Resets "));
                assert_eq!(resets_at.as_deref(), Some("2026-05-28T12:50:41+00:00"));
            }
            _ => panic!("expected bar row"),
        }
        assert_eq!(blocks[2].title.as_deref(), Some("Current week"));
        assert_eq!(
            blocks[3].title.as_deref(),
            Some("GPT-5.3-Codex-Spark — Current session (5h)")
        );
        assert_eq!(
            blocks[4].title.as_deref(),
            Some("GPT-5.3-Codex-Spark — Current week")
        );
    }

    #[test]
    fn build_blocks_without_limits_reports_plan_message() {
        let usage = usage_from_json(r#"{"plan_type": "free"}"#);
        let blocks = build_blocks(&usage, None, false);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title, None);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => {
                assert_eq!(text, "Codex usage data is not available for your plan.")
            }
            _ => panic!("expected text row"),
        }
    }

    #[test]
    fn build_blocks_unnamed_limit_falls_back_to_other() {
        let usage = usage_from_json(
            r#"{
                "additional_rate_limits": [
                    {
                        "limit_name": null,
                        "rate_limit": {
                            "primary_window": {"used_percent": 42.5, "limit_window_seconds": 18000}
                        }
                    }
                ]
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);

        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].title.as_deref(),
            Some("Other — Current session (5h)")
        );
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Bar {
                percent,
                text,
                detail,
                resets_at,
            } => {
                assert_eq!(*percent, 42.5);
                assert_eq!(text, "42% used");
                assert!(detail.is_none());
                assert!(resets_at.is_none());
            }
            _ => panic!("expected bar row"),
        }
    }

    #[test]
    fn build_blocks_credits_lines() {
        let usage = usage_from_json(
            r#"{
                "rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}},
                "credits": {"has_credits": true, "unlimited": true, "balance": "0"}
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);
        assert_eq!(blocks.len(), 2);
        match &blocks[1].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Credits: Unlimited"),
            _ => panic!("expected text row"),
        }

        let usage = usage_from_json(
            r#"{
                "rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}},
                "credits": {"has_credits": true, "unlimited": false, "balance": "12.5"}
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);
        assert_eq!(blocks.len(), 2);
        match &blocks[1].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Credits: $12.50"),
            _ => panic!("expected text row"),
        }
    }

    fn details_from_json(json: &str) -> RateLimitResetCreditsDetails {
        serde_json::from_str(json).unwrap()
    }

    fn text_row(row: &crate::snapshot::Row) -> &str {
        match row {
            crate::snapshot::Row::Text { text } => text,
            _ => panic!("expected text row"),
        }
    }

    #[test]
    fn build_blocks_reset_credits_zero_shows_count() {
        let usage = usage_from_json(
            r#"{
                "rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}},
                "rate_limit_reset_credits": {"available_count": 0}
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].title.as_deref(), Some("Reset credits"));
        assert_eq!(text_row(&blocks[1].rows[0]), "0 available");
        assert_eq!(blocks[1].rows.len(), 1);
    }

    #[test]
    fn build_blocks_reset_credits_omitted_without_summary_or_details() {
        let usage = usage_from_json(
            r#"{"rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}}}"#,
        );
        let blocks = build_blocks(&usage, None, false);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title.as_deref(), Some("Current session (5h)"));
    }

    #[test]
    fn build_blocks_reset_credits_lists_expiry_soonest_first() {
        let usage = usage_from_json(
            r#"{
                "rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}},
                "rate_limit_reset_credits": {"available_count": 2}
            }"#,
        );
        let details = details_from_json(
            r#"{
                "available_count": 2,
                "credits": [
                    {
                        "status": "available",
                        "expires_at": "2030-06-01T00:00:00Z"
                    },
                    {
                        "status": "redeemed",
                        "expires_at": "2029-01-01T00:00:00Z"
                    },
                    {
                        "status": "available",
                        "expires_at": 1893456000
                    }
                ]
            }"#,
        );
        let blocks = build_blocks(&usage, Some(&details), false);
        let reset = blocks
            .iter()
            .find(|block| block.title.as_deref() == Some("Reset credits"))
            .expect("reset credits block");
        assert_eq!(text_row(&reset.rows[0]), "2 available");
        assert_eq!(reset.rows.len(), 3);
        assert_eq!(
            text_row(&reset.rows[1]),
            format_expiry_line(1893456000, false)
        );
        assert_eq!(
            text_row(&reset.rows[2]),
            format_expiry_line(1906502400, false)
        );
        assert!(text_row(&reset.rows[1]).starts_with("Expires "));
        assert!(text_row(&reset.rows[1]).contains(" left"));
        assert!(text_row(&reset.rows[2]).starts_with("Expires "));
        assert!(text_row(&reset.rows[2]).contains(" left"));
    }

    #[test]
    fn build_blocks_reset_credits_count_without_details() {
        let usage = usage_from_json(
            r#"{
                "rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": null}},
                "rate_limit_reset_credits": {"available_count": 2}
            }"#,
        );
        let blocks = build_blocks(&usage, None, false);
        let reset = &blocks[1];
        assert_eq!(reset.title.as_deref(), Some("Reset credits"));
        assert_eq!(text_row(&reset.rows[0]), "2 available");
        assert_eq!(reset.rows.len(), 1);
    }

    #[test]
    fn format_expiry_line_matches_reset_date_style() {
        let ts = 1893456000;
        let when = format_reset_from_unix_with_options(ts, false);
        let line = format_expiry_line(ts, false);
        assert_eq!(
            line,
            format!(
                "Expires {when}, {} left",
                time_remaining_from_unix(ts).unwrap()
            )
        );
        assert!(!format_expiry_line(ts, false).contains('('));
        assert!(format_expiry_line(ts, true).contains('('));
    }
}
