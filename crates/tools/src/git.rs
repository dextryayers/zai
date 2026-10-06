use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

/// Run git status and diff with caps. No shell, direct argv to avoid injection.
pub fn git_status(root: &Path) -> Result<String> {
    run_git(root, &["status", "--short"], Duration::from_secs(10))
}

pub fn git_diff(root: &Path, staged: bool) -> Result<String> {
    if staged {
        run_git(root, &["diff", "--cached"], Duration::from_secs(10))
    } else {
        run_git(root, &["diff"], Duration::from_secs(10))
    }
}

fn run_git(root: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let t0 = Instant::now();
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()?;
    if t0.elapsed() > timeout {
        anyhow::bail!("git timeout");
    }
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("not a git repository") {
            return Ok("not a git repository".to_string());
        }
        anyhow::bail!("git failed: {}", err.chars().take(300).collect::<String>());
    }
    if text.len() > 200 * 1024 {
        text.truncate(200 * 1024);
        text.push_str("\n... truncated at 200 KB");
    }
    Ok(text)
}
