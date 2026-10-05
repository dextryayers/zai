use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub status: String,
    pub created_at: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

pub fn create_session(conn: &Connection, title: &str, model: &str) -> Result<Session> {
    let s = Session {
        id: new_id("s"),
        title: if title.trim().is_empty() {
            "untitled".to_string()
        } else {
            title.trim().chars().take(80).collect()
        },
        created_at: now(),
        updated_at: now(),
        model: model.to_string(),
        status: "active".to_string(),
    };
    conn.execute(
        "INSERT INTO sessions(id, title, created_at, updated_at, model, status) VALUES (?1,?2,?3,?4,?5,?6)",
        params![s.id, s.title, s.created_at, s.updated_at, s.model, s.status],
    )?;
    Ok(s)
}

/// Get or create session with explicit id. Used by `ask --session <id>`.
pub fn ensure_with_id(conn: &Connection, id: &str, model: &str) -> Result<Session> {
    if let Some(s) = get_session(conn, id)? {
        return Ok(s);
    }
    let clean = id.trim();
    anyhow::ensure!(!clean.is_empty(), "session id must not be empty");
    anyhow::ensure!(
        clean.len() <= 64
            && clean
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "session id must be 1-64 chars of a-z 0-9 - _"
    );
    let s = Session {
        id: clean.to_string(),
        title: format!("chat {}", chrono::Utc::now().format("%Y-%m-%d")),
        created_at: now(),
        updated_at: now(),
        model: model.to_string(),
        status: "active".to_string(),
    };
    conn.execute(
        "INSERT INTO sessions(id, title, created_at, updated_at, model, status) VALUES (?1,?2,?3,?4,?5,?6)",
        params![s.id, s.title, s.created_at, s.updated_at, s.model, s.status],
    )?;
    Ok(s)
}

pub fn ensure_session(conn: &Connection, id: Option<&str>, model: &str) -> Result<Session> {
    if let Some(sid) = id {
        if let Some(s) = get_session(conn, sid)? {
            return Ok(s);
        }
    }
    // Resume last active when no id given, else create.
    if id.is_none() {
        if let Some(last) = last_active(conn)? {
            return Ok(last);
        }
    }
    create_session(
        conn,
        &format!("chat {}", chrono::Utc::now().format("%Y-%m-%d")),
        model,
    )
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<Session>> {
    conn.query_row(
        "SELECT id, title, created_at, updated_at, model, status FROM sessions WHERE id=?1",
        [id],
        |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                model: r.get(4)?,
                status: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub fn last_active(conn: &Connection) -> Result<Option<Session>> {
    conn.query_row(
        "SELECT id, title, created_at, updated_at, model, status FROM sessions WHERE status='active' ORDER BY updated_at DESC LIMIT 1",
        [],
        |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                model: r.get(4)?,
                status: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub fn list_sessions(conn: &Connection, limit: usize) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at, model, status FROM sessions ORDER BY updated_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], |r| {
        Ok(Session {
            id: r.get(0)?,
            title: r.get(1)?,
            created_at: r.get(2)?,
            updated_at: r.get(3)?,
            model: r.get(4)?,
            status: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn rename_session(conn: &Connection, id: &str, title: &str) -> Result<bool> {
    let n = conn.execute(
        "UPDATE sessions SET title=?1, updated_at=?2 WHERE id=?3",
        params![title, now(), id],
    )?;
    Ok(n > 0)
}

pub fn delete_session(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM sessions WHERE id=?1", [id])?;
    Ok(n > 0)
}

pub fn append_turn(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
    tokens_in: i64,
    tokens_out: i64,
    status: &str,
) -> Result<Turn> {
    let t = Turn {
        id: new_id("t"),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        tokens_in,
        tokens_out,
        status: status.to_string(),
        created_at: now(),
    };
    conn.execute(
        "INSERT INTO turns(id, session_id, role, content, tokens_in, tokens_out, status, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![t.id, t.session_id, t.role, t.content, t.tokens_in, t.tokens_out, t.status, t.created_at],
    )?;
    conn.execute(
        "UPDATE sessions SET updated_at=?1 WHERE id=?2",
        params![t.created_at, session_id],
    )?;
    Ok(t)
}

pub fn list_turns(conn: &Connection, session_id: &str) -> Result<Vec<Turn>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, tokens_in, tokens_out, status, created_at FROM turns WHERE session_id=?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok(Turn {
            id: r.get(0)?,
            session_id: r.get(1)?,
            role: r.get(2)?,
            content: r.get(3)?,
            tokens_in: r.get(4)?,
            tokens_out: r.get(5)?,
            status: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// JSONL mirror path for one session.
pub fn jsonl_path(sessions_dir: &std::path::Path, session_id: &str) -> std::path::PathBuf {
    sessions_dir.join(format!("{session_id}.jsonl"))
}

/// Append turn to JSONL mirror. Tolerates partial line after kill on read.
pub fn mirror_append(sessions_dir: &std::path::Path, turn: &Turn) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(sessions_dir)?;
    let p = jsonl_path(sessions_dir, &turn.session_id);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    writeln!(f, "{}", serde_json::to_string(turn)?)?;
    Ok(())
}

pub fn export_markdown(session: &Session, turns: &[Turn]) -> String {
    let mut out = format!(
        "# {}\n\nSession `{}` model `{}`\n\n",
        session.title, session.id, session.model
    );
    for t in turns {
        let tag = if t.status == "stopped" {
            " (stopped)"
        } else {
            ""
        };
        out.push_str(&format!("## {}{}\n\n{}\n\n", t.role, tag, t.content));
    }
    out
}

/// Extractive compact: keep first user goal plus last 6 turns, summarize middle.
/// No model call, deterministic, temp 0.0 style. Returns compacted list plus notice.
pub fn compact_history(turns: &[Turn]) -> (Vec<Turn>, Option<String>) {
    if turns.len() <= 8 {
        return (turns.to_vec(), None);
    }
    let first = turns.first().unwrap().clone();
    let tail: Vec<Turn> = turns[turns.len() - 6..].to_vec();
    let dropped = turns.len() - 1 - 6;
    let mut compacted = vec![first];
    compacted.extend(tail);
    let notice = format!("compacted {dropped} middle turns, kept first goal plus last 6");
    (compacted, Some(notice))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn resume_and_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let dbp = tmp.path().join("db.sqlite");
        let conn = db::open(&dbp).unwrap();
        let s = create_session(&conn, "demo", "m").unwrap();
        append_turn(&conn, &s.id, "user", "goal: fix test", 10, 0, "done").unwrap();
        // Simulate kill during stream: partial assistant turn marked stopped.
        append_turn(&conn, &s.id, "assistant", "partial...", 10, 5, "stopped").unwrap();
        let turns = list_turns(&conn, &s.id).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].status, "stopped");
        // Resume returns same session.
        let r = ensure_session(&conn, None, "m").unwrap();
        assert_eq!(r.id, s.id);
    }

    #[test]
    fn compact_keeps_first_and_last6() {
        let tmp = tempfile::tempdir().unwrap();
        let dbp = tmp.path().join("db.sqlite");
        let conn = db::open(&dbp).unwrap();
        let s = create_session(&conn, "c", "m").unwrap();
        for i in 0..50 {
            append_turn(&conn, &s.id, "user", &format!("q{i}"), 1, 0, "done").unwrap();
        }
        let turns = list_turns(&conn, &s.id).unwrap();
        let (compacted, notice) = compact_history(&turns);
        assert!(notice.is_some());
        assert_eq!(compacted.len(), 7);
        assert_eq!(compacted[0].content, "q0");
    }
}
