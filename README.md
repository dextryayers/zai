# Zai AI - Offline Coding and Daily Assistant

Rust only CLI, single binary, local GGUF runtime, safe coding agent, daily capture, local RAG with citations.

Status: v1.0.0. Phases 0 to 7 complete. Mock inference stream with real GGUF validate, budget, sessions, agent, daily, RAG. Real llama.cpp stream behind `llama` feature next.

## Install

```bash
./install.sh
# fast local install without release optimize:
./install.sh --debug
# then in a new shell:
zai doctor
```

## Quickstart

```bash
zai doctor
zai models list
zai index ./docs --rebuild
zai ask "where is thread pool built" --index ./docs --show-sources
zai code "fix failing test" --path ./crates/tools
zai tasks add "write tests"
zai daily --today
zai chat
```

Flags: `--profile`, `--plain`, `--json`, `--quiet`, `--offline`, `--verbose`.

## Commands

```text
zai chat                        REPL with /help /code /note /memory /daily
zai ask "<q>" --show-sources    cited answer, --no-rag to skip retrieval
zai code "<goal>" --apply       12 step loop, atomic patch, allowlisted verify
zai patch show|apply|drop       inspect pending diffs
zai run -- <cmd>                allowlisted shell with approval
zai index <path> --rebuild      build local RAG, --status, --eval
zai models list|pull|verify     GGUF cache with resume plus sha256
zai tasks|notes|daily|memory    local capture plus week report
zai sessions|config|doctor      sessions, config, diagnostics
```

## Models

| Use | Default | Fallback |
|-----|---------|----------|
| chat and code | qwen2.5-3b-instruct-q4_k_m, ctx 4096 | llama-3.2-1b-instruct-q4_k_m, ctx 2048 |
| embeddings | hash dim 128 local, no download | fastembed ONNX optional upgrade |

## Benchmarks

See `benches/baselines/README.md`. Targets: startup under 300 ms, index 200 files under 60 s, ask under 5 s warm, notes search under 200 ms, rag eval 8 of 10 in top 3. Current eval 10 of 10 pass.

## Offline

Shell, agent dry run, daily, index, ask with cached index need no network. Only `models pull` uses network. Verify with `bash scripts/offline-test.sh`.

## Quality

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check-no-emdash.sh
bash scripts/offline-test.sh
cargo run -q -p zai -- index --eval
```

## Known limits

- Inference stream is deterministic mock with GGUF header validate, not yet llama.cpp tokens. Backend interface ready for `llama` feature.
- RAG uses FTS5 plus hash vectors, not ONNX embeddings. Good for code and docs, weaker for semantic paraphrase.
- No Windows CI in v1, Linux plus macOS only.

## Roadmap

- v1.1: real llama-cpp-2 stream behind feature, tokens per second bench
- v1.2: fastembed optional, tantivy eval, Windows build
- v2: plugin tools, team sync

See `plan.md` and `docs/` for full spec.
