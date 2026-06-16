use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;

const QUOTA_SUMMARY_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
const LOAD_CODE_ASSIST_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const MODEL_QUOTA_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";

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
    pub model_id: Option<String>,
    pub display_name: Option<String>,
    pub remaining_fraction: Option<f64>,
    pub remaining_amount: Option<i64>,
    pub disabled: Option<bool>,
    pub reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadCodeAssistResponse {
    cloudaicompanion_project: Option<String>,
}

pub async fn fetch_user_quota_summary(
    access_token: &str,
    user_agent: &str,
) -> Result<UserQuotaSummaryResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut quota: UserQuotaSummaryResponse = post_json(
        &client,
        QUOTA_SUMMARY_URL,
        access_token,
        user_agent,
        quota_summary_request_body(),
        "failed to fetch Antigravity quota data",
        "failed to parse Antigravity quota data",
    )
    .await?;

    if let Ok(model_buckets) = fetch_model_quota_buckets(&client, access_token, user_agent).await
        && !model_buckets.is_empty()
    {
        quota.buckets.extend(model_buckets);
    }

    Ok(quota)
}

async fn fetch_model_quota_buckets(
    client: &reqwest::Client,
    access_token: &str,
    user_agent: &str,
) -> Result<Vec<QuotaSummaryBucket>, String> {
    let Some(project) = fetch_code_assist_project(client, access_token, user_agent).await? else {
        return Ok(Vec::new());
    };

    let quota: UserQuotaSummaryResponse = post_json(
        client,
        MODEL_QUOTA_URL,
        access_token,
        user_agent,
        model_quota_request_body(&project),
        "failed to fetch Antigravity model quota data",
        "failed to parse Antigravity model quota data",
    )
    .await?;

    Ok(quota.buckets)
}

async fn fetch_code_assist_project(
    client: &reqwest::Client,
    access_token: &str,
    user_agent: &str,
) -> Result<Option<String>, String> {
    let response: LoadCodeAssistResponse = post_json(
        client,
        LOAD_CODE_ASSIST_URL,
        access_token,
        user_agent,
        load_code_assist_request_body(),
        "failed to fetch Antigravity Code Assist project",
        "failed to parse Antigravity Code Assist project",
    )
    .await?;

    Ok(response
        .cloudaicompanion_project
        .map(|project| project.trim().to_string())
        .filter(|project| !project.is_empty()))
}

async fn post_json<T>(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
    user_agent: &str,
    body: Value,
    fetch_error: &str,
    parse_error: &str,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let response = client
        .post(url)
        .bearer_auth(access_token)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("{fetch_error}: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(
            "authentication failed — try restarting Antigravity to refresh your Google login"
                .to_string(),
        );
    }

    if !response.status().is_success() {
        return Err(format!("{fetch_error}: HTTP {}", response.status()));
    }

    response
        .json::<T>()
        .await
        .map_err(|e| format!("{parse_error}: {e}"))
}

fn quota_summary_request_body() -> Value {
    json!({})
}

fn load_code_assist_request_body() -> Value {
    json!({
        "cloudaicompanionProject": null,
        "metadata": {
            "ideType": "IDE_UNSPECIFIED",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI",
            "duetProject": null,
        }
    })
}

fn model_quota_request_body(project: &str) -> Value {
    json!({ "project": project })
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

    #[test]
    fn test_deserialize_model_bucket_id() {
        let json = r#"{
            "buckets": [
                {
                    "modelId": "gemini-3.1-pro-preview",
                    "remainingFraction": 0.5856,
                    "resetTime": "2026-06-16T17:20:00Z"
                }
            ]
        }"#;

        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.buckets[0].model_id.as_deref(),
            Some("gemini-3.1-pro-preview")
        );
    }

    #[test]
    fn test_deserialize_load_code_assist_project() {
        let json = r#"{
            "cloudaicompanionProject": "projects/example",
            "currentTier": {"id": "free-tier"}
        }"#;

        let response: LoadCodeAssistResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.cloudaicompanion_project.as_deref(),
            Some("projects/example")
        );
    }

    #[test]
    fn test_load_code_assist_request_body_matches_gemini_shape() {
        assert_eq!(
            load_code_assist_request_body(),
            json!({
                "cloudaicompanionProject": null,
                "metadata": {
                    "ideType": "IDE_UNSPECIFIED",
                    "platform": "PLATFORM_UNSPECIFIED",
                    "pluginType": "GEMINI",
                    "duetProject": null,
                }
            })
        );
    }

    #[test]
    fn test_model_quota_request_body_includes_project() {
        assert_eq!(
            model_quota_request_body("projects/example"),
            json!({ "project": "projects/example" })
        );
    }
}
