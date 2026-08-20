use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};
use crate::commands::usage::format_duration_short;
use crate::kimi::api::{ManagedUsage, UsageRow};

const FILL_CHAR: char = '\u{2588}';
const EMPTY_CHAR: char = '\u{2591}';

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

fn used_percent(row: &UsageRow) -> f64 {
    if row.limit <= 0 {
        return 0.0;
    }

    ((row.used as f64 / row.limit as f64) * 100.0).clamp(0.0, 100.0)
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

fn format_reset_clock(reset_at: &str, show_timezone: bool) -> String {
    let Ok(parsed) = DateTime::parse_from_rfc3339(reset_at) else {
        return reset_at.to_string();
    };
    format_local(
        parsed.with_timezone(&Local),
        Local::now().date_naive(),
        show_timezone,
    )
}

fn time_remaining(reset_at: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(reset_at).ok()?;
    let secs = parsed.signed_duration_since(Local::now()).num_seconds();
    if secs <= 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

fn reset_detail(row: &UsageRow, show_timezone: bool) -> Option<String> {
    let reset_at = row.reset_at.as_deref()?;
    let clock = format_reset_clock(reset_at, show_timezone);
    if clock.is_empty() {
        return None;
    }
    Some(match time_remaining(reset_at) {
        Some(rem) => format!("Resets {clock}, {rem} left"),
        None => format!("Resets {clock}"),
    })
}

fn print_row(row: &UsageRow, show_timezone: bool) {
    let used_percent = used_percent(row);

    println!("{}", row.label.bold());
    println!(
        "{} {:.0}% used",
        progress_bar(used_percent, bar_width()),
        used_percent
    );

    if let Some(detail) = reset_detail(row, show_timezone) {
        println!("{}", detail.dimmed());
    }
}

pub async fn run(show_timezone: bool) {
    if let Err(e) = render(show_timezone).await {
        status::print_provider_error(Provider::Kimi, &e);
        std::process::exit(1);
    }
}

pub async fn run_json(show_timezone: bool) {
    let snapshot = crate::snapshot::ProviderSnapshot::from_result(
        Provider::Kimi,
        snapshot(show_timezone).await,
    );
    crate::commands::usage_all::print_json(vec![snapshot]);
}

// --- Snapshot support (JSON output / claudex-bar). Keep in sync with
// print_usage/print_row below; see src/snapshot.rs.

fn bar_row(row: &UsageRow, show_timezone: bool) -> crate::snapshot::Row {
    let used_percent = used_percent(row);

    crate::snapshot::Row::bar(
        used_percent,
        format!("{used_percent:.0}% used"),
        reset_detail(row, show_timezone),
        row.reset_at.clone(),
    )
}

fn usage_rows(usage: &ManagedUsage) -> Vec<&UsageRow> {
    usage
        .limits
        .iter()
        .chain(usage.summary.iter())
        .chain(usage.monthly.iter())
        .collect()
}

fn usage_row_block(row: &UsageRow, show_timezone: bool) -> crate::snapshot::Block {
    crate::snapshot::Block::titled(row.label.clone(), vec![bar_row(row, show_timezone)])
}

fn build_blocks(usage: &ManagedUsage, show_timezone: bool) -> Vec<crate::snapshot::Block> {
    let mut blocks = Vec::new();

    if let Some(subscription) = &usage.subscription {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text(format!("Subscription: {subscription}")),
        ]));
    }

    let rows: Vec<&UsageRow> = usage_rows(usage);

    if rows.is_empty() {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text("Kimi Code usage data is not available for your plan."),
        ]));
        return blocks;
    }

    blocks.extend(
        rows.into_iter()
            .map(|row| usage_row_block(row, show_timezone)),
    );
    blocks
}

pub async fn snapshot(show_timezone: bool) -> Result<crate::snapshot::ProviderSnapshot, String> {
    let creds = crate::kimi::auth::read_credentials()?;
    let usage = fetch_usage_with_recovery(creds).await?;

    Ok(crate::snapshot::ProviderSnapshot::ok(
        Provider::Kimi,
        build_blocks(&usage, show_timezone),
    ))
}

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let creds = crate::kimi::auth::read_credentials()?;
    let usage = fetch_usage_with_recovery(creds).await?;

    print_usage(&usage, show_timezone);
    Ok(())
}

fn is_auth_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authentication failed")
}

async fn fetch_usage_with_recovery(
    creds: crate::kimi::auth::KimiCredentials,
) -> Result<ManagedUsage, String> {
    let mut usage = match crate::kimi::api::fetch_usage(&creds.access_token).await {
        Ok(usage) => usage,
        Err(e) if is_auth_error(&e) => {
            let refreshed = crate::kimi::auth::refresh_credentials(&creds).await?;
            crate::kimi::api::fetch_usage(&refreshed.access_token).await?
        }
        Err(e) => return Err(e),
    };
    usage.monthly = crate::kimi::web::fetch_monthly_limit(usage.user_id.as_deref()).await;
    Ok(usage)
}

fn print_usage(usage: &ManagedUsage, show_timezone: bool) {
    let rows: Vec<&UsageRow> = usage_rows(usage);

    if let Some(subscription) = &usage.subscription {
        println!("{} {}\n", "Subscription:".bold(), subscription);
    }

    if rows.is_empty() {
        println!("Kimi Code usage data is not available for your plan.");
        return;
    }

    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_row(row, show_timezone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_clamped_to_full_bar() {
        let row = UsageRow {
            label: "5h limit".to_string(),
            used: 120,
            limit: 100,
            reset_at: None,
        };

        assert_eq!(used_percent(&row), 100.0);
    }

    #[test]
    fn percent_is_zero_without_limit() {
        let row = UsageRow {
            label: "Weekly limit".to_string(),
            used: 1,
            limit: 0,
            reset_at: None,
        };

        assert_eq!(used_percent(&row), 0.0);
    }

    fn sample_usage() -> ManagedUsage {
        ManagedUsage {
            subscription: Some("Allegro".to_string()),
            user_id: Some("co0js84udu6f887phqfg".to_string()),
            monthly: Some(UsageRow {
                label: "Monthly limit".to_string(),
                used: 11,
                limit: 100,
                reset_at: Some("2099-09-17T00:52:31Z".to_string()),
            }),
            summary: Some(UsageRow {
                label: "Weekly limit".to_string(),
                used: 2,
                limit: 100,
                reset_at: Some("2099-08-21T00:52:31Z".to_string()),
            }),
            limits: vec![UsageRow {
                label: "5h limit".to_string(),
                used: 1,
                limit: 100,
                reset_at: Some("2099-08-20T09:52:31Z".to_string()),
            }],
        }
    }

    fn assert_reset_bar(row: &crate::snapshot::Row, percent: f64, resets_at: &str) {
        match row {
            crate::snapshot::Row::Bar {
                percent: got_percent,
                text,
                detail,
                resets_at: got_reset,
            } => {
                assert_eq!(*got_percent, percent);
                assert_eq!(text, &format!("{percent:.0}% used"));
                let detail = detail.as_deref().expect("reset detail");
                assert!(detail.starts_with("Resets "), "detail={detail}");
                assert!(detail.contains(" left"), "detail={detail}");
                assert!(
                    detail.contains("am") || detail.contains("pm"),
                    "detail should include a clock time: {detail}"
                );
                assert_eq!(got_reset.as_deref(), Some(resets_at));
            }
            _ => panic!("expected bar row"),
        }
    }

    #[test]
    fn build_blocks_includes_subscription_and_limit_blocks() {
        let blocks = build_blocks(&sample_usage(), false);

        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].title, None);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Subscription: Allegro"),
            _ => panic!("expected text row"),
        }

        assert_eq!(blocks[1].title.as_deref(), Some("5h limit"));
        assert_reset_bar(&blocks[1].rows[0], 1.0, "2099-08-20T09:52:31Z");
        assert_eq!(blocks[2].title.as_deref(), Some("Weekly limit"));
        assert_reset_bar(&blocks[2].rows[0], 2.0, "2099-08-21T00:52:31Z");
        assert_eq!(blocks[3].title.as_deref(), Some("Monthly limit"));
        assert_reset_bar(&blocks[3].rows[0], 11.0, "2099-09-17T00:52:31Z");
    }

    #[test]
    fn build_blocks_reports_unavailable_plan_without_rows() {
        let blocks = build_blocks(&ManagedUsage::default(), false);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].title, None);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => {
                assert_eq!(text, "Kimi Code usage data is not available for your plan.")
            }
            _ => panic!("expected text row"),
        }
    }

    #[test]
    fn build_blocks_keeps_subscription_before_unavailable_message() {
        let usage = ManagedUsage {
            subscription: Some("Andante".to_string()),
            ..ManagedUsage::default()
        };
        let blocks = build_blocks(&usage, false);

        assert_eq!(blocks.len(), 2);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Subscription: Andante"),
            _ => panic!("expected text row"),
        }
        match &blocks[1].rows[0] {
            crate::snapshot::Row::Text { text } => {
                assert_eq!(text, "Kimi Code usage data is not available for your plan.")
            }
            _ => panic!("expected text row"),
        }
    }

    #[test]
    fn build_blocks_without_reset_omits_detail() {
        let usage = ManagedUsage {
            subscription: None,
            summary: Some(UsageRow {
                label: "Weekly limit".to_string(),
                used: 5,
                limit: 0,
                reset_at: None,
            }),
            limits: Vec::new(),
            ..ManagedUsage::default()
        };
        let blocks = build_blocks(&usage, false);

        assert_eq!(blocks.len(), 1);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Bar {
                percent,
                text,
                detail,
                ..
            } => {
                assert_eq!(*percent, 0.0);
                assert_eq!(text, "0% used");
                assert!(detail.is_none());
            }
            _ => panic!("expected bar row"),
        }
    }
}
