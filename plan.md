# AICLI: Offline Coding and Daily Assistant
## Product Requirements, Architecture, UX Spec, and Execution Plan
## Stack: Rust only + GGUF, local first

## Metadata

* Product code: AICLI
* Version of this plan: 2.0.0
* Date: 2026-10-06
* Owner: Solo developer portfolio build
* Runtime language: Rust only for shipped artifacts
* Inference format: GGUF, offline by default
* Working directory for v1 build: clean workspace, no legacy code carried over
* Doc rule: ASCII hyphen `-` only for dashes. No em dash in docs, code, or CLI strings.

## Revision History

| Version | Date       | Change                                           | Author |
|---------|------------|--------------------------------------------------|--------|
| 1.0.0   | 2026-10-06 | Initial phased plan                              | Owner  |
| 2.0.0   | 2026-10-06 | Full rewrite, workspace cleared, deeper WBS, UX tokens, risk register, eval gates | Owner |

## Table of Contents

1. Executive Summary
2. Objectives and Success Metrics
3. Scope
4. Stakeholders and Responsibility
5. Constraints, Assumptions, Dependencies
6. Functional Requirements
7. Non Functional Requirements
8. Architecture
9. Data Design
10. Configuration Design
11. Inference Engineering
12. Coding Agent Design
13. Daily Assistant Design
14. Local RAG Design
15. Premium CLI UX Design System
16. Command Reference
17. Workspace and Repository Design
18. Execution Phases and WBS
19. Schedule and Milestones
20. Quality Plan
21. Security and Privacy Plan
22. DevOps and Release Plan
23. Documentation Plan
24. Risk Register
25. Decision Log
26. Glossary
27. Appendices

---

## 1. Executive Summary

AICLI is a single binary CLI for developers who need chat, coding help, and daily capture without network access at runtime.

Problem: hosted assistants fail on planes, restricted networks, and private repos. Python based local assistants fail on install size, startup time, and environment drift.

Solution: one Rust binary plus one or two GGUF files in local cache. Chat streams locally. Coding agent edits only after visible diff approval. Daily notes and tasks persist in local sqlite. Local folders are indexed with local embeddings for cited answers.

Portfolio intent: demonstrate systems engineering, inference integration, retrieval, CLI product design, testing discipline, and release engineering in one coherent repo.

## 2. Objectives and Success Metrics

### 2.1 Objectives

* O1: Ship installable Rust binary for Linux x86_64 and macOS ARM64.
* O2: Provide offline chat with session resume and context meter.
* O3: Provide coding agent limited to scoped reads, explicit patch approval, and allowlisted shell commands.
* O4: Provide daily notes, tasks, and week review in under 200 ms for 1000 records.
* O5: Provide local RAG with path and line citations.
* O6: Provide model management with resume, checksum, and quant guidance.
* O7: Meet premium CLI UX bar in 80x24 fallback and in modern terminals.

### 2.2 Success Metrics

| ID  | Metric | Target | Measurement |
|-----|--------|--------|-------------|
| M1 | Cold start to prompt without model | under 300 ms | `hyperfine` median of 20 runs |
| M2 | Model load 3B Q4_K_M from SSD with progress | under 15 s | `doctor --bench-load` |
| M3 | CPU tokens per second, 3B Q4_K_M, ctx 4096 | at least 8 tok/s | `doctor --bench-gen 256` |
| M4 | Index 200 mixed files | under 60 s, incremental 5 files under 5 s | criterion bench plus e2e timer |
| M5 | RAG regression | at least 8 of 10 fixtures in top 3 | `cargo test rag_regression` |
| M6 | Search 1000 notes | under 200 ms p95 | bench |
| M7 | Test coverage on core, tools, rag, models | at least 80 pct line | `cargo tarpaulin` or `llvm-cov` |
| M8 | Zero network calls in offline commands | 0 egress | proxy counter test |
| M9 | Fresh install to first offline answer | under 10 min by new user script | manual acceptance script |
| M10 | CI green | fmt plus clippy deny warnings plus tests | GitHub Actions log |

Release 1.0.0 requires all ten metrics met on reference machine: 4 core CPU, 16 GB RAM, SSD, no discrete GPU.

## 3. Scope

### 3.1 In Scope for v1

* Interactive chat with streaming and sessions
* One shot `ask` with optional RAG
* Coding agent with read, search, patch preview, apply, allowlisted shell
* Daily notes and tasks with date scope and search
* Local index over explicit folders with rebuild and incremental update
* Model registry, download with resume, verify, set default
* `doctor` diagnostics, `config` management, `sessions` export
* Human output, plain output, JSON output, quiet output
* Linux and macOS binaries, docs, CI

### 3.2 Out of Scope for v1

* No hosted sync, no accounts, no telemetry
* No background daemon or scheduler
* No plugin API
* No training, fine tuning, or GGUF quantization in CLI. Download only.
* No image, audio, or video input
* No multi agent swarm. One agent loop with max 12 steps.
* No Windows support in v1. Accept patches but do not block release on Windows.

## 4. Stakeholders and Responsibility

Single owner acts in four roles. RACI uses R responsible, A accountable, C consulted, I informed.

| Role | Responsibility | RACI for release |
|------|----------------|------------------|
| Product owner | Freeze scope, accept milestones | A for scope |
| Architect | Crate boundaries, storage schema, inference API | R for arch |
| UX owner | Tokens, screens, keymap, golden snapshots | R for UX |
| QA and release | Tests, benches, CI, binaries, docs | R for quality |

No external stakeholder approval is required. Portfolio reviewers are treated as readers, not approvers.

## 5. Constraints, Assumptions, Dependencies

### 5.1 Constraints

* CC1: Shipped runtime is Rust only. Build time may use shell and cargo only.
* CC2: No network at runtime except `models pull` and explicit update check.
* CC3: GGUF only. No SafeTensors runtime, no Python weights at runtime.
* CC4: Local data under user profile dirs. No root writes. No system service install.
* CC5: Terminal baseline is 80 columns by 24 rows, `NO_COLOR` supported, `TERM=dumb` supported.
* CC6: No em dash in repo. CI includes a lint script for it.

### 5.2 Assumptions

* AS1: User can download a 2 to 4 GB GGUF file once with network.
* AS2: Reference laptop has at least 8 GB RAM. 1B fallback model covers low end.
* AS3: Repo under analysis is a normal source tree under 50k files. Monorepo scale above that is best effort.
* AS4: `llama.cpp` upstream API available through `llama-cpp-2` crate. If upstream breaks, pin to last good commit.

### 5.3 Dependencies

* External code: `llama.cpp` via crate, ONNX runtime via `fastembed`, sqlite via `rusqlite`, tantivy index.
* External data, one time: chat GGUF from Hugging Face, embedding model files cached by `fastembed`.
* Build: stable Rust 1.78 or later, `cargo-dist` for releases, GitHub Actions Ubuntu and macOS runners.

## 6. Functional Requirements

Format: REQ-ID, statement, acceptance pointer.

* REQ-CHAT-01: User can start chat, send messages, see streamed markdown, stop with Ctrl+C, and resume later. Pointer: Phase 3 acceptance.
* REQ-CHAT-02: User can set model, temperature, context size per session. Pointer: `config` plus `--temp`, `--ctx`.
* REQ-CHAT-03: System must show context usage `used/total` and warn at 85 pct. Pointer: Section 11.
* REQ-CODE-01: Agent can list, read, and search files inside scoped root only. Pointer: Section 12.
* REQ-CODE-02: Agent must show unified diff before any write. Apply requires explicit approval. Pointer: Phase 4.
* REQ-CODE-03: Shell execution requires allowlist match and per session confirm unless `--allow-shell` with still visible command. Pointer: Section 12.
* REQ-DAILY-01: User can add, list, complete tasks by date. Pointer: Section 13.
* REQ-DAILY-02: User can add, list, search notes with date filter. Pointer: Section 13.
* REQ-DAILY-03: User can view today and week summaries in human and JSON form. Pointer: Section 16.
* REQ-RAG-01: User can index a folder, rebuild, and query with citations. Pointer: Section 14.
* REQ-RAG-02: Index stores config hash and version and refuses stale reads with clear rebuild instruction. Pointer: Section 14.
* REQ-MODEL-01: User can list, pull with resume, verify checksum, remove, set default. Pointer: Section 11.
* REQ-SESS-01: User can list, open, rename, delete, export sessions as markdown or JSON. Pointer: Phase 3.
* REQ-DOC-01: `doctor` reports model presence, index health, disk use, backend flags, and missing fix commands. Pointer: Section 16.
* REQ-OUT-01: Every command supports human, `--plain`, `--json`, `--quiet` contracts. Pointer: Section 15.8.

## 7. Non Functional Requirements

* NFR-PERF-01: Budgets in Section 2.2 are hard gates for 1.0.0.
* NFR-OFF-01: Offline commands pass zero egress test. See Section 21.
* NFR-REL-01: Partial stream on Ctrl+C or kill must persist as `stopped`, never corrupt DB. Crash recovery is automatic migrate plus WAL checkpoint.
* NFR-SEC-01: Path sandbox deny outside root, secret file redaction, shell denylist. See Section 21.
* NFR-UX-01: All interactive screens usable at 80x24. No truncation of primary action. Secondary panels collapse to toggles.
* NFR-MAINT-01: Workspace crates with public API docs on lib targets, no cyclic deps. `cargo doc` warning free.
* NFR-PORT-01: Linux x86_64 glibc and macOS ARM64 primary. Static musl best effort.
* NFR-SIZE-01: Stripped Linux CPU binary under 35 MB excluding models and embedding cache.

## 8. Architecture

### 8.1 Crate Map

```text
crates/cli      binary, clap tree, REPL entry, exit codes
crates/ui       theme, layout, tables, markdown, diff, progress
crates/core     config, profile paths, db, sessions, tasks, notes, events
crates/models   registry, download, verify, select
crates/infer    GGUF loader, prompt builder, sampler, streamer
crates/tools    fs.read, fs.list, fs.search, fs.patch, shell.run, git.*
crates/ingest   walk, ignore rules, text extract, chunk
crates/rag      embed facade, index store, retrieve, eval harness
```

Dependency direction, no cycles:

```text
cli -> ui, core, models, infer, tools, rag, ingest
rag -> ingest, core
tools -> core
infer -> core, models
core -> ui for shared text types only where needed, otherwise standalone
```

### 8.2 Process Model

Single process. Inference runs on blocking thread pool via `spawn_blocking` to keep REPL responsive. Token callbacks push to MPSC channel, UI drains at 16 ms cadence. Ctrl+C sets atomic cancel flag, inference loop checks flag between batches.

No daemon. No lock server. Sqlite WAL with single writer enforced by `core::Db` mutex. Concurrent CLI invocations serialize writes with busy timeout 5 s and clear error.

### 8.3 Data Flow: Chat with RAG

```text
user input
  -> core sessions append user turn
  -> rag retrieve top-k if enabled
  -> infer build prompt with budget
  -> infer stream tokens to ui
  -> core append assistant turn on done or stopped
  -> events log with token counts
```

### 8.4 Data Flow: Code Fix

```text
goal string
  -> tools fs.search plus fs.read, read only plan
  -> infer draft patch hunks
  -> tools validate hunks
  -> ui diff preview
  -> user approve
  -> tools apply atomic plus backup
  -> tools shell.run verify
  -> core record outcome
```

## 9. Data Design

### 9.1 File Layout

```text
~/.config/aicli/config.toml
~/.local/share/aicli/profiles/default/db.sqlite
~/.local/share/aicli/profiles/default/sessions/<id>.jsonl
~/.local/share/aicli/profiles/default/patches/<id>.diff
~/.local/share/aicli/profiles/default/memory.md
~/.local/share/aicli/profiles/default/logs/aicli.log
~/.cache/aicli/models/<model-id>.gguf
~/.cache/aicli/models/registry.toml
~/.cache/aicli/embed/<embed-id>/
~/.cache/aicli/index/<index-id>/tantivy/
~/.cache/aicli/index/<index-id>/meta.json
```

XDG override respected. `--profile` switches `profiles/<name>`.

### 9.2 Sqlite Schema v1

Migrations numbered `001_init.sql` to `00N_*.sql`. Idempotent up migrations.

Tables:

```sql
CREATE TABLE sessions(
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  model TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE turns(
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  tokens_in INTEGER NOT NULL DEFAULT 0,
  tokens_out INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'done',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_turns_session ON turns(session_id, created_at);

CREATE TABLE tasks(
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  text TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_tasks_date ON tasks(date, done);

CREATE TABLE notes(
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  text TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_notes_date ON notes(date);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(text, content='notes', content_rowid='rowid');

CREATE TABLE models(
  id TEXT PRIMARY KEY,
  repo TEXT NOT NULL,
  file TEXT NOT NULL,
  quant TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  path TEXT NOT NULL,
  is_default INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE index_docs(
  path TEXT PRIMARY KEY,
  mtime INTEGER NOT NULL,
  size INTEGER NOT NULL,
  chunk_count INTEGER NOT NULL,
  indexed_at TEXT NOT NULL
);

CREATE TABLE events(
  ts TEXT NOT NULL,
  kind TEXT NOT bikinlah
  session_id TEXT,
  detail_json TEXT NOT NULL
);
```

Note: final DDL reviewed in Phase 3. `notes_fts` triggers kept in sync on insert and delete.

### 9.3 JSONL Session Mirror

Each turn one JSON line with fields `id, role, content, tokens_in, tokens_out, status, created_at`. Used for export and for manual recovery if sqlite is locked. Writer is append only. Reader tolerates trailing partial line after kill.

## 10. Configuration Design

Full annotated sample lives in `docs/CONFIG.md` and in `assets/default-config.toml`. Core excerpt:

```toml
[profile]
name = "default"

[model]
default = "qwen2.5-3b-instruct-q4_k_m"
n_ctx = 4096
n_threads = 0
n_gpu_layers = 0
temp_chat = 0.6
temp_code = 0.2
top_p = 0.9
seed = 0
timeout_s = 120

[index]
paths = ["./docs", "./src"]
ext = ["md", "rs", "toml", "txt"]
exclude = ["target/**", ".git/**", "node_modules/**", "dist/**"]
chunk_tokens = 512
overlap_tokens = 64
top_k = 5
max_file_mb = 5

[tools]
shell_allowlist = [
  "cargo test",
  "cargo test --quiet",
  "cargo fmt --check",
  "git status",
  "git diff"
]
shell_denylist = ["rm -rf", "sudo", "curl", "wget"]
auto_apply = false
confirm_shell = true
max_steps = 12

[ui]
theme = "auto"
right_rail = true
stream_flush_ms = 16
code_languages = ["rust", "toml", "bash", "markdown", "json"]
```

Config precedence: defaults, file, env `AICLI_*`, CLI flags. `config show` prints resolved source per key. `config set` validates type and range before write.

## 11. Inference Engineering

### 11.1 Model Selection Matrix

| Use | Primary GGUF | Fallback | n_ctx | Notes |
|-----|--------------|----------|-------|-------|
| Chat general | Qwen2.5-3B-Instruct Q4_K_M | Llama-3.2-1B-Instruct Q4_K_M | 4096, fallback 2048 | Balanced quality and speed on CPU |
| Code edit | Qwen2.5-Coder-3B-Instruct Q4_K_M if available, else chat model | 1B chat | 4096 | Lower temp, strict diff post check |
| Embeddings | all-MiniLM-L6-v2 via fastembed | BM25 only mode | n/a | Cached ONNX, offline after first fetch |

Registry entry must include repo, file, sha256, size, license URL, and prompt template id. No model without checksum ships as default.

### 11.2 Prompt Construction

System prompt budget 300 tokens. Template id `aicli-v1`:

```text
You are AICLI, a local assistant. Answer concisely.
Use provided sources first. Cite paths when used.
For code, output minimal unified diff plus file list.
Do not invent file paths. If unsure, say what is missing.
```

Context budget for 4096:

* system: 300
* retrieved chunks: max 1400
* history window: max 1800, keep first goal plus last 6 turns
* current input plus slack: remainder
* overflow policy: drop oldest chunk first, then oldest middle turns, never drop system or current input

`--show-budget` prints token estimates per section for debugging.

### 11.3 Sampling

* chat: temp 0.6, top_p 0.9, repeat_penalty 1.05
* code: temp 0.2, top_p 0.95, repeat_penalty 1.1
* extractive or eval: temp 0.0 with fixed seed
* CLI flags `--temp`, `--top-p`, `--seed` override config. Export records values for repro.

### 11.4 Streaming Contract

* Flush cadence 16 ms or 32 tokens.
* Events: `start`, `delta`, `done`, `stopped`, `error{code}`.
* Error codes: `E_MODEL_MISSING`, `E_CTX_OVERFLOW`, `E_OOM`, `E_TIMEOUT`, `E_BACKEND`.
* Each error prints cause plus exact fix command. Example: missing model prints `aicli models pull <id>`.

### 11.5 Backend Flags

* CPU default. Threads 0 means auto from available parallelism minus one, min 2.
* macOS Metal on by default when available. Linux CUDA behind `--features cuda`, Vulkan behind `--features vulkan`.
* `doctor` prints: llama.cpp version, threads, ctx, gpu layers active, tokens per second from last bench.

## 12. Coding Agent Design

### 12.1 Tool Schemas

All tools take JSON args and return JSON result plus truncated human preview. Limits enforced before model output is trusted.

* `fs.read { path, start_line, end_line }` cap 200 lines, 64 KB. Deny outside root. Deny secret filenames with redacted notice.
* `fs.list { dir, glob }` cap 500 entries, sorted, dirs first.
* `fs.search { query, root, ext[], regex, ignore_case, max_hits }` default max 100, time cap 2 s, uses ingest walker.
* `git.status {}` and `git.diff { staged }` pass through with 200 KB cap.
* `fs.patch { patch_id, diff }` validate only in dry run. Apply only after approval.
* `shell.run { cmd, cwd, timeout_s }` default timeout 60 s. Output cap 4000 chars to UI, full log to session file.

### 12.2 Permission Model

* Read: always allowed inside root.
* Write: never implicit. States: `proposed`, `approved`, `applied`, `dropped`.
* Shell: allowlist exact or prefix match. Example prefix `cargo test` allows `cargo test --quiet` but not `cargo test; rm -rf /`.
* Denylist wins over allowlist. Deny on substring match for `rm -rf /`, `mkfs`, `:(){:|:&};:`, `sudo`, pipe to `sh` from `curl` or `wget`.
* Approval cache per session only. New session requires re approval. `--allow-shell` still shows command and 3 s countdown, cancellable with Ctrl+C.

### 12.3 Patch Protocol

* Input must be unified diff with `--- a/path` and `+++ b/path`.
* Validate: paths inside root, no mode change, no binary, hunks apply cleanly to current file content hash.
* Apply is atomic per patch set. On any hunk failure, zero files changed, error lists file plus hunk index plus expected versus actual context.
* Backup: copy each touched file to `<file>.aicli.bak.<ts>` before write. `patch drop` restores on request.
* Post apply: run `cargo fmt --check` if Rust files changed, then configured verify command.

### 12.4 Agent Loop Controller

State machine with max 12 steps:

```text
PLAN -> INVESTIGATE -> DRAFT -> REVIEW -> VERIFY -> DONE
           ^              |        |
           +--------------+        +-> REJECTED -> DONE
```

* INVESTIGATE uses read only tools only. Controller blocks write tool calls in this state and returns tool error to model.
* DRAFT must include file list, risk note, and test plan in addition to diff.
* REVIEW renders in Code screen. Options: apply, edit instruction, reject, run read only check.
* VERIFY runs allowlisted command and attaches exit code to session log.
* Budget guard: stop at 12 steps with summary of completed reads and pending diff.

## 13. Daily Assistant Design

* Tasks support `add "text" --date YYYY-MM-DD`, `list --date`, `done <id>`, `carry --from yesterday`.
* Notes support `add`, `list`, `search`, date range `--from` and `--to`.
* `daily --today` layout:
  1. header date and profile
  2. open tasks with ids
  3. last 5 notes
  4. active session pointer
  5. index status one line
* `daily --week` aggregates counts, completed tasks, notes per day, top terms from local frequency, no model invented narrative. If model is loaded, week summary uses temp 0.0 extractive style only.
* `memory.md` is user owned. CLI never overwrites. `/note promote` appends quoted block with source turn id and timestamp after preview.

Performance: date indexed queries, FTS for notes search, pagination default 50.

## 14. Local RAG Design

### 14.1 Ingest

* Walker respects `.gitignore` plus config `exclude`. Skips hidden unless `--include-hidden`.
* Skip dirs always: `.git`, `target`, `node_modules`, `dist`, `build`, `.cache`.
* Text extract: lossy UTF-8 per line, skip null byte files as binary, record skipped list.
* Size guard: skip files over `max_file_mb`, default 5 MB.

### 14.2 Chunking

* Target 512 tokens estimated as 4 chars per token for English code and docs, plus line anchor preservation.
* Overlap 64 tokens, split on blank line or closing brace when possible, never split mid line number reference.
* Metadata per chunk: `path, start_line, end_line, chunk_id, mtime, config_hash`.
* Index version string `rag-v1` plus config hash stored in `meta.json`. Mismatch forces error with `index --rebuild` instruction.

### 14.3 Embeddings and Index

* Default embedder `fastembed` model `all-MiniLM-L6-v2`. First `index` downloads and caches. Later runs offline.
* Store: tantivy for BM25 plus file `vectors.jsonl` or binary store for dense vectors in v1. Keep abstraction `VectorStore` so `usearch` can replace without CLI change.
* Incremental update by `mtime` plus size. Delete missing files from index. Report added, updated, removed, skipped counts.

### 14.4 Retrieval

* Query both BM25 top 20 and vector top 20, merge, dedupe overlapping chunks from same file, keep top `top_k` default 5.
* Score fusion: z normalize per list then weighted 0.5 and 0.5. Tie break by shorter path then smaller start line for determinism.
* Prompt injection format:

```text
[SOURCE 1] path:crates/rag/src/retrieve.rs:12-68 score=0.81
<chunk text>
```

* UI shows sources rail. `ask --show-sources` prints them. JSON includes scores and ranges.

### 14.5 Eval Harness

* Fixtures in `crates/rag/tests/fixtures/`: 10 queries with expected paths.
* Test asserts at least 8 in top 3. Failure prints rank table for debug.
* Sample corpus of 200 files stored as generator script plus seed, not as 200 checked in files, to keep repo small.

## 15. Premium CLI UX Design System

### 15.1 Principles

* P1: Speed over decoration. No full screen redraw over 60 ms on reference machine.
* P2: Text first. Every visual has plain text equivalent.
* P3: Predictable. Same command always prints same field order in human and JSON.
* P4: Forgiving. Destructive actions need explicit confirm and show undo path.
* P5: Quiet by default in pipes. Style only when TTY and not disabled.

### 15.2 Tokens

* Font: inherit terminal monospace. No embedded fonts.
* Scale: single density. Row height 1. No double spaced lists.
* Color roles:
  * `accent`: cyan for selection, caret, links
  * `ok`: green for applied, verified, done
  * `warn`: amber for ctx pressure, skipped files, rebuilt required
  * `danger`: red for denied, failed, destructive
  * `muted`: gray for hints, timestamps, secondary meta
  * `border`: gray, low contrast
* Spacing: 1 blank line between sections, 0 blank lines inside tables, 2 space indent for nested lists.
* Borders: rounded `╭╮╰╯─│` in capable terminals, ASCII `+-|` fallback. Detection order: `--plain`, `NO_COLOR`, `TERM=dumb`, width under 80, then unicode probe.

### 15.3 Layout Grid

* Max content width 100 columns, centered with 2 column gutters when terminal is wider.
* Chat: main 70 pct, right rail 30 pct collapsible with `F3`. Below 90 columns, rail becomes overlay toggle.
* Code: plan 25 pct left, diff 75 pct center, prompt bar bottom fixed 3 rows.
* Tables: header bold, numeric right aligned, path left aligned truncated from left with `...` prefix to keep filename visible.

### 15.4 Components

1. Status line: product, version, model, ctx meter, offline flag, profile. Always one row. Never wraps. Truncates model id from left if needed.
2. Hint line: context sensitive shortcuts. One row. Hidden in `--quiet` and pipe mode.
3. Panel: title top left, optional action top right, body padded 1. Used for sources, plan, errors.
4. Table: used for models, sessions, tasks, index report. Supports `--json` same column set.
5. Progress: `label ... 12.4 MB/s | 42 pct | ETA 00:12 | 84/200 MB`. Bar width adapts, text remains for screen readers.
6. Diff view: hunk header muted, added line green gutter `+`, removed red gutter `-`, changed word bold. Line numbers both sides.
7. Code block: language tag plus path hint, top border only to reduce noise, copy hint muted.
8. Empty state: icon free, one line reason plus one command to fix. Example: `No sessions yet. Run: aicli chat --session intro`.
9. Error card: code, cause one line, fix command in code style, log path muted. No stack trace unless `--verbose`.

### 15.5 Screen Mockups, ASCII

Home:

```text
 aicli 0.2.0 | model qwen2.5-3b-q4_k_m | ctx 0/4096 | offline | default
 + Sessions ---------------------- + Quick actions ------------------ +
 | * intro             2 turns     | 1 chat       start daily chat    |
 |  fix-scan-test      14 turns    | 2 code       fix from prompt     |
 |  week-review        6 turns     | 3 daily      today overview      |
 +--------------------------------+---------------------------------+
 Tasks today (2 open)                                          [F2 all]
 [ ] wire patch apply              Carry from yesterday? tasks carry
 [ ] write README quickstart
 /help  Tab complete  Ctrl+C stop  Ctrl+D exit  F2 palette  F3 rail
```

Chat streaming:

```text
 You: where is thread pool built
 AICLI [qwen2.5-3b] ctx 1840/4096
 Based on indexed sources:
 [1] crates/tools/src/scan.rs:30-58
 Answer streams here with caret ...
 Sources [1] path:line  Score 0.81
```

Code review:

```text
 Goal: add --json flag to tasks list
 Plan [3/4] read tasks cmd, draft diff, verify
 --- a/crates/cli/src/tasks.rs
 +++ b/crates/cli/src/tasks.rs
 @@ -42,7 +42,12 @@
 +  #[arg(long)] json: bool,
 Apply? [a]pply  [e]dit instruction  [r]eject  [t]run cargo test
```

### 15.6 Slash Command UX

* `/` opens completion with description and usage example.
* Fuzzy match tolerates 2 typos for commands under 8 chars, shows `Did you mean /sessions`.
* Args complete from local data: session ids, model ids, file paths.
* History across restarts in `~/.local/share/aicli/history.txt`, 1000 lines max, no secrets filter beyond secret filename block.

### 15.7 Keymap

| Key | Action |
|-----|--------|
| Ctrl+C | Stop stream, keep partial as stopped |
| Ctrl+D | Save and exit |
| Ctrl+L | Clear view only |
| Ctrl+R | Search input history |
| Ctrl+P or F2 | Command palette |
| F3 | Toggle right rail |
| Alt+1, Alt+2, Alt+3 | Go to Chat, Code, Daily |
| Tab | Complete |
| Alt+Enter | Newline in multiline input |
| Up, Down | History, with prefix search |

All bindings listed in `/help keys` and in `docs/UX.md`. No hidden binding for destructive action.

### 15.8 Output Contracts

Human mode is default on TTY. Pipe mode auto plain. Rules:

* Human: styled, may use color and panels.
* `--plain`: no ANSI, ASCII borders, same field order.
* `--json`: envelope below, stdout only JSON, logs to stderr, exit code preserved.
* `--quiet`: only primary result lines. Example `tasks list --quiet` prints task ids and text, no header.

JSON envelope:

```json
{
  "ok": true,
  "data": {},
  "error": {"code": "", "message": "", "fix": ""}
}
```

Golden files for `--help`, `--json`, and error envelope live in `tests/golden/`.

## 16. Command Reference

Notation: `<>` required, `[]` optional. All commands accept global flags `--profile`, `--plain`, `--json`, `--quiet`, `--offline`, `--verbose`.

### 16.1 chat

```text
aicli chat [--session <id>] [--model <name>] [--temp <float>] [--ctx <int>]
```

Behavior: no args resumes last active session. New session if none. Streams markdown. Persists turns. Exit Ctrl+D.

Examples:

```text
aicli chat
aicli chat --session fix-scan-test --model qwen2.5-3b-instruct-q4_k_m
```

### 16.2 ask

```text
aicli ask "<query>" [--index <path>] [--top-k <int>] [--show-sources] [--no-rag]
```

One shot. Prints answer plus optional sources block. Exit 0 on answer, 4 if index stale and `--offline` blocks rebuild data fetch.

### 16.3 code

```text
aicli code "<goal>" [--path <dir>] [--dry-run] [--apply] [--allow-shell] [--max-steps <int>]
```

Default `--dry-run`. `--apply` still prompts. `--allow-shell` still shows command. Every run writes patch file under `patches/`.

### 16.4 patch

```text
aicli patch show --patch-id <id>
aicli patch apply --patch-id <id> [--yes]
aicli patch drop --patch-id <id>
```

`apply` without `--yes` prompts. Atomic apply only.

### 16.5 run

```text
aicli run -- <cmd...>
```

Allowlist gate. Example `aicli run -- cargo test --quiet`. Denied prints code `E_TOOL_DENIED` plus closest allowed command.

### 16.6 index

```text
aicli index <path> [--rebuild] [--ext <csv>] [--exclude <glob>] [--max-mb <n>] [--include-hidden]
aicli index status [--json]
```

Report fields: added, updated, removed, skipped with reasons, elapsed, index version, config hash.

### 16.7 models

```text
aicli models list [--json]
aicli models pull <id> [--resume]
aicli models verify <id>
aicli models remove <id>
aicli models set-default <id>
```

Pull shows progress with speed, percent, ETA, bytes. Verify checks sha256 and prints mismatch offset hint, not full dump.

### 16.8 daily, tasks, notes

```text
aicli daily [--today] [--week] [--search <q>] [--add-note "<text>"]
aicli tasks add "<text>" [--date <yyyy-mm-dd>]
aicli tasks list [--date <yyyy-mm-dd>] [--json] [--quiet]
aicli tasks done <id>
aicli tasks carry --from <yesterday|yyyy-mm-dd>
aicli notes add "<text>" [--date <yyyy-mm-dd>]
aicli notes list [--date <yyyy-mm-dd>] [--limit <n>]
aicli notes search "<q>" [--from <date>] [--to <date>]
```

### 16.9 sessions, config, doctor

```text
aicli sessions list [--limit <n>] [--json]
aicli sessions open <id>
aicli sessions rename <id> "<title>"
aicli sessions delete <id> [--yes]
aicli sessions export --id <id> --format <md|json> [--out <path>]
aicli config show [--json]
aicli config set <key> <value>
aicli config reset [--yes]
aicli doctor [--bench-load] [--bench-gen <n>] [--check-updates]
```

### 16.10 Exit Codes

| Code | Meaning | Example |
|------|---------|---------|
| 0 | ok | done |
| 2 | bad args or validation | unknown flag, bad date |
| 3 | model missing or invalid | pull hint printed |
| 4 | offline resource missing or stale index | rebuild hint printed |
| 5 | tool denied or sandbox violation | path escape, shell deny |
| 6 | internal error | panic guard with log path |

## 17. Workspace and Repository Design

### 17.1 Target Tree for v1

```text
Cargo.toml
rustfmt.toml
clippy.toml
deny.toml
.gitignore
LICENSE-MIT
README.md
CHANGELOG.md
plan.md
assets/default-config.toml
assets/demo-cast.sh
configs/models-registry.toml
migrations/001_init.sql
docs/ARCHITECTURE.md
docs/USER_GUIDE.md
docs/CONFIG.md
docs/UX.md
docs/SECURITY.md
docs/TROUBLESHOOTING.md
docs/DECISIONS.md
crates/cli/
crates/ui/
crates/core/
crates/models/
crates/infer/
crates/tools/
crates/ingest/
crates/rag/
tests/golden/
tests/e2e/
benches/
scripts/check-no-emdash.sh
scripts/offline-test.sh
.github/workflows/ci.yml
.github/workflows/release.yml
```

### 17.2 Branch and Commit Rules

* Branches: `main` protected, `feat/<phase>-<slug>`, `fix/<slug>`, `docs/<slug>`.
* Commits: Conventional Commits `feat:`, `fix:`, `docs:`, `test:`, `chore:`, `refactor:`.
* PR template requires: scope, manual test, golden update yes or no, docs update yes or no, perf impact.
* No direct push to `main`. Squash merge with phase tag after gate.

## 18. Execution Phases and WBS

Legend per task: ID, description, owner role, effort hours, dependency.

Effort is solo calendar estimate for advanced Rust developer with local test machine. Total indicative range 320 to 420 hours including docs and polish.

### Phase 0: Clean Foundation, 24 to 32 hours

Entry: plan 2.0.0 approved, workspace empty except plan and git.
Exit: workspace builds, CI green, walker baseline restored from memory without legacy copy paste debt.

PH0.1 Workspace skeleton, 8h

* PH0-T01: Init Cargo workspace with 8 crates and lib plus bin targets. [Arch, 3h]
* PH0-T02: Add rustfmt, clippy, deny, audit, em dash lint script. [QA, 2h]
* PH0-T03: Add CI with fmt, clippy, test, audit matrix Linux plus macOS. [QA, 3h]
* Deliverable: `cargo test --workspace` green on empty stubs.
* Acceptance: clippy deny warnings passes, em dash script passes.

PH0.2 Core paths and config stub, 8h

* PH0-T04: Implement profile paths plus XDG plus `--profile`. [Arch, 3h]
* PH0-T05: Implement config load with defaults plus file plus env plus flags. [Arch, 5h]
* Acceptance: `config show --json` prints resolved values with source column in test.

PH0.3 Ingest baseline, 8 to 12h

* PH0-T06: Implement walker with gitignore, hidden handling, skip dirs, ext filter. [Arch, 5h]
* PH0-T07: Add golden fixtures for walker and search matcher. [QA, 3h]
* PH0-T08: Add benches for walk and search on generated 200 file tree. [QA, 2h]
* Acceptance: walker matches prior `logscan` behavior on fixtures, bench baseline recorded.
* Tag: `m0-foundation`.

Risk in phase: over abstraction. Mitigation: keep facade thin, no generic plugin trait in v1.

### Phase 1: Premium CLI Shell, 56 to 72 hours

Entry: M0 tagged.
Exit: polished mock shell demo without model.

PH1.1 Command tree and output contracts, 16h

* PH1-T01: clap tree for all Section 16 commands with stubs returning not implemented JSON envelope. [Arch, 8h]
* PH1-T02: Global flags, exit codes, pipe detection, NO_COLOR handling. [UX, 4h]
* PH1-T03: Golden help tests for every command. [QA, 4h]
* Acceptance: help snapshots pass, pipe output has zero ANSI bytes verified by test.

PH1.2 Theme and primitives, 16h

* PH1-T04: Theme tokens plus auto detect plus fallback. [UX, 6h]
* PH1-T05: Table, panel, status line, hint line, progress. [UX, 6h]
* PH1-T06: Snapshot tests unicode versus ASCII. [QA, 4h]
* Acceptance: 80x24 render test passes, snapshots reviewed in PR.

PH1.3 REPL and palette, 16 to 24h

* PH1-T07: reedline integration with history, slash completion, path completion. [UX, 10h]
* PH1-T08: Command palette F2 with fuzzy filter. [UX, 6h]
* PH1-T09: Manual keymap test script plus history persistence test. [QA, 4h]
* Acceptance: 12 flow manual script passes, history file created with correct perms 600.

PH1.4 Markdown and code render, 12 to 16h

* PH1-T10: Markdown subset renderer with code fences and tables. [UX, 8h]
* PH1-T11: Syntax highlight for 5 languages plus copy hint. [UX, 4h]
* PH1-T12: 20 golden fixtures plus 50 ms perf test. [QA, 4h]
* Acceptance: no panic on malformed fences, perf gate passes.
* Tag: `m1-shell`. Demo: `chat --mock`, `daily --today`, `doctor`.

### Phase 2: Offline Model Runtime, 56 to 72 hours

Entry: M1 tagged, test GGUF fixture selected.
Exit: real offline streaming answers.

PH2.1 Registry and download, 16h

* PH2-T01: Registry TOML plus cache layout plus `models list`. [Arch, 6h]
* PH2-T02: `models pull` resume plus progress plus sha256. [Arch, 6h]
* PH2-T03: Corrupt download and resume kill tests. [QA, 4h]
* Acceptance: kill at 50 pct resumes and verifies.

PH2.2 Loader and backend, 16h

* PH2-T04: llama-cpp-2 loader with n_ctx, threads, gpu layers. [Arch, 10h]
* PH2-T05: `doctor` backend report plus actionable errors. [Arch, 4h]
* PH2-T06: OOM and bad file fixture tests. [QA, 2h]
* Acceptance: missing model exits 3 with exact pull command.

PH2.3 Streaming and prompt budget, 16 to 24h

* PH2-T07: Prompt builder with budget and `--show-budget`. [Arch, 8h]
* PH2-T08: Stream loop with flush cadence, cancel, timeout, overflow. [Arch, 8h]
* PH2-T09: First token and cancel persistence tests with mock backend. [QA, 4h]
* Acceptance: Ctrl+C keeps partial turn marked stopped.

PH2.4 Sampling and repro, 8h

* PH2-T10: Temp, top_p, seed plumbing plus export record. [Arch, 4h]
* PH2-T11: Deterministic seed test temp 0.0 byte identical. [QA, 4h]
* Tag: `m2-infer`. Demo: offline ask with ctx meter.

### Phase 3: Sessions and Memory, 32 to 44 hours

Entry: M2 tagged.
Exit: durable chat across restarts.

PH3.1 Storage, 12h

* PH3-T01: Migrations 001 plus Db wrapper with WAL and busy timeout. [Arch, 6h]
* PH3-T02: Session CRUD plus JSONL mirror plus export. [Arch, 4h]
* PH3-T03: Migration idempotence and lock contention tests. [QA, 2h]

PH3.2 History and compact, 12 to 16h

* PH3-T04: Sliding window plus 85 pct warning plus `/ctx compact`. [Arch, 8h]
* PH3-T05: 50 turn compact fixture test. [QA, 4h]

PH3.3 Session UX, 8 to 12h

* PH3-T06: Resume last, rename, delete with confirm, right rail. [UX, 6h]
* PH3-T07: Kill during stream recovery test. [QA, 4h]
* Tag: `m3-chat`.

### Phase 4: Coding Agent, 64 to 84 hours

Entry: M3 tagged.
Exit: safe end to end fix demo.

PH4.1 Read tools and sandbox, 16h

* PH4-T01: fs.read, list, search, git status and diff with caps. [Arch, 10h]
* PH4-T02: Sandbox deny plus secret redact plus events log. [Arch, 4h]
* PH4-T03: Escape and secret tests. [QA, 2h]

PH4.2 Patch engine, 16 to 20h

* PH4-T04: Diff parse and validate with similar. [Arch, 8h]
* PH4-T05: Atomic apply plus backup plus drop restore. [Arch, 6h]
* PH4-T06: Malformed hunk and multi file atomicity tests. [QA, 4h]

PH4.3 Shell gate, 12 to 16h

* PH4-T07: Allowlist plus denylist plus per session approval. [Arch, 8h]
* PH4-T08: Output capture plus truncate plus log full. [Arch, 2h]
* PH4-T09: Deny table tests. [QA, 4h]

PH4.4 Loop and Code screen, 20 to 28h

* PH4-T10: 12 step controller with state guard. [Arch, 12h]
* PH4-T11: Code screen with plan checklist and apply prompt. [UX, 8h]
* PH4-T12: E2E fixture fix failing test on sample repo. [QA, 4h]
* Tag: `m4-code`. Demo script in `docs/USER_GUIDE.md`.

### Phase 5: Daily Assistant, 24 to 32 hours

Entry: M3 tagged, can run parallel with Phase 4 after core DB done.
Exit: daily workflow usable.

PH5.1 Tasks and notes, 16h

* PH5-T01: Task and note CRUD plus FTS plus pagination. [Arch, 10h]
* PH5-T02: Today and week views human plus JSON. [UX, 4h]
* PH5-T03: 1000 record perf test. [QA, 2h]

PH5.2 Memory file, 8 to 12h

* PH5-T04: memory.md read path plus promote preview. [Arch, 4h]
* PH5-T05: Export with opt in memory section. [UX, 4h]
* Tag: `m5-daily`.

### Phase 6: Local RAG, 40 to 56 hours

Entry: M2 and ingest baseline done.
Exit: cited answers with regression gate.

PH6.1 Chunk and version, 12h

* PH6-T01: Chunker with overlap and line anchors plus meta hash. [Arch, 8h]
* PH6-T02: Stale index enforcement tests. [QA, 4h]

PH6.2 Embed and store, 12 to 16h

* PH6-T03: fastembed facade plus lazy download plus offline guard. [Arch, 8h]
* PH6-T04: tantivy plus vector sidecar plus incremental update. [Arch, 4h]

PH6.3 Retrieve and eval, 16 to 24h

* PH6-T05: Fusion scorer plus dedupe plus citation format. [Arch, 10h]
* PH6-T06: 10 query regression harness. [QA, 6h]
* PH6-T07: `ask --show-sources` UX plus JSON scores. [UX, 4h]
* Tag: `m6-rag`.

### Phase 7: Hardening and Release, 40 to 56 hours

Entry: M4, M5, M6 tagged.
Exit: 1.0.0 release.

PH7.1 Coverage and benches, 12h

* PH7-T01: Fill gaps to 80 pct on four crates. [QA, 6h]
* PH7-T02: Criterion benches plus baselines. [QA, 4h]
* PH7-T03: Golden refresh and flake audit. [QA, 2h]

PH7.2 Security review, 8 to 12h

* PH7-T04: Sandbox, allowlist, secret, log audit. [Arch, 6h]
* PH7-T05: Offline egress test with proxy counter. [QA, 4h]

PH7.3 Release engineering, 12 to 16h

* PH7-T06: cargo-dist plus checksums plus install script. [QA, 6h]
* PH7-T07: Fresh VM install test. [QA, 4h]

PH7.4 Docs and portfolio, 12 to 16h

* PH7-T08: README, guides, arch, decisions, troubleshooting. [UX, 8h]
* PH7-T09: Demo cast plus benchmark table plus roadmap. [UX, 4h]
* Tag: `v1.0.0`.

## 19. Schedule and Milestones

Order is fixed. Phase 5 can overlap Phase 4. Phase 6 can start after Phase 2 prompt builder is stable.

| Milestone | Phase exit | Demo |
|-----------|------------|------|
| M0 | Phase 0 | workspace green, walker bench |
| M1 | Phase 1 | mock shell video |
| M2 | Phase 2 | offline streaming chat |
| M3 | Phase 3 | resume plus compact plus export |
| M4 | Phase 4 | coding fix with diff approval |
| M5 | Phase 5 | today plus week plus search |
| M6 | Phase 6 | cited RAG answers |
| V1 | Phase 7 | release binaries plus docs |

No next phase starts with red acceptance. Scope cuts go to roadmap section, not silent descoping.

## 20. Quality Plan

### 20.1 Test Pyramid

* 60 pct unit: budget calc, chunk, patch validate, config, level detect, fusion sort.
* 30 pct integration: CLI golden, DB migrate, session resume, patch atomic, index stale, download resume.
* 10 pct e2e: chat mock, code fixture, RAG regression, offline proxy, fresh install.

### 20.2 Gates

Per PR: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, em dash lint, docs flag check if help changed.
Nightly: benches, `cargo audit`, `cargo deny check`, coverage.
Release: full matrix Linux plus macOS, manual 80x24 checklist, offline egress, fresh VM install.

### 20.3 Golden and Fixture Rules

* Golden dir `tests/golden/` is source of truth. Update only with `--bless` style env plus reviewer note.
* No network in tests. Model tests use mock backend trait. Download tests use local HTTP fixture server with fixed bytes.
* Deterministic sort for all user visible lists: by path then line, by date then id.

## 21. Security and Privacy Plan

Controls with test mapping:

* SEC-01 Path sandbox: canonicalize plus root prefix check. Test: `..` escape, symlink escape, absolute outside root. Must exit 5.
* SEC-02 Secret redact: filenames `.env`, `*.pem`, `id_rsa*`, `credentials.json` return redacted stub. Test: read attempt returns stub plus event log.
* SEC-03 Shell gate: allowlist plus denylist plus confirm. Test matrix of 20 commands.
* SEC-04 Log hygiene: no full secret file content in logs. Test scans log fixture for `BEGIN PRIVATE KEY`.
* SEC-05 Offline proof: `scripts/offline-test.sh` runs chat, ask, code dry run, daily, index status under blocked egress proxy and asserts zero connections.
* SEC-06 File perms: config dir 700, DB 600, history 600. Test on Unix perms.
* SEC-07 Supply chain: locked deps, audit, deny bans copyleft strong licenses and unmaintained crates. Review quarterly.

Privacy statement for README: all prompts stay on device except explicit `models pull`. No telemetry. State log path and retention.

## 22. DevOps and Release Plan

* CI file `.github/workflows/ci.yml`: jobs fmt, clippy, test workspace, golden, audit, deny, em dash lint.
* Release file `.github/workflows/release.yml`: on tag `v*`, build Linux x86_64 and macOS ARM64, run `cargo-dist`, attach checksums, draft notes from CHANGELOG.
* Versioning: SemVer. `0.1.0` after M1, `0.2.0` after M3, `0.3.0` after M6, `1.0.0` after V1 gates.
* Install paths:
  1. tarball to `/usr/local/bin/aicli` plus `aicli doctor`
  2. `cargo install --locked --path crates/cli`
  3. build from source documented with feature flags
* Rollback: keep prior binary as `aicli.old` on manual install script, DB migrations forward only with backup copy `db.sqlite.bak.<ts>` before migrate.
* Feature flags: default `cpu`, optional `cuda`, `vulkan`, `metal`. Document binary size delta per flag.

## 23. Documentation Plan

Each doc has owner and update trigger.

| Doc | Content | Trigger |
|-----|---------|---------|
| README.md | quickstart, install, model table, 5 commands, demo link, benchmarks | every release |
| docs/USER_GUIDE.md | chat, code, daily, RAG workflows copy paste | new flag or flow |
| docs/CONFIG.md | annotated config plus precedence | config change |
| docs/ARCHITECTURE.md | crate map, flows, storage, inference | arch change |
| docs/UX.md | tokens, screens, keymap, fallback | UI change |
| docs/SECURITY.md | threat model plus controls plus offline proof | security change |
| docs/TROUBLESHOOTING.md | OOM, slow, index mismatch, GPU, logs | new error code |
| docs/DECISIONS.md | ADR log | new ADR |
| CHANGELOG.md | Keep a Changelog | every PR with user impact |

Readability bar: new user reaches first offline answer by following README only. Tested in V1 acceptance.

## 24. Risk Register

Score is probability 1 to 5 times impact 1 to 5. Owner is role.

| ID | Risk | P | I | Score | Mitigation | Owner |
|----|------|---|---|-------|------------|-------|
| RS-01 | llama-cpp-2 build breaks or slows CI | 4 | 4 | 16 | Pin version, cache target, CPU default, optional GPU features | Arch |
| RS-02 | 3B model too slow on 8 GB laptop | 4 | 4 | 16 | Ship 1B fallback, ctx auto down, bench in doctor, clear model table | Arch |
| RS-03 | Embedding first download confuses offline claim | 3 | 4 | 12 | Lazy download with progress, BM25 only fallback, explicit offline error | Arch |
| RS-04 | Tantivy binary size plus build time high | 3 | 3 | 9 | Abstract VectorStore, measure at M6, fallback to sqlite plus usearch | Arch |
| RS-05 | TUI scope creep delays inference | 4 | 3 | 12 | Freeze screens at M1, snapshot tests, no new widget without ADR | UX |
| RS-06 | Patch apply corrupts user files | 2 | 5 | 10 | Atomic apply, backup, dry run default, golden atomicity tests | QA |
| RS-07 | Shell allowlist bypass | 2 | 5 | 10 | Exact plus prefix match, denylist wins, 20 case matrix, review | QA |
| RS-08 | Context overflow on large answers | 3 | 3 | 9 | Budget meter, compact, top-k cap, overflow error with fix | Arch |
| RS-09 | Terminal compat breaks demo | 3 | 3 | 9 | ASCII fallback, NO_COLOR, 80x24 checklist per release | UX |
| RS-10 | Solo burnout from 400h scope | 3 | 4 | 12 | Milestone demos, cut list to roadmap, freeze Phase 1 early | Product |

Top risks RS-01 and RS-02 reviewed at end of Phase 2. If tokens per second below target, change default model before Phase 3.

## 25. Decision Log

ADR format lives in `docs/DECISIONS.md`. Initial entries:

* ADR-001: Rust only runtime. Rejected Go plus Python sidecar for distribution and offline guarantees.
* ADR-002: GGUF via llama-cpp-2. Rejected Python bindings and remote API for offline v1.
* ADR-003: fastembed for embeddings with BM25 fallback. Rejected Python sentence transformers for C1.
* ADR-004: tantivy default index with VectorStore abstraction. Revisit on binary size at M6.
* ADR-005: reedline for REPL. Fallback rustyline if completion UX fails prototype.
* ADR-006: sqlite WAL plus JSONL mirror. Rejected pure JSON store for query speed and FTS.
* ADR-007: No em dash in repo. Enforced by `scripts/check-no-emdash.sh` for consistent voice and to avoid LLM style artifacts.

New ADR required for: inference crate change, index store change, REPL change, markdown renderer change, release tool change.

## 26. Glossary

* GGUF: file format for local LLM weights with quantization.
* Q4_K_M: 4 bit quantization profile balancing size and quality.
* ctx: context window token budget for one request.
* RAG: retrieval augmented generation, answer grounded in local files.
* BM25: lexical ranking function for text search.
* Top-k: number of retrieved chunks injected into prompt.
* Allowlist: explicit list of permitted shell commands.
* Sandbox: path root outside which reads and writes are denied.
* Golden test: snapshot comparison against checked in expected output.
* FTS: full text search, here sqlite FTS5 for notes.

## Appendix A. Definition of Done Checklist for v1

* [ ] All Section 2.2 metrics met and logs attached.
* [ ] `cargo test --workspace` green on Linux and macOS.
* [ ] Clippy deny warnings green, fmt clean, audit clean.
* [ ] Offline egress test zero connections for chat, ask, code dry run, daily.
* [ ] Fresh VM install script passes with only README steps.
* [ ] Release has Linux and macOS archives plus checksums.
* [ ] Docs set complete per Section 23.
* [ ] Security controls SEC-01 to SEC-07 tested.
* [ ] No em dash lint passes.
* [ ] CHANGELOG entry for 1.0.0 with known limits.

## Appendix B. Cut List if Behind Schedule

Cut in this order, move to roadmap v1.1, never silently drop acceptance:

1. Right rail overlay becomes simple `--show-sources` flag output.
2. Command palette becomes slash completion only.
3. Week summary model narrative becomes counts only.
4. Syntax highlight drops to 3 languages.
5. `.deb` packaging drops, tarball only.
6. CUDA and Vulkan flags drop, CPU plus Metal only.

## Appendix C. Immediate Next Actions After Plan Approval

1. Approve model ids and registry entries.
2. Create workspace skeleton per Phase 0.
3. Implement `scripts/check-no-emdash.sh` first so later PRs stay clean.
4. Freeze Section 16 command tree before Phase 1 code.
5. Schedule M1 mock demo before any inference work.

## Appendix D. Reference Prompts v1

System prompt id `aicli-v1` is stored in `crates/infer/src/prompts/aicli-v1.md` and versioned. Change requires ADR and regression rerun for code fixtures.

Compact prompt id `compact-v1` uses temp 0.0 and preserves file paths, task states, and open patch ids. Output limit 400 tokens.
