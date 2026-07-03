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

pub async fn render(_show_timezone: bool) -> Result<(), String> {
    let creds = crate::kimi::auth::read_credentials()?;
    let usage = crate::kimi::api::fetch_usage(&creds.access_token).await?;

    print_usage(&usage);
    Ok(())
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
}
