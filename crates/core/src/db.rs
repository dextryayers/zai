use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

const SCHEMA: &str = include_str!("../../..//migrations/001_init.sql");

/// Open sqlite with WAL, busy timeout, foreign keys. Creates parent dirs.
pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create db parent {}", parent.display()))?;
    }
    let conn =
        Connection::open(db_path).with_context(|| format!("open db {}", db_path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

/// Idempotent forward migrations.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    // FTS for notes search, kept in sync by triggers. Best effort if FTS5 missing.
    let _ = conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(text, content='notes', content_rowid='rowid');
         CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
           INSERT INTO notes_fts(rowid, text) VALUES (new.rowid, new.text);
         END;
         CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
           INSERT INTO notes_fts(notes_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
         END;",
    );
    Ok(())
}

/// Backup before migrate or destructive op.
pub fn backup(db_path: &Path) -> Result<std::path::PathBuf> {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let bak = db_path.with_extension(format!("bak.{ts}"));
    if db_path.exists() {
        std::fs::copy(db_path, &bak)?;
    }
    Ok(bak)
}

/// Import legacy store.json once. Idempotent: skips rows with same id.
pub fn import_legacy_json(conn: &Connection, data_dir: &Path) -> Result<(usize, usize)> {
    let p = data_dir.join("store.json");
    if !p.exists() {
        return Ok((0, 0));
    }
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    if text.trim().is_empty() {
        return Ok((0, 0));
    }
    #[derive(serde::Deserialize, Default)]
    struct Legacy {
        #[serde(default)]
        tasks: Vec<LegacyTask>,
        #[serde(default)]
        notes: Vec<LegacyNote>,
    }
    #[derive(serde::Deserialize)]
    struct LegacyTask {
        id: String,
        date: String,
        text: String,
        #[serde(default)]
        done: bool,
    }
    #[derive(serde::Deserialize)]
    struct LegacyNote {
        id: String,
        date: String,
        text: String,
    }
    let legacy: Legacy = serde_json::from_str(&text).unwrap_or_default();
    let mut t = 0;
    for item in legacy.tasks {
        let n = conn.execute(
            "INSERT OR IGNORE INTO tasks(id, date, text, done, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![item.id, item.date, item.text, item.done as i32, chrono::Utc::now().to_rfc3339()],
        )?;
        t += n;
    }
    let mut nn = 0;
    for item in legacy.notes {
        let n = conn.execute(
            "INSERT OR IGNORE INTO notes(id, date, text, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                item.id,
                item.date,
                item.text,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        nn += n;
    }
    Ok((t, nn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let dbp = tmp.path().join("db.sqlite");
        let c1 = open(&dbp).unwrap();
        migrate(&c1).unwrap();
        migrate(&c1).unwrap();
        let count: i64 = c1
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
