use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

const SECRET_FILES: &[&str] = &[".env", "id_rsa", "id_ed25519", "credentials.json"];
const SECRET_EXTS: &[&str] = &["pem", "key", "p12"];

/// Check path stays inside root and is not a secret. Returns canonical file path.
pub fn resolve_inside(root: &Path, rel: &str) -> Result<PathBuf> {
    if rel.trim().is_empty() {
        bail!("empty path");
    }
    // Reject .. segments up front, before any canonicalize fallback can hide them.
    for seg in rel.replace('\\', "/").split('/') {
        if seg == ".." {
            bail!("path escape denied: {rel} outside root");
        }
    }
    let root_c = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // Reject absolute escape attempts that contain .. segments before join.
    let candidate = root.join(rel.trim_start_matches('/'));
    // Canonicalize parent when file may not exist yet for patch paths.
    let parent = candidate.parent().unwrap_or(root);
    let parent_c = parent.canonicalize().unwrap_or_else(|_| root_c.clone());
    if !parent_c.starts_with(&root_c) {
        bail!("path escape denied: {rel} outside root");
    }
    let name = candidate.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if is_secret_name(name) {
        bail!("secret file redacted: {name}, path only");
    }
    Ok(candidate)
}

fn is_secret_name(name: &str) -> bool {
    if SECRET_FILES.contains(&name) {
        return true;
    }
    if let Some(ext) = name.rsplit('.').next() {
        if SECRET_EXTS.contains(&ext) {
            return true;
        }
    }
    false
}

/// Read file with 200 line and 64 KB caps.
pub fn fs_read(root: &Path, rel: &str, start: usize, end: usize) -> Result<(Vec<String>, usize)> {
    let path = resolve_inside(root, rel)?;
    let lines = aicli_ingest::extract::read_text_lines(&path, 5000)?;
    let total = lines.len();
    let s = start.max(1).min(total.max(1));
    let e = end.max(s).min(s + 199).min(total);
    let mut bytes = 0usize;
    let mut out = Vec::new();
    for line in lines.iter().take(e).skip(s - 1) {
        bytes += line.len() + 1;
        if bytes > 64 * 1024 {
            break;
        }
        out.push(line.clone());
    }
    Ok((out, total))
}

/// List dir with 500 entry cap, dirs first, sorted.
pub fn fs_list(root: &Path, rel: &str, glob: Option<&str>) -> Result<Vec<String>> {
    let dir = resolve_inside(root, rel)?;
    if !dir.is_dir() {
        bail!("not a directory: {rel}");
    }
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(g) = glob {
            // Minimal glob: prefix match or *.ext suffix.
            if g.starts_with("*.") {
                if !name.ends_with(&g[1..]) {
                    continue;
                }
            } else if !name.contains(g) {
                continue;
            }
        }
        if entry.path().is_dir() {
            dirs.push(format!("{name}/"));
        } else {
            files.push(name);
        }
        if dirs.len() + files.len() >= 500 {
            break;
        }
    }
    dirs.sort();
    files.sort();
    dirs.extend(files);
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_escape() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_inside(tmp.path(), "../x").is_err());
        assert!(resolve_inside(tmp.path(), "a/../../etc").is_err());
    }

    #[test]
    fn redacts_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_inside(tmp.path(), ".env").is_err());
        assert!(resolve_inside(tmp.path(), "id_rsa").is_err());
    }

    #[test]
    fn read_caps_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("d")).unwrap();
        let mut text = String::new();
        for i in 0..500 {
            text.push_str(&format!("line {i}\n"));
        }
        std::fs::write(root.join("d").join("f.txt"), text).unwrap();
        let (lines, total) = fs_read(root, "d/f.txt", 1, 500).unwrap();
        assert_eq!(total, 500);
        assert_eq!(lines.len(), 200);
    }
}
