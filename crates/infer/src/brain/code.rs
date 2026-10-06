/// Static code reader: language detect, structure counts, risk flags.
/// No execution, read only heuristics with line numbers.

/// True when the text is probably source code rather than prose.
pub fn looks_like_code(q: &str) -> bool {
    if q.contains("```") {
        return true;
    }
    if q.len() < 20 {
        return false;
    }
    let markers = [
        "fn ", "def ", "class ", "struct ", "impl ", "import ", "function ", "=>",
        "#include", "package ", "func ", "const ", "let mut", "pub fn", "SELECT ",
        "fn main", "if __name__",
    ];
    markers.iter().filter(|m| q.contains(*m)).count() >= 2
        || (q.contains('{') && q.contains('}') && q.contains(';'))
}

pub fn detect_language(text: &str) -> &'static str {
    let code = strip_fences(text);
    if code.contains("fn main") || code.contains("let mut") || code.contains("impl ") {
        return "rust";
    }
    if code.contains("def ") && (code.contains("import ") || code.contains("print(")) {
        return "python";
    }
    if code.contains("#include") || (code.contains("int main") || code.contains("std::")) {
        return "cpp";
    }
    if code.contains("package ") && code.contains("func ") {
        return "go";
    }
    if (code.contains("function ") || code.contains("=>") || code.contains("const "))
        && (code.contains('{') || code.contains(';'))
    {
        return "javascript";
    }
    if code.contains("#!/bin/bash") || code.contains("echo ") && code.contains("fi") {
        return "bash";
    }
    if code.trim_start().starts_with("SELECT ") || code.contains("CREATE TABLE") {
        return "sql";
    }
    if code.contains('[') && code.contains('=') && !code.contains('{') {
        return "toml";
    }
    "text"
}

fn strip_fences(text: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut has_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            has_fence = true;
            continue;
        }
        if !has_fence || in_code {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.trim().is_empty() {
        text.to_string()
    } else {
        out
    }
}

pub struct CodeReport {
    pub language: String,
    pub lines: usize,
    pub functions: usize,
    pub todos: Vec<String>,
    pub risks: Vec<String>,
}

impl CodeReport {
    pub fn to_markdown(&self) -> String {
        let mut out = format!(
            "## Code analysis ({})\n\nLines: {}  Functions: {}\n",
            self.language, self.lines, self.functions
        );
        if !self.todos.is_empty() {
            out.push_str("\n### Markers\n\n");
            for t in &self.todos {
                out.push_str(&format!("- `{t}`\n"));
            }
        }
        if !self.risks.is_empty() {
            out.push_str("\n### Review flags\n\n");
            for r in &self.risks {
                out.push_str(&format!("- {r}\n"));
            }
        } else {
            out.push_str("\nNo risky patterns matched. Still review logic by hand.\n");
        }
        out.push_str("\nPaste a focused question for deeper review, for example which function can panic.\n");
        out
    }
}

fn fn_patterns(lang: &str) -> Vec<&str> {
    match lang {
        "rust" => vec!["\nfn ", "    fn "],
        "python" => vec!["\ndef ", "\n    def "],
        "go" => vec!["\nfunc "],
        "javascript" => vec!["function ", "=>"],
        "cpp" => vec!["\nint ", "\nvoid ", "\nstd::"],
        _ => vec!["\nfn ", "\ndef ", "\nfunc ", "function "],
    }
}

pub fn analyze(text: &str) -> CodeReport {
    let code = strip_fences(text);
    let lang = detect_language(text).to_string();
    let lines: Vec<&str> = code.lines().collect();
    let padded = format!("\n{code}");
    let mut functions = 0;
    for pat in fn_patterns(&lang) {
        functions += padded.matches(pat).count();
    }
    // Main entry counts once even at file start.
    if code.trim_start().starts_with("fn ") && lang == "rust" {
        functions += 1;
    }
    let mut todos = Vec::new();
    let mut risks = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let no = i + 1;
        let t = line.trim();
        for marker in ["TODO", "FIXME", "XXX", "HACK", "unwrap()", "expect("] {
            if t.contains(marker) && (marker == "TODO" || marker == "FIXME" || marker == "XXX" || marker == "HACK") {
                todos.push(format!("line {no}: {t}"));
            }
        }
        let lower = t.to_lowercase();
        if t.contains(".unwrap()") || t.contains(".expect(") {
            risks.push(format!("line {no}: unwrap or expect can panic, prefer `?` or context"));
        }
        if lower.contains("eval(") || lower.contains("exec(") {
            risks.push(format!("line {no}: eval or exec runs strings as code, sanitize input first"));
        }
        if t.contains("rm -rf") {
            risks.push(format!("line {no}: destructive shell, gate it behind allowlist"));
        }
        if lower.contains("password") && lower.contains('=') || lower.contains("passwd") && lower.contains('=') {
            risks.push(format!("line {no}: possible hardcoded credential, move to env"));
        }
        if t.contains("AKIA") {
            risks.push(format!("line {no}: looks like an AWS key, rotate it"));
        }
        if (lower.contains("select ") || lower.contains("insert ")) && t.contains('+') && t.contains('"') {
            risks.push(format!("line {no}: string built SQL, use bound parameters"));
        }
        if t.contains("unsafe ") && lang == "rust" {
            risks.push(format!("line {no}: unsafe block, document the invariant"));
        }
        if risks.len() > 12 {
            break;
        }
    }
    CodeReport {
        language: lang,
        lines: lines.len(),
        functions,
        todos: todos.into_iter().take(8).collect(),
        risks: risks.into_iter().take(10).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_counts_rust() {
        let src = "fn main() {\n    let x = foo().unwrap();\n}\nfn foo() -> i32 { 1 }\n// TODO: handle err\n";
        assert!(looks_like_code(src));
        let r = analyze(src);
        assert_eq!(r.language, "rust");
        assert!(r.functions >= 2);
        assert!(r.risks.iter().any(|x| x.contains("unwrap")));
        assert!(r.todos.iter().any(|x| x.contains("TODO")));
    }

    #[test]
    fn prose_is_not_code() {
        assert!(!looks_like_code("hello world, how are you today"));
        assert!(!looks_like_code("a-b"));
    }

    #[test]
    fn detects_python() {
        let src = "import os\ndef run(x):\n    return os.system(x)\n";
        assert_eq!(detect_language(src), "python");
    }
}
