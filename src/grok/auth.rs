use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct GrokCredentials {
    pub access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<String>,
    oidc_issuer: Option<String>,
    oidc_client_id: Option<String>,
    /// Map key under which this session is stored in auth.json.
    storage_key: Option<String>,
    /// Original auth.json body, used when writing a refreshed token back.
    source_json: Option<String>,
}

#[derive(Deserialize)]
struct TokenRefresh {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[serde(default)]
    expires_at: Option<String>,
}

pub fn read_credentials() -> Result<GrokCredentials, String> {
    // Match Grok itself: a stored interactive session takes precedence over
    // `XAI_API_KEY`. The API key is only a fallback when no session exists.
    let path = grok_auth_path();
    match std::fs::read_to_string(&path) {
        Ok(json) => match parse_auth_json(&json) {
            Ok(creds) => return Ok(creds),
            Err(e) if e.contains("no sessions") || e.contains("no usable access token") => {}
            Err(e) => return Err(e),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "could not read Grok credentials at {}: {e}",
                path.display()
            ));
        }
    }

    if let Ok(token) = std::env::var("XAI_API_KEY") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(GrokCredentials {
                access_token: token,
                refresh_token: None,
                expires_at: None,
                oidc_issuer: None,
                oidc_client_id: None,
                storage_key: None,
                source_json: None,
            });
        }
    }

    Err(format!(
        "could not find Grok credentials at {} — run `grok login`",
        path.display()
    ))
}

fn parse_auth_json(json: &str) -> Result<GrokCredentials, String> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| format!("could not parse Grok auth.json: {e}"))?;
    let record = root
        .as_object()
        .ok_or_else(|| "could not parse Grok auth.json: expected JSON object".to_string())?;

    if record.is_empty() {
        return Err("Grok auth.json has no sessions — run `grok login`".to_string());
    }

    // Prefer a non-expired session; otherwise take the first entry.
    let mut fallback: Option<(String, &Value)> = None;
    for (key, value) in record {
        let Some(session) = value.as_object() else {
            continue;
        };
        let access_token = session
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(access_token) = access_token else {
            continue;
        };

        let creds = GrokCredentials {
            access_token: access_token.to_string(),
            refresh_token: non_empty_string(session.get("refresh_token")),
            expires_at: non_empty_string(session.get("expires_at")),
            oidc_issuer: non_empty_string(session.get("oidc_issuer")),
            oidc_client_id: non_empty_string(session.get("oidc_client_id")),
            storage_key: Some(key.clone()),
            source_json: Some(json.trim().to_string()),
        };

        if !creds.is_expired() {
            return Ok(creds);
        }
        if fallback.is_none() {
            fallback = Some((key.clone(), value));
        }
    }

    if let Some((key, value)) = fallback {
        let session = value.as_object().unwrap();
        return Ok(GrokCredentials {
            access_token: session
                .get("key")
                .and_then(Value::as_str)
                .unwrap()
                .trim()
                .to_string(),
            refresh_token: non_empty_string(session.get("refresh_token")),
            expires_at: non_empty_string(session.get("expires_at")),
            oidc_issuer: non_empty_string(session.get("oidc_issuer")),
            oidc_client_id: non_empty_string(session.get("oidc_client_id")),
            storage_key: Some(key),
            source_json: Some(json.trim().to_string()),
        });
    }

    Err("Grok auth.json has no usable access token — run `grok login`".to_string())
}

impl GrokCredentials {
    pub fn is_expired(&self) -> bool {
        let Some(expires_at) = &self.expires_at else {
            return false;
        };
        match chrono::DateTime::parse_from_rfc3339(expires_at) {
            Ok(dt) => dt.timestamp() <= current_time_secs().unwrap_or(0),
            Err(_) => false,
        }
    }
}

pub async fn refresh_credentials(credentials: &GrokCredentials) -> Result<GrokCredentials, String> {
    let refresh_token = credentials
        .refresh_token
        .as_deref()
        .ok_or_else(grok_auth_error)?;
    let issuer = credentials
        .oidc_issuer
        .as_deref()
        .unwrap_or("https://auth.x.ai");
    let client_id = credentials
        .oidc_client_id
        .as_deref()
        .ok_or_else(grok_auth_error)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let body = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ]
    .into_iter()
    .map(|(k, v)| format!("{k}={}", form_encode(v)))
    .collect::<Vec<_>>()
    .join("&");

    let response = client
        .post(token_url(issuer))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("failed to refresh Grok session: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || body.contains("invalid_grant")
        {
            return Err(grok_auth_error());
        }
        return Err(format!("failed to refresh Grok session: HTTP {status}"));
    }

    let refreshed = response
        .json::<TokenRefresh>()
        .await
        .map_err(|e| format!("failed to parse Grok refresh response: {e}"))?;
    save_refreshed_credentials(credentials, &refreshed)
}

fn save_refreshed_credentials(
    credentials: &GrokCredentials,
    refreshed: &TokenRefresh,
) -> Result<GrokCredentials, String> {
    let (Some(storage_key), Some(source_json)) =
        (&credentials.storage_key, &credentials.source_json)
    else {
        return Ok(GrokCredentials {
            access_token: refreshed.access_token.clone(),
            refresh_token: refreshed
                .refresh_token
                .clone()
                .or_else(|| credentials.refresh_token.clone()),
            expires_at: refreshed
                .expires_at
                .clone()
                .or_else(|| expires_at_from_expires_in(refreshed.expires_in)),
            oidc_issuer: credentials.oidc_issuer.clone(),
            oidc_client_id: credentials.oidc_client_id.clone(),
            storage_key: None,
            source_json: None,
        });
    };

    let updated =
        apply_refresh_to_auth_json(source_json, storage_key, refreshed, current_time_secs()?)?;
    let path = grok_auth_path();
    std::fs::write(&path, &updated).map_err(|e| format!("could not update Grok auth.json: {e}"))?;
    parse_auth_json(&updated)
}

fn apply_refresh_to_auth_json(
    json: &str,
    storage_key: &str,
    refreshed: &TokenRefresh,
    now_secs: i64,
) -> Result<String, String> {
    let mut root: Value =
        serde_json::from_str(json).map_err(|e| format!("could not parse Grok auth.json: {e}"))?;
    let record = root
        .as_object_mut()
        .ok_or_else(|| "could not parse Grok auth.json: expected JSON object".to_string())?;
    let session = record
        .get_mut(storage_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "could not find Grok session entry to update".to_string())?;

    session.insert(
        "key".to_string(),
        Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh_token) = &refreshed.refresh_token {
        session.insert(
            "refresh_token".to_string(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_at) = &refreshed.expires_at {
        session.insert("expires_at".to_string(), Value::String(expires_at.clone()));
    } else if let Some(expires_in) = refreshed.expires_in {
        let expires_at = chrono::DateTime::from_timestamp(now_secs + expires_in, 0)
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
            .ok_or_else(|| "invalid expires_in from Grok refresh response".to_string())?;
        session.insert("expires_at".to_string(), Value::String(expires_at));
    }

    serde_json::to_string_pretty(&root)
        .map_err(|e| format!("could not serialize Grok auth.json: {e}"))
}

fn expires_at_from_expires_in(expires_in: Option<i64>) -> Option<String> {
    let expires_in = expires_in?;
    let now = current_time_secs().ok()?;
    chrono::DateTime::from_timestamp(now + expires_in, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
}

fn token_url(issuer: &str) -> String {
    format!("{}/oauth2/token", issuer.trim_end_matches('/'))
}

fn form_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn current_time_secs() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock is before Unix epoch: {e}"))?;
    Ok(duration.as_secs() as i64)
}

fn grok_auth_error() -> String {
    "authentication failed — run `grok login` and sign in again".to_string()
}

fn grok_auth_path() -> PathBuf {
    match std::env::var_os("GROK_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path).join("auth.json"),
        _ => home_dir().join(".grok").join("auth.json"),
    }
}

fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_oidc_session_map() {
        let json = r#"{
            "https://auth.x.ai::client-id": {
                "key": "access-token",
                "refresh_token": "refresh-token",
                "expires_at": "2099-01-01T00:00:00Z",
                "oidc_issuer": "https://auth.x.ai",
                "oidc_client_id": "client-id",
                "auth_mode": "oidc"
            }
        }"#;

        let creds = parse_auth_json(json).unwrap();
        assert_eq!(creds.access_token, "access-token");
        assert_eq!(creds.refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(creds.oidc_client_id.as_deref(), Some("client-id"));
        assert!(!creds.is_expired());
    }

    #[test]
    fn prefers_non_expired_session() {
        let json = r#"{
            "https://auth.x.ai::old": {
                "key": "old-token",
                "expires_at": "2000-01-01T00:00:00Z",
                "oidc_issuer": "https://auth.x.ai",
                "oidc_client_id": "old",
                "refresh_token": "old-refresh"
            },
            "https://auth.x.ai::new": {
                "key": "new-token",
                "expires_at": "2099-01-01T00:00:00Z",
                "oidc_issuer": "https://auth.x.ai",
                "oidc_client_id": "new",
                "refresh_token": "new-refresh"
            }
        }"#;

        let creds = parse_auth_json(json).unwrap();
        assert_eq!(creds.access_token, "new-token");
    }

    #[test]
    fn rejects_empty_auth_file() {
        let err = parse_auth_json("{}").unwrap_err();
        assert!(err.contains("no sessions"));
    }

    #[test]
    fn applies_refresh_to_session_entry() {
        let json = r#"{
            "https://auth.x.ai::client": {
                "key": "old-access",
                "refresh_token": "old-refresh",
                "expires_at": "2000-01-01T00:00:00Z",
                "email": "keep@example.com"
            }
        }"#;
        let refreshed = TokenRefresh {
            access_token: "new-access".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            expires_in: Some(3600),
            expires_at: None,
        };

        let updated =
            apply_refresh_to_auth_json(json, "https://auth.x.ai::client", &refreshed, 1_000)
                .unwrap();
        let value: Value = serde_json::from_str(&updated).unwrap();
        let session = &value["https://auth.x.ai::client"];

        assert_eq!(session["key"], "new-access");
        assert_eq!(session["refresh_token"], "new-refresh");
        assert_eq!(session["email"], "keep@example.com");
        assert!(session["expires_at"].as_str().unwrap().starts_with("1970-"));
    }
}
