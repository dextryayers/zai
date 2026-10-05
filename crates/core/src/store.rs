use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::db;

/// Task and note store backed by sqlite. Same API shape as Phase 1 file store.
/// Legacy store.json is imported once on first open.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct TaskItem {
    pub id: String,
    pub date: String,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct NoteItem {
    pub id: String,
    pub date: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub turns: u32,
}

fn db_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("db.sqlite")
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn open_and_migrate(data_dir: &Path) -> Result<rusqlite::Connection> {
    let conn = db::open(&db_path(data_dir))?;
    let _ = db::import_legacy_json(&conn, data_dir)?;
    Ok(conn)
}

fn next_id(conn: &rusqlite::Connection, table: &str, prefix: &str) -> Result<String> {
    let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    Ok(format!("{prefix}{:04}", count + 1))
}

pub fn task_add(data_dir: &Path, text: &str, date: Option<&str>) -> Result<TaskItem> {
    let conn = open_and_migrate(data_dir)?;
    let id = next_id(&conn, "tasks", "t")?;
    let d = date.unwrap_or(&today()).to_string();
    let created = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks(id, date, text, done, created_at) VALUES (?1,?2,?3,0,?4)",
        params![id, d, text, created],
    )?;
    Ok(TaskItem {
        id,
        date: d,
        text: text.to_string(),
        done: false,
    })
}

pub fn task_list(data_dir: &Path, date: Option<&str>) -> Vec<TaskItem> {
    let conn = match open_and_migrate(data_dir) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let sql = if date.is_some() {
        "SELECT id, date, text, done FROM tasks WHERE date=?1 ORDER BY rowid ASC"
    } else {
        "SELECT id, date, text, done FROM tasks ORDER BY rowid ASC"
    };
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let map = |r: &rusqlite::Row| {
        Ok(TaskItem {
            id: r.get(0)?,
            date: r.get(1)?,
            text: r.get(2)?,
            done: r.get::<_, i32>(3)? != 0,
        })
    };
    let rows: Vec<TaskItem> = if let Some(d) = date {
        stmt.query_map([d], map)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    } else {
        stmt.query_map([], map)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    };
    rows
}

pub fn task_done(data_dir: &Path, id: &str) -> Result<bool> {
    let conn = open_and_migrate(data_dir)?;
    let n = conn.execute("UPDATE tasks SET done=1 WHERE id=?1", [id])?;
    Ok(n > 0)
}

pub fn note_add(data_dir: &Path, text: &str, date: Option<&str>) -> Result<NoteItem> {
    let conn = open_and_migrate(data_dir)?;
    let id = next_id(&conn, "notes", "n")?;
    let d = date.unwrap_or(&today()).to_string();
    let created = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO notes(id, date, text, created_at) VALUES (?1,?2,?3,?4)",
        params![id, d, text, created],
    )?;
    Ok(NoteItem {
        id,
        date: d,
        text: text.to_string(),
        created_at: created,
    })
}

pub fn note_search(data_dir: &Path, query: &str, limit: usize) -> Vec<NoteItem> {
    let conn = match open_and_migrate(data_dir) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let q = query.trim();
    // Prefer FTS5 when query present, fallback to LIKE for compat.
    if !q.is_empty() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT n.id, n.date, n.text, n.created_at FROM notes_fts f JOIN notes n ON n.rowid=f.rowid WHERE notes_fts MATCH ?1 LIMIT ?2",
        ) {
            if let Ok(rows) = stmt.query_map(params![q, limit as i64], |r| {
                Ok(NoteItem {
                    id: r.get(0)?,
                    date: r.get(1)?,
                    text: r.get(2)?,
                    created_at: r.get(3)?,
                })
            }) {
                let out: Vec<NoteItem> = rows.flatten().collect();
                if !out.is_empty() {
                    return out;
                }
            }
        }
    }
    let like = format!("%{q}%");
    let mut stmt = match conn.prepare(
        "SELECT id, date, text, created_at FROM notes WHERE (?1='' OR text LIKE ?2) ORDER BY rowid DESC LIMIT ?3",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(params![q, like, limit as i64], |r| {
        Ok(NoteItem {
            id: r.get(0)?,
            date: r.get(1)?,
            text: r.get(2)?,
            created_at: r.get(3)?,
        })
    })
    .map(|it| it.flatten().collect())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_tasks_roundtrip_and_legacy_import() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // Legacy file is imported once.
        std::fs::write(
            dir.join("store.json"),
            r#"{"tasks":[{"id":"t0001","date":"2026-01-01","text":"old","done":false}],"notes":[]}"#,
        )
        .unwrap();
        let t = task_add(dir, "new", Some("2026-01-02")).unwrap();
        assert!(t.id.starts_with('t'));
        let all = task_list(dir, None);
        assert_eq!(all.len(), 2);
        assert!(task_done(dir, &t.id).unwrap());
    }

    #[test]
    fn notes_search_under_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for i in 0..200 {
            note_add(dir, &format!("note {i} about rust and offline"), None).unwrap();
        }
        let t0 = std::time::Instant::now();
        let hits = note_search(dir, "rust", 20);
        assert!(!hits.is_empty());
        assert!(t0.elapsed().as_millis() < 200);
    }
}
