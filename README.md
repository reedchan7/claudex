# claudex

> Supercharge the [Claude Code](https://claude.com/claude-code) CLI.

**claudex** is a power-user companion for the `claude` command line — a growing toolkit of extra commands that make your Claude Code workflow faster, slicker, and more fun. Think of it as the "missing extras" pack for Claude Code.

Two commands set the tone:

- **`claudex usage`** — see your *entire* Claude plan budget at a glance: current session, weekly limits, Sonnet-only, and usage credits, all rendered as crisp colored bars in a single command.
- **`claudex codex usage`** — the same treatment for your [OpenAI Codex](https://developers.openai.com/codex/cli) / ChatGPT plan: subscription tier, 5-hour session window, weekly window, and any per-model limits.

No interactive session, no digging through a web app — just run the command and you're done. More commands are on the way.

> [!WARNING]
> **Unofficial & unaffiliated.** claudex is a personal, non-commercial project. It is **not** affiliated with, endorsed by, or supported by Anthropic or OpenAI. It works by reusing the OAuth tokens that Claude Code and the Codex CLI already store locally, and calling **undocumented** endpoints (`api.anthropic.com` and `chatgpt.com`) with the matching `User-Agent`. Those endpoints may change or disappear without notice, and this usage may be against the providers' Terms of Service. Use it at your own risk. No warranty — see [LICENSE](LICENSE).

## Example

```console
$ claudex usage
Current session
█████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 34% used
Resets 2:30pm (Asia/Shanghai), 2h 30m left

Current week (all models)
███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 6% used
Resets May 30 at 3am (Asia/Shanghai), 4d 11h left

Current week (Sonnet only)
██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 3% used
Resets May 30 at 3am (Asia/Shanghai), 4d 11h left

Usage credits   off
```

```console
$ claudex codex usage
Subscription: Pro

Current session (5h)
██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 4% used
Resets 6:19pm (Asia/Shanghai), 4h 35m left

Current week
█████████████████████████████░░░░░░░░░░░░░░░░░░░░░ 58% used
Resets May 31 at 2:55pm (Asia/Shanghai), 1d 1h left

GPT-5.3-Codex-Spark — Current session (5h)
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Resets 6:44pm (Asia/Shanghai), 5h left

GPT-5.3-Codex-Spark — Current week
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0% used
Resets Jun 6 at 1:44pm (Asia/Shanghai), 7d left
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

## Requirements

To **run** claudex (using a prebuilt binary), you only need:

- **macOS or Linux** (x86_64 or arm64). Windows is best-effort — no prebuilt binary; build from source.
- An authenticated **Claude Code** install for `claudex usage`, and/or an authenticated **Codex CLI** install for `claudex codex usage`, with an active subscription.

No Rust toolchain is required to run a prebuilt binary. Rust (edition 2024, so 1.85+) is only needed if you build from source.

## Install

### Install or upgrade (recommended)

Download the right prebuilt binary for your platform and install it — no Rust required:

```sh
curl -fsSL https://raw.githubusercontent.com/reedchan7/claudex/main/install.sh | sh
```

**The same command also upgrades.** Re-run it anytime: it detects an existing install, does nothing if you already have the latest version, and otherwise updates the binary you actually run, in place. Pass `CLAUDEX_FORCE=1` to reinstall even when you're already up to date.

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
claudex --help        # list available commands
claudex --version     # print the version
```

If your Claude token lives somewhere non-standard (or you just want to be explicit), set it directly:

```sh
export CLAUDE_CODE_OAUTH_TOKEN="sk-ant-oat01-..."
claudex usage
```

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
| `make install` | Install to `~/.cargo/bin` |
| `make clean` | Remove build artifacts |

## License

[MIT](LICENSE) © Reed Chan
