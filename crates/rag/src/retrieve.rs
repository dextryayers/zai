use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    embed::{cosine, embed_text, DIM},
    store::{index_dir_for, open_index_db},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Retrieved {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub bm25: f64,
    pub vector: f64,
    pub score: f64,
}

/// Retrieve top_k chunks with BM25 plus vector fusion.
/// Scores z normalized per list then weighted 0.5 and 0.5. Deterministic tie break.
pub fn retrieve(
    cache_dir: &Path,
    root: &Path,
    query: &str,
    top_k: usize,
) -> Result<Vec<Retrieved>> {
    let dir = index_dir_for(
        cache_dir,
        &root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    );
    if !dir.join("meta.json").exists() {
        return Ok(vec![]);
    }
    let conn = open_index_db(&dir)?;
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }

    // BM25 candidates via FTS5 rank. Use bm25() ascending (more negative is better in FTS5).
    let mut bm25_rows: Vec<(i64, String, usize, usize, String, f64)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT rowid, path, start_line, end_line, text, bm25(chunks_fts) FROM chunks_fts WHERE chunks_fts MATCH ?1 LIMIT 20",
    ) {
        // Escape FTS5 special chars by quoting terms.
        let fts_q = fts_escape(q);
        if let Ok(rows) = stmt.query_map([fts_q], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
                r.get::<_, i64>(3)? as usize,
                r.get::<_, String>(4)?,
                r.get::<_, f64>(5)?,
            ))
        }) {
            for r in rows.flatten() {
                bm25_rows.push(r);
            }
        }
    }
    // Fallback to LIKE when FTS yields nothing (short tokens, code symbols).
    if bm25_rows.is_empty() {
        if let Ok(mut stmt) =
            conn.prepare("SELECT id, path, start_line, end_line, text FROM chunks LIMIT 200")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? as usize,
                    r.get::<_, i64>(3)? as usize,
                    r.get::<_, String>(4)?,
                ))
            }) {
                let ql = q.to_lowercase();
                for r in rows.flatten() {
                    if r.4.to_lowercase().contains(&ql)
                        || ql
                            .split_whitespace()
                            .any(|t| r.4.to_lowercase().contains(t))
                    {
                        bm25_rows.push((r.0, r.1, r.2, r.3, r.4, -1.0));
                        if bm25_rows.len() >= 20 {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Vector candidates: scan vectors, cosine with query embedding.
    let qvec = embed_text(q);
    let mut vec_rows: Vec<(i64, String, usize, usize, String, f64)> = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT id, path, start_line, end_line, text, vector FROM chunks LIMIT 2000")
    {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
                r.get::<_, i64>(3)? as usize,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        }) {
            let mut scored: Vec<(i64, String, usize, usize, String, f64)> = Vec::new();
            for r in rows.flatten() {
                let vec: Vec<f32> = serde_json::from_str(&r.5).unwrap_or_default();
                if vec.len() != DIM {
                    continue;
                }
                let mut arr = [0f32; DIM];
                arr.copy_from_slice(&vec);
                let s = cosine(&qvec, &arr);
                scored.push((r.0, r.1, r.2, r.3, r.4, s));
            }
            scored.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap());
            scored.truncate(20);
            vec_rows = scored;
        }
    }

    // Merge by chunk id. Normalize each list to 0..1.
    use std::collections::HashMap;
    let norm =
        |vals: &[(i64, String, usize, usize, String, f64)], invert: bool| -> HashMap<i64, f64> {
            let mut m = HashMap::new();
            if vals.is_empty() {
                return m;
            }
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for v in vals {
                min = min.min(v.5);
                max = max.max(v.5);
            }
            let span = (max - min).max(1e-9);
            for v in vals {
                let n = if invert {
                    // BM25 lower is better, invert.
                    (max - v.5) / span
                } else {
                    (v.5 - min) / span
                };
                m.insert(v.0, n);
            }
            m
        };
    let bmap = norm(&bm25_rows, true);
    let vmap = norm(&vec_rows, false);

    let mut by_id: HashMap<i64, (String, usize, usize, String)> = HashMap::new();
    for v in bm25_rows.iter().chain(vec_rows.iter()) {
        by_id
            .entry(v.0)
            .or_insert((v.1.clone(), v.2, v.3, v.4.clone()));
    }
    let mut merged: Vec<Retrieved> = Vec::new();
    for (id, (path, s, e, text)) in by_id {
        let b = bmap.get(&id).copied().unwrap_or(0.0);
        let vv = vmap.get(&id).copied().unwrap_or(0.0);
        merged.push(Retrieved {
            path,
            start_line: s,
            end_line: e,
            text,
            bm25: b,
            vector: vv,
            score: 0.5 * b + 0.5 * vv,
        });
    }
    // Dedupe overlapping chunks from same file: keep highest score per 50 line window.
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    let mut kept: Vec<Retrieved> = Vec::new();
    for r in merged {
        let overlap = kept.iter().any(|k: &Retrieved| {
            k.path == r.path
                && k.start_line.saturating_sub(50) <= r.end_line
                && r.start_line <= k.end_line + 50
        });
        if !overlap {
            kept.push(r);
        }
        if kept.len() >= top_k.max(1) {
            break;
        }
    }
    Ok(kept)
}

fn fts_escape(q: &str) -> String {
    // Quote each term to avoid FTS5 syntax errors on code symbols.
    q.split_whitespace()
        .map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" OR ")
}
