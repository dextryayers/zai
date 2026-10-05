use aicli_ingest::{matcher::Matcher, walk::WalkOptions};
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub file: String,
    pub line_no: usize,
    pub level: String,
    pub line: String,
}

/// Search files under root with matcher. Caps hits and time by caller.
pub fn search_files(
    root: &Path,
    query: &str,
    regex: bool,
    ignore_case: bool,
    exts: &[String],
    max_hits: usize,
) -> Result<Vec<SearchHit>> {
    let opts = WalkOptions {
        exts: exts.to_vec(),
        ..Default::default()
    };
    let rep = aicli_ingest::walk::collect_files(root, &opts);
    let matcher = Matcher::new(
        if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        },
        regex,
        ignore_case,
    )?;
    let mut out = Vec::new();
    for path in rep.files {
        if out.len() >= max_hits {
            break;
        }
        let lines = match aicli_ingest::extract::read_text_lines(&path, 5000) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for (i, line) in lines.iter().enumerate() {
            if matcher.is_match(line) {
                out.push(SearchHit {
                    file: path.to_string_lossy().to_string(),
                    line_no: i + 1,
                    level: aicli_ingest::extract::detect_level(line).to_string(),
                    line: truncate(line, 300),
                });
                if out.len() >= max_hits {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}..", &s[..max.saturating_sub(2)])
    }
}
