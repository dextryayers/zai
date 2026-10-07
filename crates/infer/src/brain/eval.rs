/// Brain training gate: regression fixtures mapping queries to expected
/// intents. Run with `cargo test -p aicli-infer`. Add a row whenever a
/// misclassification is fixed so the brain keeps getting smarter instead of
/// regressing. This is the offline equivalent of a training eval set.
use super::understand::{understand, Intent};

struct Case {
    query: &'static str,
    want: Intent,
}

const CASES: &[Case] = &[
    Case {
        query: "who are you?",
        want: Intent::Identity,
    },
    Case {
        query: "who developed you?",
        want: Intent::Identity,
    },
    Case {
        query: "what can you do?",
        want: Intent::Identity,
    },
    Case {
        query: "calculate 2+3*4",
        want: Intent::Math,
    },
    Case {
        query: "how much is (10-2)/4?",
        want: Intent::Math,
    },
    Case {
        query: "create a REST API in rust",
        want: Intent::CodeGen,
    },
    Case {
        query: "write a python fibonacci function",
        want: Intent::CodeGen,
    },
    Case {
        query: "fix the login bug",
        want: Intent::CodeGen,
    },
    Case {
        query: "how to prevent sql injection in login",
        want: Intent::Security,
    },
    Case {
        query: "harden ssh server",
        want: Intent::Security,
    },
    Case {
        query: "nmap scan tutorial for my own server",
        want: Intent::Security,
    },
    Case {
        query: "explain quantum entanglement",
        want: Intent::General,
    },
];

/// Returns (passed, total, failures).
pub fn run() -> (usize, usize, Vec<String>) {
    let mut fails = Vec::new();
    for c in CASES {
        let got = understand(c.query).intent;
        if got != c.want {
            fails.push(format!("{:?} -> got {:?}, want {:?}", c.query, got, c.want));
        }
    }
    (CASES.len() - fails.len(), CASES.len(), fails)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brain_eval_gate() {
        let (passed, total, fails) = run();
        assert!(
            fails.is_empty(),
            "brain eval {passed}/{total} failed:\n{}",
            fails.join("\n")
        );
    }
}
