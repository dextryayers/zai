use anyhow::{bail, Result};
use std::path::Path;

use crate::fs::resolve_inside;

/// One hunk with 1 based old and new starts.
#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub body: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FilePatch {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

/// Validate unified diff without applying. Returns file count.
pub fn validate_patch(diff: &str, root: &str) -> Result<usize> {
    Ok(parse_patch(diff, Path::new(if root.is_empty() { "." } else { root }))?.len())
}

/// Parse and validate paths, hunks, binary guards.
pub fn parse_patch(diff: &str, root: &Path) -> Result<Vec<FilePatch>> {
    if diff.trim().is_empty() {
        bail!("empty diff");
    }
    let mut files: Vec<FilePatch> = Vec::new();
    let mut cur_old = String::new();
    let mut cur_new = String::new();
    let mut cur_hunks: Vec<Hunk> = Vec::new();
    let mut cur_hunk: Option<Hunk> = None;

    let flush_hunk = |cur_hunk: &mut Option<Hunk>, cur_hunks: &mut Vec<Hunk>| {
        if let Some(h) = cur_hunk.take() {
            cur_hunks.push(h);
        }
    };
    let flush_file = |cur_old: &mut String,
                      cur_new: &mut String,
                      cur_hunks: &mut Vec<Hunk>,
                      files: &mut Vec<FilePatch>| {
        if !cur_old.is_empty() || !cur_new.is_empty() {
            files.push(FilePatch {
                old_path: std::mem::take(cur_old),
                new_path: std::mem::take(cur_new),
                hunks: std::mem::take(cur_hunks),
            });
        }
    };

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("--- a/") {
            flush_hunk(&mut cur_hunk, &mut cur_hunks);
            flush_file(&mut cur_old, &mut cur_new, &mut cur_hunks, &mut files);
            cur_old = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("+++ b/") {
            cur_new = rest.trim().to_string();
            check_path(&cur_new)?;
        } else if line.starts_with("@@ ") {
            flush_hunk(&mut cur_hunk, &mut cur_hunks);
            cur_hunk = Some(parse_hunk_header(line)?);
        } else if line.starts_with("Binary ") {
            bail!("binary patch not allowed in v1");
        } else if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("new file")
            || line.starts_with("deleted ")
        {
            continue;
        } else if cur_hunk.is_some() {
            // Hunk body lines must start with space, plus, or minus.
            if !(line.starts_with(' ')
                || line.starts_with('+')
                || line.starts_with('-')
                || line.is_empty())
            {
                bail!("malformed hunk line: {line}");
            }
            if let Some(h) = cur_hunk.as_mut() {
                h.body.push(line.to_string());
            }
        }
    }
    flush_hunk(&mut cur_hunk, &mut cur_hunks);
    flush_file(&mut cur_old, &mut cur_new, &mut cur_hunks, &mut files);

    if files.is_empty() {
        bail!("no files found, expected --- a/ and +++ b/ headers");
    }
    for f in &files {
        if f.hunks.is_empty() {
            bail!("no hunks found for {}", f.new_path);
        }
        // Validate each path stays inside root.
        let rel = f.new_path.trim();
        let _ = resolve_inside(root, rel).map_err(|e| anyhow::anyhow!("{}: {e}", f.new_path))?;
    }
    Ok(files)
}

fn check_path(p: &str) -> Result<()> {
    if p.contains("..") {
        bail!("path escape denied: {p}");
    }
    if p.is_empty() {
        bail!("empty path in patch header");
    }
    Ok(())
}

fn parse_hunk_header(line: &str) -> Result<Hunk> {
    // Format: @@ -old_start[,old_lines] +new_start[,new_lines] @@
    let inner = line
        .strip_prefix("@@ ")
        .and_then(|s| s.split(" @@").next())
        .ok_or_else(|| anyhow::anyhow!("malformed hunk header: {line}"))?;
    let mut parts = inner.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad hunk {line}"))?;
    let new = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad hunk {line}"))?;
    let (old_start, old_lines) = parse_range(old.trim_start_matches('-'))?;
    let (new_start, new_lines) = parse_range(new.trim_start_matches('+'))?;
    Ok(Hunk {
        old_start: old_start.max(1),
        old_lines,
        new_start: new_start.max(1),
        new_lines,
        body: Vec::new(),
    })
}

fn parse_range(s: &str) -> Result<(usize, usize)> {
    if let Some((a, b)) = s.split_once(',') {
        Ok((a.parse()?, b.parse()?))
    } else {
        Ok((s.parse()?, 1))
    }
}

/// Apply patch set atomically. All hunks must apply or zero files change.
/// Returns touched paths. Creates `<file>.aicli.bak.<ts>` before write.
pub fn apply_patch_set(root: &Path, diff: &str) -> Result<Vec<String>> {
    let files = parse_patch(diff, root)?;
    // Stage 1: compute new contents without touching disk.
    let mut staged: Vec<(std::path::PathBuf, String)> = Vec::new();
    for f in &files {
        let rel = f.new_path.trim();
        // resolve_inside denies secrets and escapes.
        let abs = resolve_inside(root, rel)?;
        let old_text = std::fs::read_to_string(&abs).unwrap_or_default();
        let new_text = apply_file(&old_text, f)?;
        staged.push((abs, new_text));
    }
    // Stage 2: backup then atomic write via temp plus rename.
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let mut touched = Vec::new();
    for (abs, new_text) in staged {
        if abs.exists() {
            let bak = abs.with_extension(format!("aicli.bak.{ts}"));
            std::fs::copy(&abs, &bak)?;
        }
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = abs.with_extension(format!("aicli.tmp.{ts}"));
        std::fs::write(&tmp, &new_text)?;
        std::fs::rename(&tmp, &abs)?;
        touched.push(abs.to_string_lossy().to_string());
    }
    Ok(touched)
}

fn apply_file(old_text: &str, patch: &FilePatch) -> Result<String> {
    let mut old_lines: Vec<&str> = old_text.lines().collect();
    // Apply hunks in order. Verify context lines match.
    // Track offset as hunks apply sequentially on evolving buffer.
    let mut buf: Vec<String> = old_lines.iter().map(|s| s.to_string()).collect();
    let mut offset: i64 = 0;
    for h in &patch.hunks {
        let mut idx = (h.old_start as i64 - 1 + offset) as usize;
        let mut del = 0usize;
        let mut add: Vec<String> = Vec::new();
        for line in &h.body {
            if let Some(expect) = line.strip_prefix(' ') {
                let got = buf.get(idx).map(|s| s.as_str()).unwrap_or("");
                if got != expect {
                    anyhow::bail!(
                        "hunk context mismatch at line {}: expected {expect:?} got {got:?}",
                        idx + 1
                    );
                }
                idx += 1;
            } else if let Some(expect) = line.strip_prefix('-') {
                let got = buf.get(idx).map(|s| s.as_str()).unwrap_or("");
                if got != expect {
                    anyhow::bail!(
                        "hunk delete mismatch at line {}: expected {expect:?} got {got:?}",
                        idx + 1
                    );
                }
                buf.remove(idx);
                del += 1;
                offset -= 1;
            } else if let Some(added) = line.strip_prefix('+') {
                add.push(added.to_string());
            } else if line.trim().is_empty() {
                idx += 1;
            }
        }
        // Insert collected adds at final idx position.
        // idx already advanced past context and deletes.
        let insert_at = idx;
        for (k, a) in add.into_iter().enumerate() {
            buf.insert(insert_at + k, a);
            offset += 1;
        }
        let _ = del;
    }
    let mut out = buf.join("\n");
    if old_text.ends_with('\n') || !out.is_empty() {
        out.push('\n');
    }
    let _ = &mut old_lines;
    Ok(out)
}

/// Word diff for UI preview using similar crate.
pub fn word_diff(old: &str, new: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            out.push_str(&format!("{sign}{change}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_minimal_diff() {
        let d = "--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,3 @@\n line1\n-line2\n+line2x\n+line3\n";
        assert_eq!(validate_patch(d, ".").unwrap(), 1);
    }

    #[test]
    fn rejects_escape() {
        let d = "--- a/../x\n+++ b/../x\n@@ -1 +1 @@\n+x\n";
        assert!(validate_patch(d, ".").is_err());
    }

    #[test]
    fn atomic_apply_all_or_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(root.join("b.txt"), "alpha\nbeta\n").unwrap();
        // Second file hunk is wrong on purpose, nothing must change.
        let bad = "--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+two2\n--- a/b.txt\n+++ b/b.txt\n@@ -1,2 +1,2 @@\n WRONG\n-beta\n+beta2\n";
        assert!(apply_patch_set(root, bad).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "one\ntwo\n"
        );
        let good = "--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+two2\n";
        let touched = apply_patch_set(root, good).unwrap();
        assert_eq!(touched.len(), 1);
        assert!(std::fs::read_to_string(root.join("a.txt"))
            .unwrap()
            .contains("two2"));
    }
}
