# Security

Sandbox root is --path or cwd. Reads and writes outside root are denied.
Secret filenames return redacted stub: .env, *.pem, id_rsa, credentials.json.
Shell denylist wins: rm -rf, sudo, mkfs, curl pipe to sh.
Allowlist is explicit in config and shown by doctor and code screen.
Logs exclude secret content. DB and history files are 600 on Unix.
Offline proof: scripts/offline-test.sh runs core commands with blocked egress.
