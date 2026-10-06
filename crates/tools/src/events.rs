use anyhow::Result;
use std::path::Path;

/// Append only audit log for every tool call.
/// Stored as JSONL in data_dir/events.jsonl plus sqlite events table best effort.
pub fn log_event(
    data_dir: &Path,
    kind: &str,
    session_id: Option<&str>,
    detail: &str,
) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(data_dir)?;
    let p = data_dir.join("events.jsonl");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    // Truncate detail to keep log bounded.
    let short: String = detail.chars().take(2000).collect();
    let line = serde_json::json!({
        "ts": ts,
        "kind": kind,
        "session_id": session_id.unwrap_or(""),
        "detail": short,
    });
    writeln!(f, "{line}")?;
    // Mirror into sqlite events table when db exists.
    let dbp = data_dir.join("db.sqlite");
    if dbp.exists() {
        if let Ok(conn) = rusqlite::Connection::open(&dbp) {
            let _ = conn.execute(
                "INSERT INTO events(ts, kind, session_id, detail_json) VALUES (?1,?2,?3,?4)",
                rusqlite::params![ts, kind, session_id.unwrap_or(""), short],
            );
        }
    }
    Ok(())
}
