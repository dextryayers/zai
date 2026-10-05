#!/usr/bin/env bash
# Fail if em dash or en dash appears in tracked text files.
# ASCII hyphen only, per plan.md doc rule.
set -euo pipefail
bad=0
while IFS= read -r -d '' f; do
  if grep -q $'—\|–' "$f"; then
    echo "dash violation: $f"
    bad=1
  fi
done < <(git ls-files -z '*.rs' '*.md' '*.toml' '*.sql' 2>/dev/null || find . -name '*.rs' -print0)
if [ "$bad" -eq 1 ]; then
  echo "found em/en dash, use ASCII hyphen - instead"
  exit 1
fi
echo "dash lint: clean"
