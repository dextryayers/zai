# User Guide

## Chat

```bash
aicli chat
aicli chat --session fix-1 --model qwen2.5-3b-instruct-q4_k_m
```

Inside REPL:

```text
/help
/model qwen2.5-3b-instruct-q4_k_m
/sources
/plain
/quit
```

Keys: Ctrl+C stops stream, Ctrl+D exits, Up and Down recall history.

## Ask

```bash
aicli ask "where is thread pool built" --show-sources
aicli ask "hello" --json | python3 -m json.tool
```

## Code preview

```bash
aicli code "add --json flag to tasks list" --path ./crates/cli
```

Dry run only in Phase 1. Review diff, apply lands in Phase 4.

## Daily

```bash
aicli tasks add "write tests"
aicli tasks list
aicli notes add "shipped shell demo"
aicli daily --today
```

## Models

```bash
aicli models list
aicli models pull qwen2.5-3b-instruct-q4_k_m
aicli doctor --bench-load
```
