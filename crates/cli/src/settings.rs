/// Shared settings and effort helpers used by TUI, classic REPL, and CLI handlers.
/// Single source of truth for keys, levels, and validation messages.
use crate::Ctx;
use anyhow::Result;

/// Apply one setting key. Returns confirmation message.
pub fn apply_setting(ctx: &Ctx, key: &str, value: &str) -> Result<String, String> {
    let mut cfg = ctx.config.clone();
    match key {
        "model" | "model.default" => cfg.model.default = value.to_string(),
        "effort" | "model.effort" => {
            if aicli_core::config::effort_profile(value).is_none() {
                return Err("effort must be Default Low Medium High XHigh Expert".to_string());
            }
            cfg.model.effort = value.to_string();
        }
        "temp" => {
            cfg.model.temp_chat = value
                .parse::<f32>()
                .map_err(|_| "temp must be a number 0.0-2.0")?;
            cfg.model.temp_code = cfg.model.temp_chat.min(0.5);
        }
        "top_p" => {
            cfg.model.top_p = value.parse::<f32>().map_err(|_| "top_p must be a number")?;
        }
        "seed" => {
            cfg.model.seed = value
                .parse::<u64>()
                .map_err(|_| "seed must be an integer")?;
        }
        "ctx" | "model.n_ctx" => {
            cfg.model.n_ctx = value.parse::<u32>().map_err(|_| "ctx must be an integer")?;
        }
        "threads" => {
            cfg.model.n_threads = value
                .parse::<u32>()
                .map_err(|_| "threads must be an integer")?;
        }
        "gpu_layers" => {
            cfg.model.n_gpu_layers = value
                .parse::<u32>()
                .map_err(|_| "gpu_layers must be an integer")?;
        }
        "theme" | "ui.theme" => cfg.ui.theme = value.to_string(),
        "shell" => {
            if !["deny", "ask", "allow"].contains(&value) {
                return Err("shell must be deny, ask, or allow".to_string());
            }
            cfg.tools.shell_mode = value.to_string();
        }
        _ => {
            return Err(
                "keys: model effort temp top_p seed ctx threads gpu_layers theme shell".to_string(),
            )
        }
    }
    cfg.save(&ctx.paths.config_file)
        .map_err(|e| e.to_string())?;
    if key == "shell" && value == "allow" {
        return Ok("setting shell=allow - FULL terminal access enabled, every command runs without prompt. Only you enabled this.".to_string());
    }
    Ok(format!("setting {key}={value}"))
}

/// Apply one effort level. Returns confirmation message.
pub fn apply_effort(ctx: &Ctx, level: &str) -> Result<String, String> {
    if aicli_core::config::effort_profile(level).is_none() {
        return Err("effort must be Default Low Medium High XHigh Expert".to_string());
    }
    let mut cfg = ctx.config.clone();
    cfg.model.effort = level.to_string();
    cfg.save(&ctx.paths.config_file)
        .map_err(|e| e.to_string())?;
    Ok(format!("effort {level}"))
}

/// Full settings page text for overlays and REPL panels.
pub fn settings_text(ctx: &Ctx, model_id: &str) -> String {
    let c = &ctx.config;
    format!(
        "model      {model_id}\neffort     {}\ntemp       {:.1} (chat) {:.1} (code)\ntop_p      {:.2}\nseed       {}\nctx        {}\nthreads    {}\ngpu_layers {}\ntheme      {}\nshell      {}\n\nset with: /setting set <key> <value>\nkeys: model effort temp top_p seed ctx threads gpu_layers theme shell",
        c.model.effort,
        c.model.temp_chat,
        c.model.temp_code,
        c.model.top_p,
        c.model.seed,
        c.model.n_ctx,
        c.model.n_threads,
        c.model.n_gpu_layers,
        c.ui.theme,
        c.tools.shell_mode,
    )
}
