use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::codex::api::WindowSnapshot;
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

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let creds = crate::codex::auth::read_credentials()?;

    let usage = crate::codex::api::fetch_usage(&creds).await?;

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
}
