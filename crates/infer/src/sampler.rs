use serde::{Deserialize, Serialize};

/// Sampler config per mode. Exported in session export for repro.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    pub temp: f32,
    pub top_p: f32,
    pub seed: u64,
    pub mode: String,
}

impl SamplerConfig {
    pub fn chat(temp: Option<f32>, seed: u64) -> Self {
        Self {
            temp: temp.unwrap_or(0.6),
            top_p: 0.9,
            seed,
            mode: "chat".to_string(),
        }
    }

    pub fn code(temp: Option<f32>, seed: u64) -> Self {
        Self {
            temp: temp.unwrap_or(0.2),
            top_p: 0.95,
            seed,
            mode: "code".to_string(),
        }
    }

    pub fn deterministic() -> Self {
        Self {
            temp: 0.0,
            top_p: 1.0,
            seed: 42,
            mode: "eval".to_string(),
        }
    }
}

/// Deterministic mock answer. Same input plus same sampler gives byte identical output.
/// Used for repro test and for offline demo before real llama backend.
pub fn mock_answer_with_sampler(query: &str, sampler: &SamplerConfig) -> String {
    let short = truncate_query(query, 500);
    if query.trim().is_empty() {
        return "Ask me about code, daily tasks, or local files.".to_string();
    }
    if sampler.temp == 0.0 {
        return format!(
            "## Answer (seed {})\n\nYou asked: `{short}`\n\nDeterministic output for repro.\n\n```rust\nfn apply_patch() -> bool {{ true }}\n```\n",
            sampler.seed
        );
    }
    // Non deterministic path varies only by seed in wording order, still stable per seed.
    let variant = (sampler.seed % 3) + 1;
    format!(
        "## Answer v{variant}\n\nYou asked: `{short}`\n\nMock stream with temp {:.1}. Real GGUF inference lands behind `llama` feature.\n\n```rust\n// preview of coding agent output\nfn apply_patch() -> bool {{\n    true\n}}\n```\n\n- point one with `inline code`\n- point two with **bold**\n\nSources:\n- `crates/cli/src/main.rs:1-40` (score 0.81)\n",
        sampler.temp
    )
}

fn truncate_query(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    format!("{}... [{} chars truncated]", &s[..max], s.len() - max)
}

/// Legacy entry kept for Phase 1 callers.
pub fn mock_answer(query: &str) -> String {
    mock_answer_with_sampler(query, &SamplerConfig::chat(None, 7))
}

/// Stream string to stdout in word batches with flush cadence.
pub fn stream_mock_to_stdout(answer: &str, flush_ms: u64) -> anyhow::Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    let mut buf = String::new();
    for word in answer.split_inclusive([' ', '\n']) {
        buf.push_str(word);
        if buf.len() >= 24 {
            let _ = stdout.write_all(buf.as_bytes());
            let _ = stdout.flush();
            buf.clear();
            if flush_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(flush_ms.min(20)));
            }
        }
    }
    if !buf.is_empty() {
        let _ = stdout.write_all(buf.as_bytes());
        let _ = stdout.flush();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_seed_is_byte_identical() {
        let s = SamplerConfig::deterministic();
        let a = mock_answer_with_sampler("fix test", &s);
        let b = mock_answer_with_sampler("fix test", &s);
        assert_eq!(a, b);
    }
}
