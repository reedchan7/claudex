use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    access_token: String,
}

#[derive(Debug)]
pub struct KimiCredentials {
    pub access_token: String,
}

pub fn read_credentials() -> Result<KimiCredentials, String> {
    read_credentials_from_paths(&credential_paths())
}

fn read_credentials_from_paths(paths: &[PathBuf]) -> Result<KimiCredentials, String> {
    for path in paths {
        let json = match std::fs::read_to_string(path) {
            Ok(json) => json,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!(
                    "could not read Kimi Code credentials at {}: {e}",
                    path.display()
                ));
            }
        };

        return parse_credentials_json(&json);
    }

    let path = paths.first().cloned().unwrap_or_default();
    Err(format!(
        "could not find Kimi Code credentials at {} — run `kimi login`",
        path.display()
    ))
}

fn parse_credentials_json(json: &str) -> Result<KimiCredentials, String> {
    let auth: CredentialsFile = serde_json::from_str(json)
        .map_err(|e| format!("could not parse Kimi Code credentials: {e}"))?;
    let access_token = auth.access_token.trim().to_string();

    if access_token.is_empty() {
        return Err("Kimi Code credentials have no access token — run `kimi login`".to_string());
    }

    Ok(KimiCredentials { access_token })
}

fn credential_paths() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join(".kimi-code")
            .join("credentials")
            .join("kimi-code.json"),
        home.join(".kimi")
            .join("credentials")
            .join("kimi-code.json"),
    ]
}

fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kimi_code_credentials() {
        let json = r#"{
            "access_token": "kimi-access-token",
            "refresh_token": "kimi-refresh-token",
            "expires_at": 1770000000000,
            "scope": "openid profile email",
            "token_type": "Bearer"
        }"#;

        let credentials = parse_credentials_json(json).unwrap();

        assert_eq!(credentials.access_token, "kimi-access-token");
    }

    #[test]
    fn rejects_empty_access_token() {
        let err = parse_credentials_json(r#"{ "access_token": " " }"#).unwrap_err();

        assert!(err.contains("no access token"));
    }
}
