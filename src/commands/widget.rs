//! Lifecycle management for the desktop widget: `claudex widget
//! start|stop|restart|status`. The GUI itself runs in-process via the hidden
//! `claudex widget run` command, re-executed detached by `start`.
//!
//! State lives in `~/.claudex/`: `widget.pid` (written by `run`) and
//! `widget.log` (stdout/stderr of the detached process).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::bar::BarOptions;

fn state_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claudex"))
}

fn pid_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("widget.pid"))
}

fn log_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("widget.log"))
}

fn pid_alive(pid: i32) -> bool {
    let result = unsafe { libc::kill(pid, 0) };
    result == 0
}

/// PID of the running widget, if a live one is recorded.
fn running_pid() -> Option<i32> {
    let raw = std::fs::read_to_string(pid_path()?).ok()?;
    let pid: i32 = raw.trim().parse().ok()?;
    pid_alive(pid).then_some(pid)
}

/// Serialize widget options back into `widget run` CLI arguments.
fn opts_to_args(opts: &BarOptions) -> Vec<String> {
    let mut args = Vec::new();
    for name in &opts.skip {
        args.push("--skip".to_string());
        args.push(name.clone());
    }
    if let Some(interval) = opts.interval {
        args.push("--interval".to_string());
        args.push(interval.to_string());
    }
    if opts.click_through {
        args.push("--click-through".to_string());
    }
    args
}

pub fn start(opts: BarOptions) {
    if let Some(pid) = running_pid() {
        println!("claudex widget is already running (pid {pid})");
        return;
    }

    let (Some(pid_path), Some(log_path)) = (pid_path(), log_path()) else {
        eprintln!("✗ could not resolve ~/.claudex");
        std::process::exit(1);
    };
    if let Some(dir) = pid_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(&pid_path); // stale entry from a crash

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("✗ failed to resolve current executable: {e}");
            std::process::exit(1);
        }
    };

    let log = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(log) => log,
        Err(e) => {
            eprintln!("✗ failed to open {}: {e}", log_path.display());
            std::process::exit(1);
        }
    };
    let log_err = match log.try_clone() {
        Ok(log_err) => log_err,
        Err(e) => {
            eprintln!("✗ failed to clone log handle: {e}");
            std::process::exit(1);
        }
    };

    let mut command = Command::new(exe);
    command
        .arg("widget")
        .arg("run")
        .args(opts_to_args(&opts))
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    // Detach from the controlling terminal so the widget survives the shell.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut command, || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    match command.spawn() {
        Ok(child) => {
            println!("claudex widget started (pid {})", child.id());
            println!("logs: {}", log_path.display());
        }
        Err(e) => {
            eprintln!("✗ failed to start claudex widget: {e}");
            std::process::exit(1);
        }
    }
}

pub fn stop() {
    match running_pid() {
        None => println!("claudex widget is not running"),
        Some(pid) => {
            unsafe { libc::kill(pid, libc::SIGTERM) };
            for _ in 0..30 {
                if !pid_alive(pid) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if pid_alive(pid) {
                eprintln!("✗ widget (pid {pid}) did not exit on SIGTERM");
                std::process::exit(1);
            }
            if let Some(path) = pid_path() {
                let _ = std::fs::remove_file(path);
            }
            println!("claudex widget stopped (pid {pid})");
        }
    }
}

pub fn restart(opts: BarOptions) {
    stop();
    start(opts);
}

pub fn status() {
    match running_pid() {
        Some(pid) => println!("claudex widget is running (pid {pid})"),
        None => println!("claudex widget is not running"),
    }
}

/// Foreground GUI entry point (`claudex widget run`, spawned by `start`).
pub fn run(opts: BarOptions) -> Result<(), eframe::Error> {
    if let Some(path) = pid_path()
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(path, format!("{}\n", std::process::id()));
    }

    let result = crate::bar::run(opts);

    if let Some(path) = pid_path() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opts_to_args_serializes_all_fields() {
        let opts = BarOptions {
            skip: vec!["grok".to_string(), "kimi".to_string()],
            interval: Some(300),
            click_through: true,
        };
        assert_eq!(
            opts_to_args(&opts),
            [
                "--skip",
                "grok",
                "--skip",
                "kimi",
                "--interval",
                "300",
                "--click-through"
            ]
        );
    }

    #[test]
    fn opts_to_args_is_empty_by_default() {
        assert!(opts_to_args(&BarOptions::default()).is_empty());
    }
}
