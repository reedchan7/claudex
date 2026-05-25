use serde::Deserialize;
use std::process::Command;

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

pub fn read_token() -> Result<String, String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .map_err(|e| format!("could not run security command: {e}"))?;

    if !output.status.success() {
        return Err("could not read Claude credentials from Keychain".to_string());
    }

    let json =
        String::from_utf8(output.stdout).map_err(|e| format!("invalid keychain output: {e}"))?;

    parse_token_from_json(json.trim())
}

fn version_from_filename(name: &str) -> Option<String> {
    if name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        Some(name.to_string())
    } else {
        None
    }
}

pub fn get_claude_version() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let link_path = format!("{home}/.local/bin/claude");

    if let Ok(target) = std::fs::read_link(&link_path)
        && let Some(filename) = target.file_name()
        && let Some(version) = version_from_filename(&filename.to_string_lossy())
    {
        return version;
    }
    "2.1.150".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_from_json_valid() {
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test","refreshToken":"sk-ant-ort01-x","expiresAt":9999999999000}}"#;
        let token = parse_token_from_json(json).unwrap();
        assert_eq!(token, "sk-ant-oat01-test");
    }

    #[test]
    fn test_parse_token_from_json_invalid_json() {
        let result = parse_token_from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_token_from_json_missing_field() {
        let result = parse_token_from_json(r#"{"other": "value"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_version_from_filename_valid() {
        assert_eq!(
            version_from_filename("2.1.150"),
            Some("2.1.150".to_string())
        );
    }

    #[test]
    fn test_version_from_filename_non_version() {
        assert_eq!(version_from_filename("claude"), None);
    }

    #[test]
    fn test_version_from_filename_empty() {
        assert_eq!(version_from_filename(""), None);
    }
}
