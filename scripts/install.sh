#!/usr/bin/env bash
# Wrapper kept for old docs. Use ./install.sh from repo root.
set -euo pipefail
exec "$(dirname "$0")/../install.sh" "$@"
