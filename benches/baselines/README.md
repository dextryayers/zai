# Baselines

Reference target, not a promise. Measured on 4 core laptop with SSD.

| Bench | Target | Last |
|-------|--------|------|
| index 200 files | under 60 s | 4 ms for 3 files, scales linear, see bench.sh |
| ask with RAG top 5 | under 5 s cold | under 1 s warm cache |
| notes search 200 rows | under 200 ms p95 | unit test asserts under 200 ms |
| markdown fixture | under 50 ms each | unit golden, no panic on malformed fences |
| startup no model | under 300 ms | `time zai --help`, shell only |

Run `bash benches/bench.sh` for fresh numbers. Store new baselines here per release.
