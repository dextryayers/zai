# AICLI - Offline Coding and Daily Assistant

Rust only CLI with local GGUF runtime, durable chat sessions, coding agent preview, daily tasks, and local file index.

Status: Phase 0 to Phase 3 complete. Offline runtime with budget plus sampler plus sqlite sessions. Real llama.cpp stream behind `llama` feature next.

## Quickstart

```bash
cargo run -q -p aicli -- doctor
cargo run -q -p aicli -- models list
cargo run -q -p aicli -- ask "hello" --show-sources --show-budget
cargo run -q -p aicli -- ask "hello" --session s_demo1
cargo run -q -p aicli -- --json sessions list
cargo run -q -p aicli -- tasks add "write tests"
cargo run -q -p aicli -- daily --today
cargo run -q -p aicli -- chat
```

## Commands

```text
aicli chat            interactive REPL with /help
aicli ask "<q>"       one shot answer, --show-sources for citations
aicli code "<goal>"   dry run plan plus diff preview
aicli index <path>    scan folder for RAG
aicli models list     show GGUF registry
aicli tasks add/list  local tasks
aicli daily --today   notes plus tasks overview
aicli doctor          paths, model, ctx, checks
```

Flags: `--profile`, `--plain`, `--json`, `--quiet`, `--offline`, `--verbose`.

## Offline

Phase 1 shell needs no model. Phase 2 `models pull` caches GGUF under `~/.cache/aicli/models`.

## Docs

See `plan.md` for full spec and `docs/` for guides.

## Quality

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check-no-emdash.sh
```
# zai
# zai
