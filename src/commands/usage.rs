use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::api::{ExtraUsage, RateLimit};

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

fn time_remaining(resets_at: &str) -> Option<String> {
    let dt = DateTime::parse_from_rfc3339(resets_at).ok()?;
    let secs = dt.signed_duration_since(Local::now()).num_seconds();
    if secs < 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

const FILL_CHAR: char = '█';
const EMPTY_CHAR: char = '░';

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

#[cfg(test)]
pub fn progress_bar_chars(utilization: f64, width: usize) -> String {
    let filled = (((utilization / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);
    FILL_CHAR.to_string().repeat(filled) + &EMPTY_CHAR.to_string().repeat(empty)
}

pub fn progress_bar(utilization: f64, width: usize) -> String {
    let filled = (((utilization / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    let fill_str = FILL_CHAR.to_string().repeat(filled);
    let empty_str = EMPTY_CHAR.to_string().repeat(empty);

    let colored_fill = if utilization < 50.0 {
        fill_str.truecolor(142, 192, 124)
    } else if utilization < 80.0 {
        fill_str.yellow()
    } else {
        fill_str.red()
    };
    let colored_empty = empty_str.truecolor(100, 100, 100);

    format!("{colored_fill}{colored_empty}")
}

fn format_local(local_dt: DateTime<Local>, today: NaiveDate, tz_name: &str) -> String {
    let time_str = if local_dt.minute() == 0 {
        local_dt.format("%-I%P").to_string() // "3am"
    } else {
        local_dt.format("%-I:%M%P").to_string() // "2:30pm"
    };

    if local_dt.date_naive() == today {
        format!("{time_str} ({tz_name})")
    } else {
        let date_str = local_dt.format("%b %-d").to_string(); // "May 30"
        format!("{date_str} at {time_str} ({tz_name})")
    }
}

pub fn format_reset_time(resets_at: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(resets_at) else {
        return resets_at.to_string();
    };
    let local_dt = dt.with_timezone(&Local);
    let tz_name = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string());
    format_local(local_dt, Local::now().date_naive(), &tz_name)
}

fn print_limit_bar(title: &str, limit: &RateLimit) {
    let utilization = limit.utilization.unwrap_or(0.0);
    let bar = progress_bar(utilization, bar_width());
    println!("{}", title.bold());
    println!("{} {:.0}% used", bar, utilization);
    if let Some(resets_at) = &limit.resets_at {
        let reset_str = format_reset_time(resets_at);
        let line = match time_remaining(resets_at) {
            Some(rem) => format!("Resets {reset_str}, {rem} left"),
            None => format!("Resets {reset_str}"),
        };
        println!("{}", line.dimmed());
    }
}

fn print_extra_usage(extra: &ExtraUsage) {
    if !extra.is_enabled {
        println!("{}", "Usage credits   off".dimmed());
        return;
    }
    match extra.monthly_limit {
        None => println!("Usage credits   Unlimited"),
        Some(monthly_limit) => {
            let used = extra.used_credits.unwrap_or(0);
            let utilization = extra.utilization.unwrap_or(0.0);
            let bar = progress_bar(utilization, bar_width());
            println!("{}", "Extra usage".bold());
            println!("{} {:.0}% used", bar, utilization);
            println!(
                "{}",
                format!(
                    "${:.2} / ${:.2} spent",
                    used as f64 / 100.0,
                    monthly_limit as f64 / 100.0
                )
                .dimmed()
            );
        }
    }
}

pub async fn run() {
    if let Err(e) = render().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

pub async fn render() -> Result<(), String> {
    let token = crate::auth::read_token()?;

    let version = crate::auth::get_claude_version();
    let user_agent = format!("claude-code/{version}");

    let utilization = crate::api::fetch_utilization(&token, &user_agent).await?;

    let limits: &[(&str, Option<&RateLimit>)] = &[
        ("Current session (5h)", utilization.five_hour.as_ref()),
        ("Current week (all models)", utilization.seven_day.as_ref()),
        (
            "Current week (Sonnet only)",
            utilization.seven_day_sonnet.as_ref(),
        ),
    ];

    let has_any = limits.iter().any(|(_, l)| l.is_some());
    if !has_any {
        println!("/usage is only available for subscription plans.");
        return Ok(());
    }

    let mut first = true;
    for (title, limit) in limits {
        if let Some(limit) = limit {
            if !first {
                println!();
            }
            print_limit_bar(title, limit);
            first = false;
        }
    }

    if let Some(extra) = &utilization.extra_usage {
        println!();
        print_extra_usage(extra);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_progress_bar_chars_26_percent() {
        // 26% of 10 = 2.6 → rounds to 3 filled
        let bar = progress_bar_chars(26.0, 10);
        assert_eq!(bar, "███░░░░░░░");
    }

    #[test]
    fn test_progress_bar_chars_zero() {
        let bar = progress_bar_chars(0.0, 10);
        assert_eq!(bar, "░░░░░░░░░░");
    }

    #[test]
    fn test_progress_bar_chars_full() {
        let bar = progress_bar_chars(100.0, 10);
        assert_eq!(bar, "██████████");
    }

    #[test]
    fn test_progress_bar_chars_50_percent() {
        let bar = progress_bar_chars(50.0, 10);
        assert_eq!(bar, "█████░░░░░");
    }

    #[test]
    fn test_progress_bar_chars_width_50() {
        let bar = progress_bar_chars(26.0, 50);
        // 26% of 50 = 13.0 filled
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 13);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 37);
    }

    #[test]
    fn test_progress_bar_chars_over_100_clamps() {
        let bar = progress_bar_chars(120.0, 10);
        assert_eq!(bar, "██████████");
    }

    #[test]
    fn test_format_duration_short_seconds() {
        assert_eq!(format_duration_short(0), "now");
        assert_eq!(format_duration_short(-10), "now");
        assert_eq!(format_duration_short(30), "1m");
        assert_eq!(format_duration_short(90), "1m");
        assert_eq!(format_duration_short(119), "1m");
        assert_eq!(format_duration_short(120), "2m");
    }

    #[test]
    fn test_format_duration_short_hours() {
        assert_eq!(format_duration_short(3600), "1h");
        assert_eq!(format_duration_short(3660), "1h 1m");
        assert_eq!(format_duration_short(5400), "1h 30m");
        assert_eq!(format_duration_short(7200), "2h");
    }

    #[test]
    fn test_format_duration_short_days() {
        assert_eq!(format_duration_short(86400), "1d");
        assert_eq!(format_duration_short(90000), "1d 1h");
        assert_eq!(format_duration_short(172800), "2d");
    }

    #[test]
    fn test_format_reset_time_valid_iso_returns_non_empty() {
        let result = format_reset_time("2026-05-29T19:00:00+00:00");
        assert!(!result.is_empty());
        assert!(result.contains('(') && result.contains(')'));
    }

    #[test]
    fn test_format_reset_time_invalid_returns_original() {
        let result = format_reset_time("not-a-date");
        assert_eq!(result, "not-a-date");
    }

    #[test]
    fn test_format_reset_time_fractional_seconds() {
        let result = format_reset_time("2026-05-25T06:30:00.176572+00:00");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_format_local_same_day_with_minutes() {
        let dt = Local.with_ymd_and_hms(2026, 5, 25, 14, 30, 0).unwrap();
        let today = dt.date_naive();
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai"),
            "2:30pm (Asia/Shanghai)"
        );
    }

    #[test]
    fn test_format_local_same_day_on_the_hour() {
        let dt = Local.with_ymd_and_hms(2026, 5, 25, 15, 0, 0).unwrap();
        let today = dt.date_naive();
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai"),
            "3pm (Asia/Shanghai)"
        );
    }

    #[test]
    fn test_format_local_future_date() {
        let dt = Local.with_ymd_and_hms(2026, 5, 30, 3, 0, 0).unwrap();
        let today = Local
            .with_ymd_and_hms(2026, 5, 25, 0, 0, 0)
            .unwrap()
            .date_naive();
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai"),
            "May 30 at 3am (Asia/Shanghai)"
        );
    }
}
