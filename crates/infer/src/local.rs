use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Default max new tokens per local generation.
pub const N_PREDICT: u32 = 512;

/// One local generation request.
pub struct GenRequest {
    pub model_path: PathBuf,
    pub system: String,
    pub user: String,
    pub temp: f32,
    pub top_p: f32,
    pub seed: u64,
    pub n_ctx: u32,
    pub repeat_penalty: f32,
    pub n_predict: u32,
    pub threads: u32,
    pub timeout: Duration,
}

/// Run one blocking local generation through the llama-cli binary.
/// Kills the child on cancel or timeout. Never panics on weird output.
pub fn generate_local(req: &GenRequest, cancel: &AtomicBool) -> Result<String> {
    let bin = aicli_models::backend::find_llama_cli()
        .ok_or_else(|| anyhow::anyhow!("no llama-cli binary, run: zai backend setup"))?;
    generate_with_bin(&bin, req, cancel)
}

/// Same as generate_local with an explicit binary. Used by tests with a
/// fake script standing in for llama-cli.
pub fn generate_with_bin(bin: &Path, req: &GenRequest, cancel: &AtomicBool) -> Result<String> {
    anyhow::ensure!(
        req.model_path.exists(),
        "model file not found: {}",
        req.model_path.display()
    );
    // Empty generations happen: the same prompt can sample an immediate
    // EOS. Retry once with the next seed before calling it a failure.
    let seeds = [req.seed, req.seed.wrapping_add(1)];
    let mut last_empty = false;
    for (attempt, seed) in seeds.iter().enumerate() {
        if attempt > 0 {
            if cancel.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            if last_empty {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        let mut child = std::process::Command::new(bin)
            .args([
                "-m",
                &req.model_path.to_string_lossy(),
                "-sys",
                &req.system,
                "-p",
                &req.user,
                "--no-display-prompt",
                "-st",
                "--color",
                "off",
                "-n",
                &req.n_predict.to_string(),
                "-c",
                &req.n_ctx.to_string(),
                "--temp",
                &req.temp.to_string(),
                "--top-p",
                &req.top_p.to_string(),
                "--seed",
                &seed.to_string(),
                "--repeat-penalty",
                &req.repeat_penalty.to_string(),
                "-t",
                &req.threads.to_string(),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {}", bin.display()))?;
        let start = Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => {
                    let out = child.wait_with_output()?;
                    if !status.success() {
                        let err = String::from_utf8_lossy(&out.stderr);
                        anyhow::bail!(
                            "llama-cli exit {}: {}",
                            status,
                            err.chars().take(300).collect::<String>()
                        );
                    }
                    let text =
                        extract_with_prompt(&String::from_utf8_lossy(&out.stdout), &req.user);
                    if text.is_empty() {
                        last_empty = true;
                        break;
                    }
                    return Ok(text);
                }
                None => {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        anyhow::bail!("cancelled");
                    }
                    if start.elapsed() > req.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        anyhow::bail!(
                            "local generation timed out after {}s",
                            req.timeout.as_secs()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
    anyhow::bail!("llama-cli returned no text twice, fell back");
}

/// Strip llama-cli chrome (logo, loader spinner, info lines, prompt echo,
/// stats, exit notes) leaving just the completion text.
/// Lines identical to the request prompt are dropped too: llama-cli echoes
/// the prompt back, including continuation lines without any marker, and
/// those remnants must never pose as the answer.
pub fn extract_completion(stdout: &str) -> String {
    extract_with_prompt(stdout, "")
}

fn extract_with_prompt(stdout: &str, prompt: &str) -> String {
    // Grapheme heavy logo lines contain block characters.
    fn is_logo(line: &str) -> bool {
        let t = line.trim();
        !t.is_empty()
            && t.chars()
                .all(|c| matches!(c, '█' | '▄' | '▀' | ' ' | '░' | '▒' | '▓'))
    }
    fn is_info(line: &str) -> bool {
        let t = line.trim_start();
        ["build ", "model ", "ftype ", "modalities "]
            .iter()
            .any(|k| t.starts_with(k) && t.contains(':'))
            || t == "available commands:"
            || t == "using custom system prompt"
            || (t.starts_with('/') && t.contains("  "))
    }
    let prompt_lines: std::collections::HashSet<String> = prompt
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut past_echo = false;
    for line in stdout.lines() {
        if line.contains('\u{8}') {
            continue;
        }
        let t = line.trim();
        if t.starts_with("Loading model") {
            continue;
        }
        if is_logo(line) || is_info(line) {
            continue;
        }
        if t.starts_with("> ") || t == ">" {
            past_echo = true;
            continue;
        }
        if t.starts_with("[ Prompt") || t.starts_with("Exiting") {
            continue;
        }
        if !past_echo {
            continue;
        }
        // Prompt echo remnant: continuation lines of the echoed prompt.
        if prompt_lines.contains(t) {
            continue;
        }
        kept.push(line);
    }
    // Fallback: no echo marker seen, keep everything that is not chrome.
    let out = kept.join("\n");
    if out.trim().is_empty() {
        return String::new();
    }
    out.trim().to_string()
}

/// Threads for generation: explicit config wins, else cores minus one.
pub fn inference_threads(n_threads_cfg: u32) -> u32 {
    if n_threads_cfg != 0 {
        return n_threads_cfg;
    }
    std::thread::available_parallelism()
        .map(|n| (n.get() as u32).saturating_sub(1).max(2))
        .unwrap_or(4)
}

/// One generation through the persistent server. Messages carry roles
/// user, assistant, or system, oldest first. Pure model text back.
#[allow(clippy::too_many_arguments)]
pub fn generate_server(
    port: u16,
    system: &str,
    history: &[(String, String)],
    input: &str,
    temp: f32,
    top_p: f32,
    seed: u64,
    n_predict: u32,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String> {
    let mut messages: Vec<(String, String)> = Vec::new();
    messages.push(("system".to_string(), system.to_string()));
    // Same trailing duplicate guard as the one shot path: chat flows
    // persist the question before composing, so drop it from history.
    let hist: &[(String, String)] = match history.last() {
        Some((r, c)) if r != "assistant" && c == input => &history[..history.len() - 1],
        _ => history,
    };
    for (r, c) in hist.iter().take(7) {
        let role = if r == "assistant" {
            "assistant"
        } else {
            "user"
        };
        messages.push((role.to_string(), c.clone()));
    }
    messages.push(("user".to_string(), input.to_string()));
    let text = aicli_models::backend::chat_completions(
        port, &messages, temp, top_p, seed, n_predict, timeout, cancel,
    )?;
    anyhow::ensure!(!text.trim().is_empty(), "server returned no text");
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\nLoading model... |/- \n\n▄▄ ▄▄\n██ ██\nbuild      : b1-abc\nmodel      : /m.gguf\nftype      : Q6_K\nmodalities : text\n\navailable commands:\n  /exit or Ctrl+C     stop or exit\n\n> 2+2=\n2+2 equals 4.\n\n[ Prompt: 19.6 t/s ]\n\n> \n\nExiting...\n";

    #[test]
    fn extract_keeps_only_completion() {
        assert_eq!(extract_completion(SAMPLE), "2+2 equals 4.");
    }

    #[test]
    fn extract_drops_prompt_echo_remnants() {
        let out = "> user: hai\nhai\nHello! How can I assist you today?\n[ Prompt: 1 ]\n";
        assert_eq!(
            extract_with_prompt(out, "user: hai\nhai"),
            "Hello! How can I assist you today?"
        );
        // Same output without prompt knowledge keeps the remnant: caller
        // must pass the prompt.
        assert!(extract_completion(out).contains("hai"));
    }

    #[test]
    fn extract_empty_without_echo() {
        assert_eq!(extract_completion("build : x\n"), "");
    }

    #[test]
    fn fake_binary_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("llama-cli");
        std::fs::write(&bin, "#!/bin/sh\necho '> hi'\necho 'canned answer'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let missing = dir.path().join("m.gguf");
        std::fs::write(&missing, b"GGUF").unwrap();
        let req = GenRequest {
            model_path: missing,
            system: "s".to_string(),
            user: "u".to_string(),
            temp: 0.2,
            top_p: 0.9,
            seed: 7,
            n_ctx: 512,
            repeat_penalty: 1.1,
            n_predict: 16,
            threads: 2,
            timeout: Duration::from_secs(10),
        };
        let cancel = AtomicBool::new(false);
        assert_eq!(
            generate_with_bin(&bin, &req, &cancel).unwrap(),
            "canned answer"
        );
    }

    #[test]
    fn missing_model_fails_fast() {
        let req = GenRequest {
            model_path: PathBuf::from("/none.gguf"),
            system: "s".to_string(),
            user: "u".to_string(),
            temp: 0.2,
            top_p: 0.9,
            seed: 7,
            n_ctx: 512,
            repeat_penalty: 1.1,
            n_predict: 16,
            threads: 2,
            timeout: Duration::from_secs(10),
        };
        let cancel = AtomicBool::new(false);
        assert!(generate_with_bin(Path::new("/bin/true"), &req, &cancel).is_err());
    }
}
