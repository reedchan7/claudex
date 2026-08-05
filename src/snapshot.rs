//! Normalized usage snapshot shared by `--json` CLI output and the
//! `claudex-bar` desktop widget.
//!
//! The widget consumes `claudex usage --all --json` and renders this schema
//! without any provider-specific knowledge, so keep it stable: bump
//! [`SNAPSHOT_VERSION`] on breaking changes.
//!
//! NOTE: every provider also has a terminal `render()` path with its own copy
//! of section/title logic. When you change one, mirror the change in the
//! other (see each `*_usage.rs` `snapshot()` function).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::commands::status::{Provider, ProviderStatus};

pub const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub fetched_at: DateTime<Utc>,
    pub providers: Vec<ProviderSnapshot>,
}

impl Snapshot {
    pub fn new(providers: Vec<ProviderSnapshot>) -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            fetched_at: Utc::now(),
            providers,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    /// Canonical short name, e.g. "claude" (see `Provider::skip_name`).
    pub id: String,
    pub label: String,
    pub accent: (u8, u8, u8),
    #[serde(flatten)]
    pub state: ProviderState,
}

impl ProviderSnapshot {
    pub fn ok(provider: Provider, blocks: Vec<Block>) -> Self {
        Self {
            id: provider.skip_name().to_string(),
            label: provider.label().to_string(),
            accent: provider.accent(),
            state: ProviderState::Ok { blocks },
        }
    }

    pub fn unavailable(provider: Provider, status: &ProviderStatus) -> Self {
        Self {
            id: provider.skip_name().to_string(),
            label: provider.label().to_string(),
            accent: provider.accent(),
            state: ProviderState::Unavailable {
                heading: status.heading.clone(),
                detail: status.detail.clone(),
                next_step: status.next_step.clone(),
            },
        }
    }

    /// Wrap a provider snapshot result, converting fetch errors into the
    /// structured Unavailable state (same classification as the terminal
    /// error output).
    pub fn from_result(provider: Provider, result: Result<ProviderSnapshot, String>) -> Self {
        result.unwrap_or_else(|e| {
            let status = crate::commands::status::status_for_error(provider, &e);
            ProviderSnapshot::unavailable(provider, &status)
        })
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, ProviderState::Ok { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ProviderState {
    Ok {
        blocks: Vec<Block>,
    },
    Unavailable {
        heading: String,
        detail: String,
        next_step: String,
    },
}

/// A titled group of rows, mirroring one section of the terminal output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub rows: Vec<Row>,
}

impl Block {
    pub fn titled(title: impl Into<String>, rows: Vec<Row>) -> Self {
        Self {
            title: Some(title.into()),
            rows,
        }
    }

    pub fn untitled(rows: Vec<Row>) -> Self {
        Self { title: None, rows }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Row {
    Bar {
        percent: f64,
        /// Preformatted suffix, e.g. "34% used".
        text: String,
        /// Preformatted secondary line, e.g. "Resets 2:30pm, 2h 30m left".
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// Raw reset timestamp (RFC 3339) so consumers can recompute
        /// countdowns between polls.
        #[serde(skip_serializing_if = "Option::is_none")]
        resets_at: Option<String>,
    },
    Text {
        text: String,
    },
}

impl Row {
    pub fn bar(
        percent: f64,
        text: impl Into<String>,
        detail: Option<String>,
        resets_at: Option<String>,
    ) -> Self {
        Row::Bar {
            percent,
            text: text.into(),
            detail,
            resets_at,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Row::Text { text: text.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provider() -> ProviderSnapshot {
        ProviderSnapshot::ok(
            Provider::Claude,
            vec![
                Block::untitled(vec![Row::text("Subscription: Max")]),
                Block::titled(
                    "Current session (5h)",
                    vec![Row::bar(
                        34.0,
                        "34% used",
                        Some("Resets 2:30pm, 2h 30m left".to_string()),
                        Some("2026-08-05T14:30:00+08:00".to_string()),
                    )],
                ),
            ],
        )
    }

    #[test]
    fn snapshot_serializes_with_version_and_tagged_rows() {
        let snapshot = Snapshot::new(vec![sample_provider()]);
        let json = serde_json::to_value(&snapshot).unwrap();

        assert_eq!(json["version"], 1);
        assert!(json["fetched_at"].is_string());
        let provider = &json["providers"][0];
        assert_eq!(provider["id"], "claude");
        assert_eq!(provider["label"], "Claude Code");
        assert_eq!(provider["status"], "ok");
        assert_eq!(provider["blocks"][1]["title"], "Current session (5h)");
        let bar = &provider["blocks"][1]["rows"][0];
        assert_eq!(bar["type"], "bar");
        assert_eq!(bar["percent"], 34.0);
        assert_eq!(bar["resets_at"], "2026-08-05T14:30:00+08:00");
        // Untitled blocks and absent option fields are omitted.
        assert!(provider["blocks"][0].get("title").is_none());
        let text_row = &provider["blocks"][0]["rows"][0];
        assert_eq!(text_row["type"], "text");
        assert_eq!(text_row["text"], "Subscription: Max");
    }

    #[test]
    fn unavailable_provider_serializes_status_fields() {
        let status = ProviderStatus {
            heading: "Codex is not connected".to_string(),
            detail: "No local Codex session was found on this machine.".to_string(),
            next_step: "Run `codex` and sign in with ChatGPT.".to_string(),
            details: None,
        };
        let provider = ProviderSnapshot::unavailable(Provider::Codex, &status);
        let json = serde_json::to_value(&provider).unwrap();

        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["heading"], "Codex is not connected");
        assert_eq!(json["next_step"], "Run `codex` and sign in with ChatGPT.");
        assert!(json.get("blocks").is_none());
    }

    #[test]
    fn snapshot_roundtrips_through_json() {
        let snapshot = Snapshot::new(vec![sample_provider()]);
        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: Snapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, SNAPSHOT_VERSION);
        assert_eq!(parsed.providers.len(), 1);
        match &parsed.providers[0].state {
            ProviderState::Ok { blocks } => {
                assert_eq!(blocks.len(), 2);
                match &blocks[1].rows[0] {
                    Row::Bar {
                        percent, resets_at, ..
                    } => {
                        assert_eq!(*percent, 34.0);
                        assert!(resets_at.is_some());
                    }
                    Row::Text { .. } => panic!("expected bar row"),
                }
            }
            ProviderState::Unavailable { .. } => panic!("expected ok state"),
        }
    }

    #[test]
    fn from_result_converts_errors_to_unavailable_state() {
        let provider = ProviderSnapshot::from_result(
            Provider::Kimi,
            Err("could not find Kimi Code credentials at /tmp/kimi-code.json".to_string()),
        );

        assert!(!provider.is_available());
        assert_eq!(provider.id, "kimi");
        match &provider.state {
            ProviderState::Unavailable {
                heading, next_step, ..
            } => {
                assert_eq!(heading, "Kimi Code is not connected");
                assert_eq!(next_step, "Run `kimi login` and sign in with Kimi Code.");
            }
            ProviderState::Ok { .. } => panic!("expected unavailable state"),
        }

        let ok = ProviderSnapshot::from_result(Provider::Kimi, Ok(sample_provider()));
        assert!(ok.is_available());
    }
}
