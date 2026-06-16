use std::collections::HashMap;

use crate::agy::api::UserQuotaSummaryResponse;

/// Tier classification for a model, matching Gemini CLI's tier grouping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tier {
    Pro,
    Flash,
    FlashLite,
    /// Models not matching a known tier, displayed with a stable friendly name.
    Other(String),
}

impl Tier {
    pub fn display_name(&self) -> &str {
        match self {
            Tier::Pro => "Pro",
            Tier::Flash => "Flash",
            Tier::FlashLite => "Flash Lite",
            Tier::Other(name) => name,
        }
    }

    pub fn from_model_id(model_id: &str) -> Self {
        if model_id.contains("flash-lite") {
            return Tier::FlashLite;
        }
        if model_id.contains("pro") {
            return Tier::Pro;
        }
        if model_id.contains("flash") {
            return Tier::Flash;
        }
        if model_id.starts_with("claude-opus") {
            return Tier::Other("Claude Opus".to_string());
        }
        if model_id.starts_with("claude-sonnet") {
            return Tier::Other("Claude Sonnet".to_string());
        }
        if model_id.starts_with("gpt-oss") {
            return Tier::Other("GPT-OSS".to_string());
        }

        match model_id {
            "gemini-3.1-pro-preview"
            | "gemini-3.1-pro-preview-customtools"
            | "gemini-3-pro-preview"
            | "gemini-2.5-pro"
            | "pro" => Tier::Pro,
            "gemini-3-flash-preview" | "gemini-3.5-flash" | "gemini-2.5-flash" | "flash" => {
                Tier::Flash
            }
            "gemini-3.1-flash-lite" | "gemini-2.5-flash-lite" | "flash-lite" => Tier::FlashLite,
            _ => Tier::Other(model_id.to_string()),
        }
    }
}

/// A per-model bucket from the quota response.
#[derive(Debug)]
pub struct ModelBucket {
    pub tier: Tier,
    pub remaining_fraction: f64,
    pub reset_time: Option<String>,
}

/// Aggregate of the worst-case remaining fraction and its reset time for a tier.
#[derive(Debug)]
pub struct TierUsage {
    pub tier: Tier,
    pub remaining_fraction: f64,
    pub reset_time: Option<String>,
}

/// Build per-model bucket entries from the raw API response's top-level buckets.
pub fn build_model_buckets(response: &UserQuotaSummaryResponse) -> Vec<ModelBucket> {
    let mut result = Vec::new();
    for bucket in &response.buckets {
        if bucket.disabled.unwrap_or(false) {
            continue;
        }

        let Some(model_id) = bucket
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(remaining_fraction) = bucket.remaining_fraction else {
            continue;
        };
        if !is_user_visible_model_id(model_id) {
            continue;
        }

        result.push(ModelBucket {
            tier: Tier::from_model_id(model_id),
            remaining_fraction,
            reset_time: bucket.reset_time.clone(),
        });
    }
    result
}

fn is_user_visible_model_id(model_id: &str) -> bool {
    model_id.starts_with("gemini-")
        || model_id.starts_with("claude-")
        || model_id.starts_with("gpt-oss-")
        || matches!(model_id, "pro" | "flash" | "flash-lite")
}

/// Aggregate per-tier usage across all groups.
///
/// Within each tier the **minimum** `remaining_fraction` is kept (worst case),
/// matching Gemini CLI's behaviour.
pub fn aggregate_by_tier(model_buckets: &[ModelBucket]) -> Vec<TierUsage> {
    let mut tier_map: HashMap<String, (f64, Option<String>, Tier)> = HashMap::new();

    for mb in model_buckets {
        let key = match &mb.tier {
            Tier::Other(name) => name.clone(),
            t => t.display_name().to_string(),
        };

        if let Some(entry) = tier_map.get_mut(&key) {
            if mb.remaining_fraction < entry.0 {
                entry.0 = mb.remaining_fraction;
                entry.1 = mb.reset_time.clone();
            } else if entry.1.is_none() {
                entry.1 = mb.reset_time.clone();
            }
        } else {
            tier_map.insert(
                key,
                (
                    mb.remaining_fraction,
                    mb.reset_time.clone(),
                    mb.tier.clone(),
                ),
            );
        }
    }

    // Sort for deterministic output: known tiers first, then Others alphabetically.
    let tier_order = |t: &Tier| -> u8 {
        match t {
            Tier::Pro => 0,
            Tier::Flash => 1,
            Tier::FlashLite => 2,
            Tier::Other(_) => 3,
        }
    };

    let mut result: Vec<TierUsage> = tier_map
        .into_iter()
        .map(|(_, (rf, reset, tier))| TierUsage {
            tier,
            remaining_fraction: rf,
            reset_time: reset,
        })
        .collect();

    result.sort_by_key(|tu| (tier_order(&tu.tier), tu.tier.display_name().to_string()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_model_id() {
        assert_eq!(Tier::from_model_id("gemini-3-flash-preview"), Tier::Flash);
        assert_eq!(Tier::from_model_id("gemini-2.5-flash"), Tier::Flash);
        assert_eq!(Tier::from_model_id("gemini-3.5-flash"), Tier::Flash);
        assert_eq!(
            Tier::from_model_id("gemini-3.1-flash-lite"),
            Tier::FlashLite
        );
        assert_eq!(
            Tier::from_model_id("gemini-2.5-flash-lite"),
            Tier::FlashLite
        );
        assert_eq!(Tier::from_model_id("gemini-3.1-pro-preview"), Tier::Pro);
        assert_eq!(Tier::from_model_id("gemini-2.5-pro"), Tier::Pro);
        assert_eq!(Tier::from_model_id("gemini-3.1-pro-high"), Tier::Pro);
        assert_eq!(Tier::from_model_id("gemini-3.1-flash-image"), Tier::Flash);
        assert_eq!(
            Tier::from_model_id("claude-sonnet-4-6"),
            Tier::Other("Claude Sonnet".to_string())
        );
        assert_eq!(
            Tier::from_model_id("claude-opus-4-6-thinking"),
            Tier::Other("Claude Opus".to_string())
        );
        assert_eq!(
            Tier::from_model_id("gpt-oss-120b-medium"),
            Tier::Other("GPT-OSS".to_string())
        );
    }

    #[test]
    fn test_tier_aliases() {
        assert_eq!(Tier::from_model_id("pro"), Tier::Pro);
        assert_eq!(Tier::from_model_id("flash"), Tier::Flash);
        assert_eq!(Tier::from_model_id("flash-lite"), Tier::FlashLite);
    }

    #[test]
    fn test_tier_display_name() {
        assert_eq!(Tier::Pro.display_name(), "Pro");
        assert_eq!(Tier::Flash.display_name(), "Flash");
        assert_eq!(Tier::FlashLite.display_name(), "Flash Lite");
        assert_eq!(
            Tier::Other("custom-model".to_string()).display_name(),
            "custom-model"
        );
    }

    #[test]
    fn test_build_model_buckets_from_model_id_response() {
        let json = r#"{
            "groups": [
                {
                    "displayName": "Claude and GPT models",
                    "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
                    "buckets": [
                        {
                            "bucketId": "3p-weekly",
                            "displayName": "Weekly Limit",
                            "window": "weekly",
                            "remainingFraction": 0.66,
                            "resetTime": "2026-06-23T01:30:12Z"
                        },
                        {
                            "bucketId": "3p-5h",
                            "displayName": "Five Hour Limit",
                            "window": "5h",
                            "remainingFraction": 0.0,
                            "disabled": true
                        }
                    ]
                }
            ],
            "buckets": [
                {
                    "modelId": "gemini-3.1-pro-preview",
                    "remainingFraction": 0.92,
                    "resetTime": "2026-06-19T08:46:00Z"
                },
                {
                    "modelId": "gemini-3-flash-preview",
                    "remainingFraction": 0.99,
                    "resetTime": "2026-06-16T10:14:45Z"
                },
                {
                    "modelId": "gemini-3.1-pro-preview",
                    "disabled": true,
                    "remainingFraction": 0.0
                },
                {
                    "remainingFraction": 0.5
                }
            ]
        }"#;
        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        let buckets = build_model_buckets(&response);

        assert_eq!(buckets.len(), 2);
        assert!(buckets.iter().any(|b| b.tier == Tier::Pro));
        assert!(buckets.iter().any(|b| b.tier == Tier::Flash));
    }

    #[test]
    fn test_build_model_buckets_uses_top_level_model_id_buckets_only() {
        let json = r#"{
            "groups": [
                {
                    "displayName": "Claude and GPT models",
                    "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
                    "buckets": [
                        {
                            "displayName": "Weekly Limit",
                            "remainingFraction": 0.66,
                            "resetTime": "2026-06-23T01:30:12Z"
                        }
                    ]
                }
            ],
            "buckets": [
                {
                    "modelId": "gemini-3.1-pro-preview",
                    "remainingFraction": 0.5856,
                    "resetTime": "2026-06-16T17:20:00Z"
                },
                {
                    "modelId": "gemini-3-flash-preview",
                    "remainingFraction": 1.0,
                    "resetTime": "2026-06-16T17:57:00Z"
                }
            ]
        }"#;
        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        let buckets = build_model_buckets(&response);

        assert_eq!(buckets.len(), 2);
        assert!(buckets.iter().any(|b| b.tier == Tier::Pro));
        assert!(buckets.iter().any(|b| b.tier == Tier::Flash));
        assert!(
            !buckets
                .iter()
                .any(|b| b.tier == Tier::Other("Claude Sonnet".to_string()))
        );
        assert!(
            !buckets
                .iter()
                .any(|b| b.tier == Tier::Other("GPT-OSS".to_string()))
        );
    }

    #[test]
    fn test_build_model_buckets_skips_internal_model_ids() {
        let json = r#"{
            "buckets": [
                {
                    "modelId": "chat_20706",
                    "remainingFraction": 1.0
                },
                {
                    "modelId": "tab_jump_flash_lite_preview",
                    "remainingFraction": 1.0
                },
                {
                    "modelId": "claude-sonnet-4-6",
                    "remainingFraction": 1.0
                }
            ]
        }"#;
        let response: UserQuotaSummaryResponse = serde_json::from_str(json).unwrap();
        let buckets = build_model_buckets(&response);

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].tier, Tier::Other("Claude Sonnet".to_string()));
    }

    #[test]
    fn test_aggregate_by_tier_takes_minimum() {
        let buckets = vec![
            ModelBucket {
                tier: Tier::Pro,
                remaining_fraction: 0.92,
                reset_time: Some("2026-06-19T08:46:00Z".into()),
            },
            ModelBucket {
                tier: Tier::Pro,
                remaining_fraction: 0.66,
                reset_time: Some("2026-06-23T01:30:12Z".into()),
            },
            ModelBucket {
                tier: Tier::Flash,
                remaining_fraction: 0.99,
                reset_time: Some("2026-06-16T10:14:45Z".into()),
            },
        ];

        let tiers = aggregate_by_tier(&buckets);
        assert_eq!(tiers.len(), 2);

        let pro = tiers.iter().find(|t| t.tier == Tier::Pro).unwrap();
        assert_eq!(pro.remaining_fraction, 0.66); // worst of 0.92 and 0.66
        assert_eq!(pro.reset_time.as_deref(), Some("2026-06-23T01:30:12Z"));

        let flash = tiers.iter().find(|t| t.tier == Tier::Flash).unwrap();
        assert_eq!(flash.remaining_fraction, 0.99);
    }

    #[test]
    fn test_aggregate_by_tier_keeps_reset_time_for_full_quota() {
        let buckets = vec![ModelBucket {
            tier: Tier::Flash,
            remaining_fraction: 1.0,
            reset_time: Some("2026-06-16T10:14:45Z".into()),
        }];

        let tiers = aggregate_by_tier(&buckets);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].reset_time.as_deref(), Some("2026-06-16T10:14:45Z"));
    }

    #[test]
    fn test_aggregate_other_tiers_are_separate() {
        let buckets = vec![
            ModelBucket {
                tier: Tier::Other("GPT-OSS".into()),
                remaining_fraction: 0.66,
                reset_time: None,
            },
            ModelBucket {
                tier: Tier::Other("gemini-unknown".into()),
                remaining_fraction: 0.80,
                reset_time: None,
            },
        ];

        let tiers = aggregate_by_tier(&buckets);
        assert_eq!(tiers.len(), 2); // each unique Other gets its own row
    }
}
