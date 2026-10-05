use crate::theme::Theme;

/// Minimal markdown subset for terminal chat:
/// headings, lists, code fences with language tag, inline code, bold, links as footnotes.
/// Must never panic on malformed input and must run under 50 ms per fixture.
pub fn render_markdown(theme: &Theme, input: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut footnotes: Vec<String> = Vec::new();

    for raw in input.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            if !in_code {
                in_code = true;
                let lang = line
                    .trim_start()
                    .trim_start_matches("```")
                    .trim()
                    .to_string();
                let tag = if lang.is_empty() {
                    "code".to_string()
                } else {
                    lang
                };
                out.push_str(&theme.muted(&format!("┌─ {tag}")));
                out.push('\n');
            } else {
                in_code = false;
                out.push_str(&theme.muted("└─"));
                out.push('\n');
            }
            continue;
        }
        if in_code {
            out.push_str(&format!("│ {line}\n"));
            continue;
        }
        let t = line.trim_start();
        if let Some(h) = t.strip_prefix("### ") {
            out.push_str(&theme.bold(h));
            out.push('\n');
        } else if let Some(h) = t.strip_prefix("## ") {
            out.push_str(&theme.bold(h));
            out.push('\n');
        } else if let Some(h) = t.strip_prefix("# ") {
            out.push_str(&theme.bold(h));
            out.push('\n');
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            out.push_str(&format!("  • {}\n", inline(theme, item, &mut footnotes)));
        } else if is_ordered_item(t) {
            out.push_str(&format!("  {}\n", inline(theme, t, &mut footnotes)));
        } else if let Some(quoted) = t.strip_prefix("> ") {
            out.push_str(&theme.muted(&format!("│ {quoted}")));
            out.push('\n');
        } else if line.trim().is_empty() {
            out.push('\n');
        } else {
            out.push_str(&inline(theme, line, &mut footnotes));
            out.push('\n');
        }
    }

    if !footnotes.is_empty() {
        out.push('\n');
        for (i, link) in footnotes.iter().enumerate() {
            out.push_str(&theme.muted(&format!("[{}] {link}", i + 1)));
            out.push('\n');
        }
    }
    out
}

fn is_ordered_item(t: &str) -> bool {
    let mut chars = t.chars();
    let mut digits = 0;
    for c in &mut chars {
        if c.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    digits > 0 && chars.as_str().starts_with(". ")
}

fn inline(theme: &Theme, s: &str, footnotes: &mut Vec<String>) -> String {
    // Links [text](url) become text [n] with footnote.
    let mut out = String::new();
    let mut rest = s;
    while let Some(lb) = rest.find('[') {
        if let Some(mid) = rest[lb..].find("](") {
            if let Some(end) = rest[lb + mid + 2..].find(')') {
                let text = &rest[lb + 1..lb + mid];
                let url = &rest[lb + mid + 2..lb + mid + 2 + end];
                out.push_str(&rest[..lb]);
                footnotes.push(url.to_string());
                out.push_str(&format!("{} [{}]", text, footnotes.len()));
                rest = &rest[lb + mid + 2 + end + 1..];
                continue;
            }
        }
        break;
    }
    out.push_str(rest);
    // Inline code `x` and bold **x** simplified to styled spans.
    render_spans(theme, &out)
}

fn render_spans(theme: &Theme, s: &str) -> String {
    // Two passes: `code` then **bold**. No regex for speed.
    let mut out = String::new();
    let mut chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '`') {
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&theme.accent(&code));
                i += end + 2;
                continue;
            }
        }
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(end) = rest.find("**") {
                let bold: String = chars[i + 2..i + 2 + end].iter().collect();
                out.push_str(&theme.bold(&bold));
                i += end + 4;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // Fix unused mut warning path: chars never mutated after build.
    let _ = &mut chars;
    out
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
    fn code_fence_renders() {
        let t = plain();
        let md = "```rust\nfn a() {}\n```";
        let out = render_markdown(&t, md);
        assert!(out.contains("rust"));
        assert!(out.contains("fn a()"));
    }

    #[test]
    fn malformed_fence_never_panics() {
        let t = plain();
        let out = render_markdown(&t, "```\nno close");
        assert!(!out.is_empty());
    }
}
