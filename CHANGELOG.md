# Changelog
All notable changes to this project will be documented in this file.
Format follows Keep a Changelog. Versions follow SemVer.

## [0.4.0] - 2026-10-06
### Added
- Phase 4 agent: fs.read 200 lines plus 64 KB cap, fs.list 500 sorted, sandbox deny .. plus secret redact, git status plus diff 200 KB cap, events JSONL plus sqlite log
- Phase 4 patch: parse plus validate plus atomic apply with backup .aicli.bak.ts, temp plus rename, hunk context check, word diff preview, patch show plus apply --yes plus drop
- Phase 4 shell: allowlist prefix plus denylist wins, curl pipe to sh denied, 60s timeout, 4000 preview plus full run.log, approval prompt with 3s countdown, AICLI_AUTO_YES for automation
- Phase 4 loop: 12 step cap, plan checklist ok plus pending, grounded diff from search hits, patch file pNNNN.diff, apply prompt a plus r plus t, verify fmt when Rust touched
- Phase 5 daily: tasks carry yesterday to today, tasks clear done with --yes, week table open plus done plus notes plus load bar, top 5 terms counts only
- Phase 5 memory: memory.md user owned, memory show plus add plus promote with preview, REPL /note plus /memory plus /daily plus /code shortcuts
- UI: agent spinners, approval panels, week load bars, keyword highlight in notes search, 80x24 safe tables

## [0.3.0] - 2026-10-06
### Added
- Phase 2 model runtime: HF URL resolve, resume download with Range, indicatif bar, sha256 verify, GGUF header validate with version plus tensor count, OOM hint, backend info in ask and doctor
- Phase 2 prompt budget: estimator chars per 4, sys plus rag plus history caps, overflow exit 4, --show-budget, ctx pressure warning at 85 pct
- Phase 2 sampler: chat plus code plus deterministic configs, temp plus top-p plus seed flags, byte identical repro test
- Phase 3 sessions in sqlite WAL: create plus list plus open plus rename plus delete plus export md and json, JSONL mirror, kill recovery with stopped status, resume last
- Phase 3 compact: extractive keep first plus last 6, /ctx compact and /budget and /export in REPL, sources rail toggle
- Phase 3 tasks and notes on sqlite with FTS fallback and legacy store.json import, 200 notes search under 200 ms test
- CLI: config set for model.default plus model.n_ctx plus ui.theme with validation, models verify plus remove with real paths, doctor bench plus disk use plus cache status
- UI: ctx meter color change, overflow panel, elegant pull and bench spinners, truncated long query display

## [0.2.0] - 2026-10-06
### Added
- Workspace with 8 crates: cli, ui, core, models, infer, tools, ingest, rag
- Premium CLI shell: status line, panels, tables, markdown render, spinner and paced stream
- REPL with history, slash commands, fuzzy hint, Ctrl+C stop handling
- Tasks and notes file store, daily today view, JSON plus plain plus quiet contracts
- Walker with gitignore, ext filter, skip dirs, golden fixtures
- Shell allowlist plus denylist gate with exit 5
- Patch validate, search, models registry with simulated pull progress
- CI with fmt, clippy deny warnings, tests, dash lint

## [0.1.0] - 2026-10-06
### Added
- Plan v2 with phased WBS and UX spec
- Clean workspace baseline
