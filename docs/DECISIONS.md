# Decisions

- ADR-001: Rust only runtime. Keeps single binary and offline guarantee.
- ADR-002: GGUF via llama-cpp-2 in Phase 2. Mock stream in Phase 1 behind same function shape.
- ADR-003: fastembed with BM25 fallback for RAG. Phase 1 ships walker plus lexical score only.
- ADR-004: tantivy default with VectorStore abstraction. Measure size at M6.
- ADR-005: rustyline for REPL in Phase 1. Evaluate reedline palette upgrade after M1 demo.
- ADR-006: sqlite WAL plus JSONL mirror in Phase 3. Phase 1 uses JSON file with same API.
- ADR-007: ASCII hyphen only. Enforced by scripts/check-no-emdash.sh.
