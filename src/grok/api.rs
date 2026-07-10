use serde::Deserialize;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";

#[derive(Debug, Deserialize)]
pub struct BillingResponse {
    pub config: Option<BillingConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingConfig {
    pub current_period: Option<UsagePeriod>,
    pub credit_usage_percent: Option<f64>,
    pub monthly_limit: Option<MoneyVal>,
    pub on_demand_cap: Option<MoneyVal>,
    pub on_demand_used: Option<MoneyVal>,
    pub prepaid_balance: Option<MoneyVal>,
    pub product_usage: Option<Vec<ProductUsage>>,
    #[allow(dead_code)]
    pub is_unified_billing_user: Option<bool>,
    #[allow(dead_code)]
    pub billing_period_start: Option<String>,
    pub billing_period_end: Option<String>,
    pub subscription_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UsagePeriod {
    #[serde(rename = "type")]
    pub period_type: Option<String>,
    #[allow(dead_code)]
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProductUsage {
    pub product: Option<String>,
    #[serde(rename = "usagePercent")]
    pub usage_percent: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct MoneyVal {
    pub val: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    /// e.g. `"GrokPro"`, `"SuperGrok"`, `"Free"`.
    pub subscription_tier: Option<String>,
}

pub async fn fetch_billing(access_token: &str) -> Result<BillingResponse, String> {
    let (client, version) = http_client()?;
    let response = client
        .get(billing_url(&base_url()))
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .header("User-Agent", format!("grok-shell/{version}"))
        .header("x-grok-client-version", &version)
        .send()
        .await
        .map_err(|e| format!("failed to fetch Grok billing data: {e}"))?;

    check_auth_status(response.status(), "Grok billing data")?;

    response
        .json::<BillingResponse>()
        .await
        .map_err(|e| format!("failed to parse Grok billing data: {e}"))
}

/// Fetch account profile, including subscription tier when available.
pub async fn fetch_user(access_token: &str) -> Result<UserResponse, String> {
    let (client, version) = http_client()?;
    let response = client
        .get(user_url(&base_url()))
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .header("User-Agent", format!("grok-shell/{version}"))
        .header("x-grok-client-version", &version)
        .send()
        .await
        .map_err(|e| format!("failed to fetch Grok user data: {e}"))?;

    check_auth_status(response.status(), "Grok user data")?;

    response
        .json::<UserResponse>()
        .await
        .map_err(|e| format!("failed to parse Grok user data: {e}"))
}

fn http_client() -> Result<(reqwest::Client, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    Ok((client, grok_client_version()))
}

fn check_auth_status(status: reqwest::StatusCode, what: &str) -> Result<(), String> {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("authentication failed — run `grok login` and sign in again".to_string());
    }
    if !status.is_success() {
        return Err(format!("failed to fetch {what}: HTTP {status}"));
    }
    Ok(())
}

fn base_url() -> String {
    std::env::var("GROK_CLI_CHAT_PROXY_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

fn billing_url(base_url: &str) -> String {
    format!("{}/billing?format=credits", base_url.trim_end_matches('/'))
}

fn user_url(base_url: &str) -> String {
    format!(
        "{}/user?include=subscription",
        base_url.trim_end_matches('/')
    )
}

fn grok_client_version() -> String {
    std::process::Command::new("grok")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let raw = String::from_utf8_lossy(&o.stdout);
            // "grok 0.2.93 (f00f96316d4b)"
            raw.split_whitespace()
                .nth(1)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "0.0.0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn billing_url_trims_trailing_slash() {
        assert_eq!(
            billing_url("https://cli-chat-proxy.grok.com/v1/"),
            "https://cli-chat-proxy.grok.com/v1/billing?format=credits"
        );
    }

    #[test]
    fn user_url_includes_subscription() {
        assert_eq!(
            user_url("https://cli-chat-proxy.grok.com/v1/"),
            "https://cli-chat-proxy.grok.com/v1/user?include=subscription"
        );
    }

    #[test]
    fn deserializes_credits_response() {
        let json = r#"{
            "config": {
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-07-04T06:31:57.450351+00:00",
                    "end": "2026-07-11T06:31:57.450351+00:00"
                },
                "creditUsagePercent": 11.0,
                "onDemandCap": { "val": 0 },
                "onDemandUsed": { "val": 0 },
                "productUsage": [
                    { "product": "GrokBuild", "usagePercent": 11.0 }
                ],
                "isUnifiedBillingUser": true,
                "prepaidBalance": { "val": 0 },
                "billingPeriodStart": "2026-07-04T06:31:57.450351+00:00",
                "billingPeriodEnd": "2026-07-11T06:31:57.450351+00:00"
            }
        }"#;

        let resp: BillingResponse = serde_json::from_str(json).unwrap();
        let config = resp.config.unwrap();
        assert_eq!(config.credit_usage_percent, Some(11.0));
        assert_eq!(
            config
                .current_period
                .as_ref()
                .unwrap()
                .period_type
                .as_deref(),
            Some("USAGE_PERIOD_TYPE_WEEKLY")
        );
        let products = config.product_usage.unwrap();
        assert_eq!(products[0].product.as_deref(), Some("GrokBuild"));
        assert_eq!(products[0].usage_percent, Some(11.0));
    }

    #[test]
    fn deserializes_user_subscription_tier() {
        let json = r#"{
            "userId": "u-1",
            "email": "a@b.com",
            "hasGrokCodeAccess": true,
            "subscriptionTier": "GrokPro"
        }"#;
        let user: UserResponse = serde_json::from_str(json).unwrap();
        assert_eq!(user.subscription_tier.as_deref(), Some("GrokPro"));
    }
}
