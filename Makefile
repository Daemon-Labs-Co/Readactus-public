SHELL := /bin/bash

# Activate mise toolchain if available.
MISE := $(shell command -v mise 2>/dev/null)
ifdef MISE
  ACTIVATE := eval "$$(mise activate bash)" &&
endif

.PHONY: build build-release check test test-all fmt clippy clean gui cli

# --- Build ---

build:
	$(ACTIVATE) cargo build

build-release:
	$(ACTIVATE) cargo build --release

gui:
	$(ACTIVATE) cargo build -p readactus-gui

cli:
	$(ACTIVATE) cargo build -p readactus-cli

# --- Quality ---

check:
	$(ACTIVATE) cargo check --workspace

fmt:
	$(ACTIVATE) cargo fmt --all

fmt-check:
	$(ACTIVATE) cargo fmt --all -- --check

clippy:
	$(ACTIVATE) cargo clippy --workspace -- -D warnings

# --- Test ---

test:
	$(ACTIVATE) cargo test --workspace

test-all:
	$(ACTIVATE) cargo test --workspace -- --include-ignored

# --- Run ---

run-gui:
	$(ACTIVATE) cargo run -p readactus-gui

run-cli:
	$(ACTIVATE) cargo run -p readactus-cli -- $(ARGS)

# --- Clean ---

clean:
	$(ACTIVATE) cargo clean
