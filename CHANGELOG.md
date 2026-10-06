# Changelog
All notable changes to this project will be documented in this file.
Format follows Keep a Changelog. Versions follow SemVer.

## [0.1.0-alpha] - 2026-10-06
### Added
- Version channel reset to alpha. Note: cargo needs semver, so V0.0.1.0 is not a valid version string and 0.1.0-alpha is used.
- Local brain: offline math evaluator with precedence plus functions, code reader with language detect plus risk flags, defensive security playbooks for 12 topics, intent router wired into ask, chat, and TUI
- Full terminal access by permission: shell modes deny plus ask plus allow, allow needs explicit user opt in with warning banner, every run logged
- Ollama support: daemon detect, list, pull with progress, rm, show, generate routing for ollama/ model ids in ask, chat, and TUI
- Manage page: TUI overlay with Ollama and GGUF tabs, install via input prefill, delete with inline confirm, refresh; GGUF delete only, never download
- CLI parity: zai ollama list plus pull plus rm plus show plus status, zai models insert for unlimited local GGUF, set-default writes config, doctor gains shell plus ollama plus brain self test rows
- REPL and TUI gain /manage /ollama /run, shared slash parser plus settings helpers, fixed Tokio panic on blocking HTTP by isolating calls on dedicated threads

## [1.1.0] - 2026-10-06
### Added
- Full screen TUI on bare `zai`: left sessions panel with Tab focus plus Enter to open, main chat with styled markdown, slash completion popup, help plus models plus settings plus effort overlays, insert progress gauge, toasts, boot splash, thinking spinner with elapsed time, smooth styled streaming reveal
- Slash core shared by TUI and REPL: /help /new /sessions /open /model /insert /setting /effort /budget /compact /export /sources /clear /plain /quit with typo hints and Tab completion
- GGUF insert: `/insert <file.gguf> [--name id] [--ctx n]` and `zai models insert`, header validate plus sha256 plus progress, unlimited saved models in custom-models.toml, `models list` shows cached plus local plus active markers
- Model switch: `/model` overlay picker plus `zai models set-default` now writes config, all commands resolve user models first
- Settings page: `/setting` overlay plus `zai config set` keys model effort temp top_p seed ctx threads gpu_layers theme, validated writes via Config::save
- Effort levels Default Low Medium High XHigh Expert with temp plus top_p plus token plus step mapping, in status bar, sampler, and agent step cap
- Fixed classic REPL double render garble, AICLI header, swapped temp and seed display

## [1.0.0] - 2026-10-06
### Added
- Phase 6 RAG: chunk 512 plus 64 overlap with line anchors, hash embed dim 128 offline, cache sqlite FTS5 plus vectors, meta rag-v1 plus config hash with rebuild guard, incremental by mtime, fusion 0.5 plus 0.5 with dedupe, citations path plus lines plus scores, eval 10 of 10 pass
- Phase 6 CLI: index build with animated counts, index status, index eval table, ask with --index plus --top-k plus --show-sources plus --no-rag, JSON sources with bm25 plus vector
- Phase 7 hardening: bench script plus baselines, release with checksums plus gates, install script, security review with offline proof, docs plus benchmark table plus roadmap
- v1 scope: mock inference with real GGUF validate, honest known limits for llama stream and ONNX upgrade

## [0.4.0] - 2026-10-06
### Added
- Phase 4 agent: fs.read 200 lines plus 64 KB cap, fs.list 500 sorted, sandbox deny .. plus secret redact, git status plus diff 200 KB cap, events JSONL plus sqlite log
- Phase 4 patch: parse plus validate plus atomic apply with backup .aicli.bak.ts, temp plus rename, hunk context check, word diff preview, patch show plus apply --yes plus drop
- Phase 4 shell: allowlist prefix plus denylist wins, curl pipe to sh denied, 60s timeout, 4000 preview plus full run.log, approval prompt with 3s countdown, ZAI_AUTO_YES for automation
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
