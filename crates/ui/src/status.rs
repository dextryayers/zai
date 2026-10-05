use crate::theme::Theme;

/// Status line, always one row, never wraps. Truncates model id from left.
pub fn status_line(
    theme: &Theme,
    version: &str,
    model: &str,
    ctx_used: u32,
    ctx_total: u32,
    offline: bool,
    profile: &str,
) -> String {
    let net = if offline { "offline" } else { "online" };
    let model_short = truncate_left(model, 28);
    let ctx = format!("ctx {ctx_used}/{ctx_total}");
    let ctx_colored = if ctx_total > 0 && ctx_used * 100 / ctx_total.max(1) >= 85 {
        theme.warn(&ctx)
    } else {
        theme.muted(&ctx)
    };
    format!(
        "{} {} | model {} | {} | {} | {}",
        theme.bold("aicli"),
        theme.muted(version),
        theme.accent(&model_short),
        ctx_colored,
        theme.muted(net),
        theme.muted(profile)
    )
}

pub fn hint_line(theme: &Theme) -> String {
    theme
        .muted("/help commands  |  Tab complete  |  Ctrl+C stop  |  Ctrl+D exit  |  F2 palette")
        .to_string()
}

fn truncate_left(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    format!("...{}", &s[s.len() - (max - 3)..])
}
