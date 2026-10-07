/// Deep query understanding: scored intent classification, entity
/// extraction, complexity estimate, execution plan, and clarification needs.
/// This is the trainable core of the offline brain. Scores come from weighted
/// evidence (not single-substring matches), entities are extracted from the
/// live query, and every answer is composed from this struct plus knowledge
/// data plus model output - never a fixed reply block.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    Identity,
    Math,
    Code,
    CodeGen,
    Security,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Complexity {
    Simple,
    Medium,
    Complex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Understanding {
    pub intent: Intent,
    pub confidence: u8,
    pub scores: Vec<(String, u8)>,
    pub languages: Vec<String>,
    pub entities: Vec<String>,
    pub complexity: Complexity,
    pub plan: Vec<String>,
    pub needs_clarification: bool,
    pub clarification: String,
}

const CODE_LANGUAGES: &[&str] = &[
    "rust",
    "python",
    "javascript",
    "typescript",
    "go",
    "bash",
    "sql",
    "java",
    "toml",
    "json",
];

fn count_hits(q: &str, keys: &[&str]) -> u8 {
    keys.iter().filter(|k| q.contains(*k)).count().min(20) as u8
}

/// Score every intent with weighted evidence, pick the winner with a
/// confidence rating. Ties resolve in product-priority order.
pub fn understand(query: &str) -> Understanding {
    let q = query.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();

    let identity_score = count_hits(
        &q,
        &[
            "who are you",
            "who r u",
            "your developer",
            "your creator",
            "who made you",
            "who created you",
            "who developed you",
            "kamu siapa",
            "siapa kamu",
            "siapakah kamu",
            "nama kamu",
            "perkenalkan diri",
            "tentang kamu",
            "tentang dirimu",
            "tentang zai",
            "zai itu apa",
            "apa itu zai",
            "siapa pembuat",
            "yang membuat",
            "yang mengembangkan",
            "dibuat oleh",
            "dikembangkan oleh",
            "pembuatmu",
            "pengembangmu",
            "developer kamu",
            "creator",
            "hanif",
            "what can you do",
            "kamu bisa apa",
            "bisa apa saja",
            "fitur kamu",
        ],
    )
    .saturating_mul(16)
    .min(95);

    let math_hit = crate::brain::math::extract_expr(query).is_some();
    let math_score = if math_hit { 90 } else { 0 };

    let code_score = if crate::brain::code::looks_like_code(query) {
        88
    } else {
        0
    };

    let codegen_score = count_hits(
        &q,
        &[
            "buatkan",
            "buatin",
            "bikin",
            "tuliskan",
            "write",
            "make",
            "create",
            "generate",
            "scaffold",
            "implement",
            "perbaiki",
            "fix",
            "bug",
            "debug",
            "tambahkan",
            "refactor",
            "fibonacci",
            "factorial",
            "faktorial",
            "sorting",
            "binary search",
            "crud",
            "rest api",
            "http server",
            "login",
            "auth",
            "endpoint",
            "function",
            "fungsi",
            "program",
            "script",
        ],
    )
    .saturating_mul(9)
    .min(90);

    let security_score = count_hits(
        &q,
        &[
            "sql injection",
            "injection",
            "sqli",
            "xss",
            "csrf",
            "password",
            "hashing",
            "bcrypt",
            "argon2",
            "secret",
            "api key",
            "ssh",
            "harden",
            "tls",
            "ssl",
            "phishing",
            "malware",
            "firewall",
            "pentest",
            "penetration",
            "nmap",
            "scan",
            "metasploit",
            "burp",
            "owasp",
            "vulnerability",
            "wireshark",
            "nikto",
            "nuclei",
            "jwt",
            "oauth",
            "lynis",
            "gitleaks",
            "keamanan",
            "cyber",
        ],
    )
    .saturating_mul(8)
    .min(92);

    let mut ranked: Vec<(Intent, u8)> = vec![
        (Intent::Identity, identity_score),
        (Intent::Math, math_score),
        (Intent::Code, code_score),
        (Intent::CodeGen, codegen_score),
        (Intent::Security, security_score),
    ];
    ranked.sort_by_key(|a| std::cmp::Reverse(a.1));
    let (intent, confidence) = if ranked[0].1 >= 12 {
        ranked[0]
    } else {
        (Intent::General, 30)
    };

    let languages: Vec<String> = CODE_LANGUAGES
        .iter()
        .filter(|l| q.contains(**l))
        .map(|s| s.to_string())
        .collect();

    let mut entities: Vec<String> = Vec::new();
    for w in &words {
        let clean: String = w
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '/')
            .to_string();
        if clean.contains('/') && clean.len() > 3 {
            entities.push(format!("path:{clean}"));
        } else if clean.contains('.') && clean.len() > 4 && !clean.parse::<f64>().is_ok() {
            entities.push(format!("file:{clean}"));
        }
    }
    for tok in q.split_whitespace() {
        if let Ok(n) = tok
            .trim_matches(|c: char| !c.is_alphanumeric())
            .parse::<f64>()
        {
            if n.fract() == 0.0 {
                entities.push(format!("number:{n}"));
                if entities.len() > 6 {
                    break;
                }
            }
        }
    }

    let complexity = if words.len() > 40 || entities.len() > 4 {
        Complexity::Complex
    } else if words.len() > 12 || entities.len() > 1 {
        Complexity::Medium
    } else {
        Complexity::Simple
    };

    let plan = plan_for(intent, &languages, &entities);
    let (needs_clarification, clarification) = match intent {
        Intent::CodeGen if languages.is_empty() => (
            false,
            "No language named; defaulting to Python. Name one (rust, go, bash) to change it."
                .to_string(),
        ),
        Intent::General if words.len() < 4 => (
            true,
            "Short request. Add the goal plus a target language or file path for a precise answer."
                .to_string(),
        ),
        _ => (false, String::new()),
    };

    Understanding {
        intent,
        confidence,
        scores: ranked
            .into_iter()
            .map(|(i, s)| (format!("{i:?}").to_lowercase(), s))
            .collect(),
        languages,
        entities,
        complexity,
        plan,
        needs_clarification,
        clarification,
    }
}

fn plan_for(intent: Intent, languages: &[String], entities: &[String]) -> Vec<String> {
    match intent {
        Intent::Identity => vec!["state identity from knowledge facts".to_string()],
        Intent::Math => vec!["evaluate expression safely".to_string()],
        Intent::Code => vec![
            "detect language".to_string(),
            "count structure".to_string(),
            "flag risks".to_string(),
        ],
        Intent::CodeGen => {
            let lang = languages.first().cloned().unwrap_or("python".to_string());
            vec![
                format!("synthesize {lang} implementation"),
                "add run instructions".to_string(),
                "propose repo apply via patch".to_string(),
            ]
        }
        Intent::Security => vec![
            "match playbook topic".to_string(),
            "give verify commands".to_string(),
            "note authorized scope".to_string(),
        ],
        Intent::General => {
            if entities.is_empty() {
                vec!["route to connected model".to_string()]
            } else {
                vec![
                    "retrieve local context".to_string(),
                    "route to connected model".to_string(),
                ]
            }
        }
    }
}

/// One-line summary for `--show-budget` and `/understand` output.
pub fn summarize(u: &Understanding) -> String {
    format!(
        "intent {:?} (confidence {} pct) | langs [{}] | entities [{}] | complexity {:?} | plan: {}",
        u.intent,
        u.confidence,
        u.languages.join(", "),
        u.entities.join(", "),
        u.complexity,
        u.plan.join(" -> ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_wins_with_confidence() {
        let u = understand("who are you?");
        assert_eq!(u.intent, Intent::Identity);
        assert!(u.confidence >= 16);
    }

    #[test]
    fn math_detected() {
        let u = understand("calculate 2+3*4");
        assert_eq!(u.intent, Intent::Math);
    }

    #[test]
    fn codegen_extracts_language() {
        let u = understand("create a REST API in rust");
        assert_eq!(u.intent, Intent::CodeGen);
        assert!(u.languages.contains(&"rust".to_string()));
    }

    #[test]
    fn security_detected() {
        let u = understand("how to prevent sql injection in login");
        assert_eq!(u.intent, Intent::Security);
    }

    #[test]
    fn vague_falls_to_general_with_clarification() {
        let u = understand("help me");
        assert_eq!(u.intent, Intent::General);
        assert!(u.needs_clarification);
    }

    #[test]
    fn entities_found() {
        let u = understand("review src/main.rs line 42");
        assert!(u.entities.iter().any(|e| e.contains("main.rs")));
    }
}
