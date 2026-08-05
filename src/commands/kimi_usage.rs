use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};
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

fn print_row(row: &UsageRow) {
    let used_percent = used_percent(row);

    println!("{}", row.label.bold());
    println!(
        "{} {:.0}% used",
        progress_bar(used_percent, bar_width()),
        used_percent
    );

    let mut detail = if row.limit > 0 {
        format!("Used {} / {}", row.used, row.limit)
    } else {
        format!("Used {}", row.used)
    };

    if let Some(reset_hint) = &row.reset_hint {
        detail.push_str("; ");
        detail.push_str(reset_hint);
    }

    println!("{}", detail.dimmed());
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

fn bar_row(row: &UsageRow) -> crate::snapshot::Row {
    let used_percent = used_percent(row);

    let mut detail = if row.limit > 0 {
        format!("Used {} / {}", row.used, row.limit)
    } else {
        format!("Used {}", row.used)
    };
    if let Some(reset_hint) = &row.reset_hint {
        detail.push_str("; ");
        detail.push_str(reset_hint);
    }

    crate::snapshot::Row::bar(
        used_percent,
        format!("{used_percent:.0}% used"),
        Some(detail),
        None,
    )
}

fn usage_row_block(row: &UsageRow) -> crate::snapshot::Block {
    crate::snapshot::Block::titled(row.label.clone(), vec![bar_row(row)])
}

fn build_blocks(usage: &ManagedUsage) -> Vec<crate::snapshot::Block> {
    let mut blocks = Vec::new();

    if let Some(subscription) = &usage.subscription {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text(format!("Subscription: {subscription}")),
        ]));
    }

    let rows: Vec<&UsageRow> = usage.summary.iter().chain(usage.limits.iter()).collect();

    if rows.is_empty() {
        blocks.push(crate::snapshot::Block::untitled(vec![
            crate::snapshot::Row::text("Kimi Code usage data is not available for your plan."),
        ]));
        return blocks;
    }

    blocks.extend(rows.into_iter().map(usage_row_block));
    blocks
}

pub async fn snapshot(_show_timezone: bool) -> Result<crate::snapshot::ProviderSnapshot, String> {
    let creds = crate::kimi::auth::read_credentials()?;
    let usage = fetch_usage_with_recovery(creds).await?;

    Ok(crate::snapshot::ProviderSnapshot::ok(
        Provider::Kimi,
        build_blocks(&usage),
    ))
}

pub async fn render(_show_timezone: bool) -> Result<(), String> {
    let creds = crate::kimi::auth::read_credentials()?;
    let usage = fetch_usage_with_recovery(creds).await?;

    print_usage(&usage);
    Ok(())
}

fn is_auth_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authentication failed")
}

async fn fetch_usage_with_recovery(
    creds: crate::kimi::auth::KimiCredentials,
) -> Result<ManagedUsage, String> {
    match crate::kimi::api::fetch_usage(&creds.access_token).await {
        Ok(usage) => Ok(usage),
        Err(e) if is_auth_error(&e) => {
            let refreshed = crate::kimi::auth::refresh_credentials(&creds).await?;
            crate::kimi::api::fetch_usage(&refreshed.access_token).await
        }
        Err(e) => Err(e),
    }
}

fn print_usage(usage: &ManagedUsage) {
    let rows: Vec<&UsageRow> = usage.summary.iter().chain(usage.limits.iter()).collect();

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
        print_row(row);
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
            reset_hint: None,
        };

        assert_eq!(used_percent(&row), 100.0);
    }

    #[test]
    fn percent_is_zero_without_limit() {
        let row = UsageRow {
            label: "Weekly limit".to_string(),
            used: 1,
            limit: 0,
            reset_hint: None,
        };

        assert_eq!(used_percent(&row), 0.0);
    }

    fn sample_usage() -> ManagedUsage {
        ManagedUsage {
            subscription: Some("Allegro".to_string()),
            summary: Some(UsageRow {
                label: "Weekly limit".to_string(),
                used: 2,
                limit: 100,
                reset_hint: Some("resets in 7d".to_string()),
            }),
            limits: vec![UsageRow {
                label: "5h limit".to_string(),
                used: 1,
                limit: 100,
                reset_hint: Some("resets in 5h".to_string()),
            }],
        }
    }

    #[test]
    fn build_blocks_includes_subscription_and_limit_blocks() {
        let blocks = build_blocks(&sample_usage());

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].title, None);
        match &blocks[0].rows[0] {
            crate::snapshot::Row::Text { text } => assert_eq!(text, "Subscription: Allegro"),
            _ => panic!("expected text row"),
        }

        assert_eq!(blocks[1].title.as_deref(), Some("Weekly limit"));
        match &blocks[1].rows[0] {
            crate::snapshot::Row::Bar {
                percent,
                text,
                detail,
                resets_at,
            } => {
                assert_eq!(*percent, 2.0);
                assert_eq!(text, "2% used");
                assert_eq!(detail.as_deref(), Some("Used 2 / 100; resets in 7d"));
                assert!(resets_at.is_none());
            }
            _ => panic!("expected bar row"),
        }

        assert_eq!(blocks[2].title.as_deref(), Some("5h limit"));
        match &blocks[2].rows[0] {
            crate::snapshot::Row::Bar {
                percent, detail, ..
            } => {
                assert_eq!(*percent, 1.0);
                assert_eq!(detail.as_deref(), Some("Used 1 / 100; resets in 5h"));
            }
            _ => panic!("expected bar row"),
        }
    }

    #[test]
    fn build_blocks_reports_unavailable_plan_without_rows() {
        let blocks = build_blocks(&ManagedUsage::default());

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
        let blocks = build_blocks(&usage);

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
    fn build_blocks_without_limit_omits_denominator() {
        let usage = ManagedUsage {
            subscription: None,
            summary: Some(UsageRow {
                label: "Weekly limit".to_string(),
                used: 5,
                limit: 0,
                reset_hint: None,
            }),
            limits: Vec::new(),
        };
        let blocks = build_blocks(&usage);

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
                assert_eq!(detail.as_deref(), Some("Used 5"));
            }
            _ => panic!("expected bar row"),
        }
    }
}
