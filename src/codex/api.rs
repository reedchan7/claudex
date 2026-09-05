use chrono::DateTime;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

use super::auth::CodexCredentials;

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub plan_type: Option<String>,
    pub rate_limit: Option<RateLimitInfo>,
    pub additional_rate_limits: Option<Vec<AdditionalRateLimit>>,
    pub credits: Option<Credits>,
    pub rate_limit_reset_credits: Option<RateLimitResetCreditsSummary>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitInfo {
    pub primary_window: Option<WindowSnapshot>,
    pub secondary_window: Option<WindowSnapshot>,
}

#[derive(Debug, Deserialize)]
pub struct WindowSnapshot {
    pub used_percent: f64,
    pub limit_window_seconds: i64,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AdditionalRateLimit {
    pub limit_name: Option<String>,
    pub rate_limit: Option<RateLimitInfo>,
}

#[derive(Debug, Deserialize)]
pub struct Credits {
    pub has_credits: Option<bool>,
    pub unlimited: Option<bool>,
    pub balance: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitResetCreditsSummary {
    pub available_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitResetCreditsDetails {
    pub available_count: i64,
    #[serde(default)]
    pub credits: Vec<RateLimitResetCredit>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitResetCredit {
    pub id: Option<String>,
    pub reset_type: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_timestamp")]
    pub granted_at: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_opt_timestamp")]
    pub expires_at: Option<i64>,
    pub title: Option<String>,
    pub description: Option<String>,
}

impl RateLimitResetCredit {
    pub fn is_available(&self) -> bool {
        match self.status.as_deref() {
            None => true,
            Some(status) => status.eq_ignore_ascii_case("available"),
        }
    }
}

const BASE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";

pub async fn fetch_usage(creds: &CodexCredentials) -> Result<UsageResponse, String> {
    let client = http_client()?;
    chatgpt_get_json(&client, creds, BASE_URL, "Codex usage data").await
}

pub async fn fetch_reset_credits(
    creds: &CodexCredentials,
) -> Result<RateLimitResetCreditsDetails, String> {
    let client = http_client()?;
    chatgpt_get_json(&client, creds, RESET_CREDITS_URL, "Codex reset credits").await
}

pub async fn fetch_usage_bundle(
    creds: &CodexCredentials,
) -> Result<(UsageResponse, Option<RateLimitResetCreditsDetails>), String> {
    let client = http_client()?;
    let usage = chatgpt_get_json(&client, creds, BASE_URL, "Codex usage data");
    let details = chatgpt_get_json(&client, creds, RESET_CREDITS_URL, "Codex reset credits");
    let (usage, details) = tokio::join!(usage, details);
    Ok((usage?, details.ok()))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

async fn chatgpt_get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    creds: &CodexCredentials,
    url: &str,
    what: &str,
) -> Result<T, String> {
    let mut req = client
        .get(url)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("User-Agent", "codex-cli");

    if let Some(account_id) = &creds.account_id {
        req = req.header("ChatGPT-Account-Id", account_id);
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("failed to fetch {what}: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(
            "authentication failed — try restarting Codex to refresh your token".to_string(),
        );
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch {what}: HTTP {}",
            response.status()
        ));
    }

    response
        .json::<T>()
        .await
        .map_err(|e| format!("failed to parse {what}: {e}"))
}

fn deserialize_opt_timestamp<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| parse_timestamp(&value)))
}

fn parse_timestamp(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
        serde_json::Value::String(s) => parse_timestamp_str(s),
        _ => None,
    }
}

fn parse_timestamp_str(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Some(n);
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_full_response() {
        let json = r#"{
            "user_id": "user-123",
            "account_id": "user-123",
            "email": "test@example.com",
            "plan_type": "pro",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {
                    "used_percent": 9,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 15339,
                    "reset_at": 1779972641
                },
                "secondary_window": {
                    "used_percent": 36,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 253227,
                    "reset_at": 1780210528
                }
            },
            "additional_rate_limits": [
                {
                    "limit_name": "GPT-5.3-Codex-Spark",
                    "metered_feature": "codex_bengalfox",
                    "rate_limit": {
                        "allowed": true,
                        "limit_reached": false,
                        "primary_window": {
                            "used_percent": 0,
                            "limit_window_seconds": 18000,
                            "reset_after_seconds": 18000,
                            "reset_at": 1779975302
                        },
                        "secondary_window": {
                            "used_percent": 0,
                            "limit_window_seconds": 604800,
                            "reset_after_seconds": 604800,
                            "reset_at": 1780562102
                        }
                    }
                }
            ],
            "credits": {
                "has_credits": false,
                "unlimited": false,
                "overage_limit_reached": false,
                "balance": "0"
            },
            "rate_limit_reached_type": null
        }"#;
        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.plan_type.as_deref(), Some("pro"));

        let rl = resp.rate_limit.as_ref().unwrap();
        let primary = rl.primary_window.as_ref().unwrap();
        assert_eq!(primary.used_percent, 9.0);
        assert_eq!(primary.limit_window_seconds, 18000);

        let secondary = rl.secondary_window.as_ref().unwrap();
        assert_eq!(secondary.used_percent, 36.0);

        let additional = resp.additional_rate_limits.as_ref().unwrap();
        assert_eq!(additional.len(), 1);
        assert_eq!(
            additional[0].limit_name.as_deref(),
            Some("GPT-5.3-Codex-Spark")
        );

        let credits = resp.credits.as_ref().unwrap();
        assert_eq!(credits.has_credits, Some(false));
        assert_eq!(credits.balance.as_deref(), Some("0"));
    }

    #[test]
    fn test_deserialize_minimal_response() {
        let json = r#"{"plan_type": "free"}"#;
        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.plan_type.as_deref(), Some("free"));
        assert!(resp.rate_limit.is_none());
        assert!(resp.additional_rate_limits.is_none());
    }

    #[test]
    fn test_deserialize_ignores_unknown_fields() {
        let json = r#"{
            "plan_type": "pro",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {
                    "used_percent": 50,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 9000,
                    "reset_at": 1779900000
                }
            },
            "spend_control": {"reached": false},
            "referral_beacon": null,
            "rate_limit_reset_credits": {"available_count": 0}
        }"#;
        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.rate_limit
                .as_ref()
                .unwrap()
                .primary_window
                .as_ref()
                .unwrap()
                .used_percent,
            50.0
        );
        assert_eq!(
            resp.rate_limit_reset_credits
                .as_ref()
                .unwrap()
                .available_count,
            0
        );
    }

    #[test]
    fn test_deserialize_reset_credit_details_iso_and_unix() {
        let json = r#"{
            "available_count": 2,
            "total_earned_count": 2,
            "history_enabled": false,
            "immediate_reset_purchase_eligible": false,
            "credits": [
                {
                    "id": "RateLimitResetCredit_aaa",
                    "reset_type": "codex_rate_limits",
                    "status": "available",
                    "granted_at": "2026-06-12T01:33:14Z",
                    "expires_at": "2030-01-01T00:00:00Z",
                    "title": "Full reset (Weekly + 5 hr)",
                    "description": "One free rate limit reset"
                },
                {
                    "id": "RateLimitResetCredit_bbb",
                    "status": "available",
                    "granted_at": 1780210528,
                    "expires_at": 1893456000
                }
            ]
        }"#;
        let details: RateLimitResetCreditsDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.available_count, 2);
        assert_eq!(details.credits.len(), 2);
        assert_eq!(details.credits[0].expires_at, Some(1893456000));
        assert_eq!(details.credits[1].expires_at, Some(1893456000));
        assert!(details.credits[0].is_available());
    }

    #[test]
    fn test_reset_credit_status_filter() {
        let redeemed: RateLimitResetCredit =
            serde_json::from_str(r#"{"status": "redeemed", "expires_at": "2030-01-01T00:00:00Z"}"#)
                .unwrap();
        assert!(!redeemed.is_available());

        let missing: RateLimitResetCredit = serde_json::from_str(r#"{}"#).unwrap();
        assert!(missing.is_available());
        assert!(missing.expires_at.is_none());
    }
}
