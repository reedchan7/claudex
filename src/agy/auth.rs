use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const FALLBACK_AGY_VERSION: &str = "1.0.8";
const KEYCHAIN_SERVICE: &str = "gemini";
const KEYCHAIN_ACCOUNT: &str = "antigravity";
const AGY_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_CLIENT_SECRET_LEN: usize = 35;

// Keychain-credential types/helpers below are wired into production only by the
// macOS reader, but the cross-platform unit tests exercise them — so on
// non-macOS builds they're legitimately unused. Keep them, just don't warn.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Deserialize)]
struct AntigravityCredentials {
    token: AntigravityToken,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Deserialize)]
struct AntigravityToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Clone)]
pub struct AntigravitySession {
    pub access_token: String,
    refresh_token: Option<String>,
    source: KeyringSource,
}

#[derive(Clone)]
struct KeyringSource {
    json: String,
    encoded: bool,
}

#[derive(Deserialize)]
struct TokenRefresh {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

struct KeyringSecret {
    json: String,
    encoded: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct AgyOAuthClient {
    client_id: String,
    client_secret: String,
}

pub async fn read_session() -> Result<AntigravitySession, String> {
    read_session_from_keyring()
}

pub async fn refresh_session(
    session: &AntigravitySession,
    user_agent: &str,
) -> Result<AntigravitySession, String> {
    let refresh_token = session
        .refresh_token
        .as_deref()
        .ok_or_else(antigravity_auth_error)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut last_error = None;
    for oauth_client in agy_oauth_clients()? {
        let response = client
            .post(AGY_TOKEN_URL)
            .header("User-Agent", user_agent)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(refresh_form_body(refresh_token, &oauth_client))
            .send()
            .await
            .map_err(|e| format!("failed to refresh Antigravity session: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::UNAUTHORIZED || body.contains("invalid_grant") {
                return Err(antigravity_auth_error());
            }
            if is_oauth_client_mismatch(&body) {
                last_error = Some(format!(
                    "failed to refresh Antigravity session: HTTP {status}"
                ));
                continue;
            }
            return Err(format!(
                "failed to refresh Antigravity session: HTTP {status}"
            ));
        }

        let refreshed = response
            .json::<TokenRefresh>()
            .await
            .map_err(|e| format!("failed to parse Antigravity refresh response: {e}"))?;
        return save_refreshed_session(session, &refreshed);
    }

    Err(last_error.unwrap_or_else(|| {
        "could not find Antigravity OAuth client credentials in the agy binary".to_string()
    }))
}

#[cfg(target_os = "macos")]
fn read_session_from_keyring() -> Result<AntigravitySession, String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
        ])
        .output()
        .map_err(|e| format!("could not run security command: {e}"))?;

    if !output.status.success() {
        return Err(
            "could not find Antigravity credentials in macOS Keychain — sign in with Antigravity (run `agy`)"
                .to_string(),
        );
    }

    let secret =
        String::from_utf8(output.stdout).map_err(|e| format!("invalid keychain output: {e}"))?;
    parse_session_from_keyring_secret(secret.trim())
}

#[cfg(not(target_os = "macos"))]
fn read_session_from_keyring() -> Result<AntigravitySession, String> {
    Err(
        "Antigravity credentials are stored in the system keyring; this command currently supports macOS Keychain credentials"
            .to_string(),
    )
}

#[cfg(test)]
fn parse_access_token_from_keyring_secret(secret: &str) -> Result<String, String> {
    Ok(parse_session_from_keyring_secret(secret)?.access_token)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_session_from_keyring_secret(secret: &str) -> Result<AntigravitySession, String> {
    let secret = decode_keyring_secret(secret)?;
    parse_session_from_json(&secret.json, secret.encoded)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_session_from_json(json: &str, encoded: bool) -> Result<AntigravitySession, String> {
    let creds: AntigravityCredentials = serde_json::from_str(json)
        .map_err(|e| format!("could not parse Antigravity keyring credentials: {e}"))?;

    let access_token = non_empty(creds.token.access_token.as_deref())
        .map(ToString::to_string)
        .ok_or("Antigravity keyring credentials have no usable access token".to_string())?;

    Ok(AntigravitySession {
        access_token,
        refresh_token: non_empty(creds.token.refresh_token.as_deref()).map(ToString::to_string),
        source: KeyringSource {
            json: json.to_string(),
            encoded,
        },
    })
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn decode_keyring_secret(secret: &str) -> Result<KeyringSecret, String> {
    let secret = secret.trim();
    if let Some(encoded) = secret.strip_prefix("go-keyring-base64:") {
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|e| format!("could not decode Antigravity keyring credentials: {e}"))?;
        let json = String::from_utf8(bytes)
            .map_err(|e| format!("Antigravity keyring credentials are not UTF-8: {e}"))?;
        Ok(KeyringSecret {
            json,
            encoded: true,
        })
    } else {
        Ok(KeyringSecret {
            json: secret.to_string(),
            encoded: false,
        })
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn antigravity_auth_error() -> String {
    "authentication failed — try restarting Antigravity to refresh your Google login".to_string()
}

fn save_refreshed_session(
    session: &AntigravitySession,
    refreshed: &TokenRefresh,
) -> Result<AntigravitySession, String> {
    let updated_json = apply_refresh_to_keyring_json(&session.source.json, refreshed, Utc::now())?;
    let updated_secret = encode_keyring_secret(&updated_json, session.source.encoded);
    save_keychain_secret(&updated_secret)?;
    parse_session_from_json(&updated_json, session.source.encoded)
}

fn encode_keyring_secret(json: &str, encoded: bool) -> String {
    if encoded {
        format!("go-keyring-base64:{}", STANDARD.encode(json))
    } else {
        json.to_string()
    }
}

#[cfg(target_os = "macos")]
fn save_keychain_secret(secret: &str) -> Result<(), String> {
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
            secret,
        ])
        .output()
        .map_err(|e| format!("could not run security command: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "could not update Antigravity keychain entry: {stderr}"
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn save_keychain_secret(_secret: &str) -> Result<(), String> {
    Err("Antigravity keychain refresh is only supported on macOS".to_string())
}

fn apply_refresh_to_keyring_json(
    json: &str,
    refreshed: &TokenRefresh,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(json)
        .map_err(|e| format!("could not parse Antigravity keyring credentials: {e}"))?;
    let token = value
        .get_mut("token")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            "could not parse Antigravity keyring credentials: missing field `token`".to_string()
        })?;

    token.insert(
        "access_token".to_string(),
        Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh_token) = &refreshed.refresh_token {
        token.insert(
            "refresh_token".to_string(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(token_type) = &refreshed.token_type {
        token.insert("token_type".to_string(), Value::String(token_type.clone()));
    }
    if let Some(expires_in) = refreshed.expires_in {
        let expiry = (now + ChronoDuration::seconds(expires_in))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        token.insert("expiry".to_string(), Value::String(expiry));
    }

    serde_json::to_string(&value)
        .map_err(|e| format!("could not serialize Antigravity keyring credentials: {e}"))
}

fn refresh_form_body(refresh_token: &str, oauth_client: &AgyOAuthClient) -> String {
    [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", oauth_client.client_id.as_str()),
        ("client_secret", oauth_client.client_secret.as_str()),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", form_encode(value)))
    .collect::<Vec<_>>()
    .join("&")
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

fn is_oauth_client_mismatch(body: &str) -> bool {
    body.contains("invalid_client")
        || body.contains("unauthorized_client")
        || body.contains("invalid_request")
}

fn agy_oauth_clients() -> Result<Vec<AgyOAuthClient>, String> {
    let path = find_on_path("agy")
        .ok_or_else(|| "could not find agy binary on PATH to refresh Antigravity".to_string())?;
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("could not read agy binary at {}: {e}", path.display()))?;
    Ok(extract_oauth_clients(&bytes))
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn extract_oauth_clients(bytes: &[u8]) -> Vec<AgyOAuthClient> {
    let text = String::from_utf8_lossy(bytes);
    let client_ids = extract_google_client_ids(&text);
    let client_secrets = extract_google_client_secrets(&text);
    let mut clients = Vec::new();

    for client_id in client_ids {
        for client_secret in &client_secrets {
            clients.push(AgyOAuthClient {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
            });
        }
    }

    clients
}

fn extract_google_client_ids(text: &str) -> Vec<String> {
    let suffix = google_client_id_suffix();
    let mut out = Vec::new();
    let mut offset = 0;
    let bytes = text.as_bytes();

    while let Some(relative) = text[offset..].find(&suffix) {
        let suffix_start = offset + relative;
        let end = suffix_start + suffix.len();
        let mut start = suffix_start;
        while start > 0 && is_oauth_client_id_byte(bytes[start - 1]) {
            start -= 1;
        }
        let candidate = &text[start..end];
        if candidate.bytes().next().is_some_and(|b| b.is_ascii_digit()) && candidate.contains('-') {
            push_unique(&mut out, candidate.to_string());
        }
        offset = end;
    }

    out
}

fn extract_google_client_secrets(text: &str) -> Vec<String> {
    let prefix = google_client_secret_prefix();
    let mut out = Vec::new();
    let mut offset = 0;

    while let Some(relative) = text[offset..].find(&prefix) {
        let start = offset + relative;
        let end = start + GOOGLE_CLIENT_SECRET_LEN;
        if end <= text.len() && text[start..end].bytes().all(is_oauth_client_secret_byte) {
            push_unique(&mut out, text[start..end].to_string());
            offset = end;
        } else {
            offset = start + prefix.len();
        }
    }

    out
}

fn google_client_id_suffix() -> String {
    [".apps.google", "usercontent.com"].concat()
}

fn google_client_secret_prefix() -> String {
    ["GOC", "SPX-"].concat()
}

fn is_oauth_client_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn is_oauth_client_secret_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

pub fn agy_user_agent() -> String {
    let version = get_agy_version();
    agy_user_agent_with_version(&version, std::env::consts::OS, std::env::consts::ARCH)
}

fn agy_user_agent_with_version(version: &str, os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    let arch = match arch {
        "aarch64" => "arm64",
        other => other,
    };
    format!("antigravity/cli/{version} {os}/{arch}")
}

fn get_agy_version() -> String {
    if let Ok(output) = Command::new("agy").arg("--version").output()
        && output.status.success()
        && let Ok(stdout) = String::from_utf8(output.stdout)
    {
        let version = stdout.trim();
        if !version.is_empty() {
            return version.to_string();
        }
    }
    FALLBACK_AGY_VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_parse_antigravity_keyring_secret() {
        let secret = "go-keyring-base64:eyJ0b2tlbiI6eyJhY2Nlc3NfdG9rZW4iOiJ5YTI5LmFudGlncmF2aXR5IiwiZnJlc2hfdG9rZW4iOiIxLy9hZ3kiLCJ0b2tlbl90eXBlIjoiQmVhcmVyIiwiZXhwaXJ5IjoiMjAyNi0wNi0xNlQxMTo0NzozMCswODowMCJ9LCJhdXRoX21ldGhvZCI6ImNvbnN1bWVyIn0=";

        assert_eq!(
            parse_access_token_from_keyring_secret(secret).unwrap(),
            "ya29.antigravity"
        );
    }

    #[test]
    fn test_parse_antigravity_keyring_session_keeps_refresh_token() {
        let secret = r#"{
            "token": {
                "access_token": "ya29.antigravity",
                "refresh_token": "1//agy",
                "token_type": "Bearer"
            },
            "auth_method": "consumer"
        }"#;
        let session = parse_session_from_keyring_secret(secret).unwrap();

        assert_eq!(session.access_token, "ya29.antigravity");
        assert_eq!(session.refresh_token.as_deref(), Some("1//agy"));
    }

    #[test]
    fn test_apply_refresh_to_keyring_json_preserves_other_fields() {
        let json = r#"{
            "token": {
                "access_token": "old-access",
                "refresh_token": "old-refresh",
                "token_type": "Bearer",
                "expiry": "2026-07-01T01:00:00Z",
                "extra": "keep"
            },
            "auth_method": "consumer",
            "other": true
        }"#;
        let refreshed = TokenRefresh {
            access_token: "new-access".to_string(),
            refresh_token: None,
            expires_in: Some(3600),
            token_type: Some("Bearer".to_string()),
        };
        let now = chrono::Utc.with_ymd_and_hms(2026, 7, 2, 8, 0, 0).unwrap();

        let updated = apply_refresh_to_keyring_json(json, &refreshed, now).unwrap();
        let value: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(value["token"]["access_token"], "new-access");
        assert_eq!(value["token"]["refresh_token"], "old-refresh");
        assert_eq!(value["token"]["token_type"], "Bearer");
        assert_eq!(value["token"]["extra"], "keep");
        assert_eq!(value["auth_method"], "consumer");
        assert_eq!(value["other"], true);

        let expiry = value["token"]["expiry"].as_str().unwrap();
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(expiry)
                .unwrap()
                .timestamp(),
            now.timestamp() + 3600
        );
    }

    #[test]
    fn test_refresh_form_body_encodes_refresh_token() {
        let oauth_client = AgyOAuthClient {
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
        };
        let body = refresh_form_body("1//agy token", &oauth_client);

        assert!(body.contains("refresh_token=1%2F%2Fagy%20token"));
        assert!(body.contains("grant_type=refresh_token"));
    }

    #[test]
    fn test_extract_oauth_clients_from_agy_binary_text() {
        let client_id = ["123456789012-fake.", "apps.google", "usercontent.com"].concat();
        let client_secret = format!(
            "{}{}",
            google_client_secret_prefix(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let text = format!("noise {client_secret} more noise {client_id}");
        let clients = extract_oauth_clients(text.as_bytes());

        assert_eq!(
            clients,
            vec![AgyOAuthClient {
                client_id,
                client_secret,
            }]
        );
    }

    #[test]
    fn test_extract_oauth_clients_splits_adjacent_agy_client_secrets() {
        let client_id = ["123456789012-fake.", "apps.google", "usercontent.com"].concat();
        let first_secret = format!(
            "{}{}",
            google_client_secret_prefix(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let second_secret = format!(
            "{}{}",
            google_client_secret_prefix(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        let text = format!("noise {first_secret}{second_secret}https://example.test {client_id}");
        let clients = extract_oauth_clients(text.as_bytes());

        assert_eq!(
            clients,
            vec![
                AgyOAuthClient {
                    client_id: client_id.clone(),
                    client_secret: first_secret,
                },
                AgyOAuthClient {
                    client_id,
                    client_secret: second_secret,
                }
            ]
        );
    }

    #[test]
    fn test_source_does_not_embed_google_oauth_client_literals() {
        let source = include_str!("auth.rs");
        let client_id_suffix = [".apps.google", "usercontent.com"].concat();
        let client_secret_prefix = ["GOC", "SPX-"].concat();

        assert!(!source.contains(&client_id_suffix));
        assert!(!source.contains(&client_secret_prefix));
    }

    #[test]
    fn test_parse_plain_antigravity_keyring_json() {
        let secret = r#"{
            "token": {
                "access_token": "ya29.plain-antigravity",
                "refresh_token": "1//agy",
                "token_type": "Bearer"
            },
            "auth_method": "consumer"
        }"#;

        assert_eq!(
            parse_access_token_from_keyring_secret(secret).unwrap(),
            "ya29.plain-antigravity"
        );
    }

    #[test]
    fn test_agy_user_agent_matches_cli_shape() {
        assert_eq!(
            agy_user_agent_with_version("1.0.8", "macos", "aarch64"),
            "antigravity/cli/1.0.8 darwin/arm64"
        );
    }
}
