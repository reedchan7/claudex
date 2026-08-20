use serde_json::{Map, Value};
use std::time::Duration;

use super::api::{UsageRow, reset_at_from};
use super::cookies::web_auth_tokens;

const SUBSCRIPTION_STATS_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.membership.v2.MembershipService/GetSubscriptionStats";

/// Shared monthly membership pool shown on Kimi web as **Total usage**.
///
/// Kimi web and Kimi Code draw from the same pool. Coding `/usages` does
/// not return this percentage; it lives on the membership stats RPC.
pub async fn fetch_monthly_limit(preferred_user_id: Option<&str>) -> Option<UsageRow> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .ok()?;

    for token in web_auth_tokens(preferred_user_id) {
        if let Some(row) = fetch_monthly_limit_with(&client, &token).await {
            return Some(row);
        }
    }
    None
}

async fn fetch_monthly_limit_with(client: &reqwest::Client, token: &str) -> Option<UsageRow> {
    let response = client
        .post(SUBSCRIPTION_STATS_URL)
        .bearer_auth(token)
        .header("Cookie", format!("kimi-auth={token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "*/*")
        .header("Origin", "https://www.kimi.com")
        .header(
            "Referer",
            "https://www.kimi.com/membership/subscription?tab=quota",
        )
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36")
        .header("connect-protocol-version", "1")
        .header("x-language", "en-US")
        .header("x-msh-platform", "web")
        .body("{}")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let payload: Value = response.json().await.ok()?;
    parse_monthly_limit(&payload)
}

pub(crate) fn parse_monthly_limit(payload: &Value) -> Option<UsageRow> {
    let balance = payload.get("subscriptionBalance")?.as_object()?;
    if !is_shared_monthly_pool(balance) {
        return None;
    }

    let ratio = to_f64(balance.get("amountUsedRatio"))?;
    if !ratio.is_finite() {
        return None;
    }

    let used_percent = (ratio * 100.0).clamp(0.0, 100.0);
    Some(UsageRow {
        label: "Monthly limit".to_string(),
        used: used_percent.round() as i64,
        limit: 100,
        reset_at: reset_at_from(balance),
    })
}

fn is_shared_monthly_pool(balance: &Map<String, Value>) -> bool {
    let feature = balance.get("feature").and_then(Value::as_str);
    let kind = balance.get("type").and_then(Value::as_str);
    (feature.is_none() || feature == Some("FEATURE_OMNI"))
        && (kind.is_none() || kind == Some("SUBSCRIPTION"))
}

fn to_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shared_monthly_pool_from_membership_stats() {
        let payload: Value = serde_json::from_str(
            r#"{
                "ratelimitCode5h": {"ratio": 0.2056, "enabled": true, "resetTime": "2026-08-20T09:52:31Z"},
                "ratelimitCode7d": {"ratio": 0.6271, "enabled": true, "resetTime": "2026-08-21T00:52:31Z"},
                "subscriptionBalance": {
                    "feature": "FEATURE_OMNI",
                    "type": "SUBSCRIPTION",
                    "unit": "UNIT_CREDIT",
                    "amountUsedRatio": 0.1145,
                    "kimiCodeUsedRatio": 0.1145,
                    "expireTime": "2026-09-17T00:52:31.397894Z"
                }
            }"#,
        )
        .unwrap();

        let row = parse_monthly_limit(&payload).unwrap();
        assert_eq!(row.label, "Monthly limit");
        assert_eq!(row.used, 11);
        assert_eq!(row.limit, 100);
        assert_eq!(row.reset_at.as_deref(), Some("2026-09-17T00:52:31.397894Z"));
    }

    #[test]
    fn ignores_non_omni_balance() {
        let payload: Value = serde_json::from_str(
            r#"{ "subscriptionBalance": { "feature": "FEATURE_CODING", "amountUsedRatio": 0.5 } }"#,
        )
        .unwrap();

        assert!(parse_monthly_limit(&payload).is_none());
    }

    #[test]
    fn missing_ratio_yields_no_row() {
        let payload: Value = serde_json::from_str(
            r#"{ "subscriptionBalance": { "feature": "FEATURE_OMNI", "type": "SUBSCRIPTION" } }"#,
        )
        .unwrap();

        assert!(parse_monthly_limit(&payload).is_none());
    }
}
