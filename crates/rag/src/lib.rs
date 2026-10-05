use anyhow::Result;

/// Chunk text into overlapping pieces with path and line anchors.
/// Token estimate uses 4 chars per token for code and docs.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

pub fn chunk_text(
    path: &str,
    lines: &[String],
    target_tokens: usize,
    overlap_tokens: usize,
) -> Vec<Chunk> {
    let target_chars = target_tokens.max(64) * 4;
    let overlap_chars = overlap_tokens * 4;
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut start = 1usize;

    for (idx, line) in lines.iter().enumerate() {
        let no = idx + 1;
        if buf.len() + line.len() + 1 > target_chars && !buf.is_empty() {
            out.push(Chunk {
                path: path.to_string(),
                start_line: start,
                end_line: no - 1,
                text: buf.clone(),
            });
            // Overlap: keep tail chars.
            let keep = overlap_chars.min(buf.len());
            let tail = buf[buf.len() - keep..].to_string();
            buf = tail + "\n" + line;
            start = no;
            // Approximate start after overlap is acceptable for v1 citations.
        } else {
            if buf.is_empty() {
                start = no;
            } else {
                buf.push('\n');
            }
            buf.push_str(line);
        }
    }
    if !buf.trim().is_empty() {
        out.push(Chunk {
            path: path.to_string(),
            start_line: start,
            end_line: lines.len(),
            text: buf,
        });
    }
    out
}

/// Minimal lexical score for Phase 1 retrieve demo. Phase 6 adds BM25 plus vectors.
pub fn score_chunk(query: &str, chunk: &Chunk) -> f64 {
    let q = query.to_lowercase();
    if q.trim().is_empty() {
        return 0.0;
    }
    let text = chunk.text.to_lowercase();
    let terms: Vec<&str> = q.split_whitespace().collect();
    if terms.is_empty() {
        return 0.0;
    }
    let mut hits = 0;
    for t in &terms {
        if text.contains(t) {
            hits += 1;
        }
    }
    hits as f64 / terms.len() as f64
}

pub fn index_status() -> Result<String> {
    Ok("index: not built yet. Run: aicli index ./docs --rebuild".to_string())
}
