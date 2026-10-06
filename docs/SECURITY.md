# Security

Threat model: single user laptop, malicious prompt must not escape root, run arbitrary shell, or exfiltrate secrets. No network at runtime except models pull.

Controls with tests:

- SEC-01 Path sandbox: canonicalize plus root prefix plus `..` reject. Test `fs::denies_escape` and `patch::rejects_escape`. Exit 5 on violation.
- SEC-02 Secret redact: `.env`, `id_rsa`, `*.pem`, `*.key`, `credentials.json` return stub. Test `fs::redacts_secrets`.
- SEC-03 Shell gate: allowlist exact plus prefix, denylist wins, pipe from curl or wget to sh always denied. Test `shell::deny_matrix` with 9 cases. `run` and `code verify` share `run_blocking`.
- SEC-04 Patch atomic: temp plus rename, backup `.zai.bak.ts`, hunk context verify. Test `patch::atomic_apply_all_or_nothing`.
- SEC-05 Log hygiene: events truncate at 2000 chars, run preview at 4000 chars, full log to `logs/run.log`, no secret content. Manual review in PR.
- SEC-06 File perms: data dir 0700, history 0600 intent, db via sqlite. Check `ls -la` in release test.
- SEC-07 Supply chain: locked deps, `deny.toml` bans GPL, CI runs audit intent plus dash lint. Review quarterly.
- SEC-08 Offline proof: `scripts/offline-test.sh` runs doctor, models list, ask, tasks, daily, index with no model. Zero egress for those commands by design, only `models pull` uses network.

Privacy: prompts stay on device. Only `models pull` fetches from Hugging Face. No telemetry. Log path shown in error cards.
