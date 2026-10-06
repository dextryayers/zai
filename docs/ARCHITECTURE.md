# Architecture

## Crate map

```text
cli -> ui, core, models, infer, tools, rag, ingest
rag -> ingest
tools -> ingest
infer -> core, models
core standalone
```

No cycles. `cargo doc` warning free.

## Flows

Chat: REPL ensures sqlite session, builds prompt budget, streams mock tokens with caret, persists user plus assistant turns plus JSONL mirror. Real llama backend plugs into `infer::stream` callback shape.

Ask with RAG: resolve index root from `--index` or config, `rag::retrieve` BM25 plus hash vectors fusion, inject top chunks into prompt, render answer plus sources panel with scores. `--no-rag` skips retrieval. Overflow exits 4.

Code agent: search scope, read top hits in sandbox, draft grounded diff, validate hunks, save `pNNNN.diff`, prompt approval, atomic apply with backup, verify allowlisted command. Max 12 steps, dry run default.

Index: walk with gitignore, chunk 512 plus 64 overlap, hash embed dim 128, store in cache sqlite FTS5 plus vectors JSON, meta with version plus config hash. Incremental by mtime plus size. Stale hash forces `--rebuild`.

## Storage

Profile `~/.local/share/zai/profiles/default/db.sqlite` WAL: sessions, turns, tasks, notes, events, notes_fts.
Mirrors: `sessions/<id>.jsonl`, `patches/<id>.diff`, `memory.md`, `logs/`.
Cache `~/.cache/zai/models/*.gguf`, `~/.cache/zai/index/<slug>/index.sqlite` plus `meta.json`.
Config `~/.config/zai/config.toml` with env plus flag precedence.

## Decision log

See `docs/DECISIONS.md` and `plan.md` Section 25. RAG uses sqlite FTS5 plus hash vectors, no ONNX download, single binary under 35 MB target.
