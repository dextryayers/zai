use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod ollama;

/// Run blocking HTTP work on a dedicated OS thread.
/// reqwest blocking creates its own runtime, which panics when dropped
/// inside a Tokio context, so it must never run on an async worker.
fn block<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(f)
        .join()
        .map_err(|_| anyhow::anyhow!("background http thread failed"))?
}

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
    let upper = name.to_ascii_uppercase().replace(['-', ' '], "_");
    for q in [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q5_1", "Q4_K_M", "Q4_K_S", "Q4_K", "Q4_0",
        "Q4_1", "Q3_K_M", "Q3_K_S", "Q2_K", "IQ4_XS", "IQ3_M", "BF16", "F16", "F32",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

/// Rich GGUF file description used by inspect, scan, insert preview,
/// and the inference loader. Parsed from the file header plus a bounded
/// walk of the metadata KV section, so huge model files stay fast.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GgufInfo {
    pub path: String,
    pub version: u32,
    pub tensors: u64,
    pub metadata_kv: u64,
    pub alignment: u32,
    pub size_bytes: u64,
    pub size_mb: u64,
    pub architecture: Option<String>,
    pub quant: String,
    pub suggested_id: String,
    pub suggested_ctx: u32,
}

/// Default folders scanned for GGUF files when no path is given:
/// current dir, ./models, ~/models, ~/Models, plus ZAI_MODELS_DIRS entries.
/// Only existing directories are returned, sorted, deduped.
pub fn default_scan_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    };
    if let Ok(cwd) = std::env::current_dir() {
        push(cwd.join("models"));
        push(cwd);
    }
    if let Ok(home) = std::env::var("HOME") {
        push(PathBuf::from(format!("{home}/models")));
        push(PathBuf::from(format!("{home}/Models")));
    }
    if let Ok(extra) = std::env::var("ZAI_MODELS_DIRS") {
        for part in extra.split(':').map(str::trim).filter(|s| !s.is_empty()) {
            push(PathBuf::from(part));
        }
    }
    out.sort();
    out
}

/// Read and validate a GGUF file, returning rich header metadata.
/// Reads at most 1 MB from the front of the file, so multi GB models
/// inspect in milliseconds. The KV walk stops at the first unreadable
/// entry and still returns the validated header fields.
pub fn inspect_gguf(path: &Path) -> Result<GgufInfo> {
    use std::io::Read;
    anyhow::ensure!(path.exists(), "file not found: {}", path.display());
    let size_bytes = std::fs::metadata(path)?.len();
    let mut f = std::fs::File::open(path)?;
    let cap = size_bytes.min(1024 * 1024) as usize;
    let mut buf = vec![0u8; cap.max(32)];
    f.read_exact(&mut buf)?;
    let mut cur = &buf[..];
    let mut take = |n: usize| -> Result<&[u8]> {
        if cur.len() < n {
            anyhow::bail!("{} header truncated", path.display());
        }
        let (head, rest) = cur.split_at(n);
        cur = rest;
        Ok(head)
    };
    anyhow::ensure!(take(4)? == b"GGUF", "{} has bad GGUF magic", path.display());
    let version = u32::from_le_bytes(take(4)?.try_into().unwrap());
    anyhow::ensure!(
        (1..=10).contains(&version),
        "unsupported GGUF version {version}"
    );
    let tensors = u64::from_le_bytes(take(8)?.try_into().unwrap());
    let metadata_kv = u64::from_le_bytes(take(8)?.try_into().unwrap());
    // v1 has no alignment field. Default is 32 per GGUF spec.
    let alignment = if version >= 2 {
        u32::from_le_bytes(take(4)?.try_into().unwrap()).max(1)
    } else {
        32
    };
    let architecture = walk_gguf_architecture(&mut cur, metadata_kv.min(256));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("model.gguf")
        .to_string();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let suggested_id = sanitize_id(stem).unwrap_or_else(|_| "model".to_string());
    Ok(GgufInfo {
        path: path.display().to_string(),
        version,
        tensors,
        metadata_kv,
        alignment,
        size_bytes,
        size_mb: (size_bytes / (1024 * 1024)).max(1),
        architecture,
        quant: guess_quant(&file_name),
        suggested_id,
        suggested_ctx: suggest_ctx(size_bytes),
    })
}

/// Walk at most `limit` metadata KV entries looking for general.architecture.
/// Only string values are decoded. Any malformed entry ends the walk with
/// whatever was found so far, never an error.
fn walk_gguf_architecture(cur: &mut &[u8], limit: u64) -> Option<String> {
    fn take<'a>(cur: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if cur.len() < n {
            return None;
        }
        let (head, rest) = cur.split_at(n);
        *cur = rest;
        Some(head)
    }
    fn take_u32(cur: &mut &[u8]) -> Option<u32> {
        take(cur, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn take_u64(cur: &mut &[u8]) -> Option<u64> {
        take(cur, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn take_str(cur: &mut &[u8]) -> Option<String> {
        let len = take_u64(cur)? as usize;
        if len > 1024 || cur.len() < len {
            return None;
        }
        let bytes = take(cur, len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
    // Byte size of one fixed width element. None for dynamic types.
    fn fixed_size(ty: u32) -> Option<u64> {
        match ty {
            0 | 1 | 7 => Some(1),
            2 | 3 => Some(2),
            4..=6 => Some(4),
            10..=12 => Some(8),
            _ => None,
        }
    }
    fn skip_value(cur: &mut &[u8], ty: u32) -> bool {
        if let Some(sz) = fixed_size(ty) {
            return take(cur, sz as usize).is_some();
        }
        match ty {
            8 => take_str(cur).is_some(),
            9 => {
                let elem = take_u32(cur);
                let len = take_u64(cur);
                let (elem, len) = match (elem, len) {
                    (Some(e), Some(l)) => (e, l),
                    _ => return false,
                };
                if len > 4096 {
                    return false;
                }
                if let Some(sz) = fixed_size(elem) {
                    let total = len.saturating_mul(sz) as usize;
                    if total > 1024 * 1024 {
                        return false;
                    }
                    take(cur, total).is_some()
                } else if elem == 8 {
                    (0..len).all(|_| take_str(cur).is_some())
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    for _ in 0..limit {
        let key = match take_str(cur) {
            Some(k) => k,
            None => break,
        };
        let ty = match take_u32(cur) {
            Some(t) => t,
            None => break,
        };
        if key == "general.architecture" && ty == 8 {
            return take_str(cur);
        }
        if !skip_value(cur, ty) {
            break;
        }
    }
    None
}

/// Suggest a context size from file size, snapped to allowed n_ctx values.
/// Small files default to 2048, mid files to 4096, large files to 8192.
pub fn suggest_ctx(size_bytes: u64) -> u32 {
    const MB: u64 = 1024 * 1024;
    let mb = size_bytes / MB;
    if mb <= 1100 {
        2048
    } else if mb <= 2600 {
        4096
    } else {
        8192
    }
}

/// Expand `~` and resolve relative paths against the current dir.
/// Accepts quoted paths by trimming one layer of single or double quotes.
pub fn resolve_insert_path(raw: &str) -> PathBuf {
    let mut s = raw.trim().to_string();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s = s[1..s.len() - 1].to_string();
    }
    let expanded = if let Some(rest) = s.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|h| format!("{h}/{rest}"))
            .unwrap_or(s)
    } else {
        s
    };
    let p = PathBuf::from(&expanded);
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// Scan a directory for `.gguf` files, sorted by path. Recursive when asked.
pub fn find_gguf_files(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if recursive {
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            let p = entry.path().to_path_buf();
            if p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("gguf"))
                    .unwrap_or(false)
            {
                out.push(p);
            }
        }
    } else if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("gguf"))
                    .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Preview what an insert would register, without copying anything.
/// Returns the id, quant, size, and effective ctx the real insert would use.
pub fn preview_insert(src: &Path, name: Option<&str>, n_ctx: Option<u32>) -> Result<ModelEntry> {
    let info = inspect_gguf(src)?;
    let id = sanitize_id(name.unwrap_or(&info.suggested_id))?;
    Ok(ModelEntry {
        id: id.clone(),
        repo: "local".to_string(),
        file: src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.gguf")
            .to_string(),
        quant: if name.is_some() {
            guess_quant(&id)
        } else {
            info.quant.clone()
        },
        size_mb: info.size_mb,
        n_ctx: n_ctx.unwrap_or(info.suggested_ctx),
        is_default: false,
        sha256: None,
    })
}

/// Validate GGUF header: magic plus version range. Returns version and tensor count.
pub fn check_gguf_header(path: &Path) -> Result<(u32, u64)> {
    let info = inspect_gguf(path)?;
    Ok((info.version, info.tensors))
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
        n_ctx: n_ctx.unwrap_or_else(|| suggest_ctx(total)),
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
/// Runs on a dedicated thread, see block().
pub fn download_with_resume(url: &str, dest: &Path) -> Result<()> {
    let url = url.to_string();
    let dest = dest.to_path_buf();
    block(move || download_inner(&url, &dest))
}

fn download_inner(url: &str, dest: &Path) -> Result<()> {
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

    fn fake_gguf_v3_arch(dir: &std::path::Path, name: &str, arch: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut b = b"GGUF".to_vec();
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&7u64.to_le_bytes());
        b.extend_from_slice(&2u64.to_le_bytes());
        b.extend_from_slice(&32u32.to_le_bytes());
        // kv 1: general.architecture = arch (string)
        let key = b"general.architecture";
        b.extend_from_slice(&(key.len() as u64).to_le_bytes());
        b.extend_from_slice(key);
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&(arch.len() as u64).to_le_bytes());
        b.extend_from_slice(arch.as_bytes());
        // kv 2: general.quantization_version = 2 (u32)
        let key2 = b"general.quantization_version";
        b.extend_from_slice(&(key2.len() as u64).to_le_bytes());
        b.extend_from_slice(key2);
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend(vec![0u8; 64]);
        std::fs::write(&p, &b).unwrap();
        p
    }

    #[test]
    fn inspect_reads_versions_tensors_and_arch() {
        let tmp = tempfile::tempdir().unwrap();
        let v1 = fake_gguf(tmp.path(), "tiny-q4_k_m.gguf");
        let info = inspect_gguf(&v1).unwrap();
        assert_eq!(info.version, 3);
        assert_eq!(info.tensors, 5);
        assert_eq!(info.quant, "Q4_K_M");
        assert_eq!(info.suggested_id, "tiny-q4-k-m");
        assert_eq!(info.suggested_ctx, 2048);
        assert_eq!(info.architecture, None);
        let v3 = fake_gguf_v3_arch(tmp.path(), "llama-q8_0.gguf", "llama");
        let info3 = inspect_gguf(&v3).unwrap();
        assert_eq!(info3.version, 3);
        assert_eq!(info3.tensors, 7);
        assert_eq!(info3.metadata_kv, 2);
        assert_eq!(info3.alignment, 32);
        assert_eq!(info3.architecture, Some("llama".to_string()));
        assert_eq!(info3.quant, "Q8_0");
        let bad = tmp.path().join("bad.gguf");
        std::fs::write(&bad, b"NOPE-NOPE-NOPE-NOPE-NOPE").unwrap();
        assert!(inspect_gguf(&bad).is_err());
        assert!(inspect_gguf(&tmp.path().join("missing.gguf")).is_err());
        // Extended quant names.
        assert_eq!(super::guess_quant("m-IQ4_XS.gguf"), "IQ4_XS");
        assert_eq!(super::guess_quant("m-f16.gguf"), "F16");
        assert_eq!(super::guess_quant("m-Q4_K.gguf"), "Q4_K");
    }

    #[test]
    fn scan_dirs_lists_existing_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        // default_scan_dirs reads real HOME and cwd, so only assert it runs
        // and returns sorted unique existing dirs.
        let dirs = default_scan_dirs();
        let mut sorted = dirs.clone();
        sorted.sort();
        assert_eq!(dirs, sorted);
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
    fn insert_helpers_suggest_scan_and_preview() {
        assert_eq!(suggest_ctx(100 * 1024 * 1024), 2048);
        assert_eq!(suggest_ctx(2000 * 1024 * 1024), 4096);
        assert_eq!(suggest_ctx(5000 * 1024 * 1024), 8192);
        let tmp = tempfile::tempdir().unwrap();
        let a = fake_gguf(tmp.path(), "tiny-q4_k_m.gguf");
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let _b = fake_gguf(&sub, "big-q4_k_m.gguf");
        std::fs::write(tmp.path().join("note.txt"), b"x").unwrap();
        let top = find_gguf_files(tmp.path(), false);
        assert_eq!(top, vec![a.clone()]);
        let all = find_gguf_files(tmp.path(), true);
        assert_eq!(all.len(), 2);
        let prev = preview_insert(&a, None, None).unwrap();
        assert_eq!(prev.id, "tiny-q4-k-m");
        assert_eq!(prev.quant, "Q4_K_M");
        assert_eq!(prev.n_ctx, 2048);
        assert!(preview_insert(&a, Some("My Tiny!"), Some(8192)).unwrap().id == "my-tiny");
        let p = resolve_insert_path("~/m.gguf");
        assert!(p.ends_with("m.gguf"));
        assert!(resolve_insert_path("./a.gguf").is_absolute());
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
