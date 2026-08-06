//! Widget state persistence for claudex-bar (`~/.claudex/bar.json`):
//! window position, collapsed providers, and mini mode.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BarConfig {
    pub x: Option<f32>,
    pub y: Option<f32>,
    /// Provider ids the user collapsed in the widget (e.g. "claude").
    #[serde(default)]
    pub collapsed: Vec<String>,
    /// Whole-widget mini mode: one compact line per provider.
    #[serde(default)]
    pub mini: bool,
}

impl BarConfig {
    pub fn position(&self) -> Option<(f32, f32)> {
        match (self.x, self.y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claudex/bar.json"))
}

pub fn load() -> BarConfig {
    let Some(path) = config_path() else {
        return BarConfig::default();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return BarConfig::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save(config: &BarConfig) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(config) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_requires_both_coordinates() {
        assert_eq!(BarConfig::default().position(), None);
        assert_eq!(
            BarConfig {
                x: Some(10.0),
                y: None,
                ..Default::default()
            }
            .position(),
            None
        );
        assert_eq!(
            BarConfig {
                x: Some(10.0),
                y: Some(20.5),
                ..Default::default()
            }
            .position(),
            Some((10.0, 20.5))
        );
    }

    #[test]
    fn config_roundtrips_through_json() {
        let config = BarConfig {
            x: Some(42.0),
            y: Some(17.0),
            collapsed: vec!["claude".to_string()],
            mini: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: BarConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.position(), Some((42.0, 17.0)));
        assert_eq!(parsed.collapsed, ["claude"]);
        assert!(parsed.mini);
    }

    #[test]
    fn json_without_widget_state_uses_defaults() {
        // Config files written by older versions have only x/y.
        let parsed: BarConfig = serde_json::from_str(r#"{"x": 1.0, "y": 2.0}"#).unwrap();
        assert_eq!(parsed.position(), Some((1.0, 2.0)));
        assert!(parsed.collapsed.is_empty());
        assert!(!parsed.mini);
    }

    #[test]
    fn corrupt_json_falls_back_to_default() {
        let parsed: BarConfig = serde_json::from_str("not json").unwrap_or_default();
        assert_eq!(parsed.position(), None);
    }
}
