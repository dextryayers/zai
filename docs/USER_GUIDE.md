# User Guide

## Full screen UI

Run bare `zai`. Left panel lists history sessions, main column is chat,
every action is a `/` command with Tab completion popup.

```text
/help                         all commands
/new [title]                  new session
/sessions, /open <id>         browse and resume
/model [id]                   picker overlay or direct switch
/manage                       manage page: Ollama tab plus GGUF tab
/ollama <list|pull|rm|show>   Ollama daemon models
/run <cmd>                    shell under the shell mode gate
/insert <file.gguf> [more...] [--name id] [--ctx n] [--default]   save GGUFs, folders work too, alias /add
/setting [set <key> <value>]  settings page
/effort [level]               Default Low Medium High XHigh Expert
/budget, /compact, /export    context tools
/sources, /clear, /quit       view controls
```

Keys: Up and Down history, Tab complete or switch panel, Enter open
session when left panel focused, PageUp and PageDown scroll, Ctrl+C stop,
Ctrl+D quit, Esc close overlay or clear input.

Effort mapping: Low temp 0.3 tokens 256 steps 4, Medium 0.5 512 8,
Default and High 0.6 1024 12 and 16, XHigh 0.7 2048 20, Expert 0.8 4096 24.

## Chat

```bash
zai chat
zai chat --session fix-1 --model qwen2.5-3b-instruct-q4_k_m
```

Inside REPL:

```text
/help
/new fix-login
/sessions
/model qwen2.5-3b-instruct-q4_k_m
/ctx compact
/budget
/code add json flag
/note shipped demo
/note promote t_abc123
/memory
/daily
/sources
/export md
/plain
/quit
```

Keys: Ctrl+C stops stream, Ctrl+D exits, Up and Down recall history.

## Ask with RAG

```bash
zai index ./docs --rebuild
zai index ./docs --status
zai ask "where is thread pool built" --index ./docs --show-sources
zai ask "hitung 12* (3+4)"          # local brain math, offline
zai ask "how to prevent xss"        # local security playbook
zai ask "hello" --show-budget --top-k 5
zai ask "hello" --no-rag
zai ask "hello" --model ollama/llama3.1   # needs ollama serve
zai ask "hello" --json | python3 -m json.tool
zai index --eval
```

Citations show `path:start-end score= bm25= vec=`. Stale index errors print rebuild fix.

## Shell access

Three modes, set by you only: `deny` blocks all shell, `ask` prompts on
allowlisted commands, `allow` runs any command without prompt.

```bash
zai config set shell ask
zai run -- git status
zai run -- echo hi   # denied in ask mode unless allowlisted
```

Every run is logged to `logs/run.log` plus the events table.

## Code agent

```bash
zai code "fix failing test" --path ./crates/tools
zai code "add json flag" --path ./crates/cli --apply
ZAI_AUTO_YES=1 zai code "todo" --path /tmp/demo --apply --yes
zai patch show --patch-id p0001
zai patch apply --patch-id p0001 --yes
zai run -- git status
```

Dry run default. Apply writes backup `<file>.zai.bak.<ts>`. Verify runs allowlisted fmt when Rust touched.

## Daily and memory

```bash
zai tasks add "write tests"
zai tasks list
zai tasks carry --from yesterday
zai notes add "shipped shell demo"
zai notes search "rust"
zai daily --today
zai daily --week
zai memory show
zai memory add "prefer tabs"
```

Week shows counts only, no invented narrative. Memory is user owned, model reads on ask when wired, never auto writes.

## Models

```bash
zai models list
zai insert ~/models/tiny-q4_k_m.gguf
zai insert ~/models --recursive
zai models set-default tiny-q4-k-m
zai models verify tiny-q4-k-m
zai models pull qwen2.5-3b-instruct-q4_k_m
zai ollama status
zai ollama list
zai ollama pull llama3.1
zai ollama rm llama3.1
zai doctor
zai doctor --bench-load --bench-gen 64
```

Shortest path: `zai insert <file>` guesses the name, quant, and ctx,
and the first model becomes default automatically. `models add` and
`models import` are aliases for `models insert`. In chat use
`/insert <file> [more...]` or `/add`, bare `/insert` lists nearby files.

```bash
zai models scan
zai models scan ~/models --recursive
zai models inspect ~/models/tiny-q4_k_m.gguf
```

`scan` finds GGUF files across folders (defaults: `./models`, `.`,
`~/models`, `~/Models`, plus `ZAI_MODELS_DIRS`) and shows arch, quant,
size, ctx hint, and saved status. `inspect` reads one file deep: GGUF
version, tensor count, metadata entries, architecture, quant, and the
id plus ctx an insert would use.

## Backend

Real answers come from a local llama.cpp build, never the mock:

```bash
zai backend status
zai backend setup
zai ask "explain ownership in Rust" --session work
zai chat
```

`setup` clones the pinned llama.cpp tag plus cmake configure plus
compile (several minutes once), then shares the binary system wide when
allowed. Every chat, REPL, and ask answer above the local brain runs on
it with the model ctx, sampler, and timeout from config. Ctrl+C in the
TUI stops a running generation and keeps the visible prefix. `doctor`
reports the backend row alongside model and brain.

## Server

One persistent `llama-server` keeps weights loaded, so the first answer
warms up once and every next answer lands in seconds instead of a full
reload per question:

```bash
zai backend serve
zai backend status
zai backend stop
```

The server starts itself on the first GGUF answer when the binary
exists, and `stop` kills it. The model bar shows a green dot while it
is up. Answers served this way carry `· server` in the backend note,
one shot runs carry `· cli`.

GGUF files are local only: insert from disk, delete from cache, never download.
Ollama models install and delete through the daemon.
