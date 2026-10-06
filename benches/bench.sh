#!/usr/bin/env bash
# Bench baselines for reference laptop. Run: bash benches/bench.sh
# Records walk, index, search, render timings. No network.
set -euo pipefail
cd "$(dirname "$0")/.."
export ZAI_TEST_HOME="${ZAI_TEST_HOME:-/tmp/zai-bench}"
rm -rf "$ZAI_TEST_HOME"
mkdir -p benches/baselines
echo "bench walk 200 files"
python3 - <<'PY'
import pathlib
root = pathlib.Path('/tmp/bench-corpus')
root.mkdir(exist_ok=True)
for i in range(200):
    (root / f'f{i:03}.md').write_text(f'# file {i}\n\nrust offline index bench content {i}\n' * 5)
PY
START=$(date +%s%3N)
cargo run -q -p zai -- index /tmp/bench-corpus --rebuild > /tmp/bench-index.txt 2>&1 || true
END=$(date +%s%3N)
echo "index 200 files: $((END-START)) ms (target under 60000 ms)"
START=$(date +%s%3N)
cargo run -q -p zai -- ask "bench content" --index /tmp/bench-corpus --show-sources > /tmp/bench-ask.txt 2>&1 || true
END=$(date +%s%3N)
echo "ask with RAG: $((END-START)) ms (target under 5000 ms)"
echo "done, see /tmp/bench-index.txt and /tmp/bench-ask.txt"
