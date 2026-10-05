# Architecture

## Crate map

```text
cli -> ui, core, models, infer, tools, rag, ingest
rag -> ingest, core
tools -> ingest
infer -> core, models
```

## Flows

Chat mock in Phase 1: `repl::run_chat` calls `infer::mock_answer` and paces output.
Real GGUF in Phase 2: replace `mock_answer` with `llama-cpp-2` loader plus token channel.

Code preview: `tools::search_files` plus `tools::validate_patch`, no writes in Phase 1.

## Storage

Phase 1: JSON file `store.json` under profile data dir.
Phase 3: migrate to sqlite per `migrations/001_init.sql` with same API shape.

## Decisions

See `docs/DECISIONS.md` and `plan.md` Section 25.
