use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.kimi.com/coding/v1";

#[derive(Debug, Default)]
pub struct ManagedUsage {
    pub subscription: Option<String>,
    pub summary: Option<UsageRow>,
    pub limits: Vec<UsageRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRow {
    pub label: String,
    pub used: i64,
    pub limit: i64,
    pub reset_hint: Option<String>,
}

pub async fn fetch_usage(access_token: &str) -> Result<ManagedUsage, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .get(usage_url(&base_url()))
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("failed to fetch Kimi Code usage data: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err("authentication failed — run `kimi login` and sign in again".to_string());
    }

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Kimi Code usage endpoint is not available — update Kimi Code".to_string());
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch Kimi Code usage data: HTTP {}",
            response.status()
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("failed to parse Kimi Code usage data: {e}"))?;

    Ok(parse_managed_usage_payload(&payload))
}

fn base_url() -> String {
    std::env::var("KIMI_CODE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

fn usage_url(base_url: &str) -> String {
    format!("{}/usages", base_url.trim_end_matches('/'))
}

fn parse_managed_usage_payload(payload: &Value) -> ManagedUsage {
    let Some(record) = payload.as_object() else {
        return ManagedUsage::default();
    };

    let subscription = subscription_label(record);
    let summary = record
        .get("usage")
        .and_then(|usage| to_usage_row(usage, "Weekly limit"));

    let mut limits = Vec::new();
    if let Some(raw_limits) = record.get("limits").and_then(Value::as_array) {
        for (idx, item) in raw_limits.iter().enumerate() {
            let Some(_) = item.as_object() else {
                continue;
            };

            let detail = item
                .get("detail")
                .filter(|detail| detail.is_object())
                .unwrap_or(item);
            let window = item.get("window").and_then(Value::as_object);
            let label = limit_label(item, detail, window, idx);

            if let Some(row) = to_usage_row(detail, &label) {
                limits.push(row);
            }
        }
    }

    ManagedUsage {
        subscription,
        summary,
        limits,
    }
}

fn subscription_label(record: &Map<String, Value>) -> Option<String> {
    plan_from_kimi_code_credits(record).or_else(|| {
        first_string(record, &["subscription", "plan"])
            .map(plan_label)
            .filter(|label| !label.is_empty())
    })
}

fn plan_from_kimi_code_credits(record: &Map<String, Value>) -> Option<String> {
    let limit = record
        .get("parallel")
        .and_then(Value::as_object)
        .and_then(|parallel| to_int(parallel.get("limit")))?;

    match limit {
        1 => Some("Andante".to_string()),
        4 => Some("Moderato".to_string()),
        20 => Some("Allegretto".to_string()),
        60 => Some("Allegro".to_string()),
        _ => None,
    }
}

fn plan_label(value: &str) -> String {
    match value {
        "ANDANTE" | "LEVEL_ANDANTE" => "Andante".to_string(),
        "MODERATO" | "LEVEL_MODERATO" => "Moderato".to_string(),
        "ALLEGRETTO" | "LEVEL_ALLEGRETTO" => "Allegretto".to_string(),
        "ALLEGRO" | "LEVEL_ALLEGRO" => "Allegro".to_string(),
        _ => enum_label(value),
    }
}

#[cfg(test)]
fn membership_level(record: &Map<String, Value>) -> Option<String> {
    let raw = record
        .get("user")
        .and_then(Value::as_object)
        .and_then(|user| user.get("membership"))
        .and_then(Value::as_object)
        .and_then(|membership| membership.get("level"))
        .and_then(Value::as_str)?;

    Some(enum_label(raw))
}

fn enum_label(value: &str) -> String {
    let value = value
        .strip_prefix("LEVEL_")
        .or_else(|| value.strip_prefix("TYPE_"))
        .unwrap_or(value);

    value
        .split('_')
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

fn to_usage_row(raw: &Value, default_label: &str) -> Option<UsageRow> {
    let record = raw.as_object()?;
    let limit = to_int(record.get("limit"));
    let mut used = to_int(record.get("used"));

    if used.is_none()
        && let (Some(remaining), Some(limit)) = (to_int(record.get("remaining")), limit)
    {
        used = Some(limit - remaining);
    }

    if used.is_none() && limit.is_none() {
        return None;
    }

    Some(UsageRow {
        label: first_string(record, &["name", "title"])
            .unwrap_or(default_label)
            .to_string(),
        used: used.unwrap_or(0),
        limit: limit.unwrap_or(0),
        reset_hint: reset_hint_from(record),
    })
}

fn limit_label(
    item: &Value,
    detail: &Value,
    window: Option<&Map<String, Value>>,
    idx: usize,
) -> String {
    for key in ["title", "name", "label"] {
        if let Some(value) = string_field(item, key).or_else(|| string_field(detail, key))
            && !value.is_empty()
        {
            return value.to_string();
        }
    }

    let duration = to_int(
        window
            .and_then(|w| w.get("duration"))
            .or_else(|| item.get("duration"))
            .or_else(|| detail.get("duration")),
    );
    let time_unit = window
        .and_then(|w| w.get("timeUnit"))
        .or_else(|| item.get("timeUnit"))
        .or_else(|| detail.get("timeUnit"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if let Some(duration) = duration {
        if time_unit.contains("MINUTE") {
            if duration >= 60 && duration % 60 == 0 {
                return format!("{}h limit", duration / 60);
            }
            return format!("{duration}m limit");
        }
        if time_unit.contains("HOUR") {
            return format!("{duration}h limit");
        }
        if time_unit.contains("DAY") {
            return format!("{duration}d limit");
        }
        return format!("{duration}s limit");
    }

    format!("Limit #{}", idx + 1)
}

fn reset_hint_from(record: &Map<String, Value>) -> Option<String> {
    for key in ["reset_at", "resetAt", "reset_time", "resetTime"] {
        if let Some(value) = record.get(key).and_then(Value::as_str)
            && !value.is_empty()
        {
            return Some(format_reset_time(value));
        }
    }

    for key in ["reset_in", "resetIn", "window"] {
        if let Some(seconds) = to_int(record.get(key))
            && seconds > 0
        {
            return Some(format!("resets in {}", format_duration(seconds)));
        }
    }

    None
}

fn format_reset_time(value: &str) -> String {
    let Ok(parsed) = DateTime::parse_from_rfc3339(value) else {
        return format!("resets at {value}");
    };
    let seconds = parsed.with_timezone(&Utc).timestamp() - Utc::now().timestamp();

    if seconds <= 0 {
        "reset".to_string()
    } else {
        format!("resets in {}", format_duration(seconds))
    }
}

fn format_duration(total_seconds: i64) -> String {
    if total_seconds <= 0 {
        return "0s".to_string();
    }

    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let mut parts = Vec::new();

    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 && parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }

    if parts.is_empty() {
        "0s".to_string()
    } else {
        parts.join(" ")
    }
}

fn first_string<'a>(record: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        record
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    })
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.as_object()?.get(key)?.as_str()
}

fn to_int(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|n| n as i64)),
        Value::String(s) => s.parse::<f64>().ok().map(|n| n as i64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_url_trims_base_url_slash() {
        assert_eq!(
            usage_url("https://api.kimi.com/coding/v1/"),
            "https://api.kimi.com/coding/v1/usages"
        );
    }

    #[test]
    fn parses_summary_and_rolling_limit() {
        let payload: Value = serde_json::from_str(
            r#"{
                "user": { "membership": { "level": "LEVEL_INTERMEDIATE" } },
                "parallel": { "limit": "20" },
                "usage": {
                    "limit": "100",
                    "remaining": "98",
                    "resetIn": 604800
                },
                "limits": [
                    {
                        "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                        "detail": {
                            "limit": "100",
                            "used": "1",
                            "remaining": "99",
                            "resetIn": 18000
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        let usage = parse_managed_usage_payload(&payload);

        assert_eq!(usage.subscription.as_deref(), Some("Allegretto"));
        assert_eq!(
            usage.summary,
            Some(UsageRow {
                label: "Weekly limit".to_string(),
                used: 2,
                limit: 100,
                reset_hint: Some("resets in 7d".to_string()),
            })
        );
        assert_eq!(
            usage.limits,
            vec![UsageRow {
                label: "5h limit".to_string(),
                used: 1,
                limit: 100,
                reset_hint: Some("resets in 5h".to_string()),
            }]
        );
    }

    #[test]
    fn empty_payload_has_no_rows() {
        let usage = parse_managed_usage_payload(&Value::Null);

        assert!(usage.summary.is_none());
        assert!(usage.limits.is_empty());
    }

    #[test]
    fn membership_level_is_not_the_subscription_plan() {
        let payload: Value = serde_json::from_str(
            r#"{ "user": { "membership": { "level": "LEVEL_INTERMEDIATE" } } }"#,
        )
        .unwrap();
        let record = payload.as_object().unwrap();

        assert_eq!(subscription_label(record), None);
        assert_eq!(membership_level(record).as_deref(), Some("Intermediate"));
    }

    #[test]
    fn duration_keeps_seconds_only_when_no_larger_unit() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(3_900), "1h 5m");
    }
}
