use colored::Colorize;
use std::io::Write;
use std::process::{Command, Stdio};

// ponytail: finite confirmations cover normal installer prompts; stream them if an updater asks endlessly.
const AUTO_CONFIRM_INPUT: &[u8] = b"yes\nyes\nyes\nyes\nyes\n";

/// All supported coding agents and their update metadata.
const AGENTS: &[Agent] = &[
    Agent {
        name: "claude",
        display: "Claude Code",
        version_cmd: &["claude", "--version"],
        latest_cmd: LatestCmd::Npm("@anthropic-ai/claude-code"),
        update_cmd: &["claude", "update"],
    },
    Agent {
        name: "codex",
        display: "Codex",
        version_cmd: &["codex", "--version"],
        latest_cmd: LatestCmd::Npm("@openai/codex"),
        update_cmd: &["pnpm", "add", "-g", "@openai/codex"],
    },
    Agent {
        name: "agy",
        display: "Antigravity",
        version_cmd: &["agy", "--version"],
        latest_cmd: LatestCmd::Pip("antigravity-cli"),
        update_cmd: &["agy", "update"],
    },
    Agent {
        name: "kimi",
        display: "Kimi Code",
        version_cmd: &["kimi", "--version"],
        latest_cmd: LatestCmd::Npm("@moonshot-ai/kimi-code"),
        update_cmd: &["kimi", "upgrade"],
    },
    Agent {
        name: "reasonix",
        display: "Reasonix",
        version_cmd: &["reasonix", "--version"],
        latest_cmd: LatestCmd::Npm("reasonix"),
        update_cmd: &["pnpm", "add", "-g", "reasonix"],
    },
    Agent {
        name: "pi",
        display: "Pi",
        version_cmd: &["pi", "--version"],
        latest_cmd: LatestCmd::Npm("@earendil-works/pi-coding-agent"),
        update_cmd: &["pi", "update"],
    },
];

struct Agent {
    /// Short name used in CLI args (e.g. "claude").
    name: &'static str,
    /// Human-readable display name.
    display: &'static str,
    /// Command to get the currently installed version.
    version_cmd: &'static [&'static str],
    /// How to look up the latest published version.
    latest_cmd: LatestCmd,
    /// Command to perform the update.
    update_cmd: &'static [&'static str],
}

enum LatestCmd {
    /// npm registry lookup via `npm view <pkg> version`.
    Npm(&'static str),
    /// PyPI lookup via `pip index versions <pkg>`.
    Pip(&'static str),
}

/// Run a command and return trimmed stdout, or None on failure.
fn run_quiet(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (!s.is_empty()).then_some(s)
        })
}

/// Extract a semver-ish version from a string that may contain extra text.
/// Returns the first substring matching a `\d+\.\d+` pattern (possibly with more `.N` parts).
fn extract_version(raw: &str) -> Option<String> {
    // Walk through the string to find the first digit sequence with dots.
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i].is_ascii_digit() {
            let start = i;
            // Consume digits-dot groups.
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let candidate = &raw[start..i];
            // Must have at least one dot (e.g. "1.2").
            if candidate.contains('.') && !candidate.ends_with('.') {
                return Some(candidate.to_string());
            }
        }
        i += 1;
    }
    None
}

fn get_installed_version(agent: &Agent) -> Option<String> {
    let output = run_quiet(agent.version_cmd[0], &agent.version_cmd[1..])?;
    extract_version(&output)
}

fn npm_registry_view_programs(pnpm_available: bool) -> &'static [&'static str] {
    if pnpm_available { &["pnpm"] } else { &["npm"] }
}

fn get_latest_version(agent: &Agent) -> Option<String> {
    match &agent.latest_cmd {
        LatestCmd::Npm(pkg) => {
            for program in npm_registry_view_programs(run_quiet("pnpm", &["--version"]).is_some()) {
                if let Some(version) = run_quiet(program, &["view", pkg, "version"])
                    .and_then(|raw| extract_version(&raw))
                {
                    return Some(version);
                }
            }
            None
        }
        LatestCmd::Pip(pkg) => {
            // Try `pip index versions <pkg>` first, then fall back to PyPI JSON API.
            let from_pip = run_quiet("pip", &["index", "versions", pkg]).and_then(|raw| {
                raw.lines().find_map(|line| {
                    let trimmed = line.trim();
                    (trimmed.starts_with("LATEST:") || trimmed.starts_with("Latest version:"))
                        .then(|| extract_version(trimmed))
                        .flatten()
                })
            });
            from_pip.or_else(|| {
                // Fallback: query PyPI JSON API
                let raw = run_quiet(
                    "curl",
                    &["-sf", &format!("https://pypi.org/pypi/{pkg}/json")],
                )?;
                parse_pypi_version(&raw)
            })
        }
    }
}

/// Minimal JSON extraction of `"version"` from PyPI JSON response.
fn parse_pypi_version(json: &str) -> Option<String> {
    // Look for `"version":"..."` in the info block.
    let marker = "\"version\"";
    let idx = json.find(marker)?;
    let after = &json[idx + marker.len()..];
    // Skip whitespace and colon.
    let after = after.trim_start();
    let after = after.strip_prefix(':')?;
    let after = after.trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    let ver = &after[..end];
    (!ver.is_empty()).then(|| ver.to_string())
}

fn do_update(agent: &Agent) -> bool {
    let cmd = agent.update_cmd;
    println!("{}", format!("  Running: {}", cmd.join(" ")).dimmed());
    let mut child = match Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("  {} failed to run update command: {}", "✗".red(), e);
            return false;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(AUTO_CONFIRM_INPUT);
    }
    let status = child.wait();
    match status {
        Ok(s) if s.success() => true,
        Ok(s) => {
            eprintln!(
                "  {} update command exited with {}",
                "✗".red(),
                s.code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            );
            false
        }
        Err(e) => {
            eprintln!("  {} failed to run update command: {}", "✗".red(), e);
            false
        }
    }
}

fn update_confirmed(current: Option<&str>, expected: Option<&str>, post_check: bool) -> bool {
    !post_check || expected.map_or(current.is_some(), |expected| current == Some(expected))
}

pub fn run(targets: &[String], post_check: bool) {
    let agents: Vec<&Agent> = if targets.is_empty() {
        AGENTS.iter().collect()
    } else {
        let mut selected = Vec::new();
        for name in targets {
            let lower = name.to_ascii_lowercase();
            match AGENTS.iter().find(|a| a.name == lower) {
                Some(a) => selected.push(a),
                None => {
                    eprintln!(
                        "{} unknown agent '{}'. Available: {}",
                        "✗".red(),
                        name,
                        AGENTS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
                    );
                    std::process::exit(1);
                }
            }
        }
        selected
    };

    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;
    let mut not_installed = 0u32;

    for agent in &agents {
        println!();
        println!("{}", agent.display.bold());

        let installed = match get_installed_version(agent) {
            Some(v) => v,
            None => {
                println!("  {} not installed, skipping", "—".dimmed());
                not_installed += 1;
                continue;
            }
        };

        let latest = match get_latest_version(agent) {
            Some(v) => v,
            None => {
                println!(
                    "  installed {}  (could not check latest, updating anyway)",
                    installed.cyan()
                );
                if do_update(agent) {
                    if post_check {
                        let current = get_installed_version(agent);
                        if update_confirmed(current.as_deref(), None, post_check) {
                            println!("  {} current {}", "✓".green(), current.unwrap().cyan());
                            updated += 1;
                        } else {
                            eprintln!("  {} could not detect version after update", "✗".red());
                            failed += 1;
                        }
                    } else {
                        updated += 1;
                    }
                } else {
                    failed += 1;
                }
                continue;
            }
        };

        if installed == latest {
            println!("  {} {} already up to date", "✓".green(), installed.cyan());
            skipped += 1;
            continue;
        }

        println!("  {} → {}", installed.dimmed(), latest.green());

        if do_update(agent) {
            if post_check {
                let current = get_installed_version(agent);
                if update_confirmed(current.as_deref(), Some(&latest), post_check) {
                    println!("  {} now {}", "✓".green(), current.unwrap().cyan());
                    updated += 1;
                } else {
                    eprintln!(
                        "  {} current {} after update (expected {})",
                        "✗".red(),
                        current.as_deref().unwrap_or("unknown").yellow(),
                        latest.green()
                    );
                    failed += 1;
                }
            } else {
                updated += 1;
            }
        } else {
            failed += 1;
        }
    }

    // Summary
    println!();
    let mut parts: Vec<String> = Vec::new();
    if updated > 0 {
        parts.push(format!("{} updated", updated).green().to_string());
    }
    if skipped > 0 {
        parts.push(format!("{} up to date", skipped).to_string());
    }
    if not_installed > 0 {
        parts.push(
            format!("{} not installed", not_installed)
                .dimmed()
                .to_string(),
        );
    }
    if failed > 0 {
        parts.push(format!("{} failed", failed).red().to_string());
    }
    println!("Done: {}", parts.join(", "));

    if failed > 0 {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_version_plain() {
        assert_eq!(extract_version("1.2.3"), Some("1.2.3".into()));
    }

    #[test]
    fn extract_version_with_prefix() {
        assert_eq!(extract_version("v1.0.10"), Some("1.0.10".into()));
        assert_eq!(extract_version("claude-code 1.0.33"), Some("1.0.33".into()));
    }

    #[test]
    fn extract_version_no_dots() {
        assert_eq!(extract_version("42"), None);
        assert_eq!(extract_version("hello"), None);
    }

    #[test]
    fn extract_version_trailing_dot() {
        assert_eq!(extract_version("1.2."), None);
    }

    #[test]
    fn all_agent_names_are_lowercase() {
        for a in AGENTS {
            assert_eq!(a.name, a.name.to_ascii_lowercase());
        }
    }

    #[test]
    fn all_agents_have_non_empty_update_cmd() {
        for a in AGENTS {
            assert!(!a.update_cmd.is_empty());
        }
    }

    #[test]
    fn kimi_uses_kimi_code_metadata() {
        let kimi = AGENTS.iter().find(|a| a.name == "kimi").unwrap();
        assert_eq!(kimi.display, "Kimi Code");
        assert_eq!(kimi.version_cmd, &["kimi", "--version"]);
        assert!(matches!(
            kimi.latest_cmd,
            LatestCmd::Npm("@moonshot-ai/kimi-code")
        ));
        assert_eq!(kimi.update_cmd, &["kimi", "upgrade"]);
    }

    #[test]
    fn pi_uses_official_npm_package_metadata() {
        let pi = AGENTS.iter().find(|a| a.name == "pi").unwrap();
        assert_eq!(pi.display, "Pi");
        assert_eq!(pi.version_cmd, &["pi", "--version"]);
        assert!(matches!(
            pi.latest_cmd,
            LatestCmd::Npm("@earendil-works/pi-coding-agent")
        ));
        assert_eq!(pi.update_cmd, &["pi", "update"]);
    }

    #[test]
    fn npm_registry_metadata_prefers_pnpm_when_available() {
        assert_eq!(npm_registry_view_programs(true), ["pnpm"]);
        assert_eq!(npm_registry_view_programs(false), ["npm"]);
    }

    #[test]
    fn parse_pypi_version_basic() {
        let json = r#"{"info":{"version":"2.1.0","name":"foo"}}"#;
        assert_eq!(parse_pypi_version(json), Some("2.1.0".into()));
    }

    #[test]
    fn parse_pypi_version_missing() {
        assert_eq!(parse_pypi_version("{}"), None);
    }

    #[test]
    fn update_confirmed_requires_latest_when_known() {
        assert!(update_confirmed(Some("2.1.201"), Some("2.1.201"), true));
        assert!(!update_confirmed(Some("2.1.200"), Some("2.1.201"), true));
    }

    #[test]
    fn update_confirmed_accepts_detected_version_when_latest_unknown() {
        assert!(update_confirmed(Some("1.0.16"), None, true));
        assert!(!update_confirmed(None, None, true));
    }

    #[test]
    fn update_confirmed_can_be_skipped() {
        assert!(update_confirmed(Some("2.1.200"), Some("2.1.201"), false));
        assert!(update_confirmed(None, Some("2.1.201"), false));
    }

    #[test]
    fn do_update_confirms_prompts_by_default() {
        let agent = Agent {
            name: "confirming",
            display: "Confirming Agent",
            version_cmd: &["echo", "1.0.0"],
            latest_cmd: LatestCmd::Npm("unused"),
            update_cmd: &["sh", "-c", "read answer; test \"$answer\" = yes"],
        };

        assert!(do_update(&agent));
    }

    #[test]
    fn unknown_target_name_is_detected() {
        // We can't easily test process::exit, but we can verify find logic.
        let found = AGENTS.iter().find(|a| a.name == "nonexistent");
        assert!(found.is_none());
    }
}
