use anyhow::{bail, Result};

/// Validate unified diff without applying. Phase 4 adds atomic apply.
pub fn validate_patch(diff: &str, root: &str) -> Result<usize> {
    let mut files = 0;
    let mut has_hunk = false;
    for line in diff.lines() {
        if line.starts_with("--- a/") || line.starts_with("+++ b/") {
            let path = line[6..].trim();
            if path.contains("..") {
                bail!("path escape denied: {path}");
            }
            if !path.is_empty() {
                files += 1;
            }
        }
        if line.starts_with("@@ ") {
            has_hunk = true;
        }
        if line.starts_with("Binary ") {
            bail!("binary patch not allowed in v1");
        }
    }
    if root.is_empty() {
        bail!("empty root");
    }
    if !has_hunk {
        bail!("no hunks found, expected @@ blocks");
    }
    Ok(files / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_minimal_diff() {
        let d = "--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,3 @@\n+line\n";
        assert_eq!(validate_patch(d, ".").unwrap(), 1);
    }

    #[test]
    fn rejects_escape() {
        let d = "--- a/../x\n+++ b/../x\n@@ -1 +1 @@\n+x\n";
        assert!(validate_patch(d, ".").is_err());
    }
}
