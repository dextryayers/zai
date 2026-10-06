/// Single answer router shared by TUI, classic REPL, and one shot ask.
/// Order is cheapest first: local brain, then Ollama daemon, then mock.
use crate::Ctx;

pub struct Answer {
    pub text: String,
    pub brain: Option<String>,
    pub backend_note: String,
}

pub fn compose_answer(
    ctx: &Ctx,
    query: &str,
    model_id: &str,
    sampler: &aicli_infer::SamplerConfig,
    prompt: &str,
    n_ctx: u32,
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
    // Cached GGUF validates the header, generation stays mock until llama land.
    let note = match resolve_cached(ctx, model_id, n_ctx) {
        Some(info) => info,
        None => format!("mock backend, pull with: zai models pull {model_id}"),
    };
    Answer {
        text: aicli_infer::sampler::mock_answer_with_sampler(query, sampler),
        brain: None,
        backend_note: note,
    }
}

fn resolve_cached(ctx: &Ctx, model_id: &str, n_ctx: u32) -> Option<String> {
    let entry = aicli_models::find_any(&ctx.paths.models_dir, model_id)?;
    let path = aicli_models::local_path(&ctx.paths.models_dir, &entry);
    if !path.exists() {
        return None;
    }
    match aicli_infer::load_info(
        &path,
        n_ctx,
        ctx.config.model.n_threads,
        ctx.config.model.n_gpu_layers,
    ) {
        Ok(info) => Some(format!(
            "gguf v{} tensors {} size {} MB",
            info.gguf_version,
            info.tensor_count,
            info.size_bytes / (1024 * 1024)
        )),
        Err(e) => Some(format!("backend warn: {e}")),
    }
}
