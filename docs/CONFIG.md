# Config

Precedence: defaults, file `~/.config/aicli/config.toml`, env `AICLI_*`, CLI flags.
Run `aicli config show` and `aicli config show --json` to inspect.
Sample: see `assets/default-config.toml`.
Env: `AICLI_MODEL`, `AICLI_CTX`, `AICLI_THEME`, `AICLI_PROFILE`.
Validation: n_ctx in 1024 2048 4096 8192 16384 32768, temp 0.0 to 2.0, theme auto dark light plain.
