/// Defensive security guidance composed from the knowledge base
/// (`assets/knowledge/security.md`). Topic matching still recognizes
/// Indonesian keywords for routing; every playbook is English. No exploit
/// payloads, only hardening, detection, and fix workflows with copy-paste
/// verification commands, plus a scope note.
pub fn topic(query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();
    let topics: &[(&str, &[&str])] = &[
        (
            "sqli",
            &["sql injection", "sqli", "sqlinject", "injeksi sql"],
        ),
        (
            "xss",
            &["xss", "cross site scripting", "cross-site scripting"],
        ),
        ("csrf", &["csrf", "cross site request forgery"]),
        (
            "passwords",
            &[
                "password",
                "hashing",
                "bcrypt",
                "argon2",
                "credential",
                "kata sandi",
            ],
        ),
        (
            "secrets",
            &[
                "secret", "api key", "apikey", "leak", "env file", ".env", "bocor",
            ],
        ),
        ("ssh", &["ssh", "sshd", "key auth"]),
        (
            "tls",
            &[
                "tls",
                "ssl",
                "https",
                "certificate",
                "certbot",
                "sertifikat",
            ],
        ),
        ("phishing", &["phishing", "phish", "social engineering"]),
        ("malware", &["malware", "ransomware", "virus", "trojan"]),
        ("firewall", &["firewall", "iptables", "ufw", "firewalld"]),
        (
            "pentest",
            &[
                "pentest",
                "penetration",
                "nmap",
                "scan",
                "port scan",
                "metasploit",
                "burp",
                "owasp",
                "vulnerability scan",
                "wireshark",
                "nikto",
                "nuclei",
                "ffuf",
                "gobuster",
                "sqlmap",
            ],
        ),
        (
            "auth",
            &[
                "jwt",
                "oauth",
                "session fixation",
                "2fa",
                "mfa",
                "authentication",
                "autentikasi",
            ],
        ),
        (
            "linux",
            &[
                "linux hardening",
                "harden linux",
                "harden ubuntu",
                "audit linux",
                "lynis",
            ],
        ),
        (
            "docker",
            &["docker", "container security", "keamanan container"],
        ),
        (
            "git",
            &["git leak", "git history", "gitleaks", "trufflehog"],
        ),
    ];
    for (id, keys) in topics {
        if keys.iter().any(|k| q.contains(k)) {
            return Some(id);
        }
    }
    if q.contains("keamanan") || q.contains("cyber") || q.contains("hacker") || q.contains("retas")
    {
        return Some("general");
    }
    None
}

pub fn advise(query: &str) -> String {
    let id = topic(query).unwrap_or("general");
    let mut out = String::from("## Defensive Security Guide\n\n");
    let body = crate::knowledge::security_section(id)
        .or_else(|| crate::knowledge::security_section("general"))
        .unwrap_or_default();
    out.push_str(&body);
    let scope = crate::knowledge::security_scope();
    // Knowledge file already contains the full scope sentence, avoid
    // "Professional scope: Professional scope:" duplication.
    if scope.to_lowercase().starts_with("professional scope") {
        out.push_str(&format!("\n\n> {scope}\n"));
    } else {
        out.push_str(&format!("\n\n> Professional scope: {scope}\n"));
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
        assert_eq!(topic("nmap scan tutorial"), Some("pentest"));
        assert_eq!(topic("what is rust borrow checker"), None);
    }

    #[test]
    fn playbook_is_english_with_scope() {
        let p = advise("xss in comments");
        assert!(p.contains("Content-Security-Policy"));
        assert!(p.contains("Professional scope"));
        assert!(!p.contains("Keamanan Siber"));
    }
}
