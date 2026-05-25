use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::api::{ExtraUsage, RateLimit};

const FILL_CHAR: char = '█';
const EMPTY_CHAR: char = '░';

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

pub fn progress_bar_chars(utilization: f64, width: usize) -> String {
    let filled = (((utilization / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);
    FILL_CHAR.to_string().repeat(filled) + &EMPTY_CHAR.to_string().repeat(empty)
}

pub fn progress_bar(utilization: f64, width: usize) -> String {
    let bar = progress_bar_chars(utilization, width);
    if utilization < 50.0 {
        bar.green().to_string()
    } else if utilization < 80.0 {
        bar.yellow().to_string()
    } else {
        bar.red().to_string()
    }
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
        println!(
            "{}",
            format!("Resets {}", format_reset_time(resets_at)).dimmed()
        );
    }
}

fn print_extra_usage(extra: &ExtraUsage) {
    if !extra.is_enabled {
        println!(
            "{}",
            "Usage credits   off · /usage-credits to turn on".dimmed()
        );
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
    let token = match crate::auth::read_token() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let version = crate::auth::get_claude_version();
    let user_agent = format!("claude-code/{version}");

    let utilization = match crate::api::fetch_utilization(&token, &user_agent).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let limits: &[(&str, Option<&RateLimit>)] = &[
        ("Current session", utilization.five_hour.as_ref()),
        ("Current week (all models)", utilization.seven_day.as_ref()),
        (
            "Current week (Sonnet only)",
            utilization.seven_day_sonnet.as_ref(),
        ),
    ];

    let has_any = limits.iter().any(|(_, l)| l.is_some());
    if !has_any {
        println!("/usage is only available for subscription plans.");
        return;
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
