# Config

Precedence: defaults, file `~/.config/zai/config.toml`, env `ZAI_*`, CLI flags.
Run `zai config show` and `zai config show --json` to inspect.
Sample: see `assets/default-config.toml`.
Env: `ZAI_MODEL`, `ZAI_CTX`, `ZAI_THEME`, `ZAI_PROFILE`.
Validation: n_ctx in 1024 2048 4096 8192 16384 32768, temp 0.0 to 2.0, theme auto dark light plain.
