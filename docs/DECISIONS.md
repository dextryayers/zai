# Decisions

- ADR-001: Rust only runtime. Keeps single binary and offline guarantee.
- ADR-002: GGUF via header validate plus mock stream in v1. Real llama-cpp-2 behind `llama` feature next, same stream callback shape.
- ADR-003: RAG with sqlite FTS5 plus hash embeddings dim 128. Rejected fastembed ONNX download for v1 to keep single binary and zero fetch after install. fastembed remains optional upgrade.
- ADR-004: Index store in cache sqlite plus meta.json with version rag-v1 plus config hash. Rejected tantivy for v1 to hold binary under 35 MB and cut build time. VectorStore abstraction kept via retrieve function boundary.
- ADR-005: rustyline for REPL. Reedline palette deferred, slash completion plus history already meet M1 demo.
- ADR-006: sqlite WAL plus JSONL mirror. Legacy store.json imported once.
- ADR-007: ASCII hyphen only. Enforced by scripts/check-no-emdash.sh.
- ADR-008: Agent patch atomic with temp plus rename and backup. No silent writes, dry run default.
- ADR-009: Shell allowlist prefix plus denylist wins, pipe from curl to sh always denied, 60s timeout.
- ADR-010: Week report counts only, no model narrative. Avoids invented insights.
