# claudex

> Supercharge the [Claude Code](https://claude.com/claude-code) CLI.

**claudex** is a power-user companion for the `claude` command line — a growing toolkit of extra commands that make your Claude Code workflow faster, slicker, and more fun. Think of it as the "missing extras" pack for Claude Code.

A handful of commands set the tone:

- **`claudex usage`** — see your *entire* Claude plan budget at a glance: current session, weekly limits, model-specific limits, and usage credits, all rendered as crisp colored bars in a single command.
- **`claudex gpt usage`** — the same treatment for your [OpenAI Codex](https://developers.openai.com/codex/cli) / ChatGPT plan: subscription tier, 5-hour session window, weekly window, and any per-model limits. (`codex` remains a supported alias.)
- **`claudex kimi usage`** — show your Kimi Code plan usage from the same managed usage endpoint Kimi Code uses: weekly budget plus the rolling 5-hour limit.
- **`claudex agy usage`** — show your Gemini / Antigravity quota groups: Gemini models and Claude/GPT models, with weekly, 5-hour, and any model-level usage returned by the same Google Code Assist quota APIs. (`gemini` and `antigravity` are aliases.)
- **`claudex glm usage`** — the GLM Coding Plan budget from your [Z.ai](https://z.ai) / [智谱 BigModel](https://open.bigmodel.cn) subscription: subscription tier, 5-hour session, weekly window, and MCP quota. Works for both the overseas (Z.ai) and domestic (BigModel) editions, auto-detected from your ZCode sign-in (override with `--cn` / `--global`).
- **`claudex grok usage`** — your [Grok Build](https://docs.x.ai/build) credit / plan usage from the same billing endpoint the Grok CLI uses: weekly (or current-period) usage by product, plus any on-demand / prepaid balances.
- **`claudex update`** — one command to update all your coding agents (Claude, Codex, Antigravity, Kimi Code, Reasonix, Pi, Grok). It compares installed vs. latest versions, skips what's already current, and only runs the upgrade for what's actually outdated. Pass `--skip <agent>...` to exclude agents.
- **`claudex-bar`** — a macOS desktop widget that pins a small translucent card to your desktop showing live usage for your agents, refreshed on a timer from `claudex usage --all --json`. Comes with a menu-bar icon for refresh / click-through / quit. Build it with `make bar`.
- **`claudex self-update`** — update claudex itself in place: it downloads the latest release binary for your platform, verifies its checksum, and swaps in the new one (falling back to the install script if anything goes wrong). No Rust toolchain needed.

No interactive session, no digging through a web app — just run the command and you're done.

> [!WARNING]
> **Unofficial & unaffiliated.** claudex is a personal, non-commercial project. It is **not** affiliated with, endorsed by, or supported by Anthropic, OpenAI, Google, Moonshot AI / Kimi, Z.ai / 智谱, or xAI. It works by reusing the OAuth tokens that Claude Code, the Codex CLI, Kimi Code, Gemini / Antigravity CLI, and Grok Build already store locally — and, for GLM, the API key that ZCode stores locally (or `GLM_API_KEY`) — and calling **undocumented** endpoints (`api.anthropic.com`, `chatgpt.com`, `api.kimi.com`, `cloudcode-pa.googleapis.com`, `api.z.ai` / `open.bigmodel.cn`, and `cli-chat-proxy.grok.com`) with matching client behavior. Those endpoints may change or disappear without notice, and this usage may be against the providers' Terms of Service. Use it at your own risk. No warranty — see [LICENSE](LICENSE).

## Example

`claudex usage --all` shows everything at once — run `claudex usage`, `claudex gpt usage`, `claudex kimi usage`, `claudex agy usage`, `claudex glm usage`, or `claudex grok usage` on its own to see just that provider. Pass `--skip <agent>...` with `--all` to exclude providers.
Reset times are shown in your local timezone. Add `--show-timezone` when you also want the timezone name in the output.

```console
$ claudex usage --all
Claude Code
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Current session
█████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 34% used
Resets 2:30pm, 2h 30m left

Current week (all models)
███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 6% used
Resets May 30 at 3am, 4d 11h left

Current week (Fable)
███████████████████████████████████████████░░░░░░░ 86% used
Resets May 30 at 3am, 4d 11h left

Usage credits   off

Codex
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Subscription: Pro

Current session (5h)
██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 4% used
Resets 6:19pm, 4h 35m left

Current week
█████████████████████████████░░░░░░░░░░░░░░░░░░░░░ 58% used
Resets May 31 at 2:55pm, 1d 1h left

GPT-5.3-Codex-Spark — Current session (5h)
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Resets 6:44pm, 5h left

GPT-5.3-Codex-Spark — Current week
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Resets Jun 6 at 1:44pm, 7d left

Kimi Code
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Weekly limit
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Used 0 / 100; resets in 6d 23h

5h limit
█░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1% used
Used 1 / 100; resets in 4h 45m

Gemini / Antigravity
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Gemini Models
Models within this group: Gemini Flash, Gemini Pro

Weekly Limit
████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 7.92% used
Refreshes Jun 19 at 4:46pm, 2d 21h left

Five Hour Limit
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0.00% used
Refreshes 4:39pm, 4h 58m left

Claude and GPT models
Models within this group: Claude Opus, Claude Sonnet, GPT-OSS

Weekly Limit
██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 28.56% used
Refreshes Jun 23 at 9:30am, 6d 17h left

Five Hour Limit
██████████████████████████████████████████░░░░░░░░ 84.40% used
Refreshes 2:30pm, 2h 49m left

───────────────────────────────────────────────────────────────
Model Usage

Claude Sonnet
██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 28.56% used
Resets: Jun 23 at 9:30am, 6d 17h left

GLM / Z.ai
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Subscription: Pro

Current session (5h)
████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 8% used
Resets Jun 26 at 2:12am, 4h 24m left

Current week
██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 4% used
Resets Jul 2 at 10:42am, 6d 12h left

MCP quota
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Used 0 / 1000
  search-prime: 0
  web-reader: 0
  zread: 0
Resets Jul 25 at 10:42am, 29d 12h left
```

Progress bars are colored by utilization: green below 50%, yellow from 50–80%, red at 80% and above.

## How it works

claudex reuses the credentials these CLIs already store on your machine — no extra login or API key required.

### `claudex usage` (Claude)

It resolves the OAuth access token from the first available source:

1. The `CLAUDE_CODE_OAUTH_TOKEN` environment variable, if set.
2. On macOS, the Keychain entry `Claude Code-credentials`.
3. The credentials file at `$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json`).

It then detects your installed `claude` version (via `claude --version`) to send a matching `User-Agent`, calls `GET https://api.anthropic.com/api/oauth/usage`, and renders the response. If you can run `claude`, you can run `claudex usage`.

### `claudex gpt usage` (Codex / ChatGPT)

It reads the access token from `~/.codex/auth.json` (written when you sign in with the Codex CLI — run `codex`), sends a `codex-cli` `User-Agent` plus your `ChatGPT-Account-Id`, calls `GET https://chatgpt.com/backend-api/wham/usage`, and renders the response. If you can run `codex`, you can run `claudex gpt usage` (or the `codex` alias).

### `claudex kimi usage` (Kimi Code)

It reads the Kimi Code OAuth access token from `~/.kimi-code/credentials/kimi-code.json` (falling back to the legacy `~/.kimi/credentials/kimi-code.json`), calls `GET https://api.kimi.com/coding/v1/usages` with `Authorization: Bearer <token>`, and renders the weekly budget plus rolling limits returned by Kimi Code. If you can run `kimi usage`, you can run `claudex kimi usage`.

### `claudex agy usage` (Gemini / Antigravity)

Aliases: `gemini`, `antigravity`.

It reads Antigravity's Google OAuth access token from the system keyring (on macOS, Keychain service `gemini`, account `antigravity`), calls `POST https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary` for pooled quota groups, then uses `loadCodeAssist` plus `retrieveUserQuota` on `https://cloudcode-pa.googleapis.com/v1internal` for model-level buckets when Google returns depleted model quota. If the token has expired, run `agy` once so Antigravity refreshes its saved session.

The summary endpoint reports pooled quota groups. claudex keeps that shape, then adds a `Model Usage` section from returned `modelId` buckets that are below full quota, aggregated by tier:

- **Gemini Models** — Gemini Flash and Gemini Pro family usage.
- **Claude and GPT models** — Claude Opus, Claude Sonnet, and GPT-OSS family usage.

Because the endpoints are account- and tier-aware, the exact groups, percentages, and refresh windows come from your current Antigravity session.

### `claudex glm usage` (GLM / Z.ai / BigModel)

It resolves the edition and API key without a dedicated GLM CLI:

1. **Region** — `--cn` / `--global`, else `GLM_REGION` (`cn` / `global`), else ZCode's `providerFamilyDomain` from `~/.zcode/v2/setting.json` (`zai` → overseas, `bigmodel` → domestic), else overseas.
2. **API key** — `GLM_API_KEY`, else the plaintext key ZCode stores in `~/.zcode/v2/config.json` for the matching coding-plan provider.

It then calls `GET {base}/api/monitor/usage/quota/limit` (`https://api.z.ai` overseas, `https://open.bigmodel.cn` domestic) with `Authorization: Bearer <key>`, and renders the returned limits: the 5-hour session, the weekly window, and the MCP quota (with its per-tool breakdown). If you can sign in with ZCode, you can run `claudex glm usage`.

### `claudex grok usage` (Grok Build)

It reads the xAI OAuth access token from `~/.grok/auth.json` (written by `grok login`; this takes precedence over `XAI_API_KEY`, matching Grok itself), optionally refreshes it via `auth.x.ai` when expired, then calls `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` with a matching `x-grok-client-version` header and renders the returned credit / product usage. `XAI_API_KEY` is accepted only when no session is stored. If you can run `grok`, you can run `claudex grok usage`.

### `--json` (machine-readable snapshots)

Every usage command accepts `--json` to print a normalized JSON snapshot instead of terminal bars — one `providers` array with per-provider status, preformatted bar/detail rows, and raw `resets_at` timestamps (schema version 1, see `src/snapshot.rs`). Unavailable providers are included with a structured `unavailable` state; the exit code is non-zero only when *none* are available. This is the data source for `claudex-bar` and any other shell integration (tmux, SketchyBar, waybar, …).

### `claudex-bar` (desktop widget, macOS)

A small always-on-desktop floating card that shows the same bars the CLI prints, rebuilt from `claudex usage --all --json` every few minutes. The bar process never touches the network or credentials itself — it spawns the `claudex` CLI and renders the JSON snapshot, so token refresh stays in one place. It follows the system light/dark appearance, runs as an accessory app (no Dock icon), floats below normal windows, and can be dragged anywhere; its position is remembered in `~/.claudex/bar.json`.

The card has a header row: **▾/▸ Agent Usage** collapses the whole widget to a one-line-per-provider mini view, **↻** refreshes now (a spinner shows while a poll is in flight), and **×** hides the window. Each provider sits in its own tinted section with an accent-colored edge; click a provider's header to collapse/expand just that section (collapsed sections show their peak usage). The footer picks the poll interval: presets 2m / 5m / 10m (default) / 30m / 1h, or Custom… for values like `90s`, `10m`, `1h30m`. Collapse state, mini mode, interval, and window position all persist across restarts. The menu-bar icon offers Refresh Now, Show/Hide Widget, a Click-through toggle (mouse passes through the card), and Quit.

```sh
make bar                          # build target/release/claudex-bar (Rust required)
./target/release/claudex-bar      # run next to a claudex binary, or set CLAUDEX_BIN
./target/release/claudex-bar --skip grok,kimi --interval 120
```

Flags: `--skip <agent>...` (same names as `usage --all --skip`), `--interval <secs>` (overrides the saved interval for that run; minimum 60), `--click-through` (start in click-through mode). It finds the `claudex` binary via `$CLAUDEX_BIN`, then its own directory, then `$PATH`. The GUI dependencies (egui + tray-icon) are gated behind the `bar` cargo feature, so CLI-only builds and installs are unchanged.

### `claudex update`

No credentials needed. claudex checks each agent's installed version (via `<agent> --version`) and compares it to the latest published version from the npm registry, PyPI, or (for Grok) `grok update --check --json`. If an update is available, it runs the appropriate upgrade command:

| Agent | Latest version source | Upgrade command |
| --- | --- | --- |
| claude | npm `@anthropic-ai/claude-code` | `claude update` |
| codex | npm `@openai/codex` | `pnpm add -g @openai/codex@latest --config.minimum-release-age=0` |
| agy | PyPI `antigravity-cli` | `agy update` |
| kimi | npm `@moonshot-ai/kimi-code` | official install script (`curl …/install.sh \| bash`) |
| reasonix | npm `reasonix` | `pnpm add -g reasonix@latest --config.minimum-release-age=0` |
| pi | npm `@earendil-works/pi-coding-agent` | `pi update` |
| grok | `grok update --check --json` | `grok update` |

pnpm 11 defaults `minimum-release-age` to 24 hours, so a package published today can show up in `pnpm view … version` (and in claudex's "latest" check) while `pnpm add -g` still refuses it. claudex bypasses that gate for intentional upgrades. Kimi native installs often reject `kimi upgrade`, so claudex re-runs the official installer instead.

Agents that aren't installed are silently skipped. Pass one or more agent names to update only those, or `--skip <agent>...` (repeatable / comma-separated) to exclude agents from an all-agent run.

### `claudex self-update`

Updates claudex itself, not the agents above. It asks GitHub for the latest release, and if you're behind it downloads the prebuilt tarball for your platform, **verifies its sha256**, extracts it, and atomically replaces the running binary — no Rust toolchain required. A checksum mismatch aborts loudly; any other hiccup (network, extraction, a read-only install dir) falls back to the canonical `install.sh`. Pass `--check` to only report whether a newer version exists, or `--force` to reinstall the current version. Native self-update covers macOS and Linux (x86_64 / arm64); on Windows it points you at the releases page.

## Requirements

To **run** claudex (using a prebuilt binary), you only need:

- **macOS or Linux** (x86_64 or arm64). Windows is best-effort — no prebuilt binary; build from source.
- An authenticated **Claude Code** install for `claudex usage`, an authenticated **Codex CLI** install for `claudex gpt usage`, an authenticated **Kimi Code** install for `claudex kimi usage`, an authenticated **Gemini / Antigravity CLI** install for `claudex agy usage`, a **ZCode** sign-in (or `GLM_API_KEY`) for `claudex glm usage`, and/or an authenticated **Grok Build** install for `claudex grok usage`, with an active subscription or quota.

No Rust toolchain is required to run a prebuilt binary. Rust (edition 2024, so 1.85+) is only needed if you build from source.

## Install

### Install or upgrade (recommended)

Download the right prebuilt binary for your platform and install it — no Rust required:

```sh
curl -fsSL https://raw.githubusercontent.com/reedchan7/claudex/main/install.sh | sh
```

**The same command also upgrades.** Re-run it anytime: it detects an existing install, does nothing if you already have the latest version, and otherwise updates the binary you actually run, in place. Pass `CLAUDEX_FORCE=1` to reinstall even when you're already up to date. Once installed, `claudex self-update` does the same thing in place — with checksum verification — and falls back to this script if needed.

A fresh install lands in `~/.local/bin` (override with `CLAUDEX_INSTALL_DIR`), creating the directory if needed. If that directory isn't on your `PATH`, the installer adds it to your shell profile (`.zshrc` / `.bashrc` / `.bash_profile` / fish config) automatically — restart your shell afterwards. Set `CLAUDEX_NO_MODIFY_PATH=1` to manage `PATH` yourself.

### Download manually

Grab the archive for your platform from the [latest release](https://github.com/reedchan7/claudex/releases/latest), extract it, and put `claudex` on your `PATH`. Prebuilt targets:

| Platform | Asset |
| --- | --- |
| macOS (Apple Silicon) | `claudex-<tag>-darwin-arm64.tar.gz` |
| macOS (Intel) | `claudex-<tag>-darwin-amd64.tar.gz` |
| Linux (x86_64) | `claudex-<tag>-linux-amd64.tar.gz` |
| Linux (arm64) | `claudex-<tag>-linux-arm64.tar.gz` |
| Windows (x86_64) | `claudex-<tag>-windows-amd64.zip` |

### Build from source

Requires the Rust toolchain:

```sh
cargo install --path .
# or
make install
```

This installs the `claudex` binary to `~/.cargo/bin`.

## Usage

```sh
claudex usage         # show Claude plan usage limits
claudex gpt usage     # show Codex / ChatGPT plan usage limits
claudex agy usage     # show Gemini / Antigravity quota limits
claudex gemini usage  # same as `claudex agy usage`
claudex grok usage    # show Grok Build credit / plan usage
claudex usage --all   # show every provider together
claudex usage --all --json            # machine-readable snapshot (schema v1)
claudex usage --all --skip grok,kimi   # all providers except Grok and Kimi
claudex update --skip reasonix,pi      # update all agents except Reasonix and Pi
claudex usage --show-timezone       # include the timezone name in reset times
claudex gpt usage --show-timezone   # include the timezone name for Codex usage
claudex agy usage --show-timezone   # include the timezone name for Gemini / Antigravity usage
claudex update                # update all coding agents
claudex update claude codex   # update specific agents only
claudex self-update           # update claudex itself in place
claudex self-update --check   # only check whether a newer claudex exists
claudex --help        # list available commands
claudex --version     # print the version
```

If your Claude token lives somewhere non-standard (or you just want to be explicit), set it directly:

```sh
export CLAUDE_CODE_OAUTH_TOKEN="sk-ant-oat01-..."
claudex usage
```

### Unavailable providers

When a provider has no local session, has unreadable credentials, or rejects the saved token, claudex keeps the output structured and shows an empty usage bar with a short next step:

```console
Codex is not connected
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ unavailable
No local Codex session was found on this machine.
Next: Run `codex` and sign in with ChatGPT.
```

Single-provider commands exit non-zero when that provider is unavailable. `claudex usage --all` still renders the other providers and only exits non-zero when none of them can be shown.

## Development

Common tasks are available through the `Makefile`:

| Command | Description |
| --- | --- |
| `make build` | Build the debug binary |
| `make release` | Build the optimized release binary |
| `make bar` | Build the claudex-bar desktop widget (release, `--features bar`) |
| `make test` | Run the test suite |
| `make fmt` | Format the code with rustfmt |
| `make lint` | Run clippy with warnings denied |
| `make check` | Format check + lint + test (CI gate) |
| `make run` | Run `claudex usage` |
| `make version` | Print the current crate version |
| `make set-version VERSION=0.3.0` | Set an explicit version in `Cargo.toml` / `Cargo.lock` without committing |
| `make bump-patch` | Bump the patch version in `Cargo.toml` / `Cargo.lock` without committing |
| `make bump-minor` | Bump the minor version in files only |
| `make bump-major` | Bump the major version in files only |
| `make tag-version` | Tag the current committed version as `vX.Y.Z` |
| `make install` | Install to `~/.cargo/bin` |
| `make clean` | Remove build artifacts |

Version targets only edit the version files. Commit the version bump together with the code it releases, then run `make tag-version` after that commit if you want a release tag:

```sh
make set-version VERSION=0.3.0
git add -A
git commit
make tag-version
```

## License

[MIT](LICENSE) © Reed Chan
