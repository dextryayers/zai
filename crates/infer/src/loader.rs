use anyhow::{Context, Result};
use std::path::Path;

/// Backend error codes from plan.md Section 11.
#[derive(Debug, Clone)]
pub enum InferError {
    ModelMissing(String),
    CtxOverflow,
    Timeout,
    Backend(String),
}

impl std::fmt::Display for InferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelMissing(id) => write!(
                f,
                "E_MODEL_MISSING: {id} not found. Run: aicli models pull {id}"
            ),
            Self::CtxOverflow => write!(
                f,
                "E_CTX_OVERFLOW: context full. Run /ctx compact or raise n_ctx"
            ),
            Self::Timeout => write!(f, "E_TIMEOUT: generation timed out"),
            Self::Backend(e) => write!(f, "E_BACKEND: {e}"),
        }
    }
}

impl std::error::Error for InferError {}

#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub path: String,
    pub size_bytes: u64,
    pub gguf_version: u32,
    pub tensor_count: u64,
    pub n_ctx: u32,
    pub n_threads: u32,
    pub n_gpu_layers: u32,
    pub llama_cpp: String,
}

/// Validate GGUF file header and report backend info.
/// Real token inference via llama-cpp-2 lands behind `llama` feature in Phase 2 full.
/// This loader already enforces offline errors, OOM hints, and corrupt file paths.
pub fn load_info(
    path: &Path,
    n_ctx: u32,
    n_threads: u32,
    n_gpu_layers: u32,
) -> Result<BackendInfo> {
    let p = path.to_string_lossy().to_string();
    if !path.exists() {
        return Err(InferError::ModelMissing(p).into());
    }
    let md = std::fs::metadata(path).with_context(|| format!("stat {p}"))?;
    if md.len() < 32 {
        return Err(InferError::Backend(format!("file too small to be GGUF: {p}")).into());
    }
    let mut f = std::fs::File::open(path)?;
    use std::io::Read;
    let mut hdr = [0u8; 32];
    f.read_exact(&mut hdr)?;
    if &hdr[0..4] != b"GGUF" {
        return Err(InferError::Backend(format!("bad GGUF magic: {p}")).into());
    }
    let version = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
    let tensors = u64::from_le_bytes(hdr[8..16].try_into().unwrap());
    // Bytes 16..24 are metadata KV count in GGUF v3. Kept for diagnostics.
    if version == 0 || version > 10 {
        return Err(InferError::Backend(format!("unsupported GGUF version {version}: {p}")).into());
    }
    // OOM hint: refuse ctx that needs roughly more than 2x file size in RAM estimate.
    // Rough estimate: 1.5 MB per ctx token for 3B class plus file size.
    let need_mb = md.len() / (1024 * 1024) + (n_ctx as u64 * 2) / 1024;
    let avail_mb = available_ram_mb();
    if avail_mb > 0 && need_mb > avail_mb {
        return Err(InferError::Backend(format!(
            "likely OOM: need about {need_mb} MB, have {avail_mb} MB. Lower --ctx or use 1B model"
        ))
        .into());
    }
    Ok(BackendInfo {
        path: p,
        size_bytes: md.len(),
        gguf_version: version,
        tensor_count: tensors,
        n_ctx,
        n_threads: if n_threads == 0 {
            default_threads()
        } else {
            n_threads
        },
        n_gpu_layers,
        llama_cpp: "b5000-compat (mock backend, llama feature in next step)".to_string(),
    })
}

fn default_threads() -> u32 {
    std::thread::available_parallelism()
        .map(|n| (n.get() as u32).saturating_sub(1).max(2))
        .unwrap_or(4)
}

fn available_ram_mb() -> u64 {
    // Read /proc/meminfo on Linux, 0 when unknown so callers skip the hint.
    if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                return kb / 1024;
            }
        }
    }
    0
}
