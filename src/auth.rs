use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Used for the User-Agent when the installed `claude` version can't be detected.
const FALLBACK_VERSION: &str = "2.1.150";
const CLAUDE_CODE_CLIENT_ID: &str = "https://claude.ai/oauth/claude-code-client-metadata";
const CLAUDE_CODE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuthCredentials,
}

#[derive(Deserialize)]
struct OAuthCredentials {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
}

#[derive(Clone)]
enum OAuthSource {
    Env,
    Keychain { account: String, json: String },
    File { path: PathBuf, json: String },
}

#[derive(Clone)]
pub struct OAuthSession {
    pub access_token: String,
    refresh_token: Option<String>,
    source: OAuthSource,
}

#[derive(Deserialize)]
struct TokenRefresh {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

fn parse_oauth_credentials(json: &str) -> Result<OAuthCredentials, String> {
    let creds: Credentials =
        serde_json::from_str(json).map_err(|e| format!("could not parse credentials JSON: {e}"))?;
    Ok(creds.claude_ai_oauth)
}

fn parse_oauth_session(json: &str, source: OAuthSource) -> Result<OAuthSession, String> {
    let creds = parse_oauth_credentials(json)?;
    Ok(OAuthSession {
        access_token: creds.access_token,
        refresh_token: creds.refresh_token,
        source,
    })
}

/// Read the Claude OAuth access token, trying sources in priority order,
/// mirroring how Claude Code itself resolves the token:
///   1. `CLAUDE_CODE_OAUTH_TOKEN` environment variable (used directly)
///   2. macOS Keychain (service "Claude Code-credentials")
///   3. `$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json`)
pub fn read_oauth_session() -> Result<OAuthSession, String> {
    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(OAuthSession {
                access_token: token.to_string(),
                refresh_token: None,
                source: OAuthSource::Env,
            });
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(session) = read_session_from_keychain() {
        return Ok(session);
    }

    read_session_from_file(&credentials_file_path())
}

#[cfg(target_os = "macos")]
fn read_session_from_keychain() -> Result<OAuthSession, String> {
    let account = std::env::var("USER").unwrap_or_else(|_| "claude-code-user".to_string());
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            &account,
            "-w",
            "-s",
            "Claude Code-credentials",
        ])
        .output()
        .map_err(|e| format!("could not run security command: {e}"))?;

    if !output.status.success() {
        return Err("keychain entry not found".to_string());
    }

    let json =
        String::from_utf8(output.stdout).map_err(|e| format!("invalid keychain output: {e}"))?;
    let json = json.trim().to_string();
    parse_oauth_session(
        &json,
        OAuthSource::Keychain {
            account,
            json: json.clone(),
        },
    )
}

fn credentials_file_path() -> PathBuf {
    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".claude"));
    config_dir.join(".credentials.json")
}

fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

fn read_session_from_file(path: &Path) -> Result<OAuthSession, String> {
    let json = std::fs::read_to_string(path).map_err(|_| {
        "could not find Claude credentials — sign in with Claude Code (run `claude`), \
         or set CLAUDE_CODE_OAUTH_TOKEN"
            .to_string()
    })?;
    parse_oauth_session(
        json.trim(),
        OAuthSource::File {
            path: path.to_path_buf(),
            json: json.trim().to_string(),
        },
    )
}

pub async fn refresh_oauth_session(
    session: &OAuthSession,
    user_agent: &str,
) -> Result<OAuthSession, String> {
    let refresh_token = session
        .refresh_token
        .as_deref()
        .ok_or_else(|| "authentication failed — try restarting Claude Code".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let response = client
        .post(CLAUDE_CODE_TOKEN_URL)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLAUDE_CODE_CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|e| format!("failed to refresh Claude Code session: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED || body.contains("invalid_grant") {
            return Err("authentication failed — try restarting Claude Code".to_string());
        }
        return Err(format!(
            "failed to refresh Claude Code session: HTTP {status}"
        ));
    }

    let refreshed = response
        .json::<TokenRefresh>()
        .await
        .map_err(|e| format!("failed to parse Claude Code refresh response: {e}"))?;
    save_refreshed_session(session, &refreshed)
}

fn save_refreshed_session(
    session: &OAuthSession,
    refreshed: &TokenRefresh,
) -> Result<OAuthSession, String> {
    let source = match &session.source {
        OAuthSource::Env => OAuthSource::Env,
        OAuthSource::Keychain { account, json } => {
            let updated =
                apply_refresh_to_credentials_json(json, refreshed, current_time_millis()?)?;
            save_keychain_credentials(account, &updated)?;
            OAuthSource::Keychain {
                account: account.clone(),
                json: updated,
            }
        }
        OAuthSource::File { path, json } => {
            let updated =
                apply_refresh_to_credentials_json(json, refreshed, current_time_millis()?)?;
            std::fs::write(path, &updated)
                .map_err(|e| format!("could not update Claude credentials file: {e}"))?;
            OAuthSource::File {
                path: path.clone(),
                json: updated,
            }
        }
    };

    Ok(OAuthSession {
        access_token: refreshed.access_token.clone(),
        refresh_token: refreshed
            .refresh_token
            .clone()
            .or_else(|| session.refresh_token.clone()),
        source,
    })
}

#[cfg(target_os = "macos")]
fn save_keychain_credentials(account: &str, json: &str) -> Result<(), String> {
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            account,
            "-s",
            "Claude Code-credentials",
            "-w",
            json,
        ])
        .output()
        .map_err(|e| format!("could not run security command: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "could not update Claude Code keychain entry: {stderr}"
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn save_keychain_credentials(_account: &str, _json: &str) -> Result<(), String> {
    Err("Claude Code keychain refresh is only supported on macOS".to_string())
}

fn current_time_millis() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock is before Unix epoch: {e}"))?;
    Ok(duration.as_millis() as i64)
}

fn apply_refresh_to_credentials_json(
    json: &str,
    refreshed: &TokenRefresh,
    now_millis: i64,
) -> Result<String, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("could not parse credentials JSON: {e}"))?;
    let claude = value
        .get_mut("claudeAiOauth")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| {
            "could not parse credentials JSON: missing field `claudeAiOauth`".to_string()
        })?;

    claude.insert(
        "accessToken".to_string(),
        serde_json::Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh_token) = &refreshed.refresh_token {
        claude.insert(
            "refreshToken".to_string(),
            serde_json::Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_in) = refreshed.expires_in {
        claude.insert(
            "expiresAt".to_string(),
            serde_json::Value::from(now_millis + expires_in * 1000),
        );
    }

    serde_json::to_string(&value)
        .map_err(|e| format!("could not serialize Claude credentials JSON: {e}"))
}

/// Detect the installed Claude Code version for the User-Agent header by
/// running `claude --version` (works regardless of install location). Falls
/// back to a known version string if the CLI isn't found or output is unexpected.
pub fn get_claude_version() -> String {
    if let Ok(output) = Command::new("claude").arg("--version").output()
        && output.status.success()
        && let Ok(stdout) = String::from_utf8(output.stdout)
        && let Some(version) = parse_version(&stdout)
    {
        return version;
    }
    FALLBACK_VERSION.to_string()
}

/// Extract the version number from `claude --version` output,
/// e.g. "2.1.150 (Claude Code)" -> "2.1.150".
fn parse_version(output: &str) -> Option<String> {
    let token = output.split_whitespace().next()?;
    if token.chars().next()?.is_ascii_digit() {
        Some(token.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oauth_credentials_valid() {
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test","refreshToken":"sk-ant-ort01-x","expiresAt":9999999999000}}"#;
        assert_eq!(
            parse_oauth_credentials(json).unwrap().access_token,
            "sk-ant-oat01-test"
        );
    }

    #[test]
    fn test_parse_oauth_credentials_invalid_json() {
        assert!(parse_oauth_credentials("not json").is_err());
    }

    #[test]
    fn test_parse_oauth_credentials_missing_field() {
        assert!(parse_oauth_credentials(r#"{"other": "value"}"#).is_err());
    }

    #[test]
    fn test_parse_version_valid() {
        assert_eq!(
            parse_version("2.1.150 (Claude Code)"),
            Some("2.1.150".to_string())
        );
    }

    #[test]
    fn test_parse_version_trailing_newline() {
        assert_eq!(parse_version("2.1.150\n"), Some("2.1.150".to_string()));
    }

    #[test]
    fn test_parse_version_non_version() {
        assert_eq!(parse_version("claude code"), None);
    }

    #[test]
    fn test_parse_version_empty() {
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn test_read_session_from_file_reads_and_parses() {
        let dir = std::env::temp_dir().join(format!("claudex-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-fromfile"}}"#,
        )
        .unwrap();

        let session = read_session_from_file(&path).unwrap();
        assert_eq!(session.access_token, "sk-ant-oat01-fromfile");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_session_from_file_missing() {
        let path = Path::new("/nonexistent/claudex/.credentials.json");
        assert!(read_session_from_file(path).is_err());
    }

    #[test]
    fn test_apply_refresh_preserves_other_credentials() {
        let json = r#"{
            "mcpOAuth": {"figma": {"accessToken": "keep-me"}},
            "claudeAiOauth": {
                "accessToken": "old-access",
                "refreshToken": "old-refresh",
                "expiresAt": 1000,
                "subscriptionType": "max"
            }
        }"#;
        let refreshed = TokenRefresh {
            access_token: "new-access".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            expires_in: Some(3600),
        };

        let updated = apply_refresh_to_credentials_json(json, &refreshed, 1_700_000_000_000)
            .expect("credentials update should succeed");
        let value: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(value["mcpOAuth"]["figma"]["accessToken"], "keep-me");
        assert_eq!(value["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(value["claudeAiOauth"]["refreshToken"], "new-refresh");
        assert_eq!(value["claudeAiOauth"]["expiresAt"], 1_700_003_600_000_i64);
        assert_eq!(value["claudeAiOauth"]["subscriptionType"], "max");
    }

    #[test]
    fn test_apply_refresh_keeps_existing_refresh_token_when_absent() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "old-access",
                "refreshToken": "old-refresh"
            }
        }"#;
        let refreshed = TokenRefresh {
            access_token: "new-access".to_string(),
            refresh_token: None,
            expires_in: None,
        };

        let updated = apply_refresh_to_credentials_json(json, &refreshed, 1_700_000_000_000)
            .expect("credentials update should succeed");
        let value: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(value["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(value["claudeAiOauth"]["refreshToken"], "old-refresh");
        assert_eq!(value["claudeAiOauth"]["expiresAt"], serde_json::Value::Null);
    }
}
