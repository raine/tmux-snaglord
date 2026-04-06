# Rust project checks

set positional-arguments
set shell := ["bash", "-euo", "pipefail", "-c"]

# List available commands
default:
    @just --list

# Run all checks
[parallel]
check: format clippy build test

# Format Rust files
format:
    @cargo fmt --all

# Auto-fix clippy warnings, then fail on any remaining
clippy:
    @cargo clippy --fix --allow-dirty --quiet -- -D clippy::all 2>&1 | { grep -v "^0 errors" || true; }

# Build the project
build:
    cargo build --all

# Run tests
test:
    #!/usr/bin/env bash
    set -euo pipefail
    output=$(cargo test --quiet 2>&1) || { echo "$output"; exit 1; }
    echo "$output" | tail -1

# Install release binary globally
install:
    cargo install --offline --path . --locked

# Install debug binary globally via symlink
install-dev:
    cargo build && ln -sf $(pwd)/target/debug/tmux-snaglord ~/.cargo/bin/tmux-snaglord

# Run the application
run *ARGS:
    cargo run -- "$@"

# Release a new patch version
release-patch:
    @just _release patch

# Release a new minor version
release-minor:
    @just _release minor

# Release a new major version
release-major:
    @just _release major

# Internal release helper
_release bump:
    @cargo-release {{bump}}
