#!/usr/bin/env bash
# Zai AI installer. Builds from source and puts `zai` on PATH.
# Usage:
#   ./install.sh
#   ./install.sh --debug      fast local install without release optimize
#   ./install.sh --prefix DIR install binary into DIR
#   ./install.sh --help       show options
set -euo pipefail

MODE="release"
PREFIX="$HOME/.local/bin"
SYSTEM_DIR="/usr/local/bin"

usage() {
  cat <<'HELP'
Usage: ./install.sh [options]

Options:
  --debug       build debug profile, faster, larger binary
  --prefix DIR  install binary into DIR (default: ~/.local/bin)
  --help        show this help

After install, open a new shell or export PATH, then run: zai doctor
HELP
}

while [ $# -gt 0 ]; do
  case "$1" in
    --debug) MODE="debug"; shift ;;
    --prefix) PREFIX="${2:?missing DIR}"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust stable first: https://rustup.rs" >&2
  exit 2
fi

echo "zai: building ($MODE profile)..."
if [ "$MODE" = "release" ]; then
  cargo build --locked --release -p zai
  BIN="target/release/zai"
else
  cargo build -p zai
  BIN="target/debug/zai"
fi

mkdir -p "$PREFIX"
cp "$BIN" "$PREFIX/zai"
chmod +x "$PREFIX/zai"
echo "zai: installed to $PREFIX/zai"

# Offer system wide install when possible, never required.
if [ -w "$SYSTEM_DIR" ] && [ "$PREFIX" != "$SYSTEM_DIR" ]; then
  cp "$BIN" "$SYSTEM_DIR/zai" && chmod +x "$SYSTEM_DIR/zai" && echo "zai: also installed to $SYSTEM_DIR/zai"
elif command -v sudo >/dev/null 2>&1 && [ "$PREFIX" != "$SYSTEM_DIR" ]; then
  if sudo cp "$BIN" "$SYSTEM_DIR/zai" 2>/dev/null; then
    sudo chmod +x "$SYSTEM_DIR/zai"
    echo "zai: also installed to $SYSTEM_DIR/zai"
  fi
fi

case ":$PATH:" in
  *":$PREFIX:"*) PATH_OK=1 ;;
  *) PATH_OK=0 ;;
esac
if [ "$PATH_OK" -eq 0 ]; then
  echo "note: $PREFIX is not on PATH. Add this line to ~/.bashrc or ~/.zshrc:"
  echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
  echo "then open a new shell."
fi

echo "zai: verifying install..."
if command -v zai >/dev/null 2>&1; then
  zai doctor
else
  "$PREFIX/zai" doctor
fi
echo "done. Next: zai models pull qwen2.5-3b-instruct-q4_k_m"
