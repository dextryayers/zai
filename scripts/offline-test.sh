#!/usr/bin/env bash
# Offline proof: core commands must work with no model and no network.
set -euo pipefail
export AICLI_TEST_HOME="${AICLI_TEST_HOME:-/tmp/aicli-offline-test}"
rm -rf "$AICLI_TEST_HOME"
cargo run -q -p aicli -- doctor > /dev/null
cargo run -q -p aicli -- models list > /dev/null
cargo run -q -p aicli -- ask "hello" > /dev/null
cargo run -q -p aicli -- tasks add "offline check" > /dev/null
cargo run -q -p aicli -- daily --today > /dev/null
cargo run -q -p aicli -- index ./crates > /dev/null
echo "offline-test: ok"
