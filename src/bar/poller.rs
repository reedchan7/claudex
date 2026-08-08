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

pub enum PollEvent {
    /// A poll just started — the UI can show a loading indicator.
    Started,
    Ok(Snapshot),
    Err(String),
}

pub struct Poller {
    results: Receiver<PollEvent>,
    refresh: Sender<()>,
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
        let (refresh_tx, refresh_rx) = mpsc::channel::<()>();
        let interval = Arc::new(AtomicU64::new(interval_secs));
        let interval_shared = Arc::clone(&interval);
        let paused_thread = Arc::clone(&paused);

        thread::spawn(move || {
            loop {
                // A wake (manual refresh, interval change, pause/resume
                // signal) re-checks the flag, so pausing takes effect
                // immediately even mid-sleep.
                if !paused_thread.load(Ordering::Relaxed) {
                    if result_tx.send(PollEvent::Started).is_err() {
                        return;
                    }
                    ctx.request_repaint();

                    let result = poll_once(&claudex_bin, &skip);
                    if result_tx.send(result).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }

                let wait = Duration::from_secs(interval_shared.load(Ordering::Relaxed));
                match refresh_rx.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
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
        let _ = self.refresh.send(());
    }

    /// A clonable wake handle for out-of-band triggers (signal handler).
    pub fn refresher(&self) -> Sender<()> {
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

fn poll_once(claudex_bin: &PathBuf, skip: &[String]) -> PollEvent {
    let mut command = Command::new(claudex_bin);
    command.args(["usage", "--all", "--json"]);
    for name in skip {
        command.arg("--skip").arg(name);
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
                Ok(snapshot) => PollEvent::Ok(snapshot),
                Err(e) => PollEvent::Err(format!("failed to parse claudex JSON: {e}")),
            }
        }
        Err(e) => PollEvent::Err(format!("failed to run {}: {e}", claudex_bin.display())),
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
        let result = poll_once(&PathBuf::from("/nonexistent/claudex"), &[]);
        match result {
            PollEvent::Err(e) => assert!(e.contains("failed to run")),
            _ => panic!("expected spawn failure"),
        }
    }
}
