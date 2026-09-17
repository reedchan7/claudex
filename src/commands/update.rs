use colored::Colorize;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;

// ponytail: finite confirmations cover normal installer prompts; stream them if an updater asks endlessly.
const AUTO_CONFIRM_INPUT: &[u8] = b"yes\nyes\nyes\nyes\nyes\n";

/// Agents available by name but omitted from a no-args `update`.
const OPT_IN_ONLY: &[&str] = &["reasonix", "grok"];

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
        // pnpm 11 defaults minimum-release-age to 24h, which blocks brand-new
        // publishes; bypass for intentional upgrades. @latest rewrites the
        // global range so 0.x caret pins cannot stick on an older minor.
        update_cmd: &[
            "pnpm",
            "add",
            "-g",
            "@openai/codex@latest",
            "--config.minimum-release-age=0",
        ],
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
        // Native installs (default install.sh path) reject `kimi upgrade` on
        // some platforms; re-run the official installer instead.
        update_cmd: &[
            "sh",
            "-c",
            "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash",
        ],
    },
    Agent {
        name: "reasonix",
        display: "Reasonix",
        version_cmd: &["reasonix", "--version"],
        latest_cmd: LatestCmd::Npm("reasonix"),
        update_cmd: &[
            "pnpm",
            "add",
            "-g",
            "reasonix@latest",
            "--config.minimum-release-age=0",
        ],
    },
    Agent {
        name: "pi",
        display: "Pi",
        version_cmd: &["pi", "--version"],
        latest_cmd: LatestCmd::Npm("@earendil-works/pi-coding-agent"),
        update_cmd: &["pi", "update"],
    },
    Agent {
        name: "grok",
        display: "Grok Build",
        version_cmd: &["grok", "--version"],
        latest_cmd: LatestCmd::JsonField {
            program: "grok",
            args: &["update", "--check", "--json"],
            field: "latestVersion",
        },
        update_cmd: &["grok", "update"],
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
    /// Run a command and read a version field from its JSON stdout.
    JsonField {
        program: &'static str,
        args: &'static [&'static str],
        field: &'static str,
    },
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
        LatestCmd::JsonField {
            program,
            args,
            field,
        } => run_quiet(program, args).and_then(|raw| parse_json_version_field(&raw, field)),
    }
}

/// Extract a version string from a JSON object field (e.g. `"latestVersion":"0.2.93"`).
fn parse_json_version_field(json: &str, field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let raw = value.get(field)?.as_str()?;
    extract_version(raw).or_else(|| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
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

struct UpdateRun {
    success: bool,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

fn spawn_error(err: impl std::fmt::Display) -> UpdateRun {
    UpdateRun {
        success: false,
        stdout: String::new(),
        stderr: String::new(),
        error: Some(format!(
            "  {} failed to run update command: {}",
            "✗".red(),
            err
        )),
    }
}

fn exit_error(status: std::process::ExitStatus) -> String {
    format!(
        "  {} update command exited with {}",
        "✗".red(),
        status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string())
    )
}

fn do_update(agent: &Agent, capture: bool) -> UpdateRun {
    let cmd = agent.update_cmd;
    let mut command = Command::new(cmd[0]);
    command.args(&cmd[1..]).stdin(Stdio::piped());
    if capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return spawn_error(e),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(AUTO_CONFIRM_INPUT);
    }
    if capture {
        match child.wait_with_output() {
            Ok(output) => UpdateRun {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                error: (!output.status.success()).then(|| exit_error(output.status)),
            },
            Err(e) => spawn_error(e),
        }
    } else {
        match child.wait() {
            Ok(s) if s.success() => UpdateRun {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
                error: None,
            },
            Ok(s) => UpdateRun {
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(exit_error(s)),
            },
            Err(e) => spawn_error(e),
        }
    }
}

fn update_confirmed(current: Option<&str>, expected: Option<&str>, post_check: bool) -> bool {
    !post_check || expected.map_or(current.is_some(), |expected| current == Some(expected))
}

#[derive(Clone, Copy)]
enum AgentKind {
    Updated,
    Skipped,
    Failed,
    NotInstalled,
}

struct AgentOutcome {
    kind: AgentKind,
    output: String,
}

#[derive(Default)]
struct Counts {
    updated: u32,
    skipped: u32,
    failed: u32,
    not_installed: u32,
}

impl Counts {
    fn add(&mut self, kind: AgentKind) {
        match kind {
            AgentKind::Updated => self.updated += 1,
            AgentKind::Skipped => self.skipped += 1,
            AgentKind::Failed => self.failed += 1,
            AgentKind::NotInstalled => self.not_installed += 1,
        }
    }
}

struct Emitter {
    live: bool,
    output: String,
}

impl Emitter {
    fn new(live: bool) -> Self {
        Self {
            live,
            output: String::new(),
        }
    }

    fn line(&mut self, s: &str) {
        if self.live {
            println!("{s}");
        } else {
            self.output.push_str(s);
            self.output.push('\n');
        }
    }

    fn err(&mut self, s: &str) {
        if self.live {
            eprintln!("{s}");
        } else {
            self.output.push_str(s);
            self.output.push('\n');
        }
    }

    fn raw(&mut self, s: &str) {
        if self.live || s.is_empty() {
            return;
        }
        self.output.push_str(s);
        if !s.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn finish(self, kind: AgentKind) -> AgentOutcome {
        AgentOutcome {
            kind,
            output: self.output,
        }
    }
}

fn apply_update(
    agent: &Agent,
    expected: Option<&str>,
    post_check: bool,
    emit: &mut Emitter,
) -> AgentKind {
    emit.line(&format!(
        "{}",
        format!("  Running: {}", agent.update_cmd.join(" ")).dimmed()
    ));
    let run = do_update(agent, !emit.live);
    emit.raw(&run.stdout);
    emit.raw(&run.stderr);
    if let Some(msg) = &run.error {
        emit.err(msg);
    }
    if !run.success {
        return AgentKind::Failed;
    }
    if !post_check {
        return AgentKind::Updated;
    }
    let current = get_installed_version(agent);
    if update_confirmed(current.as_deref(), expected, post_check) {
        if expected.is_some() {
            emit.line(&format!(
                "  {} now {}",
                "✓".green(),
                current.unwrap().cyan()
            ));
        } else {
            emit.line(&format!(
                "  {} current {}",
                "✓".green(),
                current.unwrap().cyan()
            ));
        }
        AgentKind::Updated
    } else if expected.is_some() {
        emit.err(&format!(
            "  {} current {} after update (expected {})",
            "✗".red(),
            current.as_deref().unwrap_or("unknown").yellow(),
            expected.unwrap().green()
        ));
        AgentKind::Failed
    } else {
        emit.err(&format!(
            "  {} could not detect version after update",
            "✗".red()
        ));
        AgentKind::Failed
    }
}

fn process_agent(agent: &Agent, post_check: bool, live: bool) -> AgentOutcome {
    let mut emit = Emitter::new(live);

    emit.line("");
    emit.line(&agent.display.bold().to_string());

    let installed = match get_installed_version(agent) {
        Some(v) => v,
        None => {
            emit.line(&format!("  {} not installed, skipping", "—".dimmed()));
            return emit.finish(AgentKind::NotInstalled);
        }
    };

    let latest = match get_latest_version(agent) {
        Some(v) => v,
        None => {
            emit.line(&format!(
                "  installed {}  (could not check latest, updating anyway)",
                installed.cyan()
            ));
            let kind = apply_update(agent, None, post_check, &mut emit);
            return emit.finish(kind);
        }
    };

    if installed == latest {
        emit.line(&format!(
            "  {} {} already up to date",
            "✓".green(),
            installed.cyan()
        ));
        return emit.finish(AgentKind::Skipped);
    }

    emit.line(&format!("  {} → {}", installed.dimmed(), latest.green()));
    let kind = apply_update(agent, Some(&latest), post_check, &mut emit);
    emit.finish(kind)
}

fn resolve_jobs(jobs: Option<u32>, serial: bool, agent_count: usize) -> usize {
    if serial {
        1
    } else {
        jobs.map(|n| n as usize).unwrap_or(agent_count.max(1))
    }
}

/// Run `f` over `items` with at most `jobs` worker threads.
/// `on_ordered` is invoked on this thread in input order, as soon as the next
/// prefix is ready — later work that finishes first is held back.
fn for_each_bounded_ordered<T, R>(
    items: &[T],
    jobs: usize,
    f: impl Fn(&T) -> R + Sync,
    mut on_ordered: impl FnMut(R),
) where
    T: Sync,
    R: Send,
{
    if items.is_empty() {
        return;
    }
    let jobs = jobs.max(1).min(items.len());
    if jobs == 1 {
        for item in items {
            on_ordered(f(item));
        }
        return;
    }

    let f = &f;
    let next_idx = Mutex::new(0usize);
    let next_idx = &next_idx;
    thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        let n = items.len();

        for _ in 0..jobs {
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let i = {
                        let mut guard = next_idx.lock().unwrap();
                        if *guard >= n {
                            None
                        } else {
                            let i = *guard;
                            *guard += 1;
                            Some(i)
                        }
                    };
                    let Some(i) = i else { break };
                    if tx.send((i, f(&items[i]))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        let mut slots: Vec<Option<R>> = (0..n).map(|_| None).collect();
        let mut next_print = 0;
        for (i, result) in rx {
            slots[i] = Some(result);
            while next_print < n {
                match slots[next_print].take() {
                    Some(value) => {
                        on_ordered(value);
                        next_print += 1;
                    }
                    None => break,
                }
            }
        }
    });
}

fn resolve_agent_name(name: &str) -> Option<&'static Agent> {
    let lower = name.to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "grok-build" | "grokbuild" => "grok",
        "antigravity" | "gemini" => "agy",
        "gpt" => "codex",
        other => other,
    };
    AGENTS.iter().find(|a| a.name == canonical)
}

fn available_agent_names() -> String {
    AGENTS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
}

fn select_agents(targets: &[String], skip: &[String]) -> Result<Vec<&'static Agent>, String> {
    let mut selected: Vec<&'static Agent> = if targets.is_empty() {
        AGENTS
            .iter()
            .filter(|a| !OPT_IN_ONLY.contains(&a.name))
            .collect()
    } else {
        let mut selected: Vec<&'static Agent> = Vec::new();
        for name in targets {
            match resolve_agent_name(name) {
                Some(a) => {
                    if !selected.iter().any(|s| s.name == a.name) {
                        selected.push(a);
                    }
                }
                None => {
                    return Err(format!(
                        "unknown agent '{name}'. Available: {}",
                        available_agent_names()
                    ));
                }
            }
        }
        selected
    };

    for name in skip {
        match resolve_agent_name(name) {
            Some(a) => selected.retain(|s| s.name != a.name),
            None => {
                return Err(format!(
                    "unknown agent '{name}'. Available: {}",
                    available_agent_names()
                ));
            }
        }
    }

    if selected.is_empty() {
        return Err("no agents left after applying --skip".to_string());
    }

    Ok(selected)
}

pub fn run(targets: &[String], skip: &[String], post_check: bool, jobs: Option<u32>, serial: bool) {
    let agents = match select_agents(targets, skip) {
        Ok(agents) => agents,
        Err(e) => {
            eprintln!("{} {e}", "✗".red());
            std::process::exit(1);
        }
    };

    let jobs = resolve_jobs(jobs, serial, agents.len());
    let mut counts = Counts::default();
    let live = jobs <= 1 || agents.len() <= 1;

    if live {
        for agent in &agents {
            counts.add(process_agent(agent, post_check, true).kind);
        }
    } else {
        for_each_bounded_ordered(
            &agents,
            jobs,
            |agent| process_agent(agent, post_check, false),
            |outcome| {
                counts.add(outcome.kind);
                print!("{}", outcome.output);
            },
        );
    }

    // Summary
    println!();
    let mut parts: Vec<String> = Vec::new();
    if counts.updated > 0 {
        parts.push(format!("{} updated", counts.updated).green().to_string());
    }
    if counts.skipped > 0 {
        parts.push(format!("{} up to date", counts.skipped).to_string());
    }
    if counts.not_installed > 0 {
        parts.push(
            format!("{} not installed", counts.not_installed)
                .dimmed()
                .to_string(),
        );
    }
    if counts.failed > 0 {
        parts.push(format!("{} failed", counts.failed).red().to_string());
    }
    println!("Done: {}", parts.join(", "));

    if counts.failed > 0 {
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
        assert_eq!(
            kimi.update_cmd,
            &[
                "sh",
                "-c",
                "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash",
            ]
        );
    }

    #[test]
    fn pnpm_global_agents_bypass_minimum_release_age() {
        for name in ["codex", "reasonix"] {
            let agent = AGENTS.iter().find(|a| a.name == name).unwrap();
            assert_eq!(agent.update_cmd[0], "pnpm");
            assert!(
                agent.update_cmd.contains(&"--config.minimum-release-age=0"),
                "{name} update_cmd should bypass pnpm minimum-release-age"
            );
            assert!(
                agent.update_cmd.iter().any(|arg| arg.ends_with("@latest")),
                "{name} update_cmd should pin @latest"
            );
        }
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
    fn grok_uses_self_update_metadata() {
        let grok = AGENTS.iter().find(|a| a.name == "grok").unwrap();
        assert_eq!(grok.display, "Grok Build");
        assert_eq!(grok.version_cmd, &["grok", "--version"]);
        assert!(matches!(
            grok.latest_cmd,
            LatestCmd::JsonField {
                program: "grok",
                field: "latestVersion",
                ..
            }
        ));
        assert_eq!(grok.update_cmd, &["grok", "update"]);
    }

    #[test]
    fn parse_json_version_field_reads_latest() {
        let json = r#"{"currentVersion":"0.2.90","latestVersion":"0.2.93","updateAvailable":true}"#;
        assert_eq!(
            parse_json_version_field(json, "latestVersion").as_deref(),
            Some("0.2.93")
        );
    }

    #[test]
    fn select_agents_default_set_excludes_reasonix() {
        // A bare `claudex update` must not touch Reasonix or Grok; they are opt-in by name.
        let selected = select_agents(&[], &[]).unwrap();
        assert!(!selected.iter().any(|a| a.name == "reasonix"));
        assert!(!selected.iter().any(|a| a.name == "grok"));
        assert!(selected.iter().any(|a| a.name == "claude"));
        assert!(selected.iter().any(|a| a.name == "codex"));
        assert!(selected.iter().any(|a| a.name == "agy"));
        assert!(selected.iter().any(|a| a.name == "kimi"));
        assert!(selected.iter().any(|a| a.name == "pi"));
    }

    #[test]
    fn select_agents_explicit_grok_is_still_available() {
        let selected = select_agents(&["grok".into()], &[]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "grok");
    }

    #[test]
    fn select_agents_explicit_reasonix_is_still_available() {
        let selected = select_agents(&["reasonix".into()], &[]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "reasonix");
    }

    #[test]
    fn select_agents_applies_skip() {
        let selected = select_agents(&[], &["reasonix".into(), "pi".into()]).unwrap();
        assert!(!selected.iter().any(|a| a.name == "reasonix"));
        assert!(!selected.iter().any(|a| a.name == "pi"));
        assert!(selected.iter().any(|a| a.name == "claude"));
    }

    #[test]
    fn select_agents_accepts_grok_alias() {
        let selected = select_agents(&["grok-build".into()], &[]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "grok");
    }

    #[test]
    fn select_agents_accepts_gpt_and_gemini_aliases() {
        let gpt = select_agents(&["gpt".into()], &[]).unwrap();
        assert_eq!(gpt.len(), 1);
        assert_eq!(gpt[0].name, "codex");

        let gemini = select_agents(&["gemini".into()], &[]).unwrap();
        assert_eq!(gemini.len(), 1);
        assert_eq!(gemini[0].name, "agy");
    }

    #[test]
    fn select_agents_errors_when_everything_skipped() {
        match select_agents(&["claude".into()], &["claude".into()]) {
            Ok(_) => panic!("expected error when every agent is skipped"),
            Err(err) => assert!(err.contains("no agents left")),
        }
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

        assert!(do_update(&agent, false).success);
    }

    #[test]
    fn do_update_can_capture_command_output() {
        let agent = Agent {
            name: "capturing",
            display: "Capturing Agent",
            version_cmd: &["echo", "1.0.0"],
            latest_cmd: LatestCmd::Npm("unused"),
            update_cmd: &["sh", "-c", "echo captured-stdout; echo captured-stderr >&2"],
        };

        let run = do_update(&agent, true);
        assert!(run.success);
        assert!(run.stdout.contains("captured-stdout"));
        assert!(run.stderr.contains("captured-stderr"));
        assert!(run.error.is_none());
    }

    #[test]
    fn resolve_jobs_defaults_to_all_selected_agents() {
        assert_eq!(resolve_jobs(None, false, 6), 6);
        assert_eq!(resolve_jobs(None, false, 1), 1);
    }

    #[test]
    fn resolve_jobs_serial_is_one() {
        assert_eq!(resolve_jobs(None, true, 6), 1);
        assert_eq!(resolve_jobs(Some(4), true, 6), 1);
    }

    #[test]
    fn resolve_jobs_honors_explicit_limit() {
        assert_eq!(resolve_jobs(Some(2), false, 6), 2);
        assert_eq!(resolve_jobs(Some(99), false, 3), 99);
    }

    #[test]
    fn bounded_ordered_emits_in_input_order_even_when_later_work_finishes_first() {
        let items = [80u64, 10];
        let mut seen = Vec::new();
        for_each_bounded_ordered(
            &items,
            2,
            |ms| {
                thread::sleep(std::time::Duration::from_millis(*ms));
                *ms
            },
            |v| seen.push(v),
        );
        assert_eq!(seen, [80, 10]);
    }

    #[test]
    fn bounded_ordered_runs_jobs_in_parallel() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let items = [0, 1];
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn({
            let barrier = barrier.clone();
            move || {
                for_each_bounded_ordered(
                    &items,
                    2,
                    |_| {
                        barrier.wait();
                    },
                    |_| {},
                );
                done_tx.send(()).unwrap();
            }
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("two jobs should meet at the barrier");
    }

    #[test]
    fn bounded_ordered_serial_never_overlaps() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let current = AtomicUsize::new(0);
        let max = AtomicUsize::new(0);
        for_each_bounded_ordered(
            &[0, 1, 2],
            1,
            |_| {
                let n = current.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(n, Ordering::SeqCst);
                thread::sleep(std::time::Duration::from_millis(20));
                current.fetch_sub(1, Ordering::SeqCst);
            },
            |_| {},
        );
        assert_eq!(max.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unknown_target_name_is_detected() {
        // We can't easily test process::exit, but we can verify find logic.
        let found = AGENTS.iter().find(|a| a.name == "nonexistent");
        assert!(found.is_none());
    }
}
