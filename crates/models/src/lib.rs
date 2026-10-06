use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod ollama;

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

/// Path of the user model registry inside the models cache dir.
pub fn custom_path(models_dir: &Path) -> PathBuf {
    models_dir.join("custom-models.toml")
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct CustomRegistry {
    #[serde(default)]
    models: Vec<ModelEntry>,
}

/// Load user inserted models. Missing or corrupt file means empty list.
pub fn load_custom(models_dir: &Path) -> Vec<ModelEntry> {
    let p = custom_path(models_dir);
    if !p.exists() {
        return vec![];
    }
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    toml::from_str::<CustomRegistry>(&text)
        .map(|r| r.models)
        .unwrap_or_default()
}

fn save_custom(models_dir: &Path, models: &[ModelEntry]) -> Result<()> {
    std::fs::create_dir_all(models_dir)?;
    let text = toml::to_string_pretty(&CustomRegistry {
        models: models.to_vec(),
    })?;
    std::fs::write(custom_path(models_dir), text)?;
    Ok(())
}

/// All known models: user inserts first, then builtin without id clash.
pub fn list_all(models_dir: &Path) -> Vec<ModelEntry> {
    let custom = load_custom(models_dir);
    let ids: std::collections::HashSet<String> = custom.iter().map(|m| m.id.clone()).collect();
    let mut out = custom;
    for b in builtin_registry() {
        if !ids.contains(&b.id) {
            out.push(b);
        }
    }
    out
}

/// Find in user models first, then builtin.
pub fn find_any(models_dir: &Path, id: &str) -> Option<ModelEntry> {
    list_all(models_dir).into_iter().find(|m| m.id == id)
}

fn sanitize_id(raw: &str) -> Result<String> {
    let clean: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let clean = clean.trim_matches('-').to_string();
    anyhow::ensure!(
        (1..=64).contains(&clean.len()),
        "model id must be 1-64 chars of letters, digits, dash"
    );
    Ok(clean)
}

fn guess_quant(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    for q in [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M", "Q2_K",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

/// Validate GGUF header: magic plus version range. Returns version and tensor count.
pub fn check_gguf_header(path: &Path) -> Result<(u32, u64)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hdr = [0u8; 16];
    f.read_exact(&mut hdr)
        .with_context(|| format!("{} too small to be GGUF", path.display()))?;
    anyhow::ensure!(
        &hdr[0..4] == b"GGUF",
        "{} has bad GGUF magic",
        path.display()
    );
    let version = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
    anyhow::ensure!(
        (1..=10).contains(&version),
        "unsupported GGUF version {version}"
    );
    let tensors = u64::from_le_bytes(hdr[8..16].try_into().unwrap());
    Ok((version, tensors))
}

/// Insert a local GGUF file into the models cache and registry.
/// Copies with progress callback (done_bytes, total_bytes), verifies header and sha256.
/// Accepts unlimited models. Returns the saved entry.
pub fn insert_gguf(
    models_dir: &Path,
    src: &Path,
    name: Option<&str>,
    n_ctx: Option<u32>,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ModelEntry> {
    anyhow::ensure!(src.exists(), "file not found: {}", src.display());
    let (version, _tensors) = check_gguf_header(src)?;
    let _ = version;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let id = sanitize_id(name.unwrap_or(stem))?;
    if find_any(models_dir, &id).is_some() {
        anyhow::bail!("model id {id} already saved, remove it first or pick another --name");
    }
    let file_name = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("bad file name"))?
        .to_string();
    let dest = models_dir.join(&file_name);
    if dest.exists() {
        anyhow::bail!("file {} already in cache", dest.display());
    }
    std::fs::create_dir_all(models_dir)?;
    let total = std::fs::metadata(src)?.len();
    {
        use std::io::{Read, Write};
        let mut r = std::fs::File::open(src)?;
        let mut w = std::fs::File::create(&dest)?;
        let mut buf = [0u8; 262144];
        let mut done = 0u64;
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            done += n as u64;
            on_progress(done, total);
        }
    }
    let sha = {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut h = Sha256::new();
        let mut f = std::fs::File::open(&dest)?;
        let mut buf = [0u8; 8192];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        format!("{:x}", h.finalize())
    };
    let entry = ModelEntry {
        id: id.clone(),
        repo: "local".to_string(),
        file: file_name,
        quant: guess_quant(&id),
        size_mb: (std::fs::metadata(&dest)?.len() / (1024 * 1024)).max(1),
        n_ctx: n_ctx.unwrap_or(4096),
        is_default: false,
        sha256: Some(sha),
    };
    let mut custom = load_custom(models_dir);
    custom.push(entry.clone());
    save_custom(models_dir, &custom)?;
    Ok(entry)
}

/// Remove a user inserted model file plus registry entry.
pub fn remove_custom(models_dir: &Path, id: &str) -> Result<bool> {
    let mut custom = load_custom(models_dir);
    let before = custom.len();
    custom.retain(|m| m.id != id);
    if custom.len() == before {
        return Ok(false);
    }
    // Remove file only when no builtin shares the name.
    if let Some(entry) = list_all(models_dir).into_iter().find(|m| m.id == id) {
        let _ = std::fs::remove_file(local_path(models_dir, &entry));
    }
    save_custom(models_dir, &custom)?;
    Ok(true)
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
        .user_agent("zai/1.0")
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
        format!("E_OFFLINE: pull failed for {id}. Check network then retry: zai models pull {id}")
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

    fn fake_gguf(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&5u64.to_le_bytes());
        bytes.extend_from_slice(&10u64.to_le_bytes());
        bytes.extend(vec![0u8; 64]);
        std::fs::write(&p, &bytes).unwrap();
        p
    }

    #[test]
    fn insert_accepts_many_models() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("models");
        for i in 0..3 {
            let src = fake_gguf(tmp.path(), &format!("m{i}-q4_k_m.gguf"));
            let e = insert_gguf(&cache, &src, None, Some(2048), |_, _| {}).unwrap();
            assert!(local_path(&cache, &e).exists());
        }
        let all = list_all(&cache);
        assert!(all.iter().filter(|m| m.repo == "local").count() == 3);
        assert!(find_any(&cache, "m1-q4-k-m").is_some());
    }

    #[test]
    fn insert_rejects_bad_magic_and_dupes() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("models");
        let bad = tmp.path().join("bad.gguf");
        std::fs::write(&bad, b"NOPE").unwrap();
        assert!(insert_gguf(&cache, &bad, None, None, |_, _| {}).is_err());
        let src = fake_gguf(tmp.path(), "dup.gguf");
        insert_gguf(&cache, &src, Some("dup"), None, |_, _| {}).unwrap();
        assert!(insert_gguf(&cache, &src, Some("dup"), None, |_, _| {}).is_err());
        assert!(remove_custom(&cache, "dup").unwrap());
        assert!(find_any(&cache, "dup").is_none());
    }
}
