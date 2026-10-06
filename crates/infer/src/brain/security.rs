/// Defensive security guidance. Educational checklists, no exploit code.
/// Each topic maps to concrete hardening steps with safe verification commands.

/// Return the matched topic id for a query, if any.
pub fn topic(query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();
    let topics: &[(&str, &[&str])] = &[
        ("sqli", &["sql injection", "sqli", "sqlinject"]),
        ("xss", &["xss", "cross site scripting", "cross-site scripting"]),
        ("csrf", &["csrf", "cross site request forgery"]),
        ("passwords", &["password", "hashing", "bcrypt", "argon2", "credential"]),
        ("secrets", &["secret", "api key", "apikey", "leak", "env file", ".env"]),
        ("ssh", &["ssh", "sshd", "key auth"]),
        ("tls", &["tls", "ssl", "https", "certificate", "certbot"]),
        ("phishing", &["phishing", "phish", "social engineering"]),
        ("malware", &["malware", "ransomware", "virus", "trojan"]),
        ("firewall", &["firewall", "iptables", "ufw", "firewalld"]),
        ("pentest", &["pentest", "penetration", "nmap", "port scan", "metasploit", "burp", "owasp", "vulnerability scan", "wireshark"]),
        ("auth", &["jwt", "oauth", "session fixation", "2fa", "mfa", "authentication"]),
    ];
    for (id, keys) in topics {
        if keys.iter().any(|k| q.contains(k)) {
            return Some(id);
        }
    }
    None
}

pub fn advise(query: &str) -> String {
    let mut out = String::from("## Defensive security guidance\n\n");
    out.push_str("_Educational use. Test only systems you own or are hired to test._\n");
    match topic(query).unwrap_or("general") {
        "sqli" => out.push_str(
            "\n### SQL injection\n\n- Use bound parameters everywhere, never string concat SQL.\n- Least privilege DB user per service, read only where possible.\n- Validate with safe check: `rg -n \"SELECT.*\\+\" src` then fix each hit.\n- Log slow queries and alert on `' OR '1'='1` patterns in WAF.\n",
        ),
        "xss" => out.push_str(
            "\n### Cross site scripting\n\n- Escape on output, use framework templating, never innerHTML with user data.\n- Set `Content-Security-Policy` and `HttpOnly` plus `SameSite` cookies.\n- Safe check: `rg -n \"innerHTML|dangerouslySetInnerHTML\" src`.\n",
        ),
        "csrf" => out.push_str(
            "\n### CSRF\n\n- SameSite=Lax cookies plus per session CSRF token on mutations.\n- Verify Origin header on state changing endpoints.\n",
        ),
        "passwords" => out.push_str(
            "\n### Password storage\n\n- Hash with Argon2id or bcrypt cost 12, never MD5 or SHA1 alone.\n- Unique salt per password, pepper in env or HSM.\n- Rate limit logins, lock after 5 fails, require MFA for admins.\n",
        ),
        "secrets" => out.push_str(
            "\n### Secret handling\n\n- Move secrets to env or vault, never commit. Scan history: `rg -n \"AKIA|BEGIN PRIVATE KEY\" .`.\n- Rotate on any leak suspicion, revoke old keys before deploying new ones.\n- Pre commit hook with a secret scanner.\n",
        ),
        "ssh" => out.push_str(
            "\n### SSH hardening\n\n- Disable password auth, keys only: `PasswordAuthentication no`.\n- Disable root login, change port only as obscurity layer, use fail2ban.\n- Verify: `sshd -T | rg -i \"passwordauth|permitroot\"`.\n",
        ),
        "tls" => out.push_str(
            "\n### TLS\n\n- TLS 1.2 or newer only, HSTS header, auto renew with certbot timer.\n- Verify: `curl -sSI https://domain | rg -i strict` and check expiry monthly.\n",
        ),
        "phishing" => out.push_str(
            "\n### Phishing defense\n\n- Verify sender domain headers, hover links, never open unexpected attachments.\n- Report to IT, MFA everywhere so one password never suffices.\n",
        ),
        "malware" => out.push_str(
            "\n### Malware response\n\n- Isolate host from network first, snapshot disk, then scan offline.\n- Restore from known clean backup, rotate credentials entered on the host.\n",
        ),
        "firewall" => out.push_str(
            "\n### Firewall baseline\n\n- Default deny inbound, allow only needed ports, e.g. `ufw default deny incoming`.\n- Review rules quarterly: `ufw status numbered`.\n",
        ),
        "pentest" => out.push_str(
            "\n### Pentest workflow (own scope only)\n\n1. Scope in writing: hosts, time window, forbidden actions.\n2. Recon: `nmap -sV --top-ports 100 target` on your own host.\n3. Map findings to OWASP Top 10, rate by impact times likelihood.\n4. Report with repro steps plus fix, retest after patch.\n",
        ),
        "auth" => out.push_str(
            "\n### Auth sessions\n\n- Short lived access tokens, rotating refresh tokens, server side session list.\n- MFA for privileged roles, alert on impossible travel logins.\n",
        ),
        _ => out.push_str(
            "\n### General hardening\n\n- Update OS and deps weekly, remove unused services and ports.\n- Backups tested by restore, 3 copies, 1 offline.\n- Least privilege users, MFA on admins, audit logs shipped off host.\n- Ask a narrower topic like `sql injection in login form` for a focused checklist.\n",
        ),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_topics() {
        assert_eq!(topic("how to prevent sql injection in login"), Some("sqli"));
        assert_eq!(topic("harden ssh server"), Some("ssh"));
        assert_eq!(topic("what is rust borrow checker"), None);
    }

    #[test]
    fn playbook_has_steps() {
        let p = advise("xss in comments");
        assert!(p.contains("Content-Security-Policy"));
    }
}
