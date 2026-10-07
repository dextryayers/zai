/// Single answer router shared by TUI, classic REPL, and one shot ask.
/// Professional order: real model first (Ollama, then local GGUF), offline
/// brain only as fallback when no model is reachable. Language follows the
/// user (Indonesian vs English) dynamically - never a single hardcoded reply.
/// Identity is enforced via system prompt; a guardrail only patches a wrong
/// model identity instead of replacing the whole answer.
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
    let lang = aicli_infer::lang::detect(query);
    let localized_system = aicli_infer::lang::system_for(lang);
    let is_identity = aicli_infer::brain::identity::is_identity_query(query);

    // 1. Real model first: Ollama daemon when an ollama/ model is selected.
    if let Some(omodel) = aicli_models::ollama::strip_id(model_id) {
        if aicli_models::ollama::daemon_reachable() {
            // Rebuild prompt with localized identity-enforcing system.
            let localized_prompt = rebuild_with_system(prompt, &localized_system, parts.system);
            match aicli_models::ollama::generate(
                omodel,
                &localized_prompt,
                ctx.config.model.timeout_s,
            ) {
                Ok(text) => {
                    let text = guard_identity(is_identity, &text, query);
                    return Answer {
                        text,
                        brain: None,
                        backend_note: format!("ollama {omodel} via localhost:11434"),
                    };
                }
                Err(e) => {
                    // Model failed: fall through to brain fallback below,
                    // but keep the error visible in the note.
                    let fb = brain_fallback(query, sampler, model_id, Some(&e.to_string()));
                    return Answer {
                        text: fb.0,
                        brain: Some(fb.1),
                        backend_note: format!("ollama {omodel} failed ({e}), offline fallback"),
                    };
                }
            }
        }
        // Daemon down: offline fallback, language-aware.
        let fb = brain_fallback(query, sampler, model_id, None);
        return Answer {
            text: fb.0,
            brain: Some(fb.1),
            backend_note: "ollama daemon down, start with: ollama serve - offline fallback"
                .to_string(),
        };
    }

    // 2. Real model: cached GGUF through llama server/cli.
    let entry = aicli_models::find_any(&ctx.paths.models_dir, model_id);
    let path = entry
        .as_ref()
        .map(|e| aicli_models::local_path(&ctx.paths.models_dir, e));
    let usable = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    if usable {
        let path = path.unwrap();
        let entry_name = entry.as_ref().map(|e| e.file.clone()).unwrap_or_default();
        if aicli_infer::load_info(
            &path,
            n_ctx,
            ctx.config.model.n_threads,
            ctx.config.model.n_gpu_layers,
        )
        .is_ok()
        {
            let entry_ctx = entry
                .and_then(|e| if e.n_ctx > 0 { Some(e.n_ctx) } else { None })
                .unwrap_or(n_ctx);
            let timeout = std::time::Duration::from_secs(ctx.config.model.timeout_s.max(10));
            let threads = aicli_infer::local::inference_threads(ctx.config.model.n_threads);
            // Localized system with identity for the model.
            let localized_parts = AskParts {
                system: &localized_system,
                chunks: parts.chunks,
                history: parts.history,
            };
            if let Some(text) = try_server_answer(
                &path,
                entry_ctx,
                &localized_parts,
                query,
                sampler,
                threads,
                timeout,
                cancel,
            ) {
                let text = guard_identity(is_identity, &text, query);
                let tok = aicli_infer::estimate_tokens(&text);
                return Answer {
                    text,
                    brain: None,
                    backend_note: format!("local {entry_name} · {tok} tok · server"),
                };
            }
            let system_full = build_system(&localized_system, parts.chunks);
            let user_full = build_user(parts.history, query);
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
                    let text = guard_identity(is_identity, &text, query);
                    let tok = aicli_infer::estimate_tokens(&text);
                    return Answer {
                        text,
                        brain: None,
                        backend_note: format!("local {entry_name} · {tok} tok · cli"),
                    };
                }
                Err(e) => {
                    let msg = e.to_string();
                    let fb = brain_fallback(query, sampler, model_id, Some(&msg));
                    let hint = if msg.contains("no llama-cli") {
                        "run: zai backend setup"
                    } else {
                        "offline fallback"
                    };
                    return Answer {
                        text: fb.0,
                        brain: Some(fb.1),
                        backend_note: format!("local backend failed ({msg}), {hint}"),
                    };
                }
            }
        }
    }

    // 3. No model reachable: dynamic offline brain (language-aware synthesis,
    //    never one fixed hardcoded string for everything).
    let fb = brain_fallback(query, sampler, model_id, None);
    Answer {
        text: fb.0,
        brain: Some(fb.1),
        backend_note: format!(
            "offline fallback (no model cached), pull with: zai models pull {model_id} - lang {lang}"
        ),
    }
}

/// Offline fallback: brain synthesis when it can, else language-aware mock.
fn brain_fallback(
    query: &str,
    sampler: &aicli_infer::SamplerConfig,
    _model_id: &str,
    _cause: Option<&str>,
) -> (String, String) {
    if let Some(section) = aicli_infer::brain::compose(query) {
        return (section.body_md, section.kind);
    }
    let lang = aicli_infer::lang::detect(query);
    let mock = aicli_infer::sampler::mock_answer_with_sampler(query, sampler);
    let localized = if lang == "id" {
        format!(
            "{mock}\n\n> Mode offline tanpa model. Untuk jawaban penuh dari model: `zai models pull <id>` atau jalankan `ollama serve`. Pertanyaan matematika, review kode, panduan defensif, dan `/code` tetap dijawab offline."
        )
    } else {
        format!(
            "{mock}\n\n> Offline with no model cached. For full model answers: `zai models pull <id>` or run `ollama serve`."
        )
    };
    (localized, "mock-fallback".to_string())
}

/// Identity guardrail: if the user asked identity and the model answer does
/// not mention the developer, append a one-line correction dynamically.
/// Never replaces a correct model answer with a hardcoded block.
fn guard_identity(is_identity: bool, text: &str, query: &str) -> String {
    if !is_identity {
        return text.to_string();
    }
    let low = text.to_lowercase();
    if low.contains("hanif") {
        return text.to_string();
    }
    let lang = aicli_infer::lang::detect(query);
    let correction = if lang == "en" {
        "\n\n> Correction: I am Zai, developed by Hanif Abdurrohim, a young Informatics Engineering student."
    } else {
        "\n\n> Koreksi identitas: Saya Zai yang dikembangkan oleh Hanif Abdurrohim, anak muda Teknik Informatika."
    };
    format!("{text}{correction}")
}

fn rebuild_with_system(prompt: &str, new_system: &str, old_system: &str) -> String {
    if prompt.contains(old_system) {
        prompt.replacen(old_system, new_system, 1)
    } else {
        format!("System: {new_system}\n\n{prompt}")
    }
}

fn build_system(system: &str, chunks: &[String]) -> String {
    let mut out = system.to_string();
    for (i, c) in chunks.iter().enumerate() {
        out.push_str(&format!("\n[SOURCE {}] {c}", i + 1));
    }
    out
}

fn build_user(history: &[(String, String)], query: &str) -> String {
    let hist: Vec<(String, String)> = history
        .iter()
        .take(7)
        .map(|(r, c)| (r.clone(), c.clone()))
        .collect();
    let hist = match hist.last() {
        Some((r, c)) if r == "user" && c == query => &hist[..hist.len() - 1],
        _ => &hist[..],
    };
    let mut out = String::new();
    for (r, c) in hist {
        out.push_str(&format!("{r}: {c}\n"));
    }
    out.push_str(query);
    out
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
