use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Index version plus config hash enforce rebuild on param change.
pub const INDEX_VERSION: &str = "rag-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMeta {
    pub version: String,
    pub config_hash: String,
    pub indexed_at: String,
    pub root: String,
    pub files: usize,
    pub chunks: usize,
    pub chunk_tokens: usize,
    pub overlap_tokens: usize,
}

/// Short slug for cache dir from absolute root path.
pub fn slug_for(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let s = root.to_string_lossy().to_string();
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let hex = format!("{:x}", h.finalize());
    let base = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect::<String>();
    format!("{base}-{}", &hex[..12])
}

pub fn index_dir_for(cache_dir: &Path, root: &Path) -> PathBuf {
    cache_dir.join("index").join(slug_for(root))
}

pub fn config_hash(chunk_tokens: usize, overlap_tokens: usize, exts: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(format!("{INDEX_VERSION}|{chunk_tokens}|{overlap_tokens}|").as_bytes());
    let mut e = exts.to_vec();
    e.sort();
    h.update(e.join(",").as_bytes());
    format!("{:x}", h.finalize())[..16].to_string()
}

pub fn read_meta(dir: &Path) -> Option<IndexMeta> {
    let text = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn index_db_path(dir: &Path) -> PathBuf {
    dir.join("index.sqlite")
}

/// Open index sqlite with FTS5 for BM25 plus chunks table.
pub fn open_index_db(dir: &Path) -> Result<rusqlite::Connection> {
    std::fs::create_dir_all(dir)?;
    let conn = rusqlite::Connection::open(index_db_path(dir))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS chunks(
           id INTEGER PRIMARY KEY,
           path TEXT NOT NULL,
           start_line INTEGER NOT NULL,
           end_line INTEGER NOT NULL,
           text TEXT NOT NULL,
           vector TEXT NOT NULL,
           mtime INTEGER NOT NULL,
           size INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);
         CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(text, content='chunks', content_rowid='id');
         CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
           INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
         END;
         CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
           INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
         END;",
    )?;
    Ok(conn)
}
