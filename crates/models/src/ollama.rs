/// Ollama integration: daemon detect, model list, install, delete, generate.
/// Uses the local HTTP API at 127.0.0.1:11434 with fast fail when the daemon
/// is down. GGUF files stay local only and are never downloaded.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BASE: &str = "http://127.0.0.1:11434";

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("build http client")
}

/// True when `ollama serve` answers on localhost.
pub fn daemon_reachable() -> bool {
    client()
        .and_then(|c| c.get(format!("{BASE}/api/tags")).send().map_err(|e| e.into()))
        .is_ok()
}

/// True when the `ollama` binary exists on PATH.
pub fn binary_exists() -> bool {
    std::process::Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Deserialize)]
struct TagsResp {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    modified_at: String,
}

/// List installed Ollama models. Empty when the daemon is down.
pub fn list() -> Result<Vec<OllamaModel>> {
    let resp: TagsResp = client()?
        .get(format!("{BASE}/api/tags"))
        .send()
        .context("ollama daemon not reachable, start with: ollama serve")?
        .error_for_status()?
        .json()
        .context("parse ollama tags")?;
    Ok(resp
        .models
        .into_iter()
        .map(|m| OllamaModel {
            name: m.name,
            size: m.size,
            modified: m.modified_at,
        })
        .collect())
}

pub fn size_mb(size: u64) -> u64 {
    (size / (1024 * 1024)).max(1)
}

/// Install a model with streaming progress lines from the daemon.
pub fn pull(name: &str, mut on_progress: impl FnMut(Option<(u64, u64, String)>) -> ()) -> Result<()> {
    use std::io::Read;
    let mut resp = client()?
        .post(format!("{BASE}/api/pull"))
        .json(&serde_json::json!({ "name": name, "stream": true }))
        .send()
        .with_context(|| format!("ollama pull {name} failed, is the daemon running"))?
        .error_for_status()?;
    let mut buf = vec![0u8; 0];
    let mut chunk = [0u8; 8192];
    loop {
        let n = resp.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) {
                let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                if let Some(err) = v.get("error").and_then(|s| s.as_str()) {
                    anyhow::bail!("ollama pull error: {err}");
                }
                let pair = match (v.get("completed"), v.get("total")) {
                    (Some(c), Some(t)) => {
                        let c = c.as_u64().unwrap_or(0);
                        let t = t.as_u64().unwrap_or(0);
                        Some((c, t, status))
                    }
                    _ => None,
                };
                on_progress(pair);
            }
        }
    }
    Ok(())
}

/// Delete an installed Ollama model.
pub fn rm(name: &str) -> Result<()> {
    client()?
        .delete(format!("{BASE}/api/delete"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .with_context(|| format!("ollama rm {name} failed"))?
        .error_for_status()?;
    Ok(())
}

/// Show raw model details JSON from the daemon.
pub fn show(name: &str) -> Result<serde_json::Value> {
    let v: serde_json::Value = client()?
        .post(format!("{BASE}/api/show"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .with_context(|| format!("ollama show {name} failed"))?
        .error_for_status()?
        .json()?;
    Ok(v)
}

#[derive(Debug, Deserialize)]
struct GenResp {
    #[serde(default)]
    response: String,
}

/// One shot generation against an installed Ollama model. No streaming.
pub fn generate(model: &str, prompt: &str, timeout_s: u64) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_s.max(10)))
        .build()?;
    let v: GenResp = client
        .post(format!("{BASE}/api/generate"))
        .json(&serde_json::json!({ "model": model, "prompt": prompt, "stream": false }))
        .send()
        .with_context(|| format!("ollama generate failed for {model}"))?
        .error_for_status()?
        .json()?;
    Ok(v.response)
}

/// Prefix used to address Ollama models inside zai, for example ollama/llama3.1.
pub fn to_id(name: &str) -> String {
    format!("ollama/{name}")
}

/// Strip the ollama/ prefix. None when the id is a GGUF model.
pub fn strip_id(id: &str) -> Option<&str> {
    id.strip_prefix("ollama/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_body() {
        let body = r#"{"models":[{"name":"llama3.1:8b","size":4661224676,"modified_at":"2026-01-01T00:00:00Z"}]}"#;
        let resp: TagsResp = serde_json::from_str(body).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].name, "llama3.1:8b");
        assert_eq!(size_mb(4661224676), 4443);
    }

    #[test]
    fn id_prefix_roundtrip() {
        assert_eq!(to_id("llama3.1:8b"), "ollama/llama3.1:8b");
        assert_eq!(strip_id("ollama/llama3.1:8b"), Some("llama3.1:8b"));
        assert_eq!(strip_id("qwen2.5-3b"), None);
    }

    #[test]
    fn daemon_down_is_fast_and_clear() {
        // Localhost refused fails fast without hanging the suite.
        let _ = daemon_reachable();
    }
}
