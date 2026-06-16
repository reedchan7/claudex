use serde::Deserialize;
use std::time::Duration;

const BASE_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserQuotaSummaryResponse {
    #[serde(default)]
    pub buckets: Vec<QuotaSummaryBucket>,
    #[serde(default)]
    pub groups: Vec<QuotaSummaryGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSummaryGroup {
    pub display_name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub buckets: Vec<QuotaSummaryBucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSummaryBucket {
    pub display_name: Option<String>,
    pub remaining_fraction: Option<f64>,
    pub remaining_amount: Option<i64>,
    pub disabled: Option<bool>,
    pub reset_time: Option<String>,
}

pub async fn fetch_user_quota_summary(
    access_token: &str,
    user_agent: &str,
) -> Result<UserQuotaSummaryResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .post(BASE_URL)
        .bearer_auth(access_token)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| format!("failed to fetch Antigravity quota data: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(
            "authentication failed — try restarting Antigravity to refresh your Google login"
                .to_string(),
        );
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch Antigravity quota data: HTTP {}",
            response.status()
        ));
    }

    response
        .json::<UserQuotaSummaryResponse>()
        .await
        .map_err(|e| format!("failed to parse Antigravity quota data: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_quota_summary_response() {
        let json = r#"{
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "description": "Models within this group: Gemini Flash, Gemini Pro",
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "displayName": "Weekly Limit",
                            "window": "weekly",
                            "remainingFraction": 0.9207936,
                            "resetTime": "2026-06-19T08:46:00Z"
                        },
                        {
                            "bucketId": "gemini-five-hour",
                            "displayName": "Five Hour Limit",
                            "window": "five_hour",
                            "remainingFraction": 1,
                            "resetTime": "2026-06-16T08:39:13Z"
                        }
                    ]
                }
            ],
            "description": "Within each group, models share a weekly limit and a 5-hour limit."
        }"#;

        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.groups.len(), 1);
        assert_eq!(response.buckets.len(), 0);

        let group = &response.groups[0];
        assert_eq!(group.display_name, "Gemini Models");
        assert_eq!(
            group.description.as_deref(),
            Some("Models within this group: Gemini Flash, Gemini Pro")
        );
        assert_eq!(group.buckets.len(), 2);

        let weekly = &group.buckets[0];
        assert_eq!(weekly.display_name.as_deref(), Some("Weekly Limit"));
        assert_eq!(weekly.reset_time.as_deref(), Some("2026-06-19T08:46:00Z"));
        assert_eq!(weekly.remaining_fraction, Some(0.9207936));
    }

    #[test]
    fn test_deserialize_disabled_summary_bucket() {
        let json = r#"{
            "buckets": [
                {
                    "bucketId": "disabled",
                    "displayName": "Five Hour Limit",
                    "disabled": true
                }
            ],
            "ignored": true
        }"#;

        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.buckets.len(), 1);
        assert_eq!(response.buckets[0].disabled, Some(true));
    }
}
