# User Guide

## Chat

```bash
aicli chat
aicli chat --session fix-1 --model qwen2.5-3b-instruct-q4_k_m
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
aicli index ./docs --rebuild
aicli index ./docs --status
aicli ask "where is thread pool built" --index ./docs --show-sources
aicli ask "hello" --show-budget --top-k 5
aicli ask "hello" --no-rag
aicli ask "hello" --json | python3 -m json.tool
aicli index --eval
```

Citations show `path:start-end score= bm25= vec=`. Stale index errors print rebuild fix.

## Code agent

```bash
aicli code "fix failing test" --path ./crates/tools
aicli code "add json flag" --path ./crates/cli --apply
AICLI_AUTO_YES=1 aicli code "todo" --path /tmp/demo --apply --yes
aicli patch show --patch-id p0001
aicli patch apply --patch-id p0001 --yes
aicli run -- git status
```

Dry run default. Apply writes backup `<file>.aicli.bak.<ts>`. Verify runs allowlisted fmt when Rust touched.

## Daily and memory

```bash
aicli tasks add "write tests"
aicli tasks list
aicli tasks carry --from yesterday
aicli notes add "shipped shell demo"
aicli notes search "rust"
aicli daily --today
aicli daily --week
aicli memory show
aicli memory add "prefer tabs"
```

Week shows counts only, no invented narrative. Memory is user owned, model reads on ask when wired, never auto writes.

## Models

```bash
aicli models list
aicli models pull qwen2.5-3b-instruct-q4_k_m
aicli models verify qwen2.5-3b-instruct-q4_k_m
aicli doctor
aicli doctor --bench-load --bench-gen 64
```
