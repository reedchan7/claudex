.PHONY: help build release test fmt fmt-check lint check run install uninstall clean

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
