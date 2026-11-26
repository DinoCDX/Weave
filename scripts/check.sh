#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/.."

echo "==> format"
cargo fmt --all -- --check

echo "==> lint"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "==> build"
cargo build --workspace --locked

echo "==> test"
cargo test --workspace --locked

echo "all checks passed"
