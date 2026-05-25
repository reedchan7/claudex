.PHONY: help build release test fmt fmt-check lint check run install uninstall clean \
	version bump-patch bump-minor bump-major

# Default target: list available commands
help:
	@echo "claudex — available make targets:"
	@echo "  make build       Build debug binary"
	@echo "  make release     Build optimized release binary"
	@echo "  make test        Run the test suite"
	@echo "  make fmt         Format code with rustfmt"
	@echo "  make fmt-check   Check formatting without modifying files"
	@echo "  make lint        Run clippy (warnings as errors)"
	@echo "  make check       fmt-check + lint + test (CI gate)"
	@echo "  make run         Run 'claudex usage'"
	@echo "  make install     Install claudex to ~/.cargo/bin"
	@echo "  make uninstall   Remove the installed claudex binary"
	@echo "  make clean       Remove build artifacts"
	@echo "  make version     Print the current crate version"
	@echo "  make bump-patch  Bump patch version, commit, and tag (x.y.Z+1)"
	@echo "  make bump-minor  Bump minor version, commit, and tag (x.Y+1.0)"
	@echo "  make bump-major  Bump major version, commit, and tag (X+1.0.0)"

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

lint:
	cargo clippy --all-targets -- -D warnings

check: fmt-check lint test

run:
	cargo run -- usage

install:
	cargo install --path .

uninstall:
	cargo uninstall claudex

clean:
	cargo clean

version:
	@grep -m1 '^version = ' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'

# Bump the [package] version, sync Cargo.lock, commit, and tag. $(1)=patch|minor|major.
# Requires a clean working tree. Does not push — run `git push --follow-tags` to release.
define bump
	@set -eu; \
	if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree not clean — commit or stash changes first"; exit 1; \
	fi; \
	cur=$$(grep -m1 '^version = ' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'); \
	major=$${cur%%.*}; rest=$${cur#*.}; minor=$${rest%%.*}; patch=$${rest##*.}; \
	case "$(1)" in \
		major) major=$$((major + 1)); minor=0; patch=0 ;; \
		minor) minor=$$((minor + 1)); patch=0 ;; \
		patch) patch=$$((patch + 1)) ;; \
	esac; \
	new="$$major.$$minor.$$patch"; \
	awk -v ver="$$new" '/^version = / && !d { print "version = \"" ver "\""; d=1; next } { print }' \
		Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	cargo update -w; \
	git add Cargo.toml Cargo.lock; \
	git commit -q -m "chore: release v$$new"; \
	git tag "v$$new"; \
	echo "Bumped $$cur -> $$new, committed, and tagged v$$new"; \
	echo "Release it with: git push origin main --follow-tags"
endef

bump-patch:
	$(call bump,patch)

bump-minor:
	$(call bump,minor)

bump-major:
	$(call bump,major)
