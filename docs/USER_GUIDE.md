# User Guide

## Full screen UI

Run bare `zai`. Left panel lists history sessions, main column is chat,
every action is a `/` command with Tab completion popup.

```text
/help                         all commands
/new [title]                  new session
/sessions, /open <id>         browse and resume
/model [id]                   picker overlay or direct switch
/insert <file.gguf> [--name id] [--ctx n]   save a GGUF, unlimited models
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
zai ask "hello" --show-budget --top-k 5
zai ask "hello" --no-rag
zai ask "hello" --json | python3 -m json.tool
zai index --eval
```

Citations show `path:start-end score= bm25= vec=`. Stale index errors print rebuild fix.

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
zai models insert ~/models/tiny-q4_k_m.gguf --name tiny --ctx 2048
zai models set-default tiny
zai models verify tiny
zai models pull qwen2.5-3b-instruct-q4_k_m
zai doctor
zai doctor --bench-load --bench-gen 64
```
