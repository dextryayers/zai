/// Offline reasoning engine: math evaluator, code analyzer, security advisor.
/// No network, no model download. Deterministic and testable.
/// General questions fall through to RAG plus mock or Ollama backends.
pub mod math;
pub mod code;
pub mod security;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    Math,
    Code,
    Security,
    General,
}

/// Heuristic intent classifier. Order matters: math first, then code, then security.
pub fn classify(query: &str) -> Intent {
    let q = query.trim();
    if q.is_empty() {
        return Intent::General;
    }
    if math::extract_expr(q).is_some() {
        return Intent::Math;
    }
    if code::looks_like_code(q) {
        return Intent::Code;
    }
    if security::topic(q).is_some() {
        return Intent::Security;
    }
    Intent::General
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSection {
    pub kind: String,
    pub title: String,
    pub body_md: String,
}

/// Route one query through the local brain. Returns Some when the brain
/// can answer directly, None when the caller should use RAG or a model.
pub fn compose(query: &str) -> Option<BrainSection> {
    match classify(query) {
        Intent::Math => {
            let expr = math::extract_expr(query)?;
            match math::eval(&expr) {
                Ok(v) => Some(BrainSection {
                    kind: "math".to_string(),
                    title: format!("{expr} = {}", math::format_num(v)),
                    body_md: format!("## Result\n\n`{expr}` = **{}**\n", math::format_num(v)),
                }),
                Err(e) => Some(BrainSection {
                    kind: "math-error".to_string(),
                    title: "Could not evaluate".to_string(),
                    body_md: format!("## Math error\n\n`{expr}`\n\n{e}\n"),
                }),
            }
        }
        Intent::Code => {
            let report = code::analyze(query);
            Some(BrainSection {
                kind: "code-analysis".to_string(),
                title: format!("{}: {} lines, {} functions", report.language, report.lines, report.functions),
                body_md: report.to_markdown(),
            })
        }
        Intent::Security => {
            let playbook = security::advise(query);
            Some(BrainSection {
                kind: "security".to_string(),
                title: "Defensive security guidance".to_string(),
                body_md: playbook,
            })
        }
        Intent::General => None,
    }
}
