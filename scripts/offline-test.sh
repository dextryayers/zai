#!/usr/bin/env bash
# Offline proof: core commands must work with no model and no network.
set -euo pipefail
export ZAI_TEST_HOME="${ZAI_TEST_HOME:-/tmp/zai-offline-test}"
rm -rf "$ZAI_TEST_HOME"
cargo run -q -p zai -- doctor > /dev/null
cargo run -q -p zai -- models list > /dev/null
cargo run -q -p zai -- ask "hello" > /dev/null
cargo run -q -p zai -- tasks add "offline check" > /dev/null
cargo run -q -p zai -- daily --today > /dev/null
cargo run -q -p zai -- index ./crates > /dev/null
echo "offline-test: ok"
