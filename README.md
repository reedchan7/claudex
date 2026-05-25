# claudex

A small CLI that extends the [Claude Code](https://claude.com/claude-code) CLI with extra commands.

The first command, `claudex usage`, shows your current Claude plan usage limits right from the terminal — the same data as the in-session `/usage` view, but available as a standalone one-shot command.

> [!WARNING]
> **Unofficial & unaffiliated.** claudex is a personal, non-commercial project. It is **not** affiliated with, endorsed by, or supported by Anthropic. It works by reusing the OAuth token that Claude Code stores locally and calling an **undocumented** Anthropic endpoint with a Claude Code `User-Agent`. That endpoint may change or disappear without notice, and this usage may be against Anthropic's Terms of Service. Use it at your own risk. No warranty — see [LICENSE](LICENSE).

## Example

```console
$ claudex usage
Current session
█████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 34% used
Resets 2:30pm (Asia/Shanghai)

Current week (all models)
███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 6% used
Resets May 30 at 3am (Asia/Shanghai)

Current week (Sonnet only)
██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 3% used
Resets May 30 at 3am (Asia/Shanghai)

Usage credits   off · /usage-credits to turn on
```

Progress bars are colored by utilization: green below 50%, yellow from 50–80%, red at 80% and above.

## How it works

`claudex usage` reuses the credentials Claude Code already stores on your machine. It resolves the OAuth access token from the first available source:

1. The `CLAUDE_CODE_OAUTH_TOKEN` environment variable, if set.
2. On macOS, the Keychain entry `Claude Code-credentials`.
3. The credentials file at `$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json`).

It then detects your installed `claude` version (via `claude --version`) to send a matching `User-Agent`, calls `GET https://api.anthropic.com/api/oauth/usage`, and renders the response.

No extra login or API key is required — if you can run `claude`, you can run `claudex usage`.

## Requirements

To **run** claudex (using a prebuilt binary), you only need:

- **macOS or Linux** (x86_64 or arm64). Windows is best-effort — no prebuilt binary; build from source.
- **An authenticated Claude Code install** with an active Claude subscription (Pro / Max / Team).

No Rust toolchain is required to run a prebuilt binary. Rust (edition 2024, so 1.85+) is only needed if you build from source.

## Install

### Quick install (recommended)

Download the right prebuilt binary for your platform and install it — no Rust required:

```sh
curl -fsSL https://raw.githubusercontent.com/reedchan7/claudex/main/install.sh | sh
```

This installs `claudex` to `~/.local/bin` (override with `CLAUDEX_INSTALL_DIR`), creating the directory if needed. If that directory isn't on your `PATH`, the installer adds it to your shell profile (`.zshrc` / `.bashrc` / `.bash_profile` / fish config) automatically — restart your shell afterwards. Set `CLAUDEX_NO_MODIFY_PATH=1` to manage `PATH` yourself.

### Download manually

Grab the archive for your platform from the [latest release](https://github.com/reedchan7/claudex/releases/latest), extract it, and put `claudex` on your `PATH`. Prebuilt targets:

| Platform | Asset |
| --- | --- |
| macOS (Apple Silicon) | `claudex-<tag>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `claudex-<tag>-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `claudex-<tag>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (arm64) | `claudex-<tag>-aarch64-unknown-linux-gnu.tar.gz` |
| Windows (x86_64) | `claudex-<tag>-x86_64-pc-windows-msvc.zip` |

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
claudex usage      # show plan usage limits
claudex --help     # list available commands
claudex --version  # print the version
```

If your token lives somewhere non-standard (or you just want to be explicit), set it directly:

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
