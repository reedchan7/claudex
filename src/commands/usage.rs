use chrono::{DateTime, Local, NaiveDate, Timelike};
use colored::Colorize;
use terminal_size::{Width, terminal_size};

use crate::api::{ExtraUsage, RateLimit, UsageLimit};
use crate::commands::status::{self, Provider};

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

fn format_local(
    local_dt: DateTime<Local>,
    today: NaiveDate,
    tz_name: &str,
    show_timezone: bool,
) -> String {
    let time_str = if local_dt.minute() == 0 {
        local_dt.format("%-I%P").to_string() // "3am"
    } else {
        local_dt.format("%-I:%M%P").to_string() // "2:30pm"
    };
    let time_str = if show_timezone {
        format!("{time_str} ({tz_name})")
    } else {
        time_str
    };

    if local_dt.date_naive() == today {
        time_str
    } else {
        let date_str = local_dt.format("%b %-d").to_string(); // "May 30"
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

fn print_limit_bar(title: &str, limit: &RateLimit, show_timezone: bool) {
    let utilization = limit.utilization.unwrap_or(0.0);
    let bar = progress_bar(utilization, bar_width());
    println!("{}", title.bold());
    println!("{} {:.0}% used", bar, utilization);
    if let Some(resets_at) = &limit.resets_at {
        let reset_str = format_reset_time_with_options(resets_at, show_timezone);
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

fn limit_title(limit: &UsageLimit) -> Option<String> {
    match limit.kind.as_deref()? {
        "session" => Some("Current session (5h)".to_string()),
        "weekly_all" => Some("Current week (all models)".to_string()),
        "weekly_scoped" => {
            let model = limit.scope.as_ref()?.model.as_ref()?;
            let name = model.display_name.as_deref().or(model.id.as_deref())?;
            Some(format!("Current week ({name})"))
        }
        _ => None,
    }
}

fn rate_limit_from_usage_limit(limit: &UsageLimit) -> RateLimit {
    RateLimit {
        utilization: limit.percent,
        resets_at: limit.resets_at.clone(),
    }
}

fn print_usage_limits(limits: &[UsageLimit], show_timezone: bool) -> bool {
    let mut printed = false;
    for limit in limits {
        let Some(title) = limit_title(limit) else {
            continue;
        };
        if printed {
            println!();
        }
        let rate_limit = rate_limit_from_usage_limit(limit);
        print_limit_bar(&title, &rate_limit, show_timezone);
        printed = true;
    }
    printed
}

fn print_legacy_limits(utilization: &crate::api::Utilization, show_timezone: bool) -> bool {
    let limits: &[(&str, Option<&RateLimit>)] = &[
        ("Current session (5h)", utilization.five_hour.as_ref()),
        ("Current week (all models)", utilization.seven_day.as_ref()),
        (
            "Current week (Sonnet only)",
            utilization.seven_day_sonnet.as_ref(),
        ),
    ];

    let mut printed = false;
    for (title, limit) in limits {
        if let Some(limit) = limit {
            if printed {
                println!();
            }
            print_limit_bar(title, limit, show_timezone);
            printed = true;
        }
    }
    printed
}

pub async fn run(show_timezone: bool) {
    if let Err(e) = render(show_timezone).await {
        status::print_provider_error(Provider::Claude, &e);
        std::process::exit(1);
    }
}

pub async fn render(show_timezone: bool) -> Result<(), String> {
    let session = crate::auth::read_oauth_session()?;

    let version = crate::auth::get_claude_version();
    let user_agent = format!("claude-code/{version}");

    let utilization = fetch_utilization_with_recovery(session, &user_agent).await?;

    let printed = print_usage_limits(&utilization.limits, show_timezone)
        || print_legacy_limits(&utilization, show_timezone);
    if !printed {
        println!("/usage is only available for subscription plans.");
        return Ok(());
    }

    if let Some(extra) = &utilization.extra_usage {
        println!();
        print_extra_usage(extra);
    }

    Ok(())
}

fn is_auth_error(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authentication failed")
}

fn is_rate_limited(error: &str) -> bool {
    error.contains("429")
}

/// Fetch usage, recovering when the usage endpoint rejects the access token.
///
/// A `401` there doesn't always mean the token is dead — the endpoint
/// occasionally rejects a still-valid token. Because claudex shares its OAuth
/// credentials with Claude Code (same keychain entry, same *rotating* refresh
/// token), refreshing on every `401` races Claude Code's own refresh and gets
/// rate-limited (`HTTP 429`). So we refresh only when the token has actually
/// expired, and when a refresh is rate-limited we fall back to whatever token
/// Claude Code may have just written to the credential store.
async fn fetch_utilization_with_recovery(
    session: crate::auth::OAuthSession,
    user_agent: &str,
) -> Result<crate::api::Utilization, String> {
    match crate::api::fetch_utilization(&session.access_token, user_agent).await {
        Ok(util) => return Ok(util),
        Err(e) if is_auth_error(&e) => {}
        Err(e) => return Err(e),
    }

    if session.is_expired() {
        match crate::auth::refresh_oauth_session(&session, user_agent).await {
            Ok(refreshed) => {
                return crate::api::fetch_utilization(&refreshed.access_token, user_agent).await;
            }
            // A rate-limited refresh can mean Claude Code is refreshing the
            // same token; fall through once, but preserve the real cause.
            Err(e) if is_rate_limited(&e) => {
                return recover_from_credential_store(
                    &session.access_token,
                    user_agent,
                    Some(e.as_str()),
                )
                .await;
            }
            Err(e) => return Err(e),
        }
    }

    recover_from_credential_store(&session.access_token, user_agent, None).await
}

/// Last resort after a rejected token: Claude Code owns token refresh and may
/// have just written a fresher token to the shared credential store. Re-read it
/// and retry once before surfacing the original refresh failure if there was one.
async fn recover_from_credential_store(
    stale_token: &str,
    user_agent: &str,
    fallback_error: Option<&str>,
) -> Result<crate::api::Utilization, String> {
    if let Ok(session) = crate::auth::read_oauth_session()
        && session.access_token != stale_token
        && let Ok(util) = crate::api::fetch_utilization(&session.access_token, user_agent).await
    {
        return Ok(util);
    }
    Err(fallback_error
        .unwrap_or("usage is temporarily unavailable — retry shortly")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_is_auth_error_matches_case_insensitively() {
        assert!(is_auth_error(
            "authentication failed — try restarting Claude Code"
        ));
        assert!(is_auth_error("Authentication Failed"));
        assert!(!is_auth_error("failed to fetch usage data: HTTP 500"));
    }

    #[test]
    fn test_is_rate_limited_detects_429() {
        assert!(is_rate_limited(
            "failed to refresh Claude Code session: HTTP 429 Too Many Requests"
        ));
        assert!(!is_rate_limited("authentication failed"));
        assert!(!is_rate_limited("failed to fetch usage data: HTTP 500"));
    }

    #[tokio::test]
    async fn test_recovery_preserves_rate_limit_when_store_has_same_token() {
        let previous = std::env::var_os("CLAUDE_CODE_OAUTH_TOKEN");
        unsafe {
            std::env::set_var("CLAUDE_CODE_OAUTH_TOKEN", "stale-token");
        }

        let result = recover_from_credential_store(
            "stale-token",
            "claude-code/test",
            Some("failed to refresh Claude Code session: HTTP 429 Too Many Requests"),
        )
        .await;

        match previous {
            Some(value) => unsafe {
                std::env::set_var("CLAUDE_CODE_OAUTH_TOKEN", value);
            },
            None => unsafe {
                std::env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
            },
        }

        assert_eq!(
            result.unwrap_err(),
            "failed to refresh Claude Code session: HTTP 429 Too Many Requests"
        );
    }

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
    fn test_limit_title_uses_scoped_model_name() {
        let limit = crate::api::UsageLimit {
            kind: Some("weekly_scoped".to_string()),
            percent: Some(86.0),
            resets_at: None,
            scope: Some(crate::api::UsageLimitScope {
                model: Some(crate::api::UsageLimitModel {
                    id: None,
                    display_name: Some("Fable".to_string()),
                }),
            }),
        };

        assert_eq!(limit_title(&limit).as_deref(), Some("Current week (Fable)"));
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
        let result = format_reset_time_with_options("2026-05-29T19:00:00+00:00", false);
        assert!(!result.is_empty());
        assert!(!result.contains('('));
    }

    #[test]
    fn test_format_reset_time_invalid_returns_original() {
        let result = format_reset_time_with_options("not-a-date", false);
        assert_eq!(result, "not-a-date");
    }

    #[test]
    fn test_format_reset_time_fractional_seconds() {
        let result = format_reset_time_with_options("2026-05-25T06:30:00.176572+00:00", false);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_format_local_same_day_with_minutes() {
        let dt = Local.with_ymd_and_hms(2026, 5, 25, 14, 30, 0).unwrap();
        let today = dt.date_naive();
        assert_eq!(format_local(dt, today, "Asia/Shanghai", false), "2:30pm");
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai", true),
            "2:30pm (Asia/Shanghai)"
        );
    }

    #[test]
    fn test_format_local_same_day_on_the_hour() {
        let dt = Local.with_ymd_and_hms(2026, 5, 25, 15, 0, 0).unwrap();
        let today = dt.date_naive();
        assert_eq!(format_local(dt, today, "Asia/Shanghai", false), "3pm");
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai", true),
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
            format_local(dt, today, "Asia/Shanghai", false),
            "May 30 at 3am"
        );
        assert_eq!(
            format_local(dt, today, "Asia/Shanghai", true),
            "May 30 at 3am (Asia/Shanghai)"
        );
    }
}
