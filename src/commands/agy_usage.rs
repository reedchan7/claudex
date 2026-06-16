use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::agy::api::{QuotaSummaryBucket, UserQuotaSummaryResponse};

const FILL_CHAR: char = '█';
const EMPTY_CHAR: char = '░';

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

fn remaining_percent_from_remaining_fraction(remaining_fraction: f64) -> f64 {
    if !remaining_fraction.is_finite() {
        return 0.0;
    }
    (remaining_fraction.clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0)
}

fn format_remaining_percent(remaining_percent: f64) -> String {
    format!("{remaining_percent:.2}% remaining")
}

#[cfg(test)]
fn remaining_progress_bar_chars(remaining_percent: f64, width: usize) -> String {
    let filled = (((remaining_percent / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);
    FILL_CHAR.to_string().repeat(filled) + &EMPTY_CHAR.to_string().repeat(empty)
}

fn remaining_progress_bar(remaining_percent: f64, width: usize) -> String {
    let filled = (((remaining_percent / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    let fill_str = FILL_CHAR.to_string().repeat(filled);
    let empty_str = EMPTY_CHAR.to_string().repeat(empty);

    let colored_fill = if remaining_percent > 50.0 {
        fill_str.truecolor(166, 255, 98)
    } else if remaining_percent > 20.0 {
        fill_str.yellow()
    } else {
        fill_str.truecolor(255, 122, 111)
    };
    let colored_empty = empty_str.truecolor(70, 105, 101);

    format!("{colored_fill}{colored_empty}")
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

fn time_remaining(resets_at: &str) -> Option<String> {
    let dt = DateTime::parse_from_rfc3339(resets_at).ok()?;
    let secs = dt.signed_duration_since(Local::now()).num_seconds();
    if secs < 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

fn format_local(
    local_dt: DateTime<Local>,
    today: NaiveDate,
    tz_name: &str,
    show_timezone: bool,
) -> String {
    let time_str = if local_dt.minute() == 0 {
        local_dt.format("%-I%P").to_string()
    } else {
        local_dt.format("%-I:%M%P").to_string()
    };
    let time_str = if show_timezone {
        format!("{time_str} ({tz_name})")
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

fn format_reset_time_with_options(resets_at: &str, show_timezone: bool) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(resets_at) else {
        return resets_at.to_string();
    };
    let local_dt = dt.with_timezone(&Local);
    let tz_name = if show_timezone {
        iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string())
    } else {
        String::new()
    };
    format_local(local_dt, Local::now().date_naive(), &tz_name, show_timezone)
}

fn bucket_label(bucket: &QuotaSummaryBucket) -> &str {
    bucket
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Quota")
}

fn print_bucket(bucket: &QuotaSummaryBucket, show_timezone: bool) {
    println!("{}", bucket_label(bucket).bold());

    if bucket.disabled.unwrap_or(false) {
        println!("{}", "Disabled".dimmed());
    } else if let Some(remaining_fraction) = bucket.remaining_fraction {
        let remaining_percent = remaining_percent_from_remaining_fraction(remaining_fraction);
        let bar = remaining_progress_bar(remaining_percent, bar_width());
        println!("{} {}", bar, format_remaining_percent(remaining_percent));
    } else if let Some(remaining_amount) = bucket.remaining_amount {
        println!("{}", format!("{remaining_amount} remaining").dimmed());
    } else {
        println!("{}", "Quota amount was not returned.".dimmed());
    }

    if let Some(reset_time) = bucket.reset_time.as_deref() {
        let reset_str = format_reset_time_with_options(reset_time, show_timezone);
        let line = match time_remaining(reset_time) {
            Some(rem) => format!("Refreshes {reset_str}, {rem} left"),
            None => format!("Refreshes {reset_str}"),
        };
        println!("{}", line.dimmed());
    }
}

fn has_quota_data(quota: &UserQuotaSummaryResponse) -> bool {
    !quota.groups.is_empty() || !quota.buckets.is_empty()
}

fn print_quota_summary(quota: &UserQuotaSummaryResponse, show_timezone: bool) {
    let mut first_group = true;
    for group in &quota.groups {
        if !first_group {
            println!();
        }
        println!("{}", group.display_name.bold());
        if let Some(description) = group
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            println!("{}", description.dimmed());
        }

        for bucket in &group.buckets {
            println!();
            print_bucket(bucket, show_timezone);
        }
        first_group = false;
    }

    if !quota.buckets.is_empty() {
        if !first_group {
            println!();
        }
        for (index, bucket) in quota.buckets.iter().enumerate() {
            if index > 0 {
                println!();
            }
            print_bucket(bucket, show_timezone);
        }
    }
}

pub async fn run(show_timezone: bool) {
    if let Err(e) = render(show_timezone).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let access_token = crate::agy::auth::read_access_token().await?;
    let user_agent = crate::agy::auth::agy_user_agent();
    let quota = crate::agy::api::fetch_user_quota_summary(&access_token, &user_agent).await?;

    if !has_quota_data(&quota) {
        println!("Antigravity quota data is not available for your account.");
        return Ok(());
    }

    print_quota_summary(&quota, show_timezone);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remaining_percent_from_remaining_fraction() {
        assert_eq!(remaining_percent_from_remaining_fraction(1.0), 100.0);
        assert_eq!(
            remaining_percent_from_remaining_fraction(0.9207936),
            92.07936
        );
        assert_eq!(remaining_percent_from_remaining_fraction(0.0), 0.0);
    }

    #[test]
    fn test_remaining_percent_from_remaining_fraction_clamps() {
        assert_eq!(remaining_percent_from_remaining_fraction(1.5), 100.0);
        assert_eq!(remaining_percent_from_remaining_fraction(-0.25), 0.0);
        assert_eq!(remaining_percent_from_remaining_fraction(f64::NAN), 0.0);
    }

    #[test]
    fn test_format_remaining_percent() {
        assert_eq!(format_remaining_percent(92.07936), "92.08% remaining");
        assert_eq!(format_remaining_percent(100.0), "100.00% remaining");
    }

    #[test]
    fn test_remaining_progress_bar_chars() {
        assert_eq!(remaining_progress_bar_chars(20.0, 10), "██░░░░░░░░");
    }

    #[test]
    fn test_bucket_label_uses_display_name() {
        let bucket = QuotaSummaryBucket {
            display_name: Some("Weekly Limit".to_string()),
            remaining_fraction: Some(0.5),
            remaining_amount: None,
            disabled: None,
            reset_time: None,
        };

        assert_eq!(bucket_label(&bucket), "Weekly Limit");
    }

    #[test]
    fn test_format_reset_time_invalid_returns_original() {
        assert_eq!(
            format_reset_time_with_options("not-a-date", false),
            "not-a-date"
        );
    }
}
