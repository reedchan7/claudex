//! Background polling of `claudex usage --all --json` for claudex-bar.
//!
//! The bar process never touches the network or credentials itself: it spawns
//! the `claudex` CLI and parses the snapshot JSON from stdout. `claudex`
//! exits non-zero when every provider is unavailable but still prints the
//! document, so stdout is parsed regardless of the exit code.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui;

use crate::snapshot::Snapshot;

/// What a poll covers: every provider, or a single one (manual per-provider
/// refresh from the UI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollScope {
    All,
    /// Provider id, e.g. "gpt" (see `Provider::skip_name`).
    One(String),
}

pub enum PollEvent {
    /// A poll just started — the UI can show a loading indicator.
    Started(PollScope),
    Ok(PollScope, Snapshot),
    Err(String),
}

pub struct Poller {
    results: Receiver<PollEvent>,
    refresh: Sender<PollScope>,
    interval_secs: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
}

impl Poller {
    pub fn start(
        claudex_bin: PathBuf,
        skip: Vec<String>,
        interval_secs: u64,
        paused: Arc<AtomicBool>,
        ctx: egui::Context,
    ) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (refresh_tx, refresh_rx) = mpsc::channel::<PollScope>();
        let initial_tx = refresh_tx.clone();
        let interval = Arc::new(AtomicU64::new(interval_secs));
        let interval_shared = Arc::clone(&interval);
        let paused_thread = Arc::clone(&paused);

        thread::spawn(move || {
            // Kick off the first poll immediately.
            let _ = initial_tx.send(PollScope::All);
            loop {
                // A wake carries the scope to poll (manual global/per-provider
                // refresh, interval change, resume); a timeout is a scheduled
                // full poll. Pausing skips polls until resumed.
                let wait = Duration::from_secs(interval_shared.load(Ordering::Relaxed));
                let scope = match refresh_rx.recv_timeout(wait) {
                    Ok(scope) => scope,
                    Err(RecvTimeoutError::Timeout) => PollScope::All,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                if paused_thread.load(Ordering::Relaxed) {
                    continue;
                }

                if result_tx.send(PollEvent::Started(scope.clone())).is_err() {
                    return;
                }
                ctx.request_repaint();

                let result = poll_once(&claudex_bin, &skip, &scope);
                if result_tx.send(result).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });

        Self {
            results: result_rx,
            refresh: refresh_tx,
            interval_secs: interval,
            paused,
        }
    }

    pub fn refresh_now(&self) {
        let _ = self.refresh.send(PollScope::All);
    }

    /// Queue a refresh of one provider by id (e.g. "gpt").
    pub fn refresh_provider(&self, id: String) {
        let _ = self.refresh.send(PollScope::One(id));
    }

    /// A clonable wake handle for out-of-band triggers (signal handler).
    pub fn refresher(&self) -> Sender<PollScope> {
        self.refresh.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Pause/resume polling; wakes the poller so resume refreshes right away.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        self.refresh_now();
    }

    pub fn interval_secs(&self) -> u64 {
        self.interval_secs.load(Ordering::Relaxed)
    }

    /// Change the poll interval; wakes the poller so the new cadence (and a
    /// fresh poll) applies immediately.
    pub fn set_interval_secs(&self, secs: u64) {
        self.interval_secs.store(secs, Ordering::Relaxed);
        self.refresh_now();
    }

    pub fn drain(&self) -> impl Iterator<Item = PollEvent> + '_ {
        self.results.try_iter()
    }
}

fn poll_once(claudex_bin: &PathBuf, skip: &[String], scope: &PollScope) -> PollEvent {
    let mut command = Command::new(claudex_bin);
    match scope {
        PollScope::All => {
            command.args(["usage", "--all", "--json"]);
            for name in skip {
                command.arg("--skip").arg(name);
            }
        }
        PollScope::One(id) => {
            let Some(argv) = usage_argv(id) else {
                return PollEvent::Err(format!("unknown provider '{id}'"));
            };
            command.args(argv).arg("--json");
        }
    }

    match command.output() {
        Ok(output) => {
            if output.stdout.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return PollEvent::Err(format!(
                    "claudex printed no JSON (exit {:?}): {}",
                    output.status.code(),
                    stderr.trim()
                ));
            }
            match serde_json::from_slice::<Snapshot>(&output.stdout) {
                Ok(snapshot) => PollEvent::Ok(scope.clone(), snapshot),
                Err(e) => PollEvent::Err(format!("failed to parse claudex JSON: {e}")),
            }
        }
        Err(e) => PollEvent::Err(format!("failed to run {}: {e}", claudex_bin.display())),
    }
}

/// CLI argv for a single provider's JSON usage command.
fn usage_argv(id: &str) -> Option<Vec<&'static str>> {
    match id {
        "claude" => Some(vec!["usage"]),
        "gpt" => Some(vec!["gpt", "usage"]),
        "agy" => Some(vec!["agy", "usage"]),
        "glm" => Some(vec!["glm", "usage"]),
        "kimi" => Some(vec!["kimi", "usage"]),
        "grok" => Some(vec!["grok", "usage"]),
        _ => None,
    }
}

/// Resolve the claudex CLI the poller spawns for snapshots: `$CLAUDEX_BIN`,
/// else the running executable itself (the widget lives inside `claudex`).
pub fn resolve_claudex_bin() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CLAUDEX_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("CLAUDEX_BIN={} is not a file", path.display()));
    }

    std::env::current_exe().map_err(|e| format!("failed to resolve current executable: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_once_reports_missing_binary() {
        let result = poll_once(&PathBuf::from("/nonexistent/claudex"), &[], &PollScope::All);
        match result {
            PollEvent::Err(e) => assert!(e.contains("failed to run")),
            _ => panic!("expected spawn failure"),
        }
    }

    #[test]
    fn usage_argv_maps_every_provider() {
        assert_eq!(usage_argv("claude"), Some(vec!["usage"]));
        assert_eq!(usage_argv("gpt"), Some(vec!["gpt", "usage"]));
        assert_eq!(usage_argv("agy"), Some(vec!["agy", "usage"]));
        assert_eq!(usage_argv("glm"), Some(vec!["glm", "usage"]));
        assert_eq!(usage_argv("kimi"), Some(vec!["kimi", "usage"]));
        assert_eq!(usage_argv("grok"), Some(vec!["grok", "usage"]));
        assert_eq!(usage_argv("nope"), None);
    }
}
