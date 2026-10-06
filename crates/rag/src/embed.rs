/// Local hash embeddings, fully offline, no download.
/// Dim 128, FNV hashed character trigrams plus token unigrams, TF weighted, L2 normalized.
/// Deterministic and fast. fastembed ONNX remains an optional upgrade, not required for v1.
pub const DIM: usize = 128;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_string())
        .collect()
}

/// Embed text to fixed vector. Empty text gives zero vector.
pub fn embed_text(text: &str) -> [f32; DIM] {
    let mut v = [0f32; DIM];
    let toks = tokens(text);
    if toks.is_empty() {
        return v;
    }
    // Unigrams plus adjacent bigrams for phrase sensitivity.
    let mut feats: Vec<String> = toks.clone();
    for w in toks.windows(2) {
        feats.push(format!("{} {}", w[0], w[1]));
    }
    for f in &feats {
        let idx = (fnv1a(f.as_bytes()) % DIM as u64) as usize;
        v[idx] += 1.0;
    }
    // Trigram chars add robustness for code identifiers.
    let lower = text.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    for w in chars.windows(3) {
        let s: String = w.iter().collect();
        if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            let idx = (fnv1a(s.as_bytes()) % DIM as u64) as usize;
            v[idx] += 0.3;
        }
    }
    // L2 normalize.
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

pub fn cosine(a: &[f32; DIM], b: &[f32; DIM]) -> f64 {
    let mut dot = 0f64;
    for i in 0..DIM {
        dot += a[i] as f64 * b[i] as f64;
    }
    dot.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_score_one() {
        let a = embed_text("rust offline gguf index");
        let b = embed_text("rust offline gguf index");
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn unrelated_texts_score_low() {
        let a = embed_text("rust compiler borrow checker");
        let b = embed_text("banana pancake recipe honey");
        assert!(cosine(&a, &b) < 0.5);
    }
}
