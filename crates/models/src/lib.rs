use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub repo: String,
    pub file: String,
    pub quant: String,
    pub size_mb: u64,
    pub n_ctx: u32,
    pub is_default: bool,
    pub sha256: Option<String>,
}

pub fn builtin_registry() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "qwen2.5-3b-instruct-q4_k_m".to_string(),
            repo: "Qwen/Qwen2.5-3B-Instruct-GGUF".to_string(),
            file: "qwen2.5-3b-instruct-q4_k_m.gguf".to_string(),
            quant: "Q4_K_M".to_string(),
            size_mb: 1980,
            n_ctx: 4096,
            is_default: true,
            sha256: None,
        },
        ModelEntry {
            id: "llama-3.2-1b-instruct-q4_k_m".to_string(),
            repo: "meta-llama/Llama-3.2-1B-Instruct-GGUF".to_string(),
            file: "llama-3.2-1b-instruct-q4_k_m.gguf".to_string(),
            quant: "Q4_K_M".to_string(),
            size_mb: 800,
            n_ctx: 2048,
            is_default: false,
            sha256: None,
        },
    ]
}

pub fn find_model(id: &str) -> Option<ModelEntry> {
    builtin_registry().into_iter().find(|m| m.id == id)
}

pub fn hf_url(entry: &ModelEntry) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{}",
        entry.repo, entry.file
    )
}

pub fn local_path(models_dir: &Path, entry: &ModelEntry) -> PathBuf {
    models_dir.join(&entry.file)
}

pub fn is_cached(models_dir: &Path, entry: &ModelEntry) -> bool {
    let p = local_path(models_dir, entry);
    p.exists() && p.metadata().map(|m| m.len() > 1024 * 1024).unwrap_or(false)
}

/// Verify sha256 when expected is known. Returns true when no checksum pinned.
pub fn verify_sha256(path: &Path, expected: Option<&str>) -> Result<bool> {
    let exp = match expected {
        Some(e) if !e.trim().is_empty() => e.trim().to_lowercase(),
        _ => return Ok(true),
    };
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = format!("{:x}", hasher.finalize());
    Ok(got == exp)
}

/// Download with resume via HTTP Range and elegant indicatif bar.
/// Writes to dest.part then renames on complete verify.
pub fn download_with_resume(url: &str, dest: &Path) -> Result<()> {
    let part = dest.with_extension("part");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let have: u64 = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let client = reqwest::blocking::Client::builder()
        .user_agent("aicli/0.2")
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let total: Option<u64> = if have > 0 {
        // Probe total with HEAD, fallback to GET size.
        client.head(url).send().ok().and_then(|r| {
            r.headers()
                .get(reqwest::header::CONTENT_LENGTH)?
                .to_str()
                .ok()?
                .parse()
                .ok()
        })
    } else {
        None
    };

    let mut req = client.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut resp = req.send().with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        anyhow::bail!("download failed: HTTP {} for {url}", resp.status());
    }
    let total_size = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|n| n + have)
        .or(total);

    let bar = indicatif::ProgressBar::new(total_size.unwrap_or(0));
    bar.set_style(
        indicatif::ProgressStyle::with_template(
            "{msg} ... {bytes}/{total_bytes} | {bytes_per_sec} | {percent}% | {eta}",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    bar.set_message("pull");
    if have > 0 {
        bar.set_position(have);
    }

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)?;
    use std::io::Write;
    let mut buf = [0u8; 65536];
    loop {
        use std::io::Read;
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        bar.inc(n as u64);
    }
    bar.finish_and_clear();
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Pull model by id into models_dir. Real download first, clear offline error on failure.
pub fn pull(id: &str, models_dir: &Path) -> Result<PathBuf> {
    let entry = find_model(id).ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
    let dest = local_path(models_dir, &entry);
    if is_cached(models_dir, &entry) {
        if verify_sha256(&dest, entry.sha256.as_deref())? {
            return Ok(dest);
        }
        // Corrupt cache: remove and re download.
        let _ = std::fs::remove_file(&dest);
    }
    let url = hf_url(&entry);
    download_with_resume(&url, &dest).with_context(|| {
        format!("E_OFFLINE: pull failed for {id}. Check network then retry: aicli models pull {id}")
    })?;
    if !verify_sha256(&dest, entry.sha256.as_deref())? {
        let _ = std::fs::remove_file(&dest);
        anyhow::bail!("sha256 mismatch for {id}, removed corrupt file");
    }
    Ok(dest)
}

/// Simulated pull with elegant progress. Used for demo and offline tests.
pub fn pull_simulated(id: &str) -> Result<()> {
    use aicli_ui::progress::download_line;
    let m = find_model(id).ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
    for pct in (0..=100).step_by(5) {
        let done = m.size_mb as f64 * pct as f64 / 100.0;
        let speed = 24.5;
        print!("\r{}", download_line(&m.id, done, m.size_mb as f64, speed));
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let e = find_model("qwen2.5-3b-instruct-q4_k_m").unwrap();
        let p = local_path(tmp.path(), &e);
        assert!(p.ends_with(&e.file));
        assert!(hf_url(&e).starts_with("https://huggingface.co/"));
    }

    #[test]
    fn sha_verify_detects_flip() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f.bin");
        std::fs::write(&p, b"abc").unwrap();
        // sha256 of "abc".
        assert!(verify_sha256(
            &p,
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        )
        .unwrap());
        assert!(!verify_sha256(
            &p,
            Some("0000000000000000000000000000000000000000000000000000000000000000")
        )
        .unwrap());
    }
}
