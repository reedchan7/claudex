use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct AuthFile {
    tokens: Option<Tokens>,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    account_id: Option<String>,
}

pub struct CodexCredentials {
    pub access_token: String,
    pub account_id: Option<String>,
}

pub fn read_credentials() -> Result<CodexCredentials, String> {
    let path = codex_auth_path();
    let json = std::fs::read_to_string(&path).map_err(|_| {
        format!(
            "could not find Codex credentials at {} — sign in with Codex (run `codex`)",
            path.display()
        )
    })?;
    let auth: AuthFile =
        serde_json::from_str(&json).map_err(|e| format!("could not parse Codex auth.json: {e}"))?;
    let tokens = auth
        .tokens
        .ok_or("Codex auth.json has no tokens — sign in with Codex (run `codex`)")?;
    Ok(CodexCredentials {
        access_token: tokens.access_token,
        account_id: tokens.account_id,
    })
}

fn codex_auth_path() -> PathBuf {
    let home = home_dir();
    home.join(".codex").join("auth.json")
}

fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auth_json() {
        let json = r#"{
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": "ey...",
                "access_token": "ey-test-token",
                "refresh_token": "rt_xxx",
                "account_id": "809db5cf-test"
            },
            "last_refresh": "2026-05-26T01:41:15.813982Z"
        }"#;
        let auth: AuthFile = serde_json::from_str(json).unwrap();
        let tokens = auth.tokens.unwrap();
        assert_eq!(tokens.access_token, "ey-test-token");
        assert_eq!(tokens.account_id.unwrap(), "809db5cf-test");
    }

    #[test]
    fn test_parse_auth_json_no_account_id() {
        let json = r#"{
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "ey-token"
            }
        }"#;
        let auth: AuthFile = serde_json::from_str(json).unwrap();
        let tokens = auth.tokens.unwrap();
        assert_eq!(tokens.access_token, "ey-token");
        assert!(tokens.account_id.is_none());
    }

    #[test]
    fn test_parse_auth_json_no_tokens() {
        let json = r#"{"auth_mode": "api_key", "OPENAI_API_KEY": "sk-xxx"}"#;
        let auth: AuthFile = serde_json::from_str(json).unwrap();
        assert!(auth.tokens.is_none());
    }
}
