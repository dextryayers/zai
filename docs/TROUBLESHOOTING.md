# Troubleshooting

## Config parse fails
Run `aicli config show --json` to see error. Compare with `assets/default-config.toml`.

## No color or broken borders
Set `TERM=xterm-256color` or run with `--plain`. Check `NO_COLOR` is unset for styled mode.

## History not saved
Check profile dir perms and `history.txt` path from `aicli doctor`.

## Index shows zero files
Check ext filter in config and `.gitignore`. Run `aicli index <path> --rebuild`.

## Slow output in pipes
Piped stdout auto disables color. Use `--json` for scripts.
