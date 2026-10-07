# Capabilities

## Full coding
Complete runnable programs (Rust, Python, JavaScript/TypeScript, Go, Bash, SQL), code review, bug fixes, apply-ready diffs via `zai code "<goal>" --apply`.

## Daily
Capture and recall: `zai tasks add`, `zai notes add`, `zai daily --today`, `zai daily --week`.

## Terminal
Run commands under a permission gate via `zai run -- <cmd>`. Full access via `zai config set shell allow`, always logged.

## Defensive security
Hardening, code audit, OWASP checklists, pentest workflow on systems you own, plus fixes.

## Local RAG
Index folders with `zai index ./docs --rebuild`, then ask with `zai ask "..." --show-sources` for cited answers.

# Quickstart
```bash
zai                          # full-screen UI with scroll, mouse, command palette
zai chat                     # classic REPL with /help, /whoami, /code
zai ask "create a python fibonacci function"
zai code "add a --json flag" --apply
zai run -- git status
```
