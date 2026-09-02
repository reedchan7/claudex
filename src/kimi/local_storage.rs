use super::cookies::jwt_payload;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageTokens {
    pub access: Vec<String>,
    pub refresh: Vec<String>,
}

/// Read Kimi web `access_token` / `refresh_token` from a Chromium
/// `Local Storage/leveldb` directory.
///
/// Kimi stores these under `https://www.kimi.com`. After compaction they live
/// in Snappy-compressed SSTables, so a raw byte scan of `.ldb` files misses
/// them. This walks the LevelDB records without taking the browser LOCK.
#[cfg(target_os = "macos")]
fn is_table_or_wal(name: &str) -> bool {
    let numbered = name
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit());
    numbered && (name.ends_with(".ldb") || name.ends_with(".log"))
}

#[cfg(target_os = "macos")]
pub(crate) fn tokens_from_dir(dir: &std::path::Path) -> StorageTokens {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return StorageTokens::default();
    };
    let mut kimi_records = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_table_or_wal(name) {
            continue;
        }
        let Ok(buf) = std::fs::read(&path) else {
            continue;
        };
        let parsed = if name.ends_with(".ldb") {
            leveldb_core::parse_table_bytes(&buf, &path)
        } else {
            leveldb_core::parse_log_bytes(&buf, &path)
        };
        let Ok(records) = parsed else {
            continue;
        };
        kimi_records.extend(
            records
                .into_iter()
                .filter(|record| contains_kimi_bytes(&record.key)),
        );
    }
    if kimi_records.is_empty() {
        return StorageTokens::default();
    }

    let mut latest: std::collections::HashMap<(String, String), (u64, bool, String)> =
        std::collections::HashMap::new();
    for record in leveldb_forensic::decode_local_storage_records(&kimi_records) {
        let leveldb_forensic::LocalStorageRecord::Data {
            origin,
            script_key,
            value,
            seq,
            deleted,
        } = record
        else {
            continue;
        };
        if !is_kimi_origin(&origin) {
            continue;
        }
        let key = script_key.text;
        if key != "access_token" && key != "refresh_token" {
            continue;
        }
        let slot = latest
            .entry((origin, key))
            .or_insert((0, true, String::new()));
        if seq >= slot.0 {
            *slot = (seq, deleted, value.text);
        }
    }

    let mut tokens = StorageTokens::default();
    for ((_, key), (_, deleted, value)) in latest {
        if deleted {
            continue;
        }
        take_jwt(&value, &mut tokens, key == "refresh_token");
    }
    tokens
}

fn contains_kimi_bytes(key: &[u8]) -> bool {
    key.windows(8).any(|window| window == b"kimi.com")
        || key.windows(7).any(|window| window == b"kimi.ai")
}

fn take_jwt(value: &str, tokens: &mut StorageTokens, refresh: bool) {
    let token = value.trim();
    if !token.starts_with("eyJ") {
        return;
    }
    let Some(payload) = jwt_payload(token) else {
        return;
    };
    if !is_kimi_web_jwt(&payload) {
        return;
    }
    let dest = if refresh || payload.get("typ").and_then(|value| value.as_str()) == Some("refresh")
    {
        &mut tokens.refresh
    } else {
        &mut tokens.access
    };
    push_newest(dest, token.to_string(), &payload);
}

fn is_kimi_origin(origin: &str) -> bool {
    origin.contains("kimi.com") || origin.contains("kimi.ai")
}

fn is_kimi_web_jwt(payload: &serde_json::Value) -> bool {
    match payload.get("aud") {
        Some(serde_json::Value::String(value)) => is_kimi_host(value),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .any(|value| value.as_str().is_some_and(is_kimi_host)),
        _ => false,
    }
}

fn is_kimi_host(value: &str) -> bool {
    value.contains("kimi.com") || value.contains("kimi.ai")
}

fn jwt_iat(payload: &serde_json::Value) -> i64 {
    payload
        .get("iat")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

fn push_newest(tokens: &mut Vec<String>, token: String, payload: &serde_json::Value) {
    if tokens.iter().any(|existing| existing == &token) {
        return;
    }
    let iat = jwt_iat(payload);
    if let Some(index) = tokens
        .iter()
        .position(|existing| jwt_payload(existing).as_ref().map(jwt_iat).unwrap_or(0) < iat)
    {
        tokens.insert(index, token);
    } else {
        tokens.push(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn classifies_access_and_refresh_by_typ() {
        let access =
            jwt(r#"{"iss":"account","aud":["kimi.com"],"typ":"access","sub":"user-a","iat":20}"#);
        let refresh =
            jwt(r#"{"iss":"account","aud":["kimi.com"],"typ":"refresh","sub":"user-a","iat":20}"#);
        let mut tokens = StorageTokens::default();
        take_jwt(&access, &mut tokens, false);
        take_jwt(&refresh, &mut tokens, true);
        assert_eq!(tokens.access, vec![access]);
        assert_eq!(tokens.refresh, vec![refresh]);
    }

    #[test]
    fn ignores_tokens_for_other_audiences() {
        let other = jwt(r#"{"iss":"account","aud":["chatgpt.com"],"typ":"access","sub":"user-a"}"#);
        let mut tokens = StorageTokens::default();
        take_jwt(&other, &mut tokens, false);
        assert!(tokens.access.is_empty());
    }

    #[test]
    fn prefers_newer_iat() {
        let older =
            jwt(r#"{"iss":"account","aud":["kimi.com"],"typ":"access","sub":"user-a","iat":10}"#);
        let newer =
            jwt(r#"{"iss":"account","aud":["kimi.com"],"typ":"access","sub":"user-a","iat":50}"#);
        let mut tokens = StorageTokens::default();
        take_jwt(&older, &mut tokens, false);
        take_jwt(&newer, &mut tokens, false);
        assert_eq!(tokens.access, vec![newer, older]);
    }

    fn jwt(claims: &str) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims);
        format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")
    }
}
