#!/bin/sh
# claudex installer — downloads the latest prebuilt binary for your platform.
#
#   curl -fsSL https://raw.githubusercontent.com/reedchan7/claudex/main/install.sh | sh
#
# No Rust toolchain required. Supports macOS and Linux (x86_64 / arm64).
# Override the install directory with CLAUDEX_INSTALL_DIR (default: ~/.local/bin).

set -eu

REPO="reedchan7/claudex"
BIN="claudex"
INSTALL_DIR="${CLAUDEX_INSTALL_DIR:-$HOME/.local/bin}"

err() {
	echo "error: $*" >&2
	exit 1
}

# Ensure $1 is on PATH. If it isn't, append an export to the user's shell
# profile (idempotent, shell-aware). A subprocess can't change the parent
# shell, so we also tell the user how to activate it now. Set
# CLAUDEX_NO_MODIFY_PATH=1 to manage PATH yourself.
ensure_on_path() {
	dir="$1"

	case ":${PATH}:" in
	*":${dir}:"*)
		echo "Run: ${BIN} usage"
		return 0
		;;
	esac

	if [ -n "${CLAUDEX_NO_MODIFY_PATH:-}" ]; then
		echo "Note: ${dir} is not on your PATH — add it, then run: ${BIN} usage"
		return 0
	fi

	shell_name="$(basename "${SHELL:-sh}")"
	case "$shell_name" in
	zsh)
		profile="${ZDOTDIR:-$HOME}/.zshrc"
		line="export PATH=\"${dir}:\$PATH\""
		;;
	bash)
		# macOS login shells read .bash_profile; Linux reads .bashrc.
		if [ "$(uname -s)" = "Darwin" ]; then
			profile="$HOME/.bash_profile"
		else
			profile="$HOME/.bashrc"
		fi
		line="export PATH=\"${dir}:\$PATH\""
		;;
	fish)
		profile="$HOME/.config/fish/config.fish"
		line="fish_add_path \"${dir}\""
		;;
	*)
		profile="$HOME/.profile"
		line="export PATH=\"${dir}:\$PATH\""
		;;
	esac

	if [ -f "$profile" ] && grep -qF "$dir" "$profile" 2>/dev/null; then
		echo "${dir} is configured in ${profile} but not active in this shell."
		echo "Restart your shell or run: . \"${profile}\""
		return 0
	fi

	mkdir -p "$(dirname "$profile")"
	printf '\n# Added by the claudex installer\n%s\n' "$line" >>"$profile"
	echo "Added ${dir} to PATH in ${profile}"
	echo "Restart your shell (or run: . \"${profile}\"), then: ${BIN} usage"
}

# Pick a downloader.
if command -v curl >/dev/null 2>&1; then
	dl() { curl -fsSL "$1" -o "$2"; }
	fetch() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
	dl() { wget -qO "$2" "$1"; }
	fetch() { wget -qO- "$1"; }
else
	err "need curl or wget installed"
fi

# Detect OS.
os="$(uname -s)"
case "$os" in
Darwin) os_part="darwin" ;;
Linux) os_part="linux" ;;
*) err "unsupported OS: $os (use 'cargo install' or download from the releases page)" ;;
esac

# Detect architecture.
arch="$(uname -m)"
case "$arch" in
x86_64 | amd64) arch_part="amd64" ;;
arm64 | aarch64) arch_part="arm64" ;;
*) err "unsupported architecture: $arch" ;;
esac

platform="${os_part}-${arch_part}"

# Resolve the latest release tag via the GitHub API.
echo "Resolving latest release..."
tag="$(fetch "https://api.github.com/repos/${REPO}/releases/latest" |
	grep '"tag_name"' | head -1 | cut -d'"' -f4)"
[ -n "$tag" ] || err "could not determine the latest release tag"

asset="${BIN}-${tag}-${platform}.tar.gz"
url="https://github.com/${REPO}/releases/download/${tag}/${asset}"

# Download and extract into a temp dir.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading ${asset} (${tag})..."
dl "$url" "$tmp/$asset" || err "download failed: $url"
tar -xzf "$tmp/$asset" -C "$tmp" || err "failed to extract archive"

bin_path="$(find "$tmp" -type f -name "$BIN" | head -1)"
[ -n "$bin_path" ] || err "binary '$BIN' not found in archive"

# Install.
mkdir -p "$INSTALL_DIR"
mv "$bin_path" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

echo "Installed $BIN $tag to $INSTALL_DIR/$BIN"

ensure_on_path "$INSTALL_DIR"
