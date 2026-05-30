use serde::Deserialize;
use std::time::Duration;

use super::auth::CodexCredentials;

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub plan_type: Option<String>,
    pub rate_limit: Option<RateLimitInfo>,
    pub additional_rate_limits: Option<Vec<AdditionalRateLimit>>,
    pub credits: Option<Credits>,
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

const BASE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

pub async fn fetch_usage(creds: &CodexCredentials) -> Result<UsageResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut req = client
        .get(BASE_URL)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("User-Agent", "codex-cli");

    if let Some(account_id) = &creds.account_id {
        req = req.header("ChatGPT-Account-Id", account_id);
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("failed to fetch Codex usage data: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(
            "authentication failed — try restarting Codex to refresh your token".to_string(),
        );
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch Codex usage data: HTTP {}",
            response.status()
        ));
    }

    response
        .json::<UsageResponse>()
        .await
        .map_err(|e| format!("failed to parse Codex usage data: {e}"))
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
    }
}
