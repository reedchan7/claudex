use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};
use crate::glm::api::QuotaLimit;

const FILL_CHAR: char = '\u{2588}';
const EMPTY_CHAR: char = '\u{2591}';

fn limit_label(kind: Option<&str>, unit: Option<i64>) -> &'static str {
    match (kind, unit) {
        (Some("TIME_LIMIT"), _) => "MCP quota",
        (Some("TOKENS_LIMIT"), Some(3)) => "Current session (5h)",
        (Some("TOKENS_LIMIT"), Some(6)) => "Current week",
        _ => "Quota",
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

fn format_reset_from_millis_with_options(reset_at_ms: i64, show_timezone: bool) -> String {
    let Some(dt) = DateTime::from_timestamp_millis(reset_at_ms) else {
        return String::new();
    };
    let local_dt = dt.with_timezone(&Local);
    format_local(local_dt, Local::now().date_naive(), show_timezone)
}

fn time_remaining_from_millis(reset_at_ms: i64) -> Option<String> {
    let secs = (reset_at_ms - Local::now().timestamp_millis()) / 1000;
    if secs <= 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn print_limit(limit: &QuotaLimit, show_timezone: bool) {
    let label = limit_label(limit.kind.as_deref(), limit.unit);
    let used_percent = limit.percentage.unwrap_or(0.0);

    println!("{}", label.bold());
    println!(
        "{} {:.0}% used",
        progress_bar(used_percent, bar_width()),
        used_percent
    );

    // MCP quota carries absolute counters and a per-tool breakdown.
    if let (Some(used), Some(total)) = (limit.current_value, limit.usage) {
        println!("{}", format!("Used {used} / {total}").dimmed());
        for detail in &limit.usage_details {
            if let Some(code) = detail.model_code.as_deref() {
                let tool_used = detail.usage.unwrap_or(0);
                println!("{}", format!("  {code}: {tool_used}").dimmed());
            }
        }
    }

    if let Some(reset_ms) = limit.next_reset_time {
        let reset_str = format_reset_from_millis_with_options(reset_ms, show_timezone);
        if !reset_str.is_empty() {
            let line = match time_remaining_from_millis(reset_ms) {
                Some(rem) => format!("Resets {reset_str}, {rem} left"),
                None => format!("Resets {reset_str}"),
            };
            println!("{}", line.dimmed());
        }
    }
}

pub async fn run(show_timezone: bool, region_override: Option<&str>) {
    if let Err(e) = render(show_timezone, region_override).await {
        status::print_provider_error(Provider::Glm, &e);
        std::process::exit(1);
    }
}

pub async fn render(show_timezone: bool, region_override: Option<&str>) -> Result<(), String> {
    let creds = crate::glm::auth::resolve_credentials(region_override)?;
    let mut usage = crate::glm::api::fetch_usage(creds.region.base_url(), &creds.api_key).await?;

    if usage.limits.is_empty() {
        println!("GLM usage data is not available for your plan.");
        return Ok(());
    }

    if let Some(level) = usage
        .level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        println!("{} {}\n", "Subscription:".bold(), capitalize(level));
    }

    // Sort limits: Session limit first, then Week limit, then MCP quota, then others.
    usage
        .limits
        .sort_by_key(|limit| match (limit.kind.as_deref(), limit.unit) {
            (Some("TOKENS_LIMIT"), Some(3)) => 1,
            (Some("TOKENS_LIMIT"), Some(6)) => 2,
            (Some("TIME_LIMIT"), _) => 3,
            _ => 4,
        });

    for (index, limit) in usage.limits.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_limit(limit, show_timezone);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_label_maps_known_type_unit_pairs() {
        assert_eq!(
            limit_label(Some("TOKENS_LIMIT"), Some(3)),
            "Current session (5h)"
        );
        assert_eq!(limit_label(Some("TOKENS_LIMIT"), Some(6)), "Current week");
        assert_eq!(limit_label(Some("TIME_LIMIT"), Some(5)), "MCP quota");
        assert_eq!(limit_label(Some("TIME_LIMIT"), None), "MCP quota");
        assert_eq!(limit_label(Some("TOKENS_LIMIT"), Some(99)), "Quota");
        assert_eq!(limit_label(None, None), "Quota");
    }

    #[test]
    fn capitalize_uppercases_first_char() {
        assert_eq!(capitalize("pro"), "Pro");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn progress_bar_has_filled_and_empty() {
        let bar = progress_bar(50.0, 20);
        assert!(bar.contains('\u{2588}'));
        assert!(bar.contains('\u{2591}'));
    }

    #[test]
    fn format_duration_short_reads_well() {
        assert_eq!(format_duration_short(0), "now");
        assert_eq!(format_duration_short(3660), "1h 1m");
        assert_eq!(format_duration_short(90000), "1d 1h");
    }

    #[test]
    fn reset_from_millis_formats_and_counts_down() {
        // A fixed instant formats without timezone parens.
        let s = format_reset_from_millis_with_options(1782411163852, false);
        assert!(!s.is_empty());
        assert!(!s.contains('('));

        let future = Local::now().timestamp_millis() + 7_200_000;
        assert!(time_remaining_from_millis(future).unwrap().contains('h'));
        let past = Local::now().timestamp_millis() - 1000;
        assert!(time_remaining_from_millis(past).is_none());
    }
}
