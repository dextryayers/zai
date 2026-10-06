/// Slash command parser shared by the full screen TUI and the classic REPL.
/// Keeps command metadata in one place so help text never drifts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
    Help,
    New(Option<String>),
    Sessions,
    Open(String),
    Model(Option<String>),
    Insert {
        path: String,
        name: Option<String>,
        ctx: Option<u32>,
    },
    Manage,
    Ollama {
        op: String,
        arg: Option<String>,
    },
    Run(String),
    Setting {
        key: Option<String>,
        value: Option<String>,
    },
    Effort(Option<String>),
    Budget,
    Compact,
    Export(Option<String>),
    Sources,
    Clear,
    Plain,
    Quit,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct SlashMeta {
    pub name: &'static str,
    pub desc: &'static str,
    pub usage: &'static str,
}

pub const SLASHES: &[SlashMeta] = &[
    SlashMeta {
        name: "/help",
        desc: "Show all commands",
        usage: "/help",
    },
    SlashMeta {
        name: "/new",
        desc: "Start a new session",
        usage: "/new [title]",
    },
    SlashMeta {
        name: "/new-chat",
        desc: "Start a new chat session",
        usage: "/new-chat [title]",
    },
    SlashMeta {
        name: "/sessions",
        desc: "List sessions",
        usage: "/sessions",
    },
    SlashMeta {
        name: "/open",
        desc: "Open a session by id",
        usage: "/open <id>",
    },
    SlashMeta {
        name: "/model",
        desc: "List models or switch active model",
        usage: "/model [id]",
    },
    SlashMeta {
        name: "/insert",
        desc: "Save a GGUF file into the model cache",
        usage: "/insert <file.gguf> [--name id] [--ctx n]",
    },
    SlashMeta {
        name: "/manage",
        desc: "Manage page for Ollama and GGUF",
        usage: "/manage",
    },
    SlashMeta {
        name: "/ollama",
        desc: "Ollama list, pull, rm, show, status",
        usage: "/ollama <list|pull|rm|show|status> [name]",
    },
    SlashMeta {
        name: "/run",
        desc: "Run a shell command under the shell mode gate",
        usage: "/run <cmd>",
    },
    SlashMeta {
        name: "/setting",
        desc: "Show settings page or set one key",
        usage: "/setting [set <key> <value>]",
    },
    SlashMeta {
        name: "/effort",
        desc: "Show or set effort level",
        usage: "/effort [Default|Low|Medium|High|XHigh|Expert]",
    },
    SlashMeta {
        name: "/budget",
        desc: "Show context budget",
        usage: "/budget",
    },
    SlashMeta {
        name: "/compact",
        desc: "Compact history window",
        usage: "/compact",
    },
    SlashMeta {
        name: "/export",
        desc: "Export session as markdown or json",
        usage: "/export [md|json]",
    },
    SlashMeta {
        name: "/sources",
        desc: "Toggle sources rail",
        usage: "/sources",
    },
    SlashMeta {
        name: "/clear",
        desc: "Clear entire chat log view",
        usage: "/clear",
    },
    SlashMeta {
        name: "/plain",
        desc: "Plain output hint",
        usage: "/plain",
    },
    SlashMeta {
        name: "/quit",
        desc: "Save and quit",
        usage: "/quit",
    },
];

/// Complete a slash prefix against known commands. Returns matches in table order.
pub fn complete(prefix: &str) -> Vec<&'static SlashMeta> {
    SLASHES
        .iter()
        .filter(|m| m.name.starts_with(prefix))
        .collect()
}

/// Closest known command for typo hints. Needs at least 2 common prefix chars.
pub fn closest(input: &str) -> Option<&'static str> {
    let mut best = "";
    let mut best_score = 0;
    for m in SLASHES {
        let score = input
            .chars()
            .zip(m.name.chars())
            .take_while(|(a, b)| a == b)
            .count();
        if score > best_score {
            best_score = score;
            best = m.name;
        }
    }
    if best_score >= 2 {
        Some(best)
    } else {
        None
    }
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    if let Some(i) = args.iter().position(|a| a == flag) {
        args.remove(i);
        if i < args.len() {
            return Some(args.remove(i));
        }
    }
    None
}

pub fn parse(input: &str) -> Option<Slash> {
    let t = input.trim();
    if !t.starts_with('/') {
        return None;
    }
    let mut parts: Vec<String> = t.split_whitespace().map(|s| s.to_string()).collect();
    if parts.is_empty() {
        return None;
    }
    let head = parts.remove(0);
    match head.as_str() {
        "/help" => Some(Slash::Help),
        "/new" => Some(Slash::New(if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        })),
        "/new-chat" | "/newchat" | "/nc" => Some(Slash::New(if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        })),
        "/sessions" => Some(Slash::Sessions),
        "/open" => Some(Slash::Open(parts.join(" "))),
        "/model" => Some(Slash::Model(parts.first().cloned())),
        "/insert" => {
            let name = take_flag(&mut parts, "--name");
            let ctx = take_flag(&mut parts, "--ctx").and_then(|v| v.parse::<u32>().ok());
            Some(Slash::Insert {
                path: parts.first().cloned().unwrap_or_default(),
                name,
                ctx,
            })
        }
        "/setting" => {
            if parts.first().map(|s| s.as_str()) == Some("set") {
                Some(Slash::Setting {
                    key: parts.get(1).cloned(),
                    value: parts.get(2).cloned(),
                })
            } else {
                Some(Slash::Setting {
                    key: None,
                    value: None,
                })
            }
        }
        "/effort" => Some(Slash::Effort(parts.first().cloned())),
        "/manage" => Some(Slash::Manage),
        "/run" => Some(Slash::Run(parts.join(" "))),
        "/ollama" => Some(Slash::Ollama {
            op: parts
                .first()
                .cloned()
                .unwrap_or_else(|| "status".to_string()),
            arg: parts.get(1).cloned(),
        }),
        "/budget" => Some(Slash::Budget),
        "/ctx" if parts.first().map(|s| s.as_str()) == Some("compact") => Some(Slash::Compact),
        "/compact" => Some(Slash::Compact),
        "/export" => Some(Slash::Export(parts.first().cloned())),
        "/sources" => Some(Slash::Sources),
        "/clear" => Some(Slash::Clear),
        "/plain" => Some(Slash::Plain),
        "/quit" | "/exit" => Some(Slash::Quit),
        _ => Some(Slash::Unknown(head)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_insert_flags() {
        assert_eq!(
            parse("/insert /tmp/a.gguf --name tiny --ctx 2048"),
            Some(Slash::Insert {
                path: "/tmp/a.gguf".to_string(),
                name: Some("tiny".to_string()),
                ctx: Some(2048),
            })
        );
    }

    #[test]
    fn parses_setting_and_effort() {
        assert_eq!(
            parse("/setting set temp 0.2"),
            Some(Slash::Setting {
                key: Some("temp".to_string()),
                value: Some("0.2".to_string()),
            })
        );
        assert_eq!(
            parse("/setting"),
            Some(Slash::Setting {
                key: None,
                value: None
            })
        );
        assert_eq!(
            parse("/effort High"),
            Some(Slash::Effort(Some("High".to_string())))
        );
        assert_eq!(parse("hello"), None);
        assert_eq!(parse("/manage"), Some(Slash::Manage));
        assert_eq!(
            parse("/run git status"),
            Some(Slash::Run("git status".to_string()))
        );
        assert_eq!(
            parse("/ollama pull llama3.1"),
            Some(Slash::Ollama {
                op: "pull".to_string(),
                arg: Some("llama3.1".to_string()),
            })
        );
        assert_eq!(
            parse("/ollama"),
            Some(Slash::Ollama {
                op: "status".to_string(),
                arg: None,
            })
        );
    }

    #[test]
    fn completion_and_hint() {
        assert!(complete("/mod").iter().any(|m| m.name == "/model"));
        assert_eq!(complete("/").len(), SLASHES.len());
        assert_eq!(closest("/modle"), Some("/model"));
        assert_eq!(closest("/zzz"), None);
    }

    #[test]
    fn parses_new_chat_aliases() {
        assert_eq!(
            parse("/new-chat my work"),
            Some(Slash::New(Some("my work".to_string())))
        );
        assert_eq!(parse("/newchat"), Some(Slash::New(None)));
        assert_eq!(
            parse("/nc hello"),
            Some(Slash::New(Some("hello".to_string())))
        );
        assert_eq!(parse("/clear"), Some(Slash::Clear));
        assert!(complete("/new").iter().any(|m| m.name == "/new-chat"));
    }
}
