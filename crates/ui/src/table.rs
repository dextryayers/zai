use crate::theme::Theme;

/// Render a table with header plus rows.
/// Numeric columns right aligned when `align_right[i]` is true.
/// Long paths truncate from left to keep filename visible.
pub fn render_table(
    theme: &Theme,
    headers: &[&str],
    rows: &[Vec<String>],
    align_right: &[bool],
) -> String {
    let cols = headers.len();
    let mut widths = vec![0usize; cols];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    for r in rows {
        for (i, c) in r.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(c.len().min(48));
        }
    }
    // Fit to terminal width.
    let total: usize = widths.iter().sum::<usize>() + cols * 3 + 1;
    if total > theme.width {
        let over = total - theme.width;
        // Shrink widest column first.
        if let Some((idx, _)) = widths.iter().enumerate().max_by_key(|(_, w)| **w) {
            widths[idx] = widths[idx].saturating_sub(over).max(12);
        }
    }

    let h_line = if theme.unicode { "─" } else { "-" };
    let v_line = if theme.unicode { "│" } else { "|" };
    let mut out = String::new();

    let sep = format!(
        "+{}+",
        widths
            .iter()
            .map(|w| h_line.repeat(*w + 2))
            .collect::<Vec<_>>()
            .join("+")
    );
    out.push_str(&theme.muted(&sep));
    out.push('\n');
    // Header
    out.push_str(&theme.muted(v_line));
    for (i, h) in headers.iter().enumerate() {
        let cell = format!(" {:<w$} ", h, w = widths[i]);
        out.push_str(&theme.bold(&cell));
        out.push_str(&theme.muted(v_line));
    }
    out.push('\n');
    out.push_str(&theme.muted(&sep));
    out.push('\n');
    for r in rows {
        out.push_str(&theme.muted(v_line));
        for (i, w) in widths.iter().enumerate().take(cols) {
            let raw = r.get(i).map(|s| s.as_str()).unwrap_or("");
            let mut cell = truncate_cell(raw, *w);
            if align_right.get(i).copied().unwrap_or(false) {
                cell = format!(" {:>w$} ", cell, w = *w);
            } else {
                cell = format!(" {:<w$} ", cell, w = *w);
            }
            out.push_str(&cell);
            out.push_str(&theme.muted(v_line));
        }
        out.push('\n');
    }
    out.push_str(&theme.muted(&sep));
    out
}

fn truncate_cell(s: &str, w: usize) -> String {
    if s.len() <= w {
        return s.to_string();
    }
    if w <= 3 {
        return s[..w].to_string();
    }
    // Keep tail for paths so filename stays visible.
    if s.contains('/') {
        return format!("...{}", &s[s.len() - (w - 3)..]);
    }
    format!("{}..", &s[..w - 2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    fn plain() -> Theme {
        Theme {
            mode: ThemeMode::Plain,
            color: false,
            unicode: false,
            width: 100,
        }
    }

    #[test]
    fn table_renders_header_and_rows() {
        let t = render_table(
            &plain(),
            &["ID", "TEXT"],
            &[vec!["t0001".into(), "hello".into()]],
            &[false, false],
        );
        assert!(t.contains("t0001"));
        assert!(t.contains("ID"));
    }
}
