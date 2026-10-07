use crate::theme::Theme;

/// Minimal markdown subset for terminal chat:
/// headings, lists, code fences with language tag, inline code, bold, links as footnotes.
/// Must never panic on malformed input and must run under 50 ms per fixture.
/// Code fences get light syntax tint for rust/python/js/ts/go/bash/toml/json.
pub fn render_markdown(theme: &Theme, input: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut code_lang = String::new();
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
                code_lang = lang.clone();
                let tag = if lang.is_empty() {
                    "code".to_string()
                } else {
                    lang
                };
                out.push_str(&theme.muted(&format!("┌─ {tag} ── copy: select + Ctrl+Shift+C")));
                out.push('\n');
            } else {
                in_code = false;
                code_lang.clear();
                out.push_str(&theme.muted("└─"));
                out.push('\n');
            }
            continue;
        }
        if in_code {
            out.push_str(&highlight_code_line(theme, line, &code_lang));
            out.push('\n');
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

/// Lightweight code tint: comments muted, strings green, keywords cyan.
/// Falls back to plain gutter when theme has no color. Never panics.
fn highlight_code_line(theme: &Theme, line: &str, lang: &str) -> String {
    let gutter = theme.muted("│ ");
    if !theme.color {
        return format!("{gutter}{line}");
    }
    let trimmed = line.trim_start();
    // Full-line comments muted.
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("--")
        || trimmed.starts_with(';')
    {
        return format!("{gutter}{}", theme.muted(line));
    }
    // Keyword tint per language family.
    let keywords: &[&str] = match lang {
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod", "return", "if",
            "else", "for", "while", "match", "use",
        ],
        "python" | "py" => &[
            "def", "class", "import", "from", "return", "if", "else", "elif", "for", "while",
            "with", "as", "try", "except",
        ],
        "javascript" | "typescript" | "js" | "ts" => &[
            "function", "const", "let", "var", "return", "if", "else", "for", "while", "import",
            "export", "await", "async",
        ],
        "go" => &[
            "func", "package", "import", "return", "if", "else", "for", "range", "var", "const",
            "type",
        ],
        "bash" | "sh" => &[
            "if", "then", "else", "fi", "for", "do", "done", "function", "echo", "exit",
        ],
        _ => &[],
    };
    // Fast path: tint only the first keyword occurrence at word boundary.
    let mut out_line = line.to_string();
    for kw in keywords {
        let with_space = format!(" {kw} ");
        if let Some(pos) = out_line.find(&with_space) {
            let (head, tail) = out_line.split_at(pos);
            let tail = tail.replacen(kw, &theme.accent(kw), 1);
            out_line = format!("{head}{tail}");
            break;
        }
        if out_line.starts_with(&format!("{kw} ")) {
            out_line = out_line.replacen(kw, &theme.accent(kw), 1);
            break;
        }
    }
    // Strings: tint first quoted segment green when present.
    if let Some(s) = tint_first_string(theme, &out_line) {
        out_line = s;
    }
    format!("{gutter}{out_line}")
}

fn tint_first_string(theme: &Theme, line: &str) -> Option<String> {
    for q in ['"', '\''] {
        if let Some(a) = line.find(q) {
            if let Some(rel) = line[a + 1..].find(q) {
                let b = a + 1 + rel;
                let inner = &line[a..=b];
                return Some(line.replacen(inner, &theme.ok(inner), 1));
            }
        }
    }
    None
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
