use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub code: Option<i64>,
    pub success: Option<bool>,
    pub data: Option<UsageData>,
}

#[derive(Debug, Deserialize)]
pub struct UsageData {
    pub level: Option<String>,
    #[serde(default)]
    pub limits: Vec<QuotaLimit>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaLimit {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub unit: Option<i64>,
    pub percentage: Option<f64>,
    pub usage: Option<i64>,
    pub current_value: Option<i64>,
    pub next_reset_time: Option<i64>,
    #[serde(default)]
    pub usage_details: Vec<UsageDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDetail {
    pub model_code: Option<String>,
    pub usage: Option<i64>,
}

pub async fn fetch_usage(base_url: &str, api_key: &str) -> Result<UsageData, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let url = format!("{base_url}/api/monitor/usage/quota/limit");
    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("failed to fetch GLM usage data: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(
            "authentication failed — refresh your GLM session in ZCode, or update GLM_API_KEY"
                .to_string(),
        );
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch GLM usage data: HTTP {}",
            response.status()
        ));
    }

    let parsed: UsageResponse = response
        .json()
        .await
        .map_err(|e| format!("failed to parse GLM usage data: {e}"))?;

    if parsed.success == Some(false) || parsed.code.is_some_and(|code| code != 200) {
        return Err(
            "authentication failed — GLM rejected the request; refresh your session in ZCode"
                .to_string(),
        );
    }

    parsed
        .data
        .ok_or_else(|| "GLM usage data is missing from the response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_usage_response_maps_all_limits() {
        let json = r#"{
            "code": 200,
            "msg": "Operation successful",
            "success": true,
            "data": {
                "level": "pro",
                "limits": [
                    { "type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 1,
                      "nextResetTime": 1782411163852 },
                    { "type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 3,
                      "nextResetTime": 1782960156979 },
                    { "type": "TIME_LIMIT", "unit": 5, "number": 1, "usage": 1000,
                      "currentValue": 0, "remaining": 1000, "percentage": 0,
                      "nextResetTime": 1784947356991,
                      "usageDetails": [
                        { "modelCode": "search-prime", "usage": 0 },
                        { "modelCode": "web-reader", "usage": 0 },
                        { "modelCode": "zread", "usage": 0 }
                      ] }
                ]
            }
        }"#;

        let response: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.code, Some(200));
        assert_eq!(response.success, Some(true));

        let data = response.data.unwrap();
        assert_eq!(data.level.as_deref(), Some("pro"));
        assert_eq!(data.limits.len(), 3);

        let session = &data.limits[0];
        assert_eq!(session.kind.as_deref(), Some("TOKENS_LIMIT"));
        assert_eq!(session.unit, Some(3));
        assert_eq!(session.percentage, Some(1.0));
        assert_eq!(session.next_reset_time, Some(1782411163852));

        let mcp = &data.limits[2];
        assert_eq!(mcp.kind.as_deref(), Some("TIME_LIMIT"));
        assert_eq!(mcp.usage, Some(1000));
        assert_eq!(mcp.current_value, Some(0));
        assert_eq!(mcp.usage_details.len(), 3);
        assert_eq!(
            mcp.usage_details[0].model_code.as_deref(),
            Some("search-prime")
        );
    }

    #[test]
    fn deserialize_tolerates_unknown_fields_and_missing_limits() {
        let json = r#"{ "code": 200, "success": true, "data": { "level": "pro" }, "extra": 1 }"#;
        let response: UsageResponse = serde_json::from_str(json).unwrap();
        let data = response.data.unwrap();
        assert!(data.limits.is_empty());
    }
}
