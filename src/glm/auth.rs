use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Global,
    Cn,
}

impl Region {
    pub fn base_url(self) -> &'static str {
        match self {
            Region::Global => "https://api.z.ai",
            Region::Cn => "https://open.bigmodel.cn",
        }
    }

    fn zcode_provider_id(self) -> &'static str {
        match self {
            Region::Global => "builtin:zai-coding-plan",
            Region::Cn => "builtin:bigmodel-coding-plan",
        }
    }

    fn from_flag(value: &str) -> Option<Region> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cn" | "china" => Some(Region::Cn),
            "global" | "zai" => Some(Region::Global),
            _ => None,
        }
    }

    fn from_domain(domain: &str) -> Option<Region> {
        match domain.trim().to_ascii_lowercase().as_str() {
            "zai" => Some(Region::Global),
            "bigmodel" => Some(Region::Cn),
            _ => None,
        }
    }
}

pub struct GlmCredentials {
    pub region: Region,
    pub api_key: String,
}

pub fn resolve_credentials(region_override: Option<&str>) -> Result<GlmCredentials, String> {
    let region = resolve_region(region_override);
    let api_key = resolve_api_key(region)?;
    Ok(GlmCredentials { region, api_key })
}

fn pick_region(flag: Option<&str>, env: Option<&str>, domain: Option<&str>) -> Region {
    if let Some(region) = flag.and_then(Region::from_flag) {
        return region;
    }
    if let Some(region) = env.and_then(Region::from_flag) {
        return region;
    }
    if let Some(region) = domain.and_then(Region::from_domain) {
        return region;
    }
    Region::Global
}

fn resolve_region(region_override: Option<&str>) -> Region {
    let env = std::env::var("GLM_REGION").ok();
    let domain = read_zcode_setting().and_then(|s| parse_domain_from_setting(&s));
    pick_region(region_override, env.as_deref(), domain.as_deref())
}

fn resolve_api_key(region: Region) -> Result<String, String> {
    if let Ok(env_key) = std::env::var("GLM_API_KEY") {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(key) =
        read_zcode_config().and_then(|s| parse_api_key_from_config(&s, region.zcode_provider_id()))
    {
        return Ok(key);
    }
    Err("could not find GLM credentials — sign in with ZCode, or set GLM_API_KEY".to_string())
}

fn parse_api_key_from_config(config_json: &str, provider_id: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(config_json).ok()?;
    let key = value
        .get("provider")?
        .get(provider_id)?
        .get("options")?
        .get("apiKey")?
        .as_str()?
        .trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn parse_domain_from_setting(setting_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(setting_json).ok()?;
    value
        .get("providerFamilyDomain")?
        .as_str()
        .map(str::to_string)
}

fn zcode_dir() -> PathBuf {
    home_dir().join(".zcode").join("v2")
}

fn read_zcode_config() -> Option<String> {
    std::fs::read_to_string(zcode_dir().join("config.json")).ok()
}

fn read_zcode_setting() -> Option<String> {
    std::fs::read_to_string(zcode_dir().join("setting.json")).ok()
}

fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_region_prefers_flag_then_env_then_domain() {
        assert_eq!(
            pick_region(Some("cn"), Some("global"), Some("zai")),
            Region::Cn
        );
        assert_eq!(
            pick_region(None, Some("global"), Some("bigmodel")),
            Region::Global
        );
        assert_eq!(pick_region(None, None, Some("bigmodel")), Region::Cn);
        assert_eq!(pick_region(None, None, Some("zai")), Region::Global);
        assert_eq!(pick_region(None, None, None), Region::Global);
    }

    #[test]
    fn region_maps_to_base_url_and_provider_id() {
        assert_eq!(Region::Global.base_url(), "https://api.z.ai");
        assert_eq!(Region::Cn.base_url(), "https://open.bigmodel.cn");
        assert_eq!(
            Region::Global.zcode_provider_id(),
            "builtin:zai-coding-plan"
        );
        assert_eq!(
            Region::Cn.zcode_provider_id(),
            "builtin:bigmodel-coding-plan"
        );
    }

    #[test]
    fn parse_api_key_from_config_reads_provider_key() {
        let json = r#"{
            "provider": {
                "builtin:zai-coding-plan": { "options": { "apiKey": "abc.def" } },
                "builtin:bigmodel-coding-plan": { "options": { "apiKey": "" } }
            }
        }"#;
        assert_eq!(
            parse_api_key_from_config(json, "builtin:zai-coding-plan").as_deref(),
            Some("abc.def")
        );
        // Empty string is treated as "no key".
        assert_eq!(
            parse_api_key_from_config(json, "builtin:bigmodel-coding-plan"),
            None
        );
        assert_eq!(parse_api_key_from_config(json, "builtin:missing"), None);
        assert_eq!(
            parse_api_key_from_config("not json", "builtin:zai-coding-plan"),
            None
        );
    }

    #[test]
    fn parse_domain_from_setting_reads_family_domain() {
        let json = r#"{ "providerFamilyDomain": "zai", "locale": "en-US" }"#;
        assert_eq!(parse_domain_from_setting(json).as_deref(), Some("zai"));
        assert_eq!(parse_domain_from_setting("{}"), None);
        assert_eq!(parse_domain_from_setting("nope"), None);
    }
}
