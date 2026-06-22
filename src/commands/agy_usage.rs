use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::agy::api::{QuotaSummaryBucket, UserQuotaSummaryResponse};
use crate::commands::status::{self, Provider};

const FILL_CHAR: char = '█';
const EMPTY_CHAR: char = '░';
const MODEL_USAGE_USED_WIDTH: usize = 12;
const MODEL_USAGE_GAP_WIDTH: usize = 1;

fn bar_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| (w as usize).saturating_sub(10).min(50))
        .unwrap_or(50)
}

fn terminal_columns() -> usize {
    terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80)
}

fn format_used_percent(used_percent: f64) -> String {
    format!("{used_percent:.2}% used")
}

fn format_remaining_amount(remaining_amount: i64) -> String {
    format!("{remaining_amount} available")
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

fn should_print_standalone_bucket(bucket: &QuotaSummaryBucket) -> bool {
    bucket
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
}

fn print_bucket(bucket: &QuotaSummaryBucket, show_timezone: bool) {
    println!("{}", bucket_label(bucket).bold());

    if bucket.disabled.unwrap_or(false) {
        println!("{}", "Disabled".dimmed());
    } else if let Some(remaining_fraction) = bucket.remaining_fraction {
        let used_percent = used_percent_from_remaining_fraction(remaining_fraction);
        let bar = used_progress_bar(used_percent, bar_width());
        println!("{} {}", bar, format_used_percent(used_percent));
    } else if let Some(remaining_amount) = bucket.remaining_amount {
        println!("{}", format_remaining_amount(remaining_amount).dimmed());
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

// ── Model usage (per-tier) ──────────────────────────────────────────

fn used_percent_from_remaining_fraction(remaining_fraction: f64) -> f64 {
    if !remaining_fraction.is_finite() {
        return 0.0;
    }
    ((1.0 - remaining_fraction).clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0)
}

fn used_progress_bar_segments(used_percent: f64, width: usize) -> (String, String) {
    let filled = (((used_percent / 100.0) * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    (
        FILL_CHAR.to_string().repeat(filled),
        EMPTY_CHAR.to_string().repeat(empty),
    )
}

#[cfg(test)]
fn used_progress_bar_chars(used_percent: f64, width: usize) -> String {
    let (fill_str, empty_str) = used_progress_bar_segments(used_percent, width);
    format!("{fill_str}{empty_str}")
}

/// Progress bar where the filled portion represents **used** percentage.
fn used_progress_bar(used_percent: f64, width: usize) -> String {
    let (fill_str, empty_str) = used_progress_bar_segments(used_percent, width);

    let colored_fill = if used_percent >= 80.0 {
        fill_str.truecolor(255, 122, 111)
    } else if used_percent >= 50.0 {
        fill_str.yellow()
    } else {
        fill_str.truecolor(166, 255, 98)
    };
    let colored_empty = empty_str.truecolor(70, 105, 101);

    format!("{colored_fill}{colored_empty}")
}

/// Compact reset line matching Gemini CLI's "Resets: 1:20 PM (23h 34m)" style.
fn format_model_reset(reset_time: &str, show_timezone: bool) -> String {
    let reset_str = format_reset_time_with_options(reset_time, show_timezone);
    match time_remaining(reset_time) {
        Some(rem) => format!("Resets: {reset_str}, {rem} left"),
        None => format!("Resets: {reset_str}"),
    }
}

fn model_usage_bar_width(columns: usize) -> usize {
    columns.saturating_sub(10).min(50)
}

fn model_usage_section_width(bar_width: usize) -> usize {
    bar_width + MODEL_USAGE_GAP_WIDTH + MODEL_USAGE_USED_WIDTH
}

fn format_model_usage_bar_line(bar: &str, used_percent: f64) -> String {
    let gap = " ".repeat(MODEL_USAGE_GAP_WIDTH);
    let used_label = format_used_percent(used_percent);
    format!(
        "{bar}{gap}{used_label:>used_width$}",
        used_width = MODEL_USAGE_USED_WIDTH
    )
}

fn format_model_usage_reset_line(line: &str) -> String {
    line.to_string()
}

fn print_model_usage(quota: &UserQuotaSummaryResponse, show_timezone: bool) {
    let model_buckets = crate::agy::model_tier::build_model_buckets(quota);
    if model_buckets.is_empty() {
        return;
    }

    let tiers = crate::agy::model_tier::aggregate_by_tier(&model_buckets);
    if tiers.is_empty() {
        return;
    }

    let model_bar_width = model_usage_bar_width(terminal_columns());
    let section_width = model_usage_section_width(model_bar_width);

    println!();
    println!("{}", "─".repeat(section_width).dimmed());
    println!("{}", "Model Usage".bold());
    println!();

    for (index, tu) in tiers.iter().enumerate() {
        if index > 0 {
            println!();
        }

        let used_percent = used_percent_from_remaining_fraction(tu.remaining_fraction);
        let bar = used_progress_bar(used_percent, model_bar_width);
        let name = tu.tier.display_name();
        println!("{}", name.bold());
        println!("{}", format_model_usage_bar_line(&bar, used_percent));

        if let Some(reset_time) = tu.reset_time.as_deref() {
            let line = format_model_reset(reset_time, show_timezone);
            println!("{}", format_model_usage_reset_line(&line).dimmed());
        }
    }
}

// ── Group / bucket view ─────────────────────────────────────────────

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

    let standalone_buckets: Vec<_> = quota
        .buckets
        .iter()
        .filter(|bucket| should_print_standalone_bucket(bucket))
        .collect();

    if !standalone_buckets.is_empty() {
        if !first_group {
            println!();
        }
        for (index, bucket) in standalone_buckets.iter().enumerate() {
            if index > 0 {
                println!();
            }
            print_bucket(bucket, show_timezone);
        }
    }
}

pub async fn run(show_timezone: bool) {
    if let Err(e) = render(show_timezone).await {
        status::print_provider_error(Provider::Antigravity, &e);
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
    print_model_usage(&quota, show_timezone);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_used_percent_from_remaining_fraction() {
        assert_eq!(used_percent_from_remaining_fraction(1.0), 0.0);
        assert!((used_percent_from_remaining_fraction(0.9207936) - 7.92064).abs() < 1e-9);
        assert_eq!(used_percent_from_remaining_fraction(0.0), 100.0);
    }

    #[test]
    fn test_used_percent_from_remaining_fraction_clamps() {
        assert_eq!(used_percent_from_remaining_fraction(1.5), 0.0);
        assert_eq!(used_percent_from_remaining_fraction(-0.25), 100.0);
        assert_eq!(used_percent_from_remaining_fraction(f64::NAN), 0.0);
    }

    #[test]
    fn test_format_used_percent() {
        assert_eq!(format_used_percent(7.92064), "7.92% used");
        assert_eq!(format_used_percent(100.0), "100.00% used");
    }

    #[test]
    fn test_format_remaining_amount_avoids_remaining_copy() {
        assert_eq!(format_remaining_amount(42), "42 available");
    }

    #[test]
    fn test_used_progress_bar_chars() {
        assert_eq!(used_progress_bar_chars(20.0, 10), "██░░░░░░░░");
    }

    #[test]
    fn test_bucket_label_uses_display_name() {
        let bucket = QuotaSummaryBucket {
            model_id: None,
            display_name: Some("Weekly Limit".to_string()),
            remaining_fraction: Some(0.5),
            remaining_amount: None,
            disabled: None,
            reset_time: None,
        };

        assert_eq!(bucket_label(&bucket), "Weekly Limit");
    }

    #[test]
    fn test_model_buckets_are_not_printed_as_standalone_quota_buckets() {
        let mut bucket = QuotaSummaryBucket {
            model_id: Some("gemini-3.1-pro-preview".to_string()),
            display_name: None,
            remaining_fraction: Some(0.5),
            remaining_amount: None,
            disabled: None,
            reset_time: None,
        };

        assert!(!should_print_standalone_bucket(&bucket));
        bucket.model_id = None;
        assert!(should_print_standalone_bucket(&bucket));
    }

    #[test]
    fn test_format_reset_time_invalid_returns_original() {
        assert_eq!(
            format_reset_time_with_options("not-a-date", false),
            "not-a-date"
        );
    }

    #[test]
    fn test_model_usage_rows_follow_bucket_block_layout() {
        let bar_width = 20;

        let pro_row = format_model_usage_bar_line(&used_progress_bar_chars(8.0, bar_width), 8.0);
        let gpt_row =
            format_model_usage_bar_line(&used_progress_bar_chars(100.0, bar_width), 100.0);
        let reset_row = format_model_usage_reset_line("Resets: 7:30pm, 15m left");

        assert_eq!(pro_row.find(FILL_CHAR), Some(0));
        assert_eq!(gpt_row.find(FILL_CHAR), Some(0));
        assert_eq!(reset_row.find("Resets:"), Some(0));
        assert_eq!(pro_row.len(), gpt_row.len());
        assert!(!pro_row.starts_with("Pro"));
        assert!(pro_row.ends_with("8.00% used"));
        assert!(gpt_row.ends_with("100.00% used"));
    }

    #[test]
    fn test_model_usage_bar_width_has_readable_bounds() {
        assert_eq!(model_usage_bar_width(120), 50);
        assert_eq!(model_usage_bar_width(50), 40);
        assert_eq!(model_usage_bar_width(30), 20);
    }
}
