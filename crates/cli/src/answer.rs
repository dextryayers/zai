/// Single answer router shared by TUI, classic REPL, and one shot ask.
/// Order is cheapest first: local brain, then Ollama daemon, then local
/// GGUF through llama-cli, then mock fallback. Real model text never
/// carries code fences or citations unless the model wrote them.
use crate::Ctx;
use std::sync::atomic::AtomicBool;

pub struct Answer {
    pub text: String,
    pub brain: Option<String>,
    pub backend_note: String,
}

/// Prompt pieces for the local backend. The classic rendered prompt stays
/// for the Ollama path, these parts feed llama-cli system plus user args.
pub struct AskParts<'a> {
    pub system: &'a str,
    pub chunks: &'a [String],
    pub history: &'a [(String, String)],
}

/// Shared never-cancel flag for blocking callers (REPL, one shot ask).
pub static NO_CANCEL: AtomicBool = AtomicBool::new(false);

#[allow(clippy::too_many_arguments)]
pub fn compose_answer(
    ctx: &Ctx,
    query: &str,
    model_id: &str,
    sampler: &aicli_infer::SamplerConfig,
    prompt: &str,
    parts: &AskParts,
    n_ctx: u32,
    cancel: &AtomicBool,
) -> Answer {
    if let Some(section) = aicli_infer::brain::compose(query) {
        return Answer {
            text: section.body_md,
            brain: Some(section.kind),
            backend_note: "local brain, no model needed".to_string(),
        };
    }
    if let Some(omodel) = aicli_models::ollama::strip_id(model_id) {
        let note = if aicli_models::ollama::daemon_reachable() {
            format!("ollama {omodel} via localhost:11434")
        } else {
            "ollama daemon down, start with: ollama serve".to_string()
        };
        match aicli_models::ollama::generate(omodel, prompt, ctx.config.model.timeout_s) {
            Ok(text) => {
                return Answer {
                    text,
                    brain: None,
                    backend_note: note,
                };
            }
            Err(e) => {
                return Answer {
                    text: format!(
                        "{}\n\n> ollama failed ({e}), fell back to local mock. Start the daemon with `ollama serve` or pick a GGUF model with `/model`.",
                        aicli_infer::sampler::mock_answer_with_sampler(query, sampler)
                    ),
                    brain: Some("ollama-fallback".to_string()),
                    backend_note: note,
                };
            }
        }
    }
    // Cached GGUF runs real local inference through llama-cli.
    let entry = aicli_models::find_any(&ctx.paths.models_dir, model_id);
    let path = entry
        .as_ref()
        .map(|e| aicli_models::local_path(&ctx.paths.models_dir, e));
    let usable = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    if !usable {
        return Answer {
            text: aicli_infer::sampler::mock_answer_with_sampler(query, sampler),
            brain: None,
            backend_note: format!("mock backend, pull with: zai models pull {model_id}"),
        };
    }
    let path = path.unwrap();
    let entry_name = entry.as_ref().map(|e| e.file.clone()).unwrap_or_default();
    // Header plus OOM validation first, same gate as the old mock path.
    if let Err(e) = aicli_infer::load_info(
        &path,
        n_ctx,
        ctx.config.model.n_threads,
        ctx.config.model.n_gpu_layers,
    ) {
        return Answer {
            text: aicli_infer::sampler::mock_answer_with_sampler(query, sampler),
            brain: None,
            backend_note: format!("backend warn: {e}"),
        };
    }
    let mut system_full = parts.system.to_string();
    for (i, c) in parts.chunks.iter().enumerate() {
        system_full.push_str(&format!("\n[SOURCE {}] {c}", i + 1));
    }
    // History already holds the current question in chat flows (the user
    // turn persists at submit time). Drop that trailing duplicate so the
    // model sees the question once, not twice, and never echoes it back
    // as if it were the answer.
    let mut user_full = String::new();
    let hist: Vec<(String, String)> = parts
        .history
        .iter()
        .take(7)
        .map(|(r, c)| (r.clone(), c.clone()))
        .collect();
    let hist = match hist.last() {
        Some((r, c)) if r == "user" && c == query => &hist[..hist.len() - 1],
        _ => &hist[..],
    };
    for (r, c) in hist {
        user_full.push_str(&format!("{r}: {c}\n"));
    }
    user_full.push_str(query);
    let entry_ctx = entry
        .and_then(|e| if e.n_ctx > 0 { Some(e.n_ctx) } else { None })
        .unwrap_or(n_ctx);
    let timeout = std::time::Duration::from_secs(ctx.config.model.timeout_s.max(10));
    let threads = aicli_infer::local::inference_threads(ctx.config.model.n_threads);
    // Fast path: persistent server keeps weights loaded, answers land in
    // seconds. It starts itself on first use when the binary exists.
    let server_text = try_server_answer(
        &path, entry_ctx, parts, query, sampler, threads, timeout, cancel,
    );
    if let Some(text) = server_text {
        let tok = aicli_infer::estimate_tokens(&text);
        return Answer {
            text,
            brain: None,
            backend_note: format!("local {entry_name} · {tok} tok · server"),
        };
    }
    let req = aicli_infer::GenRequest {
        model_path: path,
        system: system_full,
        user: user_full,
        temp: sampler.temp,
        top_p: sampler.top_p,
        seed: sampler.seed,
        n_ctx: entry_ctx,
        repeat_penalty: 1.1,
        n_predict: aicli_infer::N_PREDICT,
        threads,
        timeout,
    };
    match aicli_infer::generate_local(&req, cancel) {
        Ok(text) => {
            let tok = aicli_infer::estimate_tokens(&text);
            Answer {
                text,
                brain: None,
                backend_note: format!("local {entry_name} · {tok} tok · cli"),
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let hint = if msg.contains("no llama-cli") {
                "run: zai backend setup".to_string()
            } else {
                "fell back to mock".to_string()
            };
            Answer {
                text: aicli_infer::sampler::mock_answer_with_sampler(query, sampler),
                brain: None,
                backend_note: format!("local backend failed ({msg}), {hint}"),
            }
        }
    }
}

/// Persistent server attempt. Returns Some on real model text, None to
/// fall through to one shot. Never mocks here.
#[allow(clippy::too_many_arguments)]
fn try_server_answer(
    path: &std::path::Path,
    entry_ctx: u32,
    parts: &AskParts,
    query: &str,
    sampler: &aicli_infer::SamplerConfig,
    threads: u32,
    timeout: std::time::Duration,
    cancel: &AtomicBool,
) -> Option<String> {
    use std::sync::atomic::Ordering;
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    // Reuse a live server for this exact model, else boot one when the
    // binary exists. Missing binary means straight to one shot.
    let live =
        aicli_models::backend::server_running().filter(|s| std::path::Path::new(&s.model) == path);
    let port = if let Some(s) = live {
        s.port
    } else {
        aicli_models::backend::find_llama_server()?;
        match aicli_models::backend::start_server(path, entry_ctx, threads) {
            Ok(s) => s.port,
            Err(_) => return None,
        }
    };
    let mut sys = parts.system.to_string();
    for (i, c) in parts.chunks.iter().enumerate() {
        sys.push_str(&format!("\n[SOURCE {}] {c}", i + 1));
    }
    aicli_infer::local::generate_server(
        port,
        &sys,
        parts.history,
        query,
        sampler.temp,
        sampler.top_p,
        sampler.seed,
        aicli_infer::N_PREDICT,
        timeout,
        cancel,
    )
    .ok()
    .filter(|t| !t.trim().is_empty())
}
