use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::process::Command;

const FALLBACK_AGY_VERSION: &str = "1.0.8";

#[derive(Debug, Deserialize)]
struct AntigravityCredentials {
    token: AntigravityToken,
}

#[derive(Debug, Deserialize)]
struct AntigravityToken {
    access_token: Option<String>,
}

pub async fn read_access_token() -> Result<String, String> {
    read_access_token_from_keyring()
}

#[cfg(target_os = "macos")]
fn read_access_token_from_keyring() -> Result<String, String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "gemini",
            "-a",
            "antigravity",
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
    parse_access_token_from_keyring_secret(secret.trim())
}

#[cfg(not(target_os = "macos"))]
fn read_access_token_from_keyring() -> Result<String, String> {
    Err(
        "Antigravity credentials are stored in the system keyring; this command currently supports macOS Keychain credentials"
            .to_string(),
    )
}

fn parse_access_token_from_keyring_secret(secret: &str) -> Result<String, String> {
    let json = keyring_secret_json(secret)?;
    let creds: AntigravityCredentials = serde_json::from_str(&json)
        .map_err(|e| format!("could not parse Antigravity keyring credentials: {e}"))?;

    non_empty(creds.token.access_token.as_deref())
        .map(ToString::to_string)
        .ok_or("Antigravity keyring credentials have no usable access token".to_string())
}

fn keyring_secret_json(secret: &str) -> Result<String, String> {
    let secret = secret.trim();
    if let Some(encoded) = secret.strip_prefix("go-keyring-base64:") {
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|e| format!("could not decode Antigravity keyring credentials: {e}"))?;
        String::from_utf8(bytes)
            .map_err(|e| format!("Antigravity keyring credentials are not UTF-8: {e}"))
    } else {
        Ok(secret.to_string())
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
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

    #[test]
    fn test_parse_antigravity_keyring_secret() {
        let secret = "go-keyring-base64:eyJ0b2tlbiI6eyJhY2Nlc3NfdG9rZW4iOiJ5YTI5LmFudGlncmF2aXR5IiwiZnJlc2hfdG9rZW4iOiIxLy9hZ3kiLCJ0b2tlbl90eXBlIjoiQmVhcmVyIiwiZXhwaXJ5IjoiMjAyNi0wNi0xNlQxMTo0NzozMCswODowMCJ9LCJhdXRoX21ldGhvZCI6ImNvbnN1bWVyIn0=";

        assert_eq!(
            parse_access_token_from_keyring_secret(secret).unwrap(),
            "ya29.antigravity"
        );
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
