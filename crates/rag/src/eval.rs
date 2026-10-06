use anyhow::Result;
use std::path::Path;

use crate::{index::build_index, retrieve::retrieve};
use aicli_ingest::walk::WalkOptions;

/// Eval fixture: 10 queries with expected path substring. Pass when 8 of 10 hit top 3.
pub struct EvalCase {
    pub query: &'static str,
    pub expect: &'static str,
}

pub fn eval_cases() -> Vec<EvalCase> {
    vec![
        EvalCase {
            query: "thread pool rayon build",
            expect: "scan",
        },
        EvalCase {
            query: "shell allowlist deny",
            expect: "gate",
        },
        EvalCase {
            query: "patch atomic backup",
            expect: "patch",
        },
        EvalCase {
            query: "session resume sqlite",
            expect: "session",
        },
        EvalCase {
            query: "gguf model pull resume",
            expect: "model",
        },
        EvalCase {
            query: "prompt budget overflow compact",
            expect: "prompt",
        },
        EvalCase {
            query: "daily week tasks carry",
            expect: "store",
        },
        EvalCase {
            query: "memory promote turn",
            expect: "memory",
        },
        EvalCase {
            query: "markdown code fence render",
            expect: "markdown",
        },
        EvalCase {
            query: "status line ctx meter",
            expect: "status",
        },
    ]
}

/// Eval detail row: query, pass, top paths.
pub type EvalDetail = Vec<(String, bool, Vec<String>)>;

/// Build tiny deterministic corpus under dir and evaluate retrieval.
/// Returns (passed, total, details).
#[allow(clippy::type_complexity)]
pub fn run_eval(cache_dir: &Path) -> Result<(usize, usize, EvalDetail)> {
    let corpus = cache_dir.join("eval-corpus");
    std::fs::create_dir_all(&corpus)?;
    let files: Vec<(&str, &str)> = vec![
        (
            "scan.rs",
            "thread pool rayon build parallel walk collect files",
        ),
        ("gate.rs", "shell allowlist deny prefix match denylist wins"),
        ("patch.rs", "patch atomic backup hunks apply all or nothing"),
        ("session.rs", "session resume sqlite turns compact history"),
        ("model.rs", "gguf model pull resume sha256 verify download"),
        ("prompt.rs", "prompt budget overflow compact context window"),
        ("store.rs", "daily week tasks carry notes search fts"),
        ("memory.rs", "memory promote turn session append owned"),
        ("markdown.rs", "markdown code fence render headings lists"),
        ("status.rs", "status line ctx meter offline profile"),
    ];
    for (name, text) in &files {
        std::fs::write(
            corpus.join(name),
            format!("// {name}\n{text}\n{text} details\n"),
        )?;
    }
    let opts = WalkOptions {
        exts: vec!["rs".to_string()],
        ..Default::default()
    };
    build_index(
        &corpus,
        cache_dir,
        &opts,
        128,
        16,
        &["rs".to_string()],
        true,
        |_, _, _| {},
    )?;
    let mut passed = 0;
    let mut details = Vec::new();
    for c in eval_cases() {
        let hits = retrieve(cache_dir, &corpus, c.query, 3)?;
        let tops: Vec<String> = hits.iter().map(|h| h.path.clone()).collect();
        let ok = tops.iter().any(|p| p.contains(c.expect));
        if ok {
            passed += 1;
        }
        details.push((c.query.to_string(), ok, tops));
    }
    Ok((passed, eval_cases().len(), details))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rag_regression_8_of_10() {
        let tmp = tempfile::tempdir().unwrap();
        let (passed, total, details) = run_eval(tmp.path()).unwrap();
        for (q, ok, tops) in &details {
            eprintln!("query {q} ok={ok} tops={tops:?}");
        }
        assert!(passed >= 8, "passed {passed}/{total}");
    }
}
