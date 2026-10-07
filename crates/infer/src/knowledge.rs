/// Knowledge base: product facts loaded from data files, not hardcoded in
/// logic. `assets/knowledge/*.md` is compiled into the binary via
/// `include_str!`, so the installer rebuilds `zai` whenever knowledge
/// changes. Brain modules compose answers from these facts plus the live
/// query understanding - no fixed reply blocks.
const IDENTITY_MD: &str = include_str!("../../../assets/knowledge/identity.md");
const CAPABILITIES_MD: &str = include_str!("../../../assets/knowledge/capabilities.md");
const SECURITY_MD: &str = include_str!("../../../assets/knowledge/security.md");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFacts {
    pub name: String,
    pub developer: String,
    pub developer_role: String,
    pub role: String,
    pub runtime: String,
    pub tagline: String,
}

fn field(md: &str, key: &str) -> String {
    for line in md.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&format!("{key}:")) {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Parse identity facts from the knowledge file. Falls back to built-in
/// defaults only if the data file is damaged, so identity never breaks.
pub fn identity_facts() -> IdentityFacts {
    let get = |k: &str, d: &str| {
        let v = field(IDENTITY_MD, k);
        if v.is_empty() {
            d.to_string()
        } else {
            v
        }
    };
    IdentityFacts {
        name: get("name", "Zai"),
        developer: get("developer", "Hanif Abdurrohim"),
        developer_role: get("developer_role", "young Informatics Engineering student"),
        role: get("role", "local-first terminal assistant"),
        runtime: get("runtime", "single Rust binary, offline GGUF runtime"),
        tagline: get(
            "tagline",
            "I am Zai, developed by Hanif Abdurrohim, a young Informatics Engineering student.",
        ),
    }
}

/// Capability sections parsed from the knowledge file as (title, body).
pub fn capabilities() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut title = String::new();
    let mut body = String::new();
    for line in CAPABILITIES_MD.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if !title.is_empty() {
                out.push((title.clone(), body.trim().to_string()));
                body.clear();
            }
            title = h.trim().to_string();
        } else if !title.is_empty() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if !title.is_empty() {
        out.push((title, body.trim().to_string()));
    }
    out
}

/// Quickstart block after the `# Quickstart` marker in capabilities.
pub fn quickstart() -> String {
    let mut take = false;
    let mut out = String::new();
    for line in CAPABILITIES_MD.lines() {
        if line.trim() == "# Quickstart" {
            take = true;
            continue;
        }
        if take {
            if line.starts_with("# ") {
                break;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Security playbook body for a topic id (`sqli`, `xss`, ...).
/// Returns None for unknown topics so callers fall back to `general`.
pub fn security_section(topic: &str) -> Option<String> {
    sections(SECURITY_MD).remove(topic)
}

/// Scope note: first blockquote in the security knowledge file.
pub fn security_scope() -> String {
    for line in SECURITY_MD.lines() {
        let t = line.trim();
        if let Some(q) = t.strip_prefix("> ") {
            return q.to_string();
        }
    }
    "Only test systems you own or have written authorization for.".to_string()
}

fn sections(md: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut title = String::new();
    let mut body = String::new();
    for line in md.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if !title.is_empty() {
                map.insert(title.clone(), body.trim().to_string());
                body.clear();
            }
            title = h.trim().to_string();
        } else if !title.is_empty() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if !title.is_empty() {
        map.insert(title, body.trim().to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_facts_load() {
        let f = identity_facts();
        assert_eq!(f.name, "Zai");
        assert!(f.developer.contains("Hanif Abdurrohim"));
        assert!(!f.tagline.is_empty());
    }

    #[test]
    fn capabilities_parse() {
        let caps = capabilities();
        assert!(caps.len() >= 5);
        assert!(caps.iter().any(|(t, _)| t == "Full coding"));
    }

    #[test]
    fn security_sections_cover_topics() {
        for t in ["sqli", "xss", "pentest", "general"] {
            assert!(security_section(t).is_some(), "missing {t}");
        }
        assert!(security_section("nope").is_none());
        assert!(!security_scope().is_empty());
    }
}
