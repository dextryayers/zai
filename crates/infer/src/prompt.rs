use serde::{Deserialize, Serialize};

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

/// Build prompt with overflow policy: drop oldest chunk first, then middle turns.
/// history: oldest first list of (role, content). chunks: retrieved sources.
pub fn build_prompt(
    system: &str,
    chunks: &[String],
    history: &[(String, String)],
    input: &str,
    n_ctx: u32,
) -> PromptUsage {
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
            history: hist_tokens.min(hist_cap),
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
    fn keeps_first_and_last6() {
        let hist: Vec<(String, String)> = (0..50)
            .map(|i| ("user".to_string(), format!("q{i}")))
            .collect();
        let u = build_prompt("sys", &[], &hist, "now", 4096);
        assert!(u.prompt.contains("q0"));
        assert!(!u.overflow || u.estimated_tokens > 0);
    }
}
