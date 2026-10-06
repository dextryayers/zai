use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::{chunk::chunk_text, embed::embed_text, store::*};
use aicli_ingest::walk::WalkOptions;

#[derive(Debug, Clone, Serialize)]
pub struct IndexReport {
    pub root: String,
    pub dir: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub skipped_large: usize,
    pub skipped_binary: usize,
    pub files: usize,
    pub chunks: usize,
    pub elapsed_ms: u128,
    pub version: String,
    pub config_hash: String,
    pub rebuild: bool,
}

/// Build or incrementally update local index.
/// Progress callback receives (files_done, files_total, chunks_so_far).
#[allow(clippy::too_many_arguments)]
pub fn build_index(
    root: &Path,
    cache_dir: &Path,
    opts: &WalkOptions,
    chunk_tokens: usize,
    overlap_tokens: usize,
    exts: &[String],
    rebuild: bool,
    mut on_progress: impl FnMut(usize, usize, usize),
) -> Result<IndexReport> {
    let t0 = std::time::Instant::now();
    let dir = index_dir_for(
        cache_dir,
        &root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    );
    let chash = config_hash(chunk_tokens, overlap_tokens, exts);

    if rebuild && dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    // Stale guard: version or hash mismatch forces rebuild error with fix.
    if let Some(meta) = read_meta(&dir) {
        if meta.version != INDEX_VERSION || meta.config_hash != chash {
            anyhow::bail!(
                "stale index: version {} hash {} differs from {} {}. Run: aicli index {} --rebuild",
                meta.version,
                meta.config_hash,
                INDEX_VERSION,
                chash,
                root.display()
            );
        }
    }

    let rep = aicli_ingest::walk::collect_files(root, opts);
    let total = rep.files.len();
    let conn = open_index_db(&dir)?;

    // Snapshot existing mtime map for incremental update.
    let mut existing: std::collections::HashMap<String, (i64, i64)> = Default::default();
    if !rebuild {
        if let Ok(mut stmt) = conn.prepare("SELECT path, mtime, size FROM chunks") {
            // Aggregate per path: use max mtime as proxy, exact per chunk check below.
            if let Ok(rows) = stmt.query_map([], |r| {
                let p: String = r.get(0)?;
                let m: i64 = r.get(1)?;
                let s: i64 = r.get(2)?;
                Ok((p, m, s))
            }) {
                for r in rows.flatten() {
                    existing.entry(r.0).or_insert((r.1, r.2));
                }
            }
        }
    } else {
        conn.execute("DELETE FROM chunks", [])?;
    }

    let mut added = 0usize;
    let mut updated = 0usize;
    let mut chunks_total = 0usize;
    let mut skipped_binary = 0usize;
    let mut done = 0usize;

    for path in &rep.files {
        done += 1;
        let md = std::fs::metadata(path).ok();
        let mtime = md
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = md.map(|m| m.len() as i64).unwrap_or(0);
        let key = path.to_string_lossy().to_string();

        if !rebuild {
            if let Some((em, es)) = existing.get(&key) {
                if *em == mtime && *es == size {
                    // Count existing chunks for report without re reading.
                    if let Ok(n) = conn.query_row(
                        "SELECT COUNT(*) FROM chunks WHERE path=?1",
                        [key.clone()],
                        |r| r.get::<_, i64>(0),
                    ) {
                        chunks_total += n as usize;
                    }
                    on_progress(done, total, chunks_total);
                    continue;
                }
            }
            // Remove stale chunks for changed file.
            let had = conn.execute("DELETE FROM chunks WHERE path=?1", [key.clone()])?;
            if had > 0 {
                updated += 1;
            } else {
                added += 1;
            }
        } else {
            added += 1;
        }

        let lines = match aicli_ingest::extract::read_text_lines(path, 20000) {
            Ok(v) => v,
            Err(_) => {
                skipped_binary += 1;
                on_progress(done, total, chunks_total);
                continue;
            }
        };
        // Null byte guard already in extract, double check.
        let chunks = chunk_text(&key, &lines, chunk_tokens, overlap_tokens);
        for c in chunks {
            let vec = embed_text(&c.text);
            let vec_json = serde_json::to_string(&vec.to_vec())?;
            conn.execute(
                "INSERT INTO chunks(path, start_line, end_line, text, vector, mtime, size) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![c.path, c.start_line as i64, c.end_line as i64, c.text, vec_json, mtime, size],
            )?;
            chunks_total += 1;
        }
        on_progress(done, total, chunks_total);
    }

    // Removed: paths in index but no longer on disk.
    let mut removed = 0usize;
    {
        let known: Vec<String> = conn
            .prepare("SELECT DISTINCT path FROM chunks")?
            .query_map([], |r| r.get(0))?
            .flatten()
            .collect();
        let live: std::collections::HashSet<String> = rep
            .files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        for k in known {
            if !live.contains(&k) {
                conn.execute("DELETE FROM chunks WHERE path=?1", [&k])?;
                removed += 1;
            }
        }
    }

    let files_indexed: i64 = conn
        .query_row("SELECT COUNT(DISTINCT path) FROM chunks", [], |r| r.get(0))
        .unwrap_or(0);
    let meta = IndexMeta {
        version: INDEX_VERSION.to_string(),
        config_hash: chash.clone(),
        indexed_at: chrono::Utc::now().to_rfc3339(),
        root: root.to_string_lossy().to_string(),
        files: files_indexed as usize,
        chunks: chunks_total,
        chunk_tokens,
        overlap_tokens,
    };
    std::fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;

    Ok(IndexReport {
        root: root.to_string_lossy().to_string(),
        dir: dir.to_string_lossy().to_string(),
        added,
        updated,
        removed,
        skipped_large: rep.skipped_large.len(),
        skipped_binary,
        files: files_indexed as usize,
        chunks: chunks_total,
        elapsed_ms: t0.elapsed().as_millis(),
        version: INDEX_VERSION.to_string(),
        config_hash: chash,
        rebuild,
    })
}

pub fn index_status_report(cache_dir: &Path, root: &Path) -> Result<String> {
    let dir = index_dir_for(
        cache_dir,
        &root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    );
    match read_meta(&dir) {
        Some(m) => Ok(format!(
            "index {} files {} chunks {} version {} hash {} at {}",
            dir.display(),
            m.files,
            m.chunks,
            m.version,
            m.config_hash,
            m.indexed_at
        )),
        None => Ok(format!(
            "index: not built for {}. Run: aicli index {} --rebuild",
            root.display(),
            root.display()
        )),
    }
}
