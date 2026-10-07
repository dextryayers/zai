# Defensive security playbooks

> Professional scope: only test systems you own or have written authorization for.

## sqli
### SQL injection - close and verify

1. Use bound parameters for every query, never string-concat SQL.
2. Least-privilege DB user per service.
3. Quick check: `rg -n "SELECT.*\+" src` then fix each hit.
4. Example fix (Python): `cursor.execute("SELECT * FROM users WHERE id=%s", (uid,))`.
5. Alert on `' OR '1'='1` patterns in the WAF.

## xss
### XSS - close and verify

1. Escape on output, never `innerHTML` with user data.
2. Headers: `Content-Security-Policy` plus `HttpOnly; SameSite=Lax` cookies.
3. Check: `rg -n "innerHTML|dangerouslySetInnerHTML" src`.
4. Test: inject `<img src=x onerror=alert(1)>` in your own staging, confirm it is escaped.

## csrf
### CSRF

1. `SameSite=Lax` cookies plus a per-session CSRF token on mutations.
2. Verify the `Origin` header on state-changing endpoints.
3. Test: send a cross-origin POST in the lab, confirm 403.

## passwords
### Password storage

1. Hash with Argon2id or bcrypt cost 12, never plain MD5/SHA1.
2. Unique salt per password, pepper in env.
3. Rate-limit logins, lock after 5 failures, MFA for admins.

## secrets
### Secret handling

1. Move secrets to env or vault, never commit. Scan: `rg -n "AKIA|BEGIN PRIVATE KEY" .`.
2. Rotate on any suspected leak, revoke old keys first.
3. Pre-commit secret scanner plus `zai run -- gitleaks detect --source .`.

## ssh
### SSH hardening

1. Keys only: `PasswordAuthentication no`, `PermitRootLogin no`.
2. fail2ban on, custom port is obscurity only.
3. Verify: `sshd -T | rg -i "passwordauth|permitroot"`.
4. Audit keys: `awk '{print $NF}' ~/.ssh/authorized_keys`.

## tls
### TLS

1. TLS 1.2 or newer only, HSTS, auto-renew with certbot timer.
2. Verify: `curl -sSI https://domain | rg -i strict` plus monthly expiry checks.
3. Lab: `nmap --script ssl-enum-ciphers -p 443 domain` on your own host.

## phishing
### Phishing defense

1. Check sender domain headers, hover links, never open unknown attachments.
2. Report to IT, MFA everywhere.
3. Regular internal drills with one-click reporting.

## malware
### Malware response

1. Isolate the host from the network first, snapshot disk, scan offline.
2. Restore from tested clean backup, rotate credentials used on the host.
3. Hunt persistence: `crontab -l`, `systemctl list-timers`, autoruns.

## firewall
### Firewall baseline

1. Default deny inbound: `ufw default deny incoming`.
2. Open only needed ports, review quarterly: `ufw status numbered`.
3. Ship drop logs to a central log host.

## pentest
### Professional pentest workflow (your own systems)

1. Written scope: hosts, time window, forbidden actions.
2. Recon: `nmap -sV --top-ports 100 TARGET` then `nmap -sC -sV -p <ports> TARGET`.
3. Web: `nuclei -u https://TARGET`, `nikto -h https://TARGET`, `ffuf -u https://TARGET/FUZZ -w wordlist`.
4. Map to OWASP Top 10, score impact times likelihood.
5. Report: repro steps plus evidence plus fix plus retest after patch.
6. Run via Zai: `zai run -- nmap -sV --top-ports 100 127.0.0.1`.

## auth
### Auth and sessions

1. Short-lived access tokens, rotating refresh tokens, server-side session list.
2. MFA for privileged roles, impossible-travel login alerts.
3. JWT: short exp, validated aud/iss, vault-stored secrets, regular rotation.

## linux
### Linux hardening

1. Weekly updates, remove unneeded services/ports: `ss -tulpn`.
2. `lynis audit system`, then chase warnings.
3. Audit sudo: `visudo`, `grep -R NOPASSWD /etc/sudoers.d/`.
4. 3-2-1 backups, restore tested.

## docker
### Container baseline

1. Minimal images, non-root user, read-only FS where possible.
2. Scan: `trivy image <img>` or `grype <img>`.
3. Never mount docker.sock into workloads, secrets via env-file or vault.

## git
### Git leak cleanup

1. Scan: `rg -n "AKIA|BEGIN PRIVATE KEY" .` plus `gitleaks detect --source .`.
2. Rotate first, then clean history (`git filter-repo`), coordinated force-push.
3. Prevent: pre-commit hook plus secret scanner in CI.

## general
### General hardening

1. Weekly OS and dependency updates, remove unused services/ports (`ss -tulpn`).
2. Tested backups (3 copies, 1 offline), least privilege, MFA on admins.
3. Ship logs off-host, review quarterly.
4. Ask narrower, e.g. `sql injection in login form`, for a focused checklist.
