use anyhow::Result;
use std::path::Path;

/// User owned long memory. CLI never overwrites, only appends quoted blocks.
pub fn memory_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("memory.md")
}

pub fn memory_show(data_dir: &Path) -> String {
    let p = memory_path(data_dir);
    std::fs::read_to_string(&p).unwrap_or_else(|_| {
        "# Memory\n\nNo entries yet. Use: zai memory add \"prefer tabs\" \n".to_string()
    })
}

pub fn memory_add(data_dir: &Path, text: &str) -> Result<String> {
    use std::io::Write;
    std::fs::create_dir_all(data_dir)?;
    let p = memory_path(data_dir);
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    writeln!(f, "\n## {ts}\n\n{text}\n")?;
    Ok(p.display().to_string())
}

/// Promote a session turn into memory with source pointer. Shows preview first in CLI.
pub fn memory_promote(
    data_dir: &Path,
    session_id: &str,
    turn_id: &str,
    turn_role: &str,
    turn_content: &str,
) -> Result<String> {
    let short: String = turn_content.chars().take(1000).collect();
    memory_add(
        data_dir,
        &format!(
            "promoted from session {session_id} turn {turn_id} role {turn_role}:\n\n> {short}"
        ),
    )
}
