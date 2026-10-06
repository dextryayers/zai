# Troubleshooting

## Config parse fails
Run `aicli config show --json` to see error. Compare with `assets/default-config.toml`.

## No color or broken borders
Set `TERM=xterm-256color` or run with `--plain`. Check `NO_COLOR` is unset for styled mode. Fallback verified at 80x24 and `TERM=dumb`.

## History not saved
Check profile dir perms and `history.txt` path from `aicli doctor`.

## Index shows zero files
Check ext filter in config and `.gitignore`. Run `aicli index <path> --rebuild`.

## Stale index error
Version or chunk params changed. Error prints exact fix: `aicli index <root> --rebuild`. Config hash covers chunk_tokens, overlap_tokens, ext list.

## FTS5 syntax error on code query
Retriever quotes terms and falls back to LIKE plus vectors. If still empty, run with `--no-rag` to confirm prompt path.

## Slow output in pipes
Piped stdout auto disables color. Use `--json` for scripts.

## OOM on large ctx
Lower `--ctx` to 2048 or use 1B model. `doctor` shows cache MB and model path. Check `/proc/meminfo` hint in loader error.

## Patch apply fails
Hunk context mismatch means file changed since draft. Re run `code` to regenerate diff, review with `patch show`, then `patch apply --yes`.
