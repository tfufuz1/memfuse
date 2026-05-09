#!/usr/bin/env bash
set -e

echo "Running Validation Loop..."
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo check --all-targets --workspace
cargo nextest run --workspace || cargo test --workspace

echo "Validation passed. System is safe."

# Proaktive Jules-Integration
bash /home/freddy/Arbeitsplatz/DEV/memfuse/.agent/scripts/jules-integrate.sh
