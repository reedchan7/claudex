# claudex

A small CLI that extends the [Claude Code](https://claude.com/claude-code) CLI with extra commands.

The first command, `claudex usage`, shows your current Claude plan usage limits right from the terminal — the same data as the in-session `/usage` view, but available as a standalone one-shot command.

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

`claudex usage` reuses the credentials Claude Code already stores on your machine:

1. Reads the OAuth access token from the macOS Keychain entry `Claude Code-credentials`.
2. Detects your installed `claude` CLI version to send a matching `User-Agent`.
3. Calls `GET https://api.anthropic.com/api/oauth/usage` and renders the response.

No extra login or API key is required — if you can run `claude`, you can run `claudex usage`.

## Requirements

- **macOS** — reads the token via the `security` command from the Keychain.
- **An authenticated Claude Code install** with an active Claude subscription (Pro / Max / Team).
- **Rust toolchain** (edition 2024, so Rust 1.85+) to build from source.

## Install

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
