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

/// Seed resolution: explicit flag wins, then a pinned nonzero config
/// seed, else fresh randomness per request. A fixed default seed freezes
/// sampling, so every identical prompt returns the identical answer with
/// identical quirks forever.
pub fn resolve_seed(flag: Option<u64>, cfg_seed: u64) -> u64 {
    if let Some(s) = flag {
        return s;
    }
    if cfg_seed != 0 {
        return cfg_seed;
    }
    random_seed()
}

/// Nondeterministic seed from clock plus pid. No rand crate needed.
pub fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    nanos ^ ((std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// Deterministic mock answer. Same input plus same sampler gives byte identical output.
/// Used for repro test and for offline demo before real llama backend.
/// English-only product language. Never emits code blocks or source
/// citations: those only come from real retrieval and real model output,
/// so chat history stays clean.
pub fn mock_answer_with_sampler(query: &str, sampler: &SamplerConfig) -> String {
    let short = truncate_query(query, 500);
    if query.trim().is_empty() {
        return "Ask me about code, daily tasks, or local files.".to_string();
    }
    if sampler.temp == 0.0 {
        return format!(
            "## Answer (seed {})\n\nYou asked: {short}\n\nDeterministic mock output for repro. Load a GGUF model or start Ollama for real inference.\n",
            sampler.seed
        );
    }
    format!(
        "## Answer\n\nYou asked: {short}\n\nMock answer (temp {:.1}). Real inference needs a GGUF model in cache or a running Ollama daemon. Try /model to switch, /insert to add a GGUF file, or ask math and code review questions answered by the local brain.\n",
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
    fn random_seed_varies_and_flag_pins() {
        assert_eq!(super::resolve_seed(Some(7), 0), 7);
        assert_eq!(super::resolve_seed(None, 9), 9);
        let a = super::resolve_seed(None, 0);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = super::resolve_seed(None, 0);
        assert_ne!(a, b, "default seed must not freeze sampling");
    }

    #[test]
    fn deterministic_seed_is_byte_identical() {
        let s = SamplerConfig::deterministic();
        let a = mock_answer_with_sampler("fix test", &s);
        let b = mock_answer_with_sampler("fix test", &s);
        assert_eq!(a, b);
    }

    #[test]
    fn mock_never_leaks_code_or_fake_sources() {
        for sampler in [
            SamplerConfig::deterministic(),
            SamplerConfig::chat(None, 7),
            SamplerConfig::code(None, 11),
        ] {
            let a = mock_answer_with_sampler("where is the pool built", &sampler);
            assert!(!a.contains("```"), "mock must not emit code fences");
            assert!(!a.contains("crates/"), "mock must not cite repo paths");
            assert!(!a.contains("Sources:"), "mock must not fake citations");
            assert!(!a.contains("fn "), "mock must not emit code");
        }
    }
}
