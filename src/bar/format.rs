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
}
