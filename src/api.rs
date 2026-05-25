use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RateLimit {
    pub utilization: Option<f64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<i64>,
    pub used_credits: Option<i64>,
    pub utilization: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Utilization {
    pub five_hour: Option<RateLimit>,
    pub seven_day: Option<RateLimit>,
    pub seven_day_sonnet: Option<RateLimit>,
    pub extra_usage: Option<ExtraUsage>,
}

pub async fn fetch_utilization(token: &str, user_agent: &str) -> Result<Utilization, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("failed to fetch usage data: {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("authentication failed — try restarting Claude Code".to_string());
    }

    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch usage data: HTTP {}",
            response.status()
        ));
    }

    response
        .json::<Utilization>()
        .await
        .map_err(|e| format!("failed to parse usage data: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_full_response() {
        let json = r#"{
            "five_hour": {"utilization": 26.0, "resets_at": "2026-05-25T06:30:00.176572+00:00"},
            "seven_day": {"utilization": 5.0, "resets_at": "2026-05-29T19:00:00.176593+00:00"},
            "seven_day_sonnet": {"utilization": 3.0, "resets_at": "2026-05-29T19:00:01.176599+00:00"},
            "extra_usage": {"is_enabled": false, "monthly_limit": null, "used_credits": null, "utilization": null}
        }"#;
        let u: Utilization = serde_json::from_str(json).unwrap();
        assert_eq!(u.five_hour.as_ref().unwrap().utilization, Some(26.0));
        assert_eq!(u.seven_day.as_ref().unwrap().utilization, Some(5.0));
        assert_eq!(u.seven_day_sonnet.as_ref().unwrap().utilization, Some(3.0));
        assert!(!u.extra_usage.as_ref().unwrap().is_enabled);
    }

    #[test]
    fn test_deserialize_null_limits() {
        let json = r#"{
            "five_hour": null,
            "seven_day": null,
            "seven_day_sonnet": null,
            "extra_usage": null
        }"#;
        let u: Utilization = serde_json::from_str(json).unwrap();
        assert!(u.five_hour.is_none());
        assert!(u.seven_day.is_none());
        assert!(u.seven_day_sonnet.is_none());
    }

    #[test]
    fn test_deserialize_ignores_unknown_fields() {
        let json = r#"{
            "five_hour": {"utilization": 10.0, "resets_at": null},
            "seven_day_cowork": null,
            "iguana_necktie": null,
            "tangelo": null
        }"#;
        let u: Utilization = serde_json::from_str(json).unwrap();
        assert_eq!(u.five_hour.as_ref().unwrap().utilization, Some(10.0));
        assert!(u.seven_day.is_none());
    }

    #[test]
    fn test_deserialize_extra_usage_enabled() {
        let json = r#"{
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 5000,
                "used_credits": 1250,
                "utilization": 25.0
            }
        }"#;
        let u: Utilization = serde_json::from_str(json).unwrap();
        let extra = u.extra_usage.as_ref().unwrap();
        assert!(extra.is_enabled);
        assert_eq!(extra.monthly_limit, Some(5000));
        assert_eq!(extra.used_credits, Some(1250));
    }
}
