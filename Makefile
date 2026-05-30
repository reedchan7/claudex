.PHONY: help build release test fmt fmt-check lint check run install uninstall clean \
	setup-hooks version set-version bump-patch bump-minor bump-major

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
	@echo "  make setup-hooks Enable local git hooks (pre-commit fmt, pre-push check)"
	@echo "  make version              Print the current crate version"
	@echo "  make set-version VERSION=x.y.z   Set an explicit version, commit, and tag"
	@echo "  make bump-patch           Bump patch version, commit, and tag (x.y.Z+1)"
	@echo "  make bump-minor           Bump minor version, commit, and tag (x.Y+1.0)"
	@echo "  make bump-major           Bump major version, commit, and tag (X+1.0.0)"

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

# Point git at the version-controlled hooks in .githooks/ so every clone can
# enable them with one command. pre-commit auto-formats staged Rust; pre-push
# runs the full `make check` gate.
setup-hooks:
	@chmod +x .githooks/*
	@git config core.hooksPath .githooks
	@echo "Git hooks enabled (core.hooksPath -> .githooks)."

version:
	@grep -m1 '^version = ' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'

# Set the [package] version, sync Cargo.lock, commit, and tag.
# $(1)=patch|minor|major bumps the current version; $(1)=set uses VERSION=x.y.z.
# Requires a clean working tree. Does not push — run `git push --follow-tags` to release.
define bump
	@set -eu; \
	if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree not clean — commit or stash changes first"; exit 1; \
	fi; \
	cur=$$(grep -m1 '^version = ' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/'); \
	if [ "$(1)" = "set" ]; then \
		new="$(VERSION)"; \
		[ -n "$$new" ] || { echo "error: provide a version, e.g. make set-version VERSION=1.2.3"; exit 1; }; \
		echo "$$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$$' || { echo "error: VERSION must be semver x.y.z (got '$$new')"; exit 1; }; \
	else \
		major=$${cur%%.*}; rest=$${cur#*.}; minor=$${rest%%.*}; patch=$${rest##*.}; \
		case "$(1)" in \
			major) major=$$((major + 1)); minor=0; patch=0 ;; \
			minor) minor=$$((minor + 1)); patch=0 ;; \
			patch) patch=$$((patch + 1)) ;; \
		esac; \
		new="$$major.$$minor.$$patch"; \
	fi; \
	awk -v ver="$$new" '/^version = / && !d { print "version = \"" ver "\""; d=1; next } { print }' \
		Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	cargo update -w; \
	git add Cargo.toml Cargo.lock; \
	git commit -q -m "chore: release v$$new"; \
	git tag "v$$new"; \
	echo "Updated $$cur -> $$new, committed, and tagged v$$new"; \
	echo "Release it with: git push origin main --follow-tags"
endef

set-version:
	$(call bump,set)

bump-patch:
	$(call bump,patch)

bump-minor:
	$(call bump,minor)

bump-major:
	$(call bump,major)
