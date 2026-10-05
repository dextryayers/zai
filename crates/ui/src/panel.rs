use crate::theme::Theme;

/// Single panel with title and body. One blank line between sections outside.
pub fn render_panel(theme: &Theme, title: &str, body: &str) -> String {
    let inner_w = (theme.width.saturating_sub(6)).clamp(40, 96);
    let top = if theme.unicode {
        format!("╭─ {} {}", theme.bold(title), "─".repeat(4))
    } else {
        format!("+-- {title} --")
    };
    let bottom = if theme.unicode {
        "╰".to_string() + &"─".repeat(inner_w.min(40))
    } else {
        "+--".to_string()
    };
    let mut out = String::new();
    out.push_str(&theme.muted(&top));
    out.push('\n');
    for line in body.lines() {
        let border = if theme.unicode { "│" } else { "|" };
        out.push_str(&theme.muted(border));
        out.push(' ');
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&theme.muted(&bottom));
    out
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
