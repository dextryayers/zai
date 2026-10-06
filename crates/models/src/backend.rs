use super::block;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Pinned llama.cpp release used for reproducible local backend builds.
pub const LLAMA_TAG: &str = "b11439";
/// Upstream source for the local backend build.
pub const LLAMA_REPO: &str = "https://github.com/ggml-org/llama.cpp";
/// Persistent server port. Fixed so every zai process shares one server.
pub const SERVER_PORT: u16 = 8011;

/// Candidate llama-cli locations in priority order for one shared bin dir.
/// System wide install wins so every user shares one build.
pub fn bin_candidates(shared_bin: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        PathBuf::from("/usr/local/bin/llama-cli"),
        shared_bin.join("llama-cli"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("llama-cli");
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// First executable llama-cli found, if any.
pub fn find_llama_cli() -> Option<PathBuf> {
    let shared = aicli_core::paths::shared_bin_dir();
    bin_candidates(&shared)
        .into_iter()
        .find(|p| p.is_file() && is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// First line of `llama-cli --version`, if it runs.
/// Reads stdout plus stderr because the version line lands on stderr.
pub fn llama_version(bin: &Path) -> Option<String> {
    let out = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let merged = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    merged
        .lines()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Backend state for status output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendState {
    pub binary: Option<String>,
    pub version: Option<String>,
    pub source_dir: String,
    pub source_present: bool,
    pub tag: String,
    pub server: Option<ServerInfo>,
}

/// One persistent llama-server instance, tracked by pidfile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub model: String,
    pub port: u16,
}

fn server_pidfile() -> std::path::PathBuf {
    aicli_core::paths::shared_data_dir().join("zai-server.json")
}

fn server_log() -> std::path::PathBuf {
    aicli_core::paths::shared_data_dir().join("zai-server.log")
}

/// Health probe for a llama-server port. Fast localhost check.
pub fn server_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    block(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        let resp = client.get(&url).send()?;
        Ok(resp.status().is_success())
    })
    .unwrap_or(false)
}

/// Live server from pidfile plus health probe, if any.
pub fn server_running() -> Option<ServerInfo> {
    let text = std::fs::read_to_string(server_pidfile()).ok()?;
    let info: ServerInfo = serde_json::from_str(&text).ok()?;
    if server_health(info.port) {
        Some(info)
    } else {
        None
    }
}

/// Candidate llama-server binaries, system wide first.
pub fn server_candidates(shared_bin: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        PathBuf::from("/usr/local/bin/llama-server"),
        shared_bin.join("llama-server"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("llama-server");
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// First executable llama-server found, if any.
pub fn find_llama_server() -> Option<PathBuf> {
    let shared = aicli_core::paths::shared_bin_dir();
    server_candidates(&shared)
        .into_iter()
        .find(|p| p.is_file() && is_executable(p))
}

/// Start the persistent server for one model, detached with a pidfile.
/// Returns the running instance, waiting up to a minute for model load.
/// When another model is served, it is replaced.
pub fn start_server(model_path: &Path, n_ctx: u32, threads: u32) -> Result<ServerInfo> {
    if let Some(cur) = server_running() {
        if Path::new(&cur.model) == model_path {
            return Ok(cur);
        }
        let _ = stop_server();
    }
    let bin = find_llama_server()
        .ok_or_else(|| anyhow::anyhow!("no llama-server binary, run: zai backend setup"))?;
    anyhow::ensure!(
        model_path.exists(),
        "model file not found: {}",
        model_path.display()
    );
    let data = aicli_core::paths::shared_data_dir();
    std::fs::create_dir_all(&data)?;
    let log = std::fs::File::create(server_log())?;
    let err = log.try_clone()?;
    let child = std::process::Command::new(&bin)
        .args([
            "-m",
            &model_path.to_string_lossy(),
            "--port",
            &SERVER_PORT.to_string(),
            "-c",
            &n_ctx.to_string(),
            "-t",
            &threads.to_string(),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .with_context(|| format!("spawn {}", bin.display()))?;
    // Detach: dropping the handle leaves the daemon reparented.
    let pid = child.id();
    std::mem::forget(child);
    let info = ServerInfo {
        pid,
        model: model_path.display().to_string(),
        port: SERVER_PORT,
    };
    std::fs::write(server_pidfile(), serde_json::to_string_pretty(&info)?)?;
    for _ in 0..120 {
        if server_health(SERVER_PORT) {
            // Health turns green before weights finish loading. Confirm
            // with a tiny probe that is not a 503.
            if server_loaded(SERVER_PORT) {
                return Ok(info);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    anyhow::bail!(
        "server did not finish loading in 60s, see {}",
        server_log().display()
    )
}

/// True when the server answers completions instead of 503 loading.
fn server_loaded(port: u16) -> bool {
    block(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&serde_json::json!({
                "messages": [{"role": "user", "content": "Reply with exactly: PING"}],
                "temperature": 0.0,
                "max_tokens": 4,
            }))
            .send()?;
        Ok(resp.status().is_success())
    })
    .unwrap_or(false)
}

/// Stop the persistent server, if any. True when something stopped.
pub fn stop_server() -> Result<bool> {
    let text = match std::fs::read_to_string(server_pidfile()) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let _ = std::fs::remove_file(server_pidfile());
    if let Ok(info) = serde_json::from_str::<ServerInfo>(&text) {
        let _ = std::process::Command::new("sh")
            .args(["-c", &format!("kill {} 2>/dev/null", info.pid)])
            .status();
        for _ in 0..20 {
            if !server_health(info.port) {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    Ok(true)
}

/// One OpenAI style chat completion through the persistent server.
/// Runs the HTTP call on a thread so cancel and timeout stay responsive.
#[allow(clippy::too_many_arguments)]
pub fn chat_completions(
    port: u16,
    messages: &[(String, String)],
    temperature: f32,
    top_p: f32,
    seed: u64,
    max_tokens: u32,
    timeout: std::time::Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<String> {
    use std::sync::atomic::Ordering;
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|(r, c)| serde_json::json!({"role": r, "content": c}))
        .collect();
    let body = serde_json::json!({
        "messages": msgs,
        "temperature": temperature,
        "top_p": top_p,
        "seed": seed,
        "max_tokens": max_tokens,
        "repeat_penalty": 1.1,
        "stream": false,
        "cache_prompt": true,
    });
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out: Result<String> = block(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(timeout)
                .build()?;
            let resp = client
                .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
                .json(&body)
                .send()?;
            if !resp.status().is_success() {
                anyhow::bail!("server status {}", resp.status());
            }
            let v: serde_json::Value = resp.json()?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.trim().to_string())
                .ok_or_else(|| anyhow::anyhow!("server returned no content"))
        });
        let _ = tx.send(out);
    });
    let start = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(r) => return r,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if cancel.load(Ordering::Relaxed) {
                    anyhow::bail!("cancelled");
                }
                if start.elapsed() > timeout {
                    anyhow::bail!("server request timed out");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                anyhow::bail!("server worker died");
            }
        }
    }
}

/// Inspect the local backend without building anything.
pub fn backend_state() -> BackendState {
    let bin = find_llama_cli();
    let version = bin.as_deref().and_then(llama_version);
    let src = aicli_core::paths::shared_build_dir();
    BackendState {
        binary: bin.map(|p| p.display().to_string()),
        version,
        source_present: src.join("CMakeLists.txt").exists(),
        source_dir: src.display().to_string(),
        tag: LLAMA_TAG.to_string(),
        server: server_running(),
    }
}

/// Clone (if needed) plus cmake configure plus build the llama-cli binary,
/// then install it into the shared bin dir. Runs several minutes on first
/// setup. Stage notes go through `on_stage` for CLI progress.
pub fn setup_llama(tag: &str, mut on_stage: impl FnMut(&str)) -> Result<PathBuf> {
    for (tool, hint) in [
        ("git", "install git first"),
        ("cmake", "install cmake 3.14 or later first"),
    ] {
        anyhow::ensure!(command_exists(tool), "{tool} not found, {hint}",);
    }
    let src = aicli_core::paths::shared_build_dir();
    let dest = aicli_core::paths::shared_bin_dir().join("llama-cli");
    if !src.join("CMakeLists.txt").exists() {
        on_stage(&format!("clone {LLAMA_REPO} tag {tag}"));
        if let Some(parent) = src.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_dir_all(&src);
        let st = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--branch", tag, LLAMA_REPO])
            .arg(&src)
            .status()
            .with_context(|| format!("git clone {LLAMA_REPO}"))?;
        anyhow::ensure!(st.success(), "git clone failed for tag {tag}");
    } else {
        on_stage("source present, skip clone");
    }
    on_stage("cmake configure");
    let build = src.join("build");
    let st = std::process::Command::new("cmake")
        .args(["-S", &src.to_string_lossy(), "-B", &build.to_string_lossy()])
        .args([
            "-DGGML_NATIVE=ON",
            "-DLLAMA_CURL=OFF",
            "-DBUILD_SHARED_LIBS=OFF",
        ])
        .status()
        .with_context(|| "cmake configure")?;
    anyhow::ensure!(st.success(), "cmake configure failed");
    on_stage("compile llama-cli, several minutes");
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "4".to_string());
    let st = std::process::Command::new("cmake")
        .args(["--build", &build.to_string_lossy(), "--config", "Release"])
        .args(["-j", &jobs, "--target", "llama-cli"])
        .status()
        .with_context(|| "cmake build llama-cli")?;
    anyhow::ensure!(st.success(), "llama-cli build failed");
    let built = build.join("bin").join("llama-cli");
    anyhow::ensure!(built.is_file(), "build produced no llama-cli binary");
    std::fs::create_dir_all(dest.parent().unwrap())?;
    std::fs::copy(&built, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(dest)
}

fn command_exists(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(tool))
                .any(|f| f.is_file())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_prefer_system_paths() {
        let c = bin_candidates(Path::new("/tmp/share/bin"));
        assert_eq!(c[0], PathBuf::from("/usr/local/bin/llama-cli"));
        assert_eq!(c[1], PathBuf::from("/tmp/share/bin/llama-cli"));
    }

    #[test]
    fn missing_binary_finds_nothing_usable() {
        assert!(llama_version(Path::new("/nonexistent-llama-cli")).is_none());
    }

    #[test]
    fn state_shape_is_stable() {
        let st = backend_state();
        assert_eq!(st.tag, LLAMA_TAG);
        assert!(!st.source_dir.is_empty());
    }
}
