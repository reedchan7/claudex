use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Used for the User-Agent when the installed `claude` version can't be detected.
const FALLBACK_VERSION: &str = "2.1.150";

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuthCredentials,
}

#[derive(Deserialize)]
struct OAuthCredentials {
    #[serde(rename = "accessToken")]
    access_token: String,
}

pub fn parse_token_from_json(json: &str) -> Result<String, String> {
    let creds: Credentials =
        serde_json::from_str(json).map_err(|e| format!("could not parse credentials JSON: {e}"))?;
    Ok(creds.claude_ai_oauth.access_token)
}

/// Read the Claude OAuth access token, trying sources in priority order,
/// mirroring how Claude Code itself resolves the token:
///   1. `CLAUDE_CODE_OAUTH_TOKEN` environment variable (used directly)
///   2. macOS Keychain (service "Claude Code-credentials")
///   3. `$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json`)
pub fn read_token() -> Result<String, String> {
    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(token) = read_token_from_keychain() {
        return Ok(token);
    }

    read_token_from_file(&credentials_file_path())
}

#[cfg(target_os = "macos")]
fn read_token_from_keychain() -> Result<String, String> {
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
    parse_token_from_json(json.trim())
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

fn read_token_from_file(path: &Path) -> Result<String, String> {
    let json = std::fs::read_to_string(path).map_err(|_| {
        "could not find Claude credentials — sign in with Claude Code (run `claude`), \
         or set CLAUDE_CODE_OAUTH_TOKEN"
            .to_string()
    })?;
    parse_token_from_json(json.trim())
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
    fn test_parse_token_from_json_valid() {
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test","refreshToken":"sk-ant-ort01-x","expiresAt":9999999999000}}"#;
        assert_eq!(parse_token_from_json(json).unwrap(), "sk-ant-oat01-test");
    }

    #[test]
    fn test_parse_token_from_json_invalid_json() {
        assert!(parse_token_from_json("not json").is_err());
    }

    #[test]
    fn test_parse_token_from_json_missing_field() {
        assert!(parse_token_from_json(r#"{"other": "value"}"#).is_err());
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
    fn test_read_token_from_file_reads_and_parses() {
        let dir = std::env::temp_dir().join(format!("claudex-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-fromfile"}}"#,
        )
        .unwrap();

        let token = read_token_from_file(&path).unwrap();
        assert_eq!(token, "sk-ant-oat01-fromfile");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_token_from_file_missing() {
        let path = Path::new("/nonexistent/claudex/.credentials.json");
        assert!(read_token_from_file(path).is_err());
    }
}
