use crate::theme::Theme;

/// Single panel with title and body. One blank line between sections outside.
/// Premium: rounded borders, padded body, title top-left with accent.
pub fn render_panel(theme: &Theme, title: &str, body: &str) -> String {
    let inner_w = (theme.width.saturating_sub(6)).clamp(40, 96);
    // Symmetric top/bottom so the card looks closed on wide terminals.
    let dash_len = inner_w.min(44).saturating_sub(title.len() + 6).max(4);
    let dashes: String =
        std::iter::repeat_n(if theme.unicode { '─' } else { '-' }, dash_len).collect();
    let top = if theme.unicode {
        format!("╭─ {} {}", theme.bold(title), dashes)
    } else {
        format!("+-- {title} --{dashes}")
    };
    let bottom = if theme.unicode {
        format!("╰{}", "─".repeat(title.len() + dash_len + 4))
    } else {
        format!("+--{}-", "-".repeat(title.len() + dash_len + 2))
    };
    let mut out = String::new();
    out.push_str(&theme.muted(&top));
    out.push('\n');
    for line in body.lines() {
        let border = if theme.unicode { "│" } else { "|" };
        out.push_str(&theme.muted(border));
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    // Empty body still renders one padded line so cards never collapse.
    if body.trim().is_empty() {
        let border = if theme.unicode { "│" } else { "|" };
        out.push_str(&theme.muted(border));
        out.push_str("  (empty)\n");
    }
    out.push_str(&theme.muted(&bottom));
    out
}

/// Premium panel with right-side action hint: "Title .... [action]".
pub fn render_panel_with_action(theme: &Theme, title: &str, action: &str, body: &str) -> String {
    render_panel(theme, &format!("{title}  [{action}]"), body)
}

/// Error card with code, cause, fix command, log path.
pub fn render_error(
    theme: &Theme,
    code: &str,
    cause: &str,
    fix: &str,
    log_path: Option<&str>,
) -> String {
    let mut body = format!("cause: {cause}\nfix: {fix}");
    if let Some(p) = log_path {
        body.push_str(&format!("\nlog: {p}"));
    }
    render_panel(theme, &format!("ERROR {code}"), &body)
}
