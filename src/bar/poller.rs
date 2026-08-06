//! Background polling of `claudex usage --all --json` for claudex-bar.
//!
//! The bar process never touches the network or credentials itself: it spawns
//! the `claudex` CLI and parses the snapshot JSON from stdout. `claudex`
//! exits non-zero when every provider is unavailable but still prints the
//! document, so stdout is parsed regardless of the exit code.

use std::path::PathBuf;
use std::process::Command;
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
}

impl Poller {
    pub fn start(
        claudex_bin: PathBuf,
        skip: Vec<String>,
        interval: Duration,
        ctx: egui::Context,
    ) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (refresh_tx, refresh_rx) = mpsc::channel::<()>();

        thread::spawn(move || {
            loop {
                if result_tx.send(PollEvent::Started).is_err() {
                    return;
                }
                ctx.request_repaint();

                let result = poll_once(&claudex_bin, &skip);
                if result_tx.send(result).is_err() {
                    return;
                }
                ctx.request_repaint();

                match refresh_rx.recv_timeout(interval) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });

        Self {
            results: result_rx,
            refresh: refresh_tx,
        }
    }

    pub fn refresh_now(&self) {
        let _ = self.refresh.send(());
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

/// Resolve the claudex CLI: `$CLAUDEX_BIN`, then a sibling of the bar's own
/// executable, then PATH.
pub fn resolve_claudex_bin() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CLAUDEX_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("CLAUDEX_BIN={} is not a file", path.display()));
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("claudex");
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    Ok(PathBuf::from("claudex"))
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
