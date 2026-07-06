use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const KIMI_CODE_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_CODE_PLATFORM: &str = "kimi_code_cli";

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KimiCredentials {
    pub access_token: String,
    refresh_token: Option<String>,
    source: Option<CredentialSource>,
}

#[derive(Debug, Clone)]
struct CredentialSource {
    path: PathBuf,
    json: String,
}

#[derive(Deserialize)]
struct TokenRefresh {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
    token_type: Option<String>,
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

        return parse_credentials_json_with_source(
            &json,
            Some(CredentialSource {
                path: path.clone(),
                json: json.trim().to_string(),
            }),
        );
    }

    let path = paths.first().cloned().unwrap_or_default();
    Err(format!(
        "could not find Kimi Code credentials at {} — run `kimi login`",
        path.display()
    ))
}

#[cfg(test)]
fn parse_credentials_json(json: &str) -> Result<KimiCredentials, String> {
    parse_credentials_json_with_source(json, None)
}

fn parse_credentials_json_with_source(
    json: &str,
    source: Option<CredentialSource>,
) -> Result<KimiCredentials, String> {
    let auth: CredentialsFile = serde_json::from_str(json)
        .map_err(|e| format!("could not parse Kimi Code credentials: {e}"))?;
    let access_token = auth.access_token.trim().to_string();

    if access_token.is_empty() {
        return Err("Kimi Code credentials have no access token — run `kimi login`".to_string());
    }

    Ok(KimiCredentials {
        access_token,
        refresh_token: non_empty(auth.refresh_token.as_deref()).map(ToString::to_string),
        source,
    })
}

pub async fn refresh_credentials(credentials: &KimiCredentials) -> Result<KimiCredentials, String> {
    let refresh_token = credentials
        .refresh_token
        .as_deref()
        .ok_or_else(kimi_auth_error)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .post(token_url(&oauth_host()))
        .headers(kimi_device_headers()?)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(refresh_form_body(refresh_token))
        .send()
        .await
        .map_err(|e| format!("failed to refresh Kimi Code session: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || body.contains("invalid_grant")
        {
            return Err(kimi_auth_error());
        }
        return Err(format!(
            "failed to refresh Kimi Code session: HTTP {status}"
        ));
    }

    let refreshed = response
        .json::<TokenRefresh>()
        .await
        .map_err(|e| format!("failed to parse Kimi Code refresh response: {e}"))?;
    save_refreshed_credentials(credentials, &refreshed)
}

fn save_refreshed_credentials(
    credentials: &KimiCredentials,
    refreshed: &TokenRefresh,
) -> Result<KimiCredentials, String> {
    let Some(source) = &credentials.source else {
        return Ok(KimiCredentials {
            access_token: refreshed.access_token.clone(),
            refresh_token: refreshed
                .refresh_token
                .clone()
                .or_else(|| credentials.refresh_token.clone()),
            source: None,
        });
    };

    let updated = apply_refresh_to_credentials_json(&source.json, refreshed, current_time_secs()?)?;
    std::fs::write(&source.path, &updated)
        .map_err(|e| format!("could not update Kimi Code credentials file: {e}"))?;
    parse_credentials_json_with_source(
        &updated,
        Some(CredentialSource {
            path: source.path.clone(),
            json: updated.clone(),
        }),
    )
}

fn apply_refresh_to_credentials_json(
    json: &str,
    refreshed: &TokenRefresh,
    now_secs: i64,
) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(json)
        .map_err(|e| format!("could not parse Kimi Code credentials: {e}"))?;
    let record = value
        .as_object_mut()
        .ok_or_else(|| "could not parse Kimi Code credentials: expected JSON object".to_string())?;

    record.insert(
        "access_token".to_string(),
        Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh_token) = &refreshed.refresh_token {
        record.insert(
            "refresh_token".to_string(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_in) = refreshed.expires_in {
        record.insert("expires_in".to_string(), Value::from(expires_in));
        record.insert("expires_at".to_string(), Value::from(now_secs + expires_in));
    }
    if let Some(scope) = &refreshed.scope {
        record.insert("scope".to_string(), Value::String(scope.clone()));
    }
    if let Some(token_type) = &refreshed.token_type {
        record.insert("token_type".to_string(), Value::String(token_type.clone()));
    }

    serde_json::to_string(&value)
        .map_err(|e| format!("could not serialize Kimi Code credentials: {e}"))
}

fn refresh_form_body(refresh_token: &str) -> String {
    [
        ("client_id", KIMI_CODE_CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", form_encode(value)))
    .collect::<Vec<_>>()
    .join("&")
}

fn kimi_device_headers() -> Result<reqwest::header::HeaderMap, String> {
    kimi_device_headers_with(&kimi_home_dir(), &kimi_cli_version())
}

fn kimi_device_headers_with(
    home_dir: &Path,
    version: &str,
) -> Result<reqwest::header::HeaderMap, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in [
        ("X-Msh-Platform", KIMI_CODE_PLATFORM.to_string()),
        (
            "X-Msh-Version",
            required_ascii_header(version, "Kimi identity version")?,
        ),
        ("X-Msh-Device-Name", ascii_header(&hostname())),
        ("X-Msh-Device-Model", ascii_header(&device_model())),
        ("X-Msh-Os-Version", ascii_header(&os_release())),
        ("X-Msh-Device-Id", create_kimi_device_id(home_dir)),
    ] {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| format!("could not build Kimi Code identity headers: {e}"))?;
        let value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|e| format!("could not build Kimi Code identity headers: {e}"))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn kimi_cli_version() -> String {
    command_output("kimi", &["--version"]).unwrap_or_else(|| "0.0.0".to_string())
}

fn hostname() -> String {
    command_output("hostname", &[]).unwrap_or_else(|| "unknown".to_string())
}

fn device_model() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    let release = os_release();

    match std::env::consts::OS {
        "macos" => {
            let version = command_output("/usr/bin/sw_vers", &["-productVersion"])
                .unwrap_or_else(|| release.clone());
            format!("macOS {version} {arch}")
        }
        "windows" => format!("Windows {release} {arch}"),
        other => format!("{other} {release} {arch}"),
    }
}

fn os_release() -> String {
    command_output("/usr/bin/uname", &["-r"])
        .or_else(|| command_output("uname", &["-r"]))
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn create_kimi_device_id(home_dir: &Path) -> String {
    let path = home_dir.join("device_id");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let device_id = text.trim();
        if !device_id.is_empty() {
            return ascii_header(device_id);
        }
    }

    let device_id = random_uuid_v4();
    let _ = std::fs::create_dir_all(home_dir);
    let _ = std::fs::write(&path, &device_id);
    device_id
}

fn random_uuid_v4() -> String {
    let mut bytes = [0_u8; 16];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_err()
    {
        let fallback = format!(
            "{}:{}:{}",
            current_time_secs().unwrap_or_default(),
            std::process::id(),
            hostname()
        );
        let digest = Sha256::digest(fallback.as_bytes());
        bytes.copy_from_slice(&digest[..16]);
    }

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn ascii_header(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|c| matches!(*c, '\u{20}'..='\u{7e}'))
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn required_ascii_header(value: &str, field_name: &str) -> Result<String, String> {
    let cleaned = ascii_header(value);
    if cleaned == "unknown" {
        Err(format!("{field_name} must be a non-empty ASCII string"))
    } else {
        Ok(cleaned)
    }
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

fn oauth_host() -> String {
    std::env::var("KIMI_CODE_OAUTH_HOST")
        .or_else(|_| std::env::var("KIMI_OAUTH_HOST"))
        .unwrap_or_else(|_| DEFAULT_OAUTH_HOST.to_string())
}

fn token_url(oauth_host: &str) -> String {
    format!("{}/api/oauth/token", oauth_host.trim_end_matches('/'))
}

fn current_time_secs() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock is before Unix epoch: {e}"))?;
    Ok(duration.as_secs() as i64)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn kimi_auth_error() -> String {
    "authentication failed — run `kimi login` and sign in again".to_string()
}

fn credential_paths() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        kimi_home_dir().join("credentials").join("kimi-code.json"),
        home.join(".kimi")
            .join("credentials")
            .join("kimi-code.json"),
    ]
}

fn kimi_home_dir() -> PathBuf {
    match std::env::var_os("KIMI_CODE_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => home_dir().join(".kimi-code"),
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
        assert_eq!(
            credentials.refresh_token.as_deref(),
            Some("kimi-refresh-token")
        );
    }

    #[test]
    fn rejects_empty_access_token() {
        let err = parse_credentials_json(r#"{ "access_token": " " }"#).unwrap_err();

        assert!(err.contains("no access token"));
    }

    #[test]
    fn applies_refreshed_token_to_kimi_credentials_json() {
        let json = r#"{
            "access_token": "old-access",
            "refresh_token": "old-refresh",
            "expires_at": 1770000000,
            "scope": "openid profile email",
            "token_type": "Bearer",
            "extra": "keep"
        }"#;
        let refreshed = TokenRefresh {
            access_token: "new-access".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            expires_in: Some(3600),
            scope: Some("openid profile".to_string()),
            token_type: Some("Bearer".to_string()),
        };

        let updated = apply_refresh_to_credentials_json(json, &refreshed, 1000).unwrap();
        let value: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(value["access_token"], "new-access");
        assert_eq!(value["refresh_token"], "new-refresh");
        assert_eq!(value["expires_at"], 4600);
        assert_eq!(value["expires_in"], 3600);
        assert_eq!(value["scope"], "openid profile");
        assert_eq!(value["extra"], "keep");
    }

    #[test]
    fn builds_kimi_device_headers_with_existing_device_id() {
        let dir = test_dir("headers");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("device_id"), "device-123\n").unwrap();

        let headers = kimi_device_headers_with(&dir, "0.22.3").unwrap();

        assert_eq!(
            headers
                .get("x-msh-platform")
                .and_then(|value| value.to_str().ok()),
            Some(KIMI_CODE_PLATFORM)
        );
        assert_eq!(
            headers
                .get("x-msh-version")
                .and_then(|value| value.to_str().ok()),
            Some("0.22.3")
        );
        assert_eq!(
            headers
                .get("x-msh-device-id")
                .and_then(|value| value.to_str().ok()),
            Some("device-123")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn creates_stable_kimi_device_id_when_missing() {
        let dir = test_dir("device-id");
        let first = create_kimi_device_id(&dir);
        let second = create_kimi_device_id(&dir);

        assert_eq!(first, second);
        assert_eq!(first.len(), 36);
        assert_eq!(
            std::fs::read_to_string(dir.join("device_id")).unwrap(),
            first
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("claudex-kimi-auth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }
}
