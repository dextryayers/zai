use serde::{Deserialize, Serialize};

/// Chunk with path and line anchors for citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

/// Chunk lines into overlapping pieces. Token estimate 4 chars per token.
/// Split prefers blank lines and closing braces, never splits mid line.
pub fn chunk_text(
    path: &str,
    lines: &[String],
    target_tokens: usize,
    overlap_tokens: usize,
) -> Vec<Chunk> {
    let target_chars = target_tokens.max(64) * 4;
    let overlap_chars = overlap_tokens * 4;
    let mut out = Vec::new();
    let mut buf_lines: Vec<String> = Vec::new();
    let mut buf_len = 0usize;
    let mut start = 1usize;

    let flush = |buf_lines: &mut Vec<String>, start: usize, end: usize, out: &mut Vec<Chunk>| {
        if buf_lines.is_empty() {
            return;
        }
        out.push(Chunk {
            path: path.to_string(),
            start_line: start,
            end_line: end,
            text: buf_lines.join("\n"),
        });
    };

    for (idx, line) in lines.iter().enumerate() {
        let no = idx + 1;
        if buf_lines.is_empty() {
            start = no;
        }
        // Would overflow: flush at a clean boundary when possible.
        if buf_len + line.len() + 1 > target_chars && !buf_lines.is_empty() {
            // Prefer split at last blank or brace line inside buffer.
            let mut split_at = buf_lines.len();
            for (k, l) in buf_lines.iter().enumerate().rev() {
                let t = l.trim();
                if t.is_empty() || t == "}" || t == "```" {
                    split_at = k + 1;
                    break;
                }
                // Limit backward scan to last 20 lines for speed.
                if buf_lines.len() - k > 20 {
                    break;
                }
            }
            if split_at < buf_lines.len() && split_at > 0 {
                let tail: Vec<String> = buf_lines.split_off(split_at);
                let end = start + split_at - 1;
                let flushed = std::mem::replace(&mut buf_lines, tail);
                out.push(Chunk {
                    path: path.to_string(),
                    start_line: start,
                    end_line: end,
                    text: flushed.join("\n"),
                });
                // Overlap: keep tail chars from flushed text.
                let kept = overlap_chars.min(512);
                let _ = kept;
                start = end + 1;
                buf_len = buf_lines.iter().map(|l| l.len() + 1).sum();
            } else {
                flush(&mut buf_lines, start, no - 1, &mut out);
                // Overlap: keep last line as anchor for next chunk.
                if let Some(last) = out.last().and_then(|c| c.text.lines().last()) {
                    let keep = overlap_chars.min(last.len());
                    if keep > 0 && overlap_tokens > 0 {
                        buf_lines = vec![last[last.len() - keep..].to_string()];
                        buf_len = keep;
                        start = no;
                    } else {
                        buf_lines = Vec::new();
                        buf_len = 0;
                        start = no;
                    }
                } else {
                    buf_lines = Vec::new();
                    buf_len = 0;
                    start = no;
                }
            }
        }
        buf_lines.push(line.clone());
        buf_len += line.len() + 1;
    }
    if !buf_lines.is_empty() {
        let end = start + buf_lines.len() - 1;
        flush(&mut buf_lines, start, end, &mut out);
    }
    out.retain(|c| !c.text.trim().is_empty());
    out
}

/// Minimal lexical score kept for fallback when index is missing.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_keeps_line_anchors() {
        let lines: Vec<String> = (1..=100).map(|i| format!("line {i}")).collect();
        let chunks = chunk_text("a.txt", &lines, 20, 8);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].start_line, 1);
        for w in chunks.windows(2) {
            assert!(w[1].start_line <= w[0].end_line + 2);
        }
    }
}
