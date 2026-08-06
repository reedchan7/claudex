//! Presentation helpers for claudex-bar.

use chrono::{DateTime, Local};

use crate::commands::usage::format_duration_short;

/// Recompute the countdown inside a preformatted detail line such as
/// "Resets 2:30pm, 2h 30m left" so it stays fresh between polls. Falls back
/// to the original text when the timestamp or the ", … left" suffix is
/// missing or the reset time has passed.
pub fn refreshed_detail(detail: &str, resets_at: Option<&str>) -> String {
    let Some(resets_at) = resets_at else {
        return detail.to_string();
    };
    let Ok(reset) = DateTime::parse_from_rfc3339(resets_at) else {
        return detail.to_string();
    };
    let seconds = reset.signed_duration_since(Local::now()).num_seconds();
    if seconds <= 0 {
        return detail.to_string();
    }
    let Some(idx) = detail.rfind(", ") else {
        return detail.to_string();
    };
    let (head, tail) = detail.split_at(idx);
    if !tail.ends_with(" left") {
        return detail.to_string();
    }
    format!("{head}, {} left", format_duration_short(seconds))
}

/// Human age of the last successful poll, e.g. "2m ago".
pub fn age_label(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        "just now".to_string()
    } else {
        format!("{} ago", format_duration_short(elapsed_secs as i64))
    }
}

/// Compact label for a poll interval, e.g. 600 → "10m", 3660 → "1h1m".
pub fn interval_label(secs: u64) -> String {
    if secs >= 3600 {
        let hours = secs / 3600;
        let rest = secs % 3600;
        if rest == 0 {
            return format!("{hours}h");
        }
        return format!("{hours}h{}", interval_label(rest));
    }
    if secs.is_multiple_of(60) {
        return format!("{}m", secs / 60);
    }
    let minutes = secs / 60;
    let seconds = secs % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m{seconds}s")
    }
}

/// Parse a user-typed interval: "90s", "10m", "1h", or combos like "1h30m".
/// Returns total seconds, or None for anything else (bare numbers rejected).
pub fn parse_interval(input: &str) -> Option<u64> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return None;
    }
    let mut total: u64 = 0;
    let mut number = String::new();
    let mut saw_unit = false;
    for c in input.chars() {
        match c {
            '0'..='9' => number.push(c),
            's' | 'm' | 'h' => {
                let value: u64 = number.parse().ok()?;
                number.clear();
                let multiplier = match c {
                    's' => 1,
                    'm' => 60,
                    'h' => 3600,
                    _ => unreachable!(),
                };
                total += value * multiplier;
                saw_unit = true;
            }
            _ => return None,
        }
    }
    if !number.is_empty() || !saw_unit {
        return None;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn refreshed_detail_rewrites_countdown_suffix() {
        let future = (Local::now() + Duration::hours(2) + Duration::minutes(31)).to_rfc3339();
        let refreshed = refreshed_detail("Resets 2:30pm, 2h 30m left", Some(&future));
        assert!(refreshed.starts_with("Resets 2:30pm, "));
        assert!(refreshed.ends_with(" left"));
        assert!(refreshed.contains("2h 31m") || refreshed.contains("2h 30m"));
    }

    #[test]
    fn refreshed_detail_keeps_text_without_left_suffix() {
        let future = (Local::now() + Duration::hours(1)).to_rfc3339();
        assert_eq!(
            refreshed_detail("Resets 2:30pm", Some(&future)),
            "Resets 2:30pm"
        );
    }

    #[test]
    fn refreshed_detail_keeps_text_when_timestamp_missing_or_past() {
        assert_eq!(
            refreshed_detail("Resets 2:30pm, 2h 30m left", None),
            "Resets 2:30pm, 2h 30m left"
        );
        let past = (Local::now() - Duration::minutes(5)).to_rfc3339();
        assert_eq!(
            refreshed_detail("Resets 2:30pm, 2h 30m left", Some(&past)),
            "Resets 2:30pm, 2h 30m left"
        );
        assert_eq!(
            refreshed_detail("Resets whenever", Some("not-a-date")),
            "Resets whenever"
        );
    }

    #[test]
    fn age_label_switches_from_just_now_to_minutes() {
        assert_eq!(age_label(5), "just now");
        assert_eq!(age_label(60), "1m ago");
        assert_eq!(age_label(65 * 60), "1h 5m ago");
    }

    #[test]
    fn interval_label_uses_compact_units() {
        assert_eq!(interval_label(60), "1m");
        assert_eq!(interval_label(600), "10m");
        assert_eq!(interval_label(3600), "1h");
        assert_eq!(interval_label(45), "45s");
        assert_eq!(interval_label(90), "1m30s");
        assert_eq!(interval_label(3660), "1h1m");
        assert_eq!(interval_label(3661), "1h1m1s");
    }

    #[test]
    fn parse_interval_accepts_unit_forms() {
        assert_eq!(parse_interval("90s"), Some(90));
        assert_eq!(parse_interval("10m"), Some(600));
        assert_eq!(parse_interval("1h"), Some(3600));
        assert_eq!(parse_interval("1h30m"), Some(5400));
        assert_eq!(parse_interval("1m30s"), Some(90));
        assert_eq!(parse_interval(" 2M "), Some(120));
    }

    #[test]
    fn parse_interval_rejects_garbage_and_bare_numbers() {
        assert_eq!(parse_interval(""), None);
        assert_eq!(parse_interval("10"), None);
        assert_eq!(parse_interval("10x"), None);
        assert_eq!(parse_interval("m"), None);
        assert_eq!(parse_interval("1h30"), None);
        assert_eq!(parse_interval("1.5h"), None);
    }
}
