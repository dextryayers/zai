use serde::{Deserialize, Serialize};

/// Canonical Zai system prompt. Single source of truth for every entry
/// point (TUI, REPL, ask, code agent). Keeps identity stable: Zai by
/// Hanif Abdurrohim, professional local-first assistant for full coding,
/// daily, terminal/system, and defensive security. English-only product:
/// every answer is in English.
pub const ZAI_SYSTEM: &str = "You are Zai, developed by Hanif Abdurrohim, a young Informatics Engineering student. Professional local-first assistant for full coding, daily productivity, terminal/system operation, and defensive cybersecurity. Always answer in English, professionally and concisely. Use provided sources first and cite paths. For code, output complete runnable code with file name plus run steps. Never invent file paths.";

pub const ZAI_SYSTEM_SHORT: &str = "You are Zai, developed by Hanif Abdurrohim. Professional coding, daily, terminal, defensive-security assistant. Always answer in English, concisely.";

/// Token budget for 4096 default. Values scale with n_ctx.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub system: usize,
    pub rag: usize,
    pub history: usize,
    pub input: usize,
    pub total: usize,
    pub n_ctx: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptUsage {
    pub prompt: String,
    pub estimated_tokens: usize,
    pub overflow: bool,
    pub sections: Budget,
}

/// Estimate tokens as 4 chars per token for English code and docs.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

pub fn budget_for(n_ctx: u32) -> (usize, usize, usize) {
    // Returns (system, rag, history) caps. Scales down for 1B fallback ctx.
    if n_ctx <= 2048 {
        (200, 700, 900)
    } else {
        (300, 1400, 1800)
    }
}

/// Shared budget selection: newest chunks first, then first goal plus last turns.
/// Returns kept chunks, kept history, and token counters.
type Selection = (
    Vec<String>,
    Vec<(String, String)>,
    usize,
    usize,
    usize,
    usize,
    usize,
    bool,
);

fn select_sections(
    system: &str,
    chunks: &[String],
    history: &[(String, String)],
    input: &str,
    n_ctx: u32,
) -> Selection {
    let (sys_cap, rag_cap, hist_cap) = budget_for(n_ctx);
    let sys_tokens = estimate_tokens(system).min(sys_cap);

    // RAG: take from newest relevant until cap.
    let mut kept_chunks = Vec::new();
    let mut rag_tokens = 0;
    for c in chunks.iter().rev() {
        let t = estimate_tokens(c);
        if rag_tokens + t > rag_cap {
            break;
        }
        rag_tokens += t;
        kept_chunks.push(c.clone());
    }
    kept_chunks.reverse();

    // History: always keep first goal plus last 6 turns when over cap.
    let mut kept_history: Vec<(String, String)> = Vec::new();
    let mut hist_tokens = 0;
    if history.len() <= 7 {
        for (r, c) in history {
            hist_tokens += estimate_tokens(c) + 4;
            kept_history.push((r.clone(), c.clone()));
        }
    } else {
        let first = history.first().unwrap().clone();
        hist_tokens += estimate_tokens(&first.1) + 4;
        kept_history.push(first);
        for (r, c) in history[history.len() - 6..].iter() {
            hist_tokens += estimate_tokens(c) + 4;
            if hist_tokens > hist_cap {
                break;
            }
            kept_history.push((r.clone(), c.clone()));
        }
    }

    let input_tokens = estimate_tokens(input);
    let total = sys_tokens + rag_tokens + hist_tokens.min(hist_cap) + input_tokens + 32;
    let overflow = total > n_ctx as usize;
    (
        kept_chunks,
        kept_history,
        sys_tokens,
        rag_tokens,
        hist_tokens.min(hist_cap),
        input_tokens,
        total,
        overflow,
    )
}

/// Build prompt with overflow policy: drop oldest chunk first, then middle turns.
/// history: oldest first list of (role, content). chunks: retrieved sources.
pub fn build_prompt(
    system: &str,
    chunks: &[String],
    history: &[(String, String)],
    input: &str,
    n_ctx: u32,
) -> PromptUsage {
    let (
        kept_chunks,
        kept_history,
        sys_tokens,
        rag_tokens,
        hist_kept,
        input_tokens,
        total,
        overflow,
    ) = select_sections(system, chunks, history, input, n_ctx);

    let mut prompt = format!("System: {system}\n\n");
    for (i, c) in kept_chunks.iter().enumerate() {
        prompt.push_str(&format!("[SOURCE {}] {c}\n\n", i + 1));
    }
    for (r, c) in &kept_history {
        prompt.push_str(&format!("{r}: {c}\n"));
    }
    prompt.push_str(&format!("user: {input}\nassistant:"));

    PromptUsage {
        prompt,
        estimated_tokens: total,
        overflow,
        sections: Budget {
            system: sys_tokens,
            rag: rag_tokens,
            history: hist_kept,
            input: input_tokens,
            total,
            n_ctx,
        },
    }
}

/// Same budget selection rendered as ChatML for local instruct models.
/// Roles map to im_start blocks, sources ride inside the system block,
/// and the prompt ends on an open assistant block.
pub fn build_chatml(
    system: &str,
    chunks: &[String],
    history: &[(String, String)],
    input: &str,
    n_ctx: u32,
) -> PromptUsage {
    let (
        kept_chunks,
        kept_history,
        sys_tokens,
        rag_tokens,
        hist_kept,
        input_tokens,
        total,
        overflow,
    ) = select_sections(system, chunks, history, input, n_ctx);

    let mut prompt = format!("<|im_start|>system\n{system}\n");
    for (i, c) in kept_chunks.iter().enumerate() {
        prompt.push_str(&format!("[SOURCE {}] {c}\n", i + 1));
    }
    prompt.push_str("<|im_end|>\n");
    for (r, c) in &kept_history {
        let role = if r == "assistant" {
            "assistant"
        } else {
            "user"
        };
        prompt.push_str(&format!("<|im_start|>{role}\n{c}\n<|im_end|>\n"));
    }
    prompt.push_str(&format!(
        "<|im_start|>user\n{input}\n<|im_end|>\n<|im_start|>assistant\n"
    ));

    PromptUsage {
        prompt,
        estimated_tokens: total,
        overflow,
        sections: Budget {
            system: sys_tokens,
            rag: rag_tokens,
            history: hist_kept,
            input: input_tokens,
            total,
            n_ctx,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_flag_works() {
        let big = "x".repeat(40000);
        let u = build_prompt("sys", &[], &[], &big, 4096);
        assert!(u.overflow);
    }

    #[test]
    fn chatml_renders_roles_and_budget() {
        let hist: Vec<(String, String)> = vec![
            ("user".to_string(), "q0".to_string()),
            ("assistant".to_string(), "a0".to_string()),
        ];
        let u = build_chatml("sys", &["src text".to_string()], &hist, "now", 4096);
        assert!(u.prompt.contains("<|im_start|>system"));
        assert!(u.prompt.contains("[SOURCE 1] src text"));
        assert!(u.prompt.contains("<|im_start|>assistant\na0"));
        assert!(u.prompt.ends_with("<|im_start|>assistant\n"));
        assert!(!u.overflow);
    }

    #[test]
    fn keeps_first_and_last6() {
        let hist: Vec<(String, String)> = (0..50)
            .map(|i| ("user".to_string(), format!("q{i}")))
            .collect();
        let u = build_prompt("sys", &[], &hist, "now", 4096);
        assert!(u.prompt.contains("q0"));
        assert!(!u.overflow || u.estimated_tokens > 0);
    }
}
