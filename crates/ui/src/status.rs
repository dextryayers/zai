use crate::theme::Theme;

/// Status line, always one row, never wraps. Truncates model id from left.
/// Premium layout: brand + version + model + ctx bar + offline + profile.
pub fn status_line(
    theme: &Theme,
    version: &str,
    model: &str,
    ctx_used: u32,
    ctx_total: u32,
    offline: bool,
    profile: &str,
) -> String {
    let net = if offline { "● offline" } else { "○ online" };
    let net_styled = if offline {
        theme.ok(net)
    } else {
        theme.warn(net)
    };
    let model_short = truncate_left(model, 28);
    // Compact bar keeps the line to one row on 80 cols.
    let bar = theme.ctx_bar(ctx_used as usize, ctx_total);
    format!(
        "{} {} | model {} | {} | {} | {}",
        theme.brand(),
        theme.muted(&format!("v{version}")),
        theme.accent(&model_short),
        bar,
        net_styled,
        theme.muted(profile)
    )
}

pub fn hint_line(theme: &Theme) -> String {
    theme
        .muted("❯ type + Enter  |  /help commands  |  Tab complete  |  Ctrl+C stop  |  Ctrl+D exit  |  /whoami")
        .to_string()
}

fn truncate_left(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    format!("...{}", &s[s.len() - (max - 3)..])
}
