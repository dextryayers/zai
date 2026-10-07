/// Identity: who is Zai. Answered from the knowledge base
/// (`assets/knowledge/identity.md` + `capabilities.md`), composed around the
/// live query focus - never a fixed reply block. Output is English-first with
/// an Indonesian identity line built dynamically from facts, so both
/// "who are you" and "kamu siapa" get a correct professional answer.
pub fn is_identity_query(query: &str) -> bool {
    super::understand::understand(query).intent == super::understand::Intent::Identity
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Developer,
    Capability,
    General,
}

fn focus_of(query: &str) -> Focus {
    let q = query.to_lowercase();
    if q.contains("pembuat")
        || q.contains("pengembang")
        || q.contains("developer")
        || q.contains("dibuat")
        || q.contains("dikembangkan")
        || q.contains("creator")
        || q.contains("hanif")
        || q.contains("who made")
        || q.contains("who created")
        || q.contains("who developed")
    {
        Focus::Developer
    } else if q.contains("bisa apa")
        || q.contains("fitur")
        || q.contains("mampu")
        || q.contains("what can you")
        || q.contains("capabilit")
    {
        Focus::Capability
    } else {
        Focus::General
    }
}

/// Compose the identity answer from knowledge facts plus query focus.
pub fn answer(query: &str) -> String {
    let facts = crate::knowledge::identity_facts();
    let caps = crate::knowledge::capabilities();
    let start = crate::knowledge::quickstart();
    let focus = focus_of(query);
    let short_q: String = query.chars().take(80).collect();
    let id_line = format!(
        "Saya Zai yang dikembangkan oleh {}, anak muda Teknik Informatika.",
        facts.developer
    );

    let mut out = String::from("## I am Zai\n\n");
    out.push_str(&facts.tagline);
    out.push('\n');
    // Always include the requested Indonesian identity line dynamically
    // composed from knowledge facts (not a pasted constant), plus English.
    out.push_str(&format!("\n{id_line}\n"));
    match focus {
        Focus::Developer => {
            out.push_str(&format!(
                "\nYou asked: _{short_q}_.\n\nMy developer is **{}** - a {} building Zai as a {}: {}.\n",
                facts.developer,
                facts.developer_role,
                facts.role,
                facts.runtime
            ));
        }
        Focus::Capability => {
            out.push_str(&format!(
                "\nYou asked: _{short_q}_.\n\nI am a {}. I work end to end:\n",
                facts.role
            ));
        }
        Focus::General => {
            out.push_str(&format!(
                "\nAnswering: _{short_q}_.\n\nI run offline in your terminal.\n"
            ));
        }
    }
    out.push_str("\n### What I do\n\n");
    for (title, body) in &caps {
        let one_line: String = body.lines().collect::<Vec<_>>().join(" ");
        out.push_str(&format!("- **{title}**: {one_line}\n"));
    }
    if !start.is_empty() {
        out.push_str(&format!("\n{start}\n"));
    }
    out.push_str("\nJust describe what you want in plain language - I build it end to end.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_identity() {
        assert!(is_identity_query("who are you?"));
        assert!(is_identity_query("who developed you?"));
        assert!(is_identity_query("siapa yang mengembangkan kamu?"));
        assert!(is_identity_query("what can you do?"));
    }

    #[test]
    fn rejects_non_identity() {
        assert!(!is_identity_query("create a fibonacci function"));
        assert!(!is_identity_query("how much is 2+3?"));
        assert!(!is_identity_query(""));
    }

    #[test]
    fn answer_is_english_with_facts() {
        let a = answer("who are you?");
        assert!(a.contains("I am Zai"));
        assert!(a.contains("Hanif Abdurrohim"));
        assert!(a.contains("who are you?"));
        // Bilingual dynamic identity: English tagline plus Indonesian line
        // composed from knowledge facts.
        assert!(a.contains("Saya Zai yang dikembangkan oleh Hanif Abdurrohim"));
        let d = answer("who developed you?");
        assert!(d.contains("Hanif Abdurrohim"));
        let id = answer("kamu siapa?");
        assert!(id.contains("Saya Zai yang dikembangkan oleh"));
    }
}
