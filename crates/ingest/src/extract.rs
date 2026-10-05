use anyhow::Result;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Read text with lossy UTF-8 per line, skip null byte files as binary.
pub fn read_text_lines(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::with_capacity(8192);
    let mut out = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if buf.contains(&0) {
            anyhow::bail!("binary file");
        }
        let line = String::from_utf8_lossy(&buf);
        out.push(line.trim_end_matches(['\r', '\n']).to_string());
        if out.len() >= max_lines {
            break;
        }
    }
    Ok(out)
}

/// Detect log level for mixed text, used by search preview.
pub fn detect_level(line: &str) -> &'static str {
    let u = line.to_ascii_uppercase();
    if u.contains("ERROR") || u.contains("FATAL") || u.contains("CRITICAL") {
        "ERROR"
    } else if u.contains("WARN") {
        "WARN"
    } else if u.contains("INFO") {
        "INFO"
    } else if u.contains("DEBUG") {
        "DEBUG"
    } else if u.contains("TRACE") {
        "TRACE"
    } else {
        "OTHER"
    }
}
