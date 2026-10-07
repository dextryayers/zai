use crate::theme::Theme;
use std::io::Write;
use std::process::{Command, Stdio};

/// Professional pager: long output scrollable with j/k, arrows, PgUp/PgDn,
/// q to quit. Uses `less -R` when available and stdout is a TTY, else plain
/// print. Respects NO_PAGER=1 and --plain (no paging).
pub fn page_or_print(theme: &Theme, text: &str) {
    let lines = text.lines().count();
    if lines <= 40 || !theme.color || std::env::var("NO_PAGER").is_ok() {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
        return;
    }
    if !is_tty() {
        print!("{text}");
        return;
    }
    if let Ok(mut child) = Command::new("less")
        .args(["-R", "--prompt", "ZAI scroll j/k PgUp/PgDn q quit "])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    } else {
        print!("{text}");
    }
}

fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    #[test]
    fn short_does_not_page() {
        let t = Theme {
            mode: ThemeMode::Plain,
            color: false,
            unicode: false,
            width: 100,
        };
        // Should just print without spawning less (NO_PAGER path).
        page_or_print(&t, "hello\nworld\n");
    }
}
