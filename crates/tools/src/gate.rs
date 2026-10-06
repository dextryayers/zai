/// Shell permission gate. Denylist wins over allowlist in ask mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    Allow,
    Deny { reason: String, hint: String },
}

/// Mode aware gate. deny blocks everything, allow permits everything,
/// anything else falls back to the ask allowlist.
pub fn check_shell_full(
    cmd: &str,
    mode: &str,
    allowlist: &[String],
    denylist: &[String],
) -> GateDecision {
    match mode {
        "deny" => GateDecision::Deny {
            reason: "shell access disabled (mode deny)".to_string(),
            hint: "enable with: zai config set shell ask".to_string(),
        },
        "allow" => GateDecision::Allow,
        _ => check_shell(cmd, allowlist, denylist),
    }
}

pub fn check_shell(cmd: &str, allowlist: &[String], denylist: &[String]) -> GateDecision {
    let c = cmd.trim();
    for d in denylist {
        if c.contains(d.as_str()) {
            return GateDecision::Deny {
                reason: format!("matched denylist entry `{d}`"),
                hint: "choose an allowlisted command, see: zai run --help".to_string(),
            };
        }
    }
    // Exact or prefix match with word boundary.
    for a in allowlist {
        if c == a.as_str() {
            return GateDecision::Allow;
        }
        if c.starts_with(a.as_str()) {
            let rest = &c[a.len()..];
            if rest.is_empty() || rest.starts_with([' ', '\t', '-', '=', '"', '\'']) {
                return GateDecision::Allow;
            }
        }
    }
    let hint = closest_allow(cmd, allowlist);
    GateDecision::Deny {
        reason: "not in allowlist".to_string(),
        hint,
    }
}

fn closest_allow(cmd: &str, allowlist: &[String]) -> String {
    // Simple prefix overlap score, deterministic.
    let mut best: Option<&String> = None;
    let mut best_score = 0;
    for a in allowlist {
        let score = common_prefix_len(cmd, a);
        if score > best_score {
            best_score = score;
            best = Some(a);
        }
    }
    match best {
        Some(b) => format!("closest allowed: `{b}`"),
        None => "allowlist is empty".to_string(),
    }
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_exact_and_prefix() {
        let allow = vec!["cargo test".to_string(), "git status".to_string()];
        let deny = vec!["rm -rf".to_string()];
        assert_eq!(
            check_shell("cargo test --quiet", &allow, &deny),
            GateDecision::Allow
        );
        assert!(matches!(
            check_shell("cargo test; rm -rf /", &allow, &deny),
            GateDecision::Deny { .. }
        ));
    }
}
