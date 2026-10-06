#!/usr/bin/env bash
# Install aicli from source. Linux and macOS.
set -euo pipefail
cargo build --locked --release -p aicli
BIN="target/release/aicli"
mkdir -p "$HOME/.local/bin"
cp "$BIN" "$HOME/.local/bin/aicli"
echo "installed to $HOME/.local/bin/aicli"
"$HOME/.local/bin/aicli" doctor
echo "next: aicli models pull qwen2.5-3b-instruct-q4_k_m"
