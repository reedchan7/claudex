use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::commands::status::{self, Provider};
use crate::grok::api::{
    BillingConfig, BillingResponse, MoneyVal, RawBillingConfig, RawBillingResponse, UserResponse,
};

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

fn format_reset_time(resets_at: &str, show_timezone: bool) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(resets_at) else {
        return resets_at.to_string();
    };
    let local_dt = dt.with_timezone(&Local);
    format_local(local_dt, Local::now().date_naive(), show_timezone)
}

fn time_remaining(resets_at: &str) -> Option<String> {
    let dt = DateTime::parse_from_rfc3339(resets_at).ok()?;
    let secs = dt.signed_duration_since(Local::now()).num_seconds();
    if secs < 0 {
        return None;
    }
    Some(format_duration_short(secs))
}

fn period_label(period_type: Option<&str>) -> &'static str {
    match period_type {
        Some(t) if t.contains("WEEKLY") => "Current week",
        Some(t) if t.contains("MONTHLY") || t.contains("MONTH") => "Current month",
        Some(t) if t.contains("DAILY") || t.contains("DAY") => "Current day",
        _ => "Current period",
    }
}

fn product_label(product: &str) -> String {
    match product {
        "GrokBuild" => "Grok Build".to_string(),
        other => {
            // Split CamelCase / snake_case into words.
            let mut out = String::new();
            for (i, ch) in other.chars().enumerate() {
                if i > 0 && (ch.is_uppercase() || ch == '_') {
                    if ch != '_' {
                        out.push(' ');
                        out.push(ch);
                    } else {
                        out.push(' ');
                    }
                } else if ch != '_' {
                    out.push(ch);
                }
            }
            out
        }
    }
}

fn money_val(val: &Option<MoneyVal>) -> Option<f64> {
    val.as_ref().and_then(|m| m.val)
}

fn print_usage_bar(title: &str, used_percent: f64, resets_at: Option<&str>, show_timezone: bool) {
    println!("{}", title.bold());
    println!(
        "{} {:.0}% used",
        progress_bar(used_percent, bar_width()),
        used_percent
    );
    if let Some(resets_at) = resets_at {
        let reset_str = format_reset_time(resets_at, show_timezone);
        let line = match time_remaining(resets_at) {
            Some(rem) => format!("Resets {reset_str}, {rem} left"),
            None => format!("Resets {reset_str}"),
        };
        println!("{}", line.dimmed());
    }
}

/// Weekly usage to render from the credits (`/billing?format=credits`) view.
#[derive(Debug, PartialEq)]
enum CreditsUsage {
    /// Per-product bars, when `productUsage` is present.
    Products(Vec<(String, f64)>),
    /// A single aggregate percentage.
    Aggregate(f64),
}

/// Resolve the weekly usage to render from the credits billing view.
///
/// Grok's `/billing?format=credits` omits `creditUsagePercent` and
/// `productUsage` when current usage is 0%. A present `currentPeriod` thus
/// means "0% this period" — not "no data", and not a reason to fall back to
/// the monthly billing view. Returns `None` only when there is no
/// `currentPeriod` at all.
fn credits_usage(config: &BillingConfig) -> Option<CreditsUsage> {
    // No current period → nothing to render.
    config.current_period.as_ref()?;

    let products: Vec<(String, f64)> = config
        .product_usage
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|p| {
            let name = p
                .product
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            p.usage_percent.map(|pct| (name, pct))
        })
        .collect();
    if !products.is_empty() {
        return Some(CreditsUsage::Products(products));
    }
    if let Some(pct) = config.credit_usage_percent {
        return Some(CreditsUsage::Aggregate(pct));
    }
    // Period present but no percent fields → usage is 0%.
    Some(CreditsUsage::Aggregate(0.0))
}

fn print_billing(
    config: &BillingConfig,
    raw_config: Option<&RawBillingConfig>,
    subscription: Option<&str>,
    show_timezone: bool,
) {
    if let Some(tier) = subscription
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            config
                .subscription_tier
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
    {
        println!("{} {}\n", "Subscription:".bold(), humanize_tier(tier));
    }

    // The credits view's `currentPeriod` is the real usage window for
    // SuperGrok (weekly). Its `end` is the weekly reset — never use the raw
    // view's monthly billing-period end for the usage bar.
    let weekly_resets_at = config
        .current_period
        .as_ref()
        .and_then(|p| p.end.as_deref())
        .or(config.billing_period_end.as_deref());
    let period = period_label(
        config
            .current_period
            .as_ref()
            .and_then(|p| p.period_type.as_deref()),
    );

    let mut printed = false;

    match credits_usage(config) {
        Some(CreditsUsage::Products(products)) => {
            for (i, (name, percent)) in products.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let title = format!("{period} ({})", product_label(name));
                print_usage_bar(&title, *percent, weekly_resets_at, show_timezone);
            }
            printed = true;
        }
        Some(CreditsUsage::Aggregate(percent)) => {
            print_usage_bar(period, percent, weekly_resets_at, show_timezone);
            printed = true;
        }
        None => {}
    }

    // Monthly billed usage (Grok Code) from the standard `/billing` view —
    // supplementary, never the main usage bar. Grok's own /usage does not
    // surface this view. Per xAI's billing docs the unit is USD; the API
    // exposes no explicit unit field, so label it as USD.
    if let Some(raw) = raw_config
        && let (Some(limit), Some(used)) = (money_val(&raw.monthly_limit), money_val(&raw.used))
        && limit > 0.0
    {
        if printed {
            println!();
        }
        // Unofficial monthly estimate from the /billing proxy. Grok only
        // exposes weekly limits, so this monthly figure is unverified and
        // shown only on opt-in (`--monthly`), labelled as an estimate.
        let percent = (used / limit * 100.0).min(100.0);
        let title = format!("Monthly estimate (USD) ({used:.0} / {limit:.0})");
        print_usage_bar(
            &title,
            percent,
            raw.billing_period_end.as_deref(),
            show_timezone,
        );
        println!(
            "{}",
            "Unofficial — Grok exposes only weekly limits; unit unconfirmed.".dimmed()
        );
        printed = true;
    }

    print_money_line(
        "On-demand used",
        money_val(&config.on_demand_used)
            .or_else(|| raw_config.and_then(|r| money_val(&r.on_demand_used))),
        &mut printed,
    );
    print_money_line(
        "On-demand cap",
        money_val(&config.on_demand_cap)
            .or_else(|| raw_config.and_then(|r| money_val(&r.on_demand_cap))),
        &mut printed,
    );
    print_money_line(
        "Prepaid balance",
        money_val(&config.prepaid_balance),
        &mut printed,
    );
    // monthlyLimit is shown above as part of "Monthly billed usage (USD)",
    // so it is not repeated as a standalone money line.

    if !printed {
        println!("Grok Build usage data is not available for your plan.");
    }
}

fn print_money_line(label: &str, amount: Option<f64>, printed: &mut bool) {
    let Some(amount) = amount else {
        return;
    };
    if amount == 0.0 {
        return;
    }
    if *printed {
        println!();
    }
    println!("{}", label.bold());
    println!("{}", format!("${amount:.2}").dimmed());
    *printed = true;
}

/// Map Grok's API / internal subscription codes to the consumer-facing plan
/// names from <https://x.ai/pricing>: Free, SuperGrok Lite, SuperGrok,
/// SuperGrok Heavy, Business, Enterprise (plus X Premium / X Premium+).
///
/// The `/user?include=subscription` field can return legacy codes such as
/// `GrokPro` for SuperGrok — never print those raw.
fn humanize_tier(tier: &str) -> String {
    let trimmed = tier
        .strip_prefix("SUBSCRIPTION_TIER_")
        .or_else(|| tier.strip_prefix("TIER_"))
        .unwrap_or(tier)
        .trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Normalize: strip spaces/underscores/hyphens, lower-case for matching.
    let key: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();

    match key.as_str() {
        // SuperGrok (standard paid) — API often uses the legacy code "GrokPro".
        "grokpro" | "pro" | "supergrok" | "super" => "SuperGrok".to_string(),
        // SuperGrok Heavy
        "supergrokheavy" | "grokheavy" | "heavy" | "superheavy" => "SuperGrok Heavy".to_string(),
        // SuperGrok Lite
        "supergroklite" | "groklite" | "lite" => "SuperGrok Lite".to_string(),
        // Free
        "free" | "grokfree" => "Free".to_string(),
        // X platform bundles
        "xpremium" | "premium" => "X Premium".to_string(),
        "xpremiumplus" | "premiumplus" | "xpremium+" => "X Premium+".to_string(),
        // Team / org
        "business" | "grokbusiness" => "Business".to_string(),
        "enterprise" | "grokenterprise" => "Enterprise".to_string(),
        _ => {
            // Unknown code: prefer readable CamelCase split over raw noise,
            // but keep SuperGrok-style compound names intact when possible.
            if !trimmed.contains('_')
                && !trimmed.contains('-')
                && trimmed.chars().any(|c| c.is_ascii_lowercase())
            {
                product_label(trimmed)
            } else {
                trimmed
                    .split(['_', '-'])
                    .filter(|part| !part.is_empty())
                    .map(|part| {
                        let lower = part.to_ascii_lowercase();
                        let mut chars = lower.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
}

pub async fn run(show_timezone: bool, show_monthly: bool) {
    if let Err(e) = render(show_timezone, show_monthly).await {
        status::print_provider_error(Provider::Grok, &e);
        std::process::exit(1);
    }
}

pub async fn render(show_timezone: bool, show_monthly: bool) -> Result<(), String> {
    let mut creds = crate::grok::auth::read_credentials()?;
    if creds.is_expired() {
        creds = crate::grok::auth::refresh_credentials(&creds).await?;
    }

    let (billing, raw_billing, user) = fetch_usage_with_recovery(creds, show_monthly).await?;
    let subscription = user
        .as_ref()
        .and_then(|u| u.subscription_tier.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let Some(config) = billing.config.as_ref() else {
        if let Some(tier) = subscription {
            println!("{} {}\n", "Subscription:".bold(), humanize_tier(tier));
        }
        println!("Grok Build usage data is not available for your plan.");
        return Ok(());
    };

    print_billing(
        config,
        raw_billing.config.as_ref(),
        subscription,
        show_timezone,
    );
    Ok(())
}

fn is_auth_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authentication failed")
}

async fn fetch_usage_with_recovery(
    creds: crate::grok::auth::GrokCredentials,
    show_monthly: bool,
) -> Result<(BillingResponse, RawBillingResponse, Option<UserResponse>), String> {
    match fetch_usage_triple(&creds.access_token, show_monthly).await {
        Ok(triple) => Ok(triple),
        Err(e) if is_auth_error(&e) => {
            let refreshed = crate::grok::auth::refresh_credentials(&creds).await?;
            fetch_usage_triple(&refreshed.access_token, show_monthly).await
        }
        Err(e) => Err(e),
    }
}

async fn fetch_usage_triple(
    access_token: &str,
    show_monthly: bool,
) -> Result<(BillingResponse, RawBillingResponse, Option<UserResponse>), String> {
    let billing = crate::grok::api::fetch_billing(access_token).await?;
    // The raw `/billing` view carries the unofficial monthly estimate. Only
    // fetch it when opted in (extra request) — Grok exposes weekly limits
    // officially, so the monthly figure stays off by default.
    let raw_billing = if show_monthly {
        match crate::grok::api::fetch_billing_raw(access_token).await {
            Ok(raw) => raw,
            Err(e) if is_auth_error(&e) => return Err(e),
            Err(_) => RawBillingResponse { config: None },
        }
    } else {
        RawBillingResponse { config: None }
    };
    // Subscription is best-effort: billing still renders if /user fails.
    let user = match crate::grok::api::fetch_user(access_token).await {
        Ok(user) => Some(user),
        Err(e) if is_auth_error(&e) => return Err(e),
        Err(_) => None,
    };
    Ok((billing, raw_billing, user))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_label_splits_grok_build() {
        assert_eq!(product_label("GrokBuild"), "Grok Build");
    }

    #[test]
    fn period_label_maps_weekly() {
        assert_eq!(
            period_label(Some("USAGE_PERIOD_TYPE_WEEKLY")),
            "Current week"
        );
    }

    #[test]
    fn humanize_tier_maps_official_plan_names() {
        // API legacy code for the standard SuperGrok plan.
        assert_eq!(humanize_tier("GrokPro"), "SuperGrok");
        assert_eq!(humanize_tier("SuperGrok"), "SuperGrok");
        assert_eq!(humanize_tier("supergrok"), "SuperGrok");
        assert_eq!(humanize_tier("SuperGrokHeavy"), "SuperGrok Heavy");
        assert_eq!(humanize_tier("supergrok_heavy"), "SuperGrok Heavy");
        assert_eq!(humanize_tier("SuperGrok Lite"), "SuperGrok Lite");
        assert_eq!(humanize_tier("Free"), "Free");
        assert_eq!(humanize_tier("x_premium_plus"), "X Premium+");
        assert_eq!(humanize_tier("SUBSCRIPTION_TIER_SUPER_GROK"), "SuperGrok");
    }

    #[test]
    fn credits_usage_zero_when_period_present_but_percent_absent() {
        // Real response shape for a SuperGrok user at 0% weekly usage: the
        // API omits creditUsagePercent/productUsage entirely. A present
        // currentPeriod must read as 0% — not "no data" and not a reason to
        // fall back to the monthly billing view.
        let json = r#"{
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "end": "2026-07-18T06:31:57.450351+00:00"
            }
        }"#;
        let config: BillingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(credits_usage(&config), Some(CreditsUsage::Aggregate(0.0)));
    }

    #[test]
    fn credits_usage_uses_product_usage_when_present() {
        let json = r#"{
            "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY"},
            "productUsage": [{"product": "GrokBuild", "usagePercent": 11.0}]
        }"#;
        let config: BillingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            credits_usage(&config),
            Some(CreditsUsage::Products(vec![(
                "GrokBuild".to_string(),
                11.0
            )]))
        );
    }

    #[test]
    fn credits_usage_none_without_current_period() {
        // No currentPeriod at all → genuinely nothing to render.
        let json = r#"{"creditUsagePercent": 11.0}"#;
        let config: BillingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(credits_usage(&config), None);
    }
}
