# claudex

> Supercharge the [Claude Code](https://claude.com/claude-code) CLI.

**claudex** is a power-user companion for the `claude` command line — a growing toolkit of extra commands that make your Claude Code workflow faster, slicker, and more fun. Think of it as the "missing extras" pack for Claude Code.

A handful of commands set the tone:

- **`claudex usage`** — see your *entire* Claude plan budget at a glance: current session, weekly limits, model-specific limits, and usage credits, all rendered as crisp colored bars in a single command.
- **`claudex codex usage`** — the same treatment for your [OpenAI Codex](https://developers.openai.com/codex/cli) / ChatGPT plan: subscription tier, 5-hour session window, weekly window, and any per-model limits.
- **`claudex agy usage`** — show your Gemini / Antigravity quota groups: Gemini models and Claude/GPT models, with weekly, 5-hour, and any model-level usage returned by the same Google Code Assist quota APIs.
- **`claudex glm usage`** — the GLM Coding Plan budget from your [Z.ai](https://z.ai) / [智谱 BigModel](https://open.bigmodel.cn) subscription: subscription tier, 5-hour session, weekly window, and MCP quota. Works for both the overseas (Z.ai) and domestic (BigModel) editions, auto-detected from your ZCode sign-in (override with `--cn` / `--global`).
- **`claudex update`** — one command to update all your coding agents (Claude, Codex, Antigravity, Kimi, Reasonix, Pi). It compares installed vs. latest versions, skips what's already current, and only runs the upgrade for what's actually outdated.
- **`claudex self-update`** — update claudex itself in place: it downloads the latest release binary for your platform, verifies its checksum, and swaps in the new one (falling back to the install script if anything goes wrong). No Rust toolchain needed.

No interactive session, no digging through a web app — just run the command and you're done.

> [!WARNING]
> **Unofficial & unaffiliated.** claudex is a personal, non-commercial project. It is **not** affiliated with, endorsed by, or supported by Anthropic, OpenAI, Google, or Z.ai / 智谱. It works by reusing the OAuth tokens that Claude Code, the Codex CLI, and Gemini / Antigravity CLI already store locally — and, for GLM, the API key that ZCode stores locally (or `GLM_API_KEY`) — and calling **undocumented** endpoints (`api.anthropic.com`, `chatgpt.com`, `cloudcode-pa.googleapis.com`, and `api.z.ai` / `open.bigmodel.cn`) with matching client behavior. Those endpoints may change or disappear without notice, and this usage may be against the providers' Terms of Service. Use it at your own risk. No warranty — see [LICENSE](LICENSE).

## Example

`claudex usage --all` shows everything at once — run `claudex usage`, `claudex codex usage`, `claudex agy usage`, or `claudex glm usage` on its own to see just that provider.
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

### `claudex codex usage` (Codex / ChatGPT)

It reads the access token from `~/.codex/auth.json` (written when you sign in with the Codex CLI — run `codex`), sends a `codex-cli` `User-Agent` plus your `ChatGPT-Account-Id`, calls `GET https://chatgpt.com/backend-api/wham/usage`, and renders the response. If you can run `codex`, you can run `claudex codex usage`.

### `claudex agy usage` (Gemini / Antigravity)

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

### `claudex update`

No credentials needed. claudex checks each agent's installed version (via `<agent> --version`) and compares it to the latest published version from the npm registry or PyPI. If an update is available, it runs the appropriate upgrade command:

| Agent | Latest version source | Upgrade command |
| --- | --- | --- |
| claude | npm `@anthropic-ai/claude-code` | `claude update` |
| codex | npm `@openai/codex` | `pnpm add -g @openai/codex` |
| agy | PyPI `antigravity-cli` | `agy update` |
| kimi | PyPI `kimi-cli` | `uv tool upgrade kimi-cli --no-cache` |
| reasonix | npm `reasonix` | `pnpm add -g reasonix` |
| pi | PyPI `pi-agent` | `pi update` |

Agents that aren't installed are silently skipped. Pass one or more agent names to update only those.

### `claudex self-update`

Updates claudex itself, not the agents above. It asks GitHub for the latest release, and if you're behind it downloads the prebuilt tarball for your platform, **verifies its sha256**, extracts it, and atomically replaces the running binary — no Rust toolchain required. A checksum mismatch aborts loudly; any other hiccup (network, extraction, a read-only install dir) falls back to the canonical `install.sh`. Pass `--check` to only report whether a newer version exists, or `--force` to reinstall the current version. Native self-update covers macOS and Linux (x86_64 / arm64); on Windows it points you at the releases page.

## Requirements

To **run** claudex (using a prebuilt binary), you only need:

- **macOS or Linux** (x86_64 or arm64). Windows is best-effort — no prebuilt binary; build from source.
- An authenticated **Claude Code** install for `claudex usage`, an authenticated **Codex CLI** install for `claudex codex usage`, an authenticated **Gemini / Antigravity CLI** install for `claudex agy usage`, and/or a **ZCode** sign-in (or `GLM_API_KEY`) for `claudex glm usage`, with an active subscription or quota.

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
claudex codex usage   # show Codex / ChatGPT plan usage limits
claudex agy usage     # show Gemini / Antigravity quota limits
claudex usage --all   # show Claude, Codex, and Gemini / Antigravity usage together
claudex usage --show-timezone       # include the timezone name in reset times
claudex codex usage --show-timezone # include the timezone name for Codex usage
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
