pub mod code;
/// Offline reasoning engine: math evaluator, code analyzer, security advisor.
/// Deep understanding (scored intents, entities, plans) lives in
/// `understand`; regression fixtures in `eval`. General questions fall
/// through to RAG plus model backends - the brain never invents answers for
/// what it does not understand.
pub mod codegen;
pub mod eval;
pub mod identity;
pub mod math;
pub mod security;
pub mod understand;

use serde::{Deserialize, Serialize};

pub use understand::{understand, Complexity, Intent as UnderstoodIntent, Understanding};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    Identity,
    Math,
    Code,
    CodeGen,
    Security,
    General,
}

impl From<UnderstoodIntent> for Intent {
    fn from(u: UnderstoodIntent) -> Self {
        match u {
            UnderstoodIntent::Identity => Intent::Identity,
            UnderstoodIntent::Math => Intent::Math,
            UnderstoodIntent::Code => Intent::Code,
            UnderstoodIntent::CodeGen => Intent::CodeGen,
            UnderstoodIntent::Security => Intent::Security,
            UnderstoodIntent::General => Intent::General,
        }
    }
}

/// Intent classifier backed by scored understanding. Pasted source code
/// still takes the analysis path even when it contains trigger words.
pub fn classify(query: &str) -> Intent {
    let q = query.trim();
    if q.is_empty() {
        return Intent::General;
    }
    if code::looks_like_code(q) {
        return Intent::Code;
    }
    understand(q).intent.into()
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
        Intent::Identity => Some(BrainSection {
            kind: "identity".to_string(),
            title: "Zai by Hanif Abdurrohim".to_string(),
            body_md: identity::answer(query),
        }),
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
                title: format!(
                    "{}: {} lines, {} functions",
                    report.language, report.lines, report.functions
                ),
                body_md: report.to_markdown(),
            })
        }
        Intent::CodeGen => Some(BrainSection {
            kind: "codegen".to_string(),
            title: format!("Full code: {}", codegen::detect_target_lang(query)),
            body_md: codegen::generate(query),
        }),
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
