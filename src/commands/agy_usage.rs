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

pub async fn run_json(show_timezone: bool) {
    let snapshot = crate::snapshot::ProviderSnapshot::from_result(
        Provider::Antigravity,
        snapshot(show_timezone).await,
    );
    crate::commands::usage_all::print_json(vec![snapshot]);
}

fn is_auth_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authentication failed")
}

async fn fetch_quota_with_recovery(
    session: crate::agy::auth::AntigravitySession,
    user_agent: &str,
) -> Result<UserQuotaSummaryResponse, String> {
    match crate::agy::api::fetch_user_quota_summary(&session.access_token, user_agent).await {
        Ok(quota) => Ok(quota),
        Err(e) if is_auth_error(&e) => {
            let refreshed = crate::agy::auth::refresh_session(&session, user_agent).await?;
            crate::agy::api::fetch_user_quota_summary(&refreshed.access_token, user_agent).await
        }
        Err(e) => Err(e),
    }
}

// --- Snapshot support (JSON output / claudex-bar). Keep in sync with the
// print_* functions above; see src/snapshot.rs.

/// Reset line printed by `print_bucket` below every quota bucket.
fn bucket_reset_line(reset_time: &str, show_timezone: bool) -> String {
    let reset_str = format_reset_time_with_options(reset_time, show_timezone);
    match time_remaining(reset_time) {
        Some(rem) => format!("Refreshes {reset_str}, {rem} left"),
        None => format!("Refreshes {reset_str}"),
    }
}

/// Content rows of `print_bucket` without its leading bold label line.
fn bucket_content_rows(
    bucket: &QuotaSummaryBucket,
    show_timezone: bool,
) -> Vec<crate::snapshot::Row> {
    let reset_detail = || {
        bucket
            .reset_time
            .as_deref()
            .map(|reset_time| bucket_reset_line(reset_time, show_timezone))
    };

    if bucket.disabled.unwrap_or(false) {
        let mut rows = vec![crate::snapshot::Row::text("Disabled")];
        if let Some(detail) = reset_detail() {
            rows.push(crate::snapshot::Row::text(detail));
        }
        return rows;
    }

    if let Some(remaining_fraction) = bucket.remaining_fraction {
        let used_percent = used_percent_from_remaining_fraction(remaining_fraction);
        return vec![crate::snapshot::Row::bar(
            used_percent,
            format_used_percent(used_percent),
            reset_detail(),
            bucket.reset_time.clone(),
        )];
    }

    let mut rows = vec![match bucket.remaining_amount {
        Some(remaining_amount) => {
            crate::snapshot::Row::text(format_remaining_amount(remaining_amount))
        }
        None => crate::snapshot::Row::text("Quota amount was not returned."),
    }];
    if let Some(detail) = reset_detail() {
        rows.push(crate::snapshot::Row::text(detail));
    }
    rows
}

/// One quota bucket as printed by `print_bucket`: bold label, then content rows.
fn bucket_rows(bucket: &QuotaSummaryBucket, show_timezone: bool) -> Vec<crate::snapshot::Row> {
    let mut rows = vec![crate::snapshot::Row::text(bucket_label(bucket))];
    rows.extend(bucket_content_rows(bucket, show_timezone));
    rows
}

fn group_block(
    group: &crate::agy::api::QuotaSummaryGroup,
    show_timezone: bool,
) -> crate::snapshot::Block {
    let mut rows = Vec::new();
    if let Some(description) = group
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        rows.push(crate::snapshot::Row::text(description));
    }
    for bucket in &group.buckets {
        rows.extend(bucket_rows(bucket, show_timezone));
    }
    crate::snapshot::Block::titled(group.display_name.clone(), rows)
}

/// "Model Usage" summary section, aggregated per tier like `print_model_usage`.
fn model_usage_block(
    quota: &UserQuotaSummaryResponse,
    show_timezone: bool,
) -> Option<crate::snapshot::Block> {
    let model_buckets = crate::agy::model_tier::build_model_buckets(quota);
    if model_buckets.is_empty() {
        return None;
    }
    let tiers = crate::agy::model_tier::aggregate_by_tier(&model_buckets);
    if tiers.is_empty() {
        return None;
    }

    let mut rows = Vec::new();
    for tu in &tiers {
        let used_percent = used_percent_from_remaining_fraction(tu.remaining_fraction);
        rows.push(crate::snapshot::Row::text(tu.tier.display_name()));
        rows.push(crate::snapshot::Row::bar(
            used_percent,
            format_used_percent(used_percent),
            tu.reset_time
                .as_deref()
                .map(|reset_time| format_model_reset(reset_time, show_timezone)),
            tu.reset_time.clone(),
        ));
    }

    Some(crate::snapshot::Block::titled("Model Usage", rows))
}

fn build_blocks(
    quota: &UserQuotaSummaryResponse,
    show_timezone: bool,
) -> Vec<crate::snapshot::Block> {
    if !has_quota_data(quota) {
        return vec![crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text("Antigravity quota data is not available for your account."),
        ])];
    }

    let mut blocks = Vec::new();
    if let Some(subscription) = quota.subscription.as_deref() {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text(format!("Subscription: {subscription}")),
        ]));
    }

    for group in &quota.groups {
        blocks.push(group_block(group, show_timezone));
    }

    for bucket in quota
        .buckets
        .iter()
        .filter(|bucket| should_print_standalone_bucket(bucket))
    {
        blocks.push(crate::snapshot::Block::titled(
            bucket_label(bucket),
            bucket_content_rows(bucket, show_timezone),
        ));
    }

    if let Some(block) = model_usage_block(quota, show_timezone) {
        blocks.push(block);
    }

    blocks
}

pub async fn snapshot(show_timezone: bool) -> Result<crate::snapshot::ProviderSnapshot, String> {
    let session = crate::agy::auth::read_session().await?;
    let user_agent = crate::agy::auth::agy_user_agent();
    let quota = fetch_quota_with_recovery(session, &user_agent).await?;

    Ok(crate::snapshot::ProviderSnapshot::ok(
        Provider::Antigravity,
        build_blocks(&quota, show_timezone),
    ))
}

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let session = crate::agy::auth::read_session().await?;
    let user_agent = crate::agy::auth::agy_user_agent();
    let quota = fetch_quota_with_recovery(session, &user_agent).await?;

    if !has_quota_data(&quota) {
        println!("Antigravity quota data is not available for your account.");
        return Ok(());
    }

    if let Some(subscription) = quota.subscription.as_deref() {
        println!("{} {}\n", "Subscription:".bold(), subscription);
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

    #[test]
    fn test_is_auth_error_matches_antigravity_token_rejection() {
        assert!(is_auth_error(
            "authentication failed — try restarting Antigravity to refresh your Google login"
        ));
        assert!(!is_auth_error(
            "failed to fetch Antigravity quota data: HTTP 500"
        ));
    }

    fn quota_from_json(json: &str) -> UserQuotaSummaryResponse {
        serde_json::from_str(json).unwrap()
    }

    fn assert_text(row: &crate::snapshot::Row, expected: &str) {
        match row {
            crate::snapshot::Row::Text { text } => assert_eq!(text, expected),
            _ => panic!("expected text row, got {row:?}"),
        }
    }

    fn assert_bar(row: &crate::snapshot::Row, percent: f64, text: &str, resets_at: Option<&str>) {
        match row {
            crate::snapshot::Row::Bar {
                percent: actual,
                text: actual_text,
                resets_at: actual_resets_at,
                ..
            } => {
                assert!(
                    (actual - percent).abs() < 1e-9,
                    "percent {actual} != {percent}"
                );
                assert_eq!(actual_text, text);
                assert_eq!(actual_resets_at.as_deref(), resets_at);
            }
            _ => panic!("expected bar row, got {row:?}"),
        }
    }

    fn bar_detail(row: &crate::snapshot::Row) -> Option<&str> {
        match row {
            crate::snapshot::Row::Bar { detail, .. } => detail.as_deref(),
            _ => panic!("expected bar row, got {row:?}"),
        }
    }

    #[test]
    fn build_blocks_mirrors_groups_standalone_and_model_usage() {
        let mut quota = quota_from_json(
            r#"{
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "description": "Models within this group: Gemini Flash, Gemini Pro",
                        "buckets": [
                            {"displayName": "Weekly Limit", "remainingFraction": 0.9207936, "resetTime": "2099-06-19T08:46:00Z"},
                            {"displayName": "Five Hour Limit", "remainingFraction": 1, "resetTime": "2099-06-16T08:39:13Z"}
                        ]
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
                        "buckets": [
                            {"displayName": "Weekly Limit", "remainingFraction": 0.66, "resetTime": "2099-06-23T01:30:12Z"},
                            {"displayName": "Five Hour Limit", "remainingFraction": 0.0, "disabled": true}
                        ]
                    }
                ],
                "buckets": [
                    {"modelId": "gemini-3.1-pro-preview", "remainingFraction": 0.5856, "resetTime": "2099-06-16T17:20:00Z"},
                    {"modelId": "gemini-3-flash-preview", "remainingFraction": 0.99, "resetTime": "2099-06-16T17:57:00Z"},
                    {"displayName": "Weekly Limit", "remainingFraction": 0.5, "resetTime": "2099-06-20T00:00:00Z"}
                ]
            }"#,
        );
        quota.subscription = Some("Antigravity".to_string());

        let blocks = build_blocks(&quota, false);

        assert_eq!(blocks.len(), 5);

        // Subscription line mirrors render's "Subscription: X".
        assert_eq!(blocks[0].title, None);
        assert_text(&blocks[0].rows[0], "Subscription: Antigravity");

        // Group block: title is the group name, description is the first row,
        // then each bucket as label row + content rows.
        assert_eq!(blocks[1].title.as_deref(), Some("Gemini Models"));
        let gemini = &blocks[1].rows;
        assert_eq!(gemini.len(), 5);
        assert_text(
            &gemini[0],
            "Models within this group: Gemini Flash, Gemini Pro",
        );
        assert_text(&gemini[1], "Weekly Limit");
        assert_bar(
            &gemini[2],
            7.92064,
            "7.92% used",
            Some("2099-06-19T08:46:00Z"),
        );
        assert!(bar_detail(&gemini[2]).unwrap().starts_with("Refreshes "));
        assert_text(&gemini[3], "Five Hour Limit");
        assert_bar(&gemini[4], 0.0, "0.00% used", Some("2099-06-16T08:39:13Z"));

        // Disabled buckets stay text rows, like print_bucket's "Disabled" line.
        assert_eq!(blocks[2].title.as_deref(), Some("Claude and GPT models"));
        let third_party = &blocks[2].rows;
        assert_eq!(third_party.len(), 5);
        assert_text(
            &third_party[0],
            "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
        );
        assert_text(&third_party[1], "Weekly Limit");
        assert_bar(
            &third_party[2],
            34.0,
            "34.00% used",
            Some("2099-06-23T01:30:12Z"),
        );
        assert_text(&third_party[3], "Five Hour Limit");
        assert_text(&third_party[4], "Disabled");

        // A standalone bucket becomes its own block titled with its label.
        assert_eq!(blocks[3].title.as_deref(), Some("Weekly Limit"));
        assert_eq!(blocks[3].rows.len(), 1);
        assert_bar(
            &blocks[3].rows[0],
            50.0,
            "50.00% used",
            Some("2099-06-20T00:00:00Z"),
        );

        // Model Usage summary: tier name row followed by its bar, detail uses
        // the "Resets: ..." copy of print_model_usage (not "Refreshes ...").
        assert_eq!(blocks[4].title.as_deref(), Some("Model Usage"));
        let usage = &blocks[4].rows;
        assert_eq!(usage.len(), 4);
        assert_text(&usage[0], "Pro");
        assert_bar(
            &usage[1],
            41.44,
            "41.44% used",
            Some("2099-06-16T17:20:00Z"),
        );
        assert!(bar_detail(&usage[1]).unwrap().starts_with("Resets: "));
        assert_text(&usage[2], "Flash");
        assert_bar(&usage[3], 1.0, "1.00% used", Some("2099-06-16T17:57:00Z"));
    }

    #[test]
    fn build_blocks_reports_missing_quota_data() {
        let mut quota = quota_from_json(r#"{}"#);
        // render returns before printing the subscription when there is no data.
        quota.subscription = Some("Antigravity".to_string());

        let blocks = build_blocks(&quota, false);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title, None);
        assert_text(
            &blocks[0].rows[0],
            "Antigravity quota data is not available for your account.",
        );
    }

    #[test]
    fn build_blocks_omits_model_usage_without_model_buckets() {
        let quota = quota_from_json(
            r#"{
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "description": "Models within this group: Gemini Flash, Gemini Pro",
                        "buckets": [
                            {"displayName": "Weekly Limit", "remainingFraction": 0.9207936, "resetTime": "2099-06-19T08:46:00Z"}
                        ]
                    }
                ]
            }"#,
        );

        let blocks = build_blocks(&quota, false);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title.as_deref(), Some("Gemini Models"));
    }

    #[test]
    fn build_blocks_disabled_and_amount_buckets_stay_text_rows() {
        let quota = quota_from_json(
            r#"{
                "groups": [
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            {"displayName": "Five Hour Limit", "disabled": true, "resetTime": "2099-06-16T08:39:13Z"},
                            {"displayName": "Burst Quota", "remainingAmount": 42}
                        ]
                    }
                ]
            }"#,
        );

        let blocks = build_blocks(&quota, false);

        // No description row: the first row is the bucket label.
        assert_eq!(blocks.len(), 1);
        let rows = &blocks[0].rows;
        assert_eq!(rows.len(), 5);
        assert_text(&rows[0], "Five Hour Limit");
        assert_text(&rows[1], "Disabled");
        match &rows[2] {
            crate::snapshot::Row::Text { text } => assert!(text.starts_with("Refreshes ")),
            _ => panic!("expected text row"),
        }
        assert_text(&rows[3], "Burst Quota");
        assert_text(&rows[4], "42 available");
    }
}
