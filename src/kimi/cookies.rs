use aes::Aes128;
use aes::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use base64::Engine as _;
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

/// Web tokens for the membership API.
///
/// Coding-plan OAuth tokens are rejected there (different signing method /
/// scope). A signed-in Kimi web session is required: `KIMI_AUTH_TOKEN`, the
/// `kimi-auth` cookie, or Local Storage `access_token` / `refresh_token`
/// from a local browser on macOS.
#[derive(Debug, Default)]
pub struct WebAuth {
    pub access_tokens: Vec<String>,
    pub refresh_tokens: Vec<String>,
}

pub fn web_auth(preferred_user_id: Option<&str>) -> WebAuth {
    let mut env_tokens = Vec::new();
    if let Ok(value) = std::env::var("KIMI_AUTH_TOKEN") {
        push_unique(&mut env_tokens, value);
    }

    #[cfg(target_os = "macos")]
    let browser = macos_browser_auth();
    #[cfg(not(target_os = "macos"))]
    let browser = WebAuth::default();

    WebAuth {
        access_tokens: order_tokens(env_tokens, browser.access_tokens, preferred_user_id),
        refresh_tokens: order_tokens(Vec::new(), browser.refresh_tokens, preferred_user_id),
    }
}

pub fn web_auth_tokens(preferred_user_id: Option<&str>) -> Vec<String> {
    web_auth(preferred_user_id).access_tokens
}

fn order_tokens(
    env_tokens: Vec<String>,
    mut browser_tokens: Vec<String>,
    preferred_user_id: Option<&str>,
) -> Vec<String> {
    if let Some(user_id) = preferred_user_id {
        browser_tokens.sort_by_key(|token| {
            if jwt_sub(token).as_deref() == Some(user_id) {
                0
            } else {
                1
            }
        });
    }

    let mut tokens = env_tokens;
    for token in browser_tokens {
        push_unique(&mut tokens, token);
    }
    tokens
}

fn push_unique(tokens: &mut Vec<String>, token: impl Into<String>) {
    let token = token.into().trim().to_string();
    if !token.is_empty() && !tokens.iter().any(|existing| existing == &token) {
        tokens.push(token);
    }
}

pub(crate) fn jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let mut payload = payload.replace('-', "+").replace('_', "/");
    while payload.len() % 4 != 0 {
        payload.push('=');
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn jwt_sub(token: &str) -> Option<String> {
    jwt_payload(token)?
        .get("sub")?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// True when the JWT `exp` claim is present and in the past.
/// Tokens without `exp` are treated as live so they can still be tried.
pub(crate) fn jwt_expired(token: &str) -> bool {
    let Some(exp) = jwt_payload(token)
        .as_ref()
        .and_then(|payload| payload.get("exp"))
        .and_then(serde_json::Value::as_i64)
    else {
        return false;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    exp <= now
}

#[cfg(target_os = "macos")]
fn macos_browser_auth() -> WebAuth {
    let mut ls_access = Vec::new();
    let mut cookie_access = Vec::new();
    let mut refresh = Vec::new();

    for (db, service, account) in chromium_cookie_dbs() {
        let password = keychain_password(service, account);
        for token in tokens_from_cookie_db(&db, password.as_deref()) {
            push_unique(&mut cookie_access, token);
        }
    }
    for token in tokens_from_cookie_db(&kimi_desktop_cookies_db(), None) {
        push_unique(&mut cookie_access, token);
    }
    for profile in chromium_profile_dirs() {
        if !ls_access.is_empty() || !refresh.is_empty() {
            break;
        }
        let dir = profile.join("Local Storage").join("leveldb");
        if !dir.is_dir() {
            continue;
        }
        let extracted = super::local_storage::tokens_from_dir(&dir);
        for token in extracted.access {
            push_unique(&mut ls_access, token);
        }
        for token in extracted.refresh {
            push_unique(&mut refresh, token);
        }
    }

    let mut access_tokens = ls_access;
    for token in cookie_access {
        push_unique(&mut access_tokens, token);
    }
    WebAuth {
        access_tokens,
        refresh_tokens: refresh,
    }
}

#[cfg(target_os = "macos")]
fn chromium_profile_dirs() -> Vec<PathBuf> {
    let mut profiles = Vec::new();
    for (root, _, _) in chromium_user_data_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != "Default" && !name.starts_with("Profile ") {
                continue;
            }
            profiles.push(entry.path());
        }
    }
    profiles
}

#[cfg(target_os = "macos")]
fn chromium_user_data_roots() -> Vec<(PathBuf, &'static str, &'static str)> {
    let home = home_dir();
    vec![
        (
            home.join("Library/Application Support/BraveSoftware/Brave-Browser"),
            "Brave Safe Storage",
            "Brave",
        ),
        (
            home.join("Library/Application Support/Google/Chrome"),
            "Chrome Safe Storage",
            "Chrome",
        ),
        (
            home.join("Library/Application Support/Microsoft Edge"),
            "Microsoft Edge Safe Storage",
            "Microsoft Edge",
        ),
        (
            home.join("Library/Application Support/Arc/User Data"),
            "Arc Safe Storage",
            "Arc",
        ),
        (
            home.join("Library/Application Support/Chromium"),
            "Chromium Safe Storage",
            "Chromium",
        ),
    ]
}

#[cfg(target_os = "macos")]
fn chromium_cookie_dbs() -> Vec<(PathBuf, &'static str, &'static str)> {
    let mut dbs = Vec::new();
    for (root, service, account) in chromium_user_data_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != "Default" && !name.starts_with("Profile ") {
                continue;
            }
            let profile = entry.path();
            for relative in ["Cookies", "Network/Cookies"] {
                let path = profile.join(relative);
                if path.is_file() {
                    dbs.push((path, service, account));
                }
            }
        }
    }
    dbs
}

#[cfg(target_os = "macos")]
fn kimi_desktop_cookies_db() -> PathBuf {
    home_dir().join("Library/Application Support/kimi-desktop/Cookies")
}

#[cfg(target_os = "macos")]
fn keychain_password(service: &str, account: &str) -> Option<String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-w", "-s", service, "-a", account])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let password = String::from_utf8(output.stdout).ok()?;
    let password = password.trim();
    (!password.is_empty()).then(|| password.to_string())
}

#[cfg(target_os = "macos")]
fn tokens_from_cookie_db(db: &Path, password: Option<&str>) -> Vec<String> {
    if !db.is_file() {
        return Vec::new();
    }

    let tmp = std::env::temp_dir().join(format!(
        "claudex-kimi-cookies-{}-{}",
        std::process::id(),
        db.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Cookies")
            .replace(' ', "_")
    ));
    let _ = std::fs::create_dir_all(&tmp);
    let copied = tmp.join("Cookies");
    let query_path = if std::fs::copy(db, &copied).is_ok() {
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", db.display()));
            if sidecar.exists() {
                let _ = std::fs::copy(&sidecar, tmp.join(format!("Cookies{suffix}")));
            }
        }
        copied.as_path()
    } else {
        db
    };

    let sql = "SELECT hex(COALESCE(encrypted_value, x'')), COALESCE(value, '') \
               FROM cookies \
               WHERE name = 'kimi-auth' \
                 AND (host_key LIKE '%kimi.com' OR host_key LIKE '%.kimi.com');";
    let output = Command::new("sqlite3")
        .args(["-readonly", &query_path.display().to_string(), sql])
        .output();
    let _ = std::fs::remove_dir_all(&tmp);

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut tokens = Vec::new();
    for line in stdout.lines() {
        let (hex, value) = line.split_once('|').unwrap_or((line, ""));
        if value.starts_with("eyJ") {
            push_unique(&mut tokens, value);
            continue;
        }
        let Some(password) = password else {
            continue;
        };
        let Some(encrypted) = decode_hex(hex) else {
            continue;
        };
        if let Some(token) = decrypt_chromium_cookie(&encrypted, password) {
            push_unique(&mut tokens, token);
        }
    }
    tokens
}

#[cfg(target_os = "macos")]
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect()
}

pub(crate) fn decrypt_chromium_cookie(encrypted: &[u8], password: &str) -> Option<String> {
    if encrypted.len() < 19 || &encrypted[..3] != b"v10" {
        return None;
    }

    let mut key = [0_u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1003, &mut key);
    let iv = [0x20_u8; 16];
    let plain = Aes128CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_vec::<Pkcs7>(&encrypted[3..])
        .ok()?;

    let jwt = match plain.windows(3).position(|window| window == b"eyJ") {
        Some(index) => &plain[index..],
        None => plain.as_slice(),
    };
    let token = std::str::from_utf8(jwt).ok()?.trim();
    (!token.is_empty()).then(|| token.to_string())
}

#[cfg(target_os = "macos")]
fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockModeEncrypt;

    #[test]
    fn env_tokens_stay_ahead_of_matching_browser_tokens() {
        let matching = jwt_with_sub("user-a");
        let other = jwt_with_sub("user-b");
        let tokens = order_tokens(
            vec!["env-token".to_string()],
            vec![other.clone(), matching.clone()],
            Some("user-a"),
        );

        assert_eq!(tokens, vec!["env-token".to_string(), matching, other]);
    }

    #[test]
    fn matching_user_id_is_preferred_among_browser_tokens() {
        let matching = jwt_with_sub("co0js84udu6f887phqfg");
        let other = jwt_with_sub("other-user");
        let tokens = order_tokens(
            Vec::new(),
            vec![other.clone(), matching.clone()],
            Some("co0js84udu6f887phqfg"),
        );

        assert_eq!(tokens[0], matching);
        assert_eq!(tokens[1], other);
    }

    #[test]
    fn decrypts_v10_cookie_with_chrome_host_checksum_prefix() {
        let password = "Q4T5S64FD7qTD8HSoOBbsw==";
        let jwt = "eyJhbGciOiJIUzI1NiJ9.e30.sig";
        let encrypted = encrypt_v10_cookie(password, jwt);

        assert_eq!(
            decrypt_chromium_cookie(&encrypted, password).as_deref(),
            Some(jwt)
        );
    }

    #[test]
    fn jwt_sub_reads_payload() {
        assert_eq!(jwt_sub(&jwt_with_sub("user-1")).as_deref(), Some("user-1"));
    }

    #[test]
    fn jwt_expired_uses_exp_claim() {
        assert!(!jwt_expired(&jwt_with_sub("user-1")));
        assert!(jwt_expired(&jwt_with_exp(1)));
        assert!(!jwt_expired(&jwt_with_exp(4_102_444_800)));
    }

    fn jwt_with_sub(sub: &str) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!(r#"{{"sub":"{sub}"}}"#));
        format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")
    }

    fn jwt_with_exp(exp: i64) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!(r#"{{"sub":"user-1","exp":{exp}}}"#));
        format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")
    }

    fn encrypt_v10_cookie(password: &str, jwt: &str) -> Vec<u8> {
        let mut key = [0_u8; 16];
        pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1003, &mut key);
        let iv = [0x20_u8; 16];
        let mut plain = vec![0_u8; 32];
        plain.extend_from_slice(jwt.as_bytes());
        let ciphertext = cbc::Encryptor::<Aes128>::new(&key.into(), &iv.into())
            .encrypt_padded_vec::<Pkcs7>(&plain);
        let mut encrypted = b"v10".to_vec();
        encrypted.extend(ciphertext);
        encrypted
    }
}
