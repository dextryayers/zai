use nu_ansi_term::Color;

/// Theme mode resolved from config plus env.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Auto,
    Dark,
    Light,
    Plain,
}

impl ThemeMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "dark" => Self::Dark,
            "light" => Self::Light,
            "plain" => Self::Plain,
            _ => Self::Auto,
        }
    }

    pub fn use_color(self) -> bool {
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }
        if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
            return false;
        }
        !matches!(self, Self::Plain)
    }

    pub fn use_unicode(self, width: usize) -> bool {
        if !self.use_color() {
            return false;
        }
        if width < 80 {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub mode: ThemeMode,
    pub color: bool,
    pub unicode: bool,
    pub width: usize,
}

impl Theme {
    pub fn new(mode: ThemeMode) -> Self {
        let width = terminal_width();
        Self {
            mode,
            color: mode.use_color(),
            unicode: mode.use_unicode(width),
            width,
        }
    }

    pub fn accent(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Cyan.bold().paint(s).to_string()
    }

    pub fn accent2(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Purple.bold().paint(s).to_string()
    }

    pub fn user_tag(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Green.bold().paint(s).to_string()
    }

    pub fn zai_tag(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Cyan.bold().paint(s).to_string()
    }

    pub fn title(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        nu_ansi_term::Style::new()
            .bold()
            .fg(Color::White)
            .paint(s)
            .to_string()
    }

    pub fn banner(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Cyan.bold().paint(s).to_string()
    }

    pub fn ctx_style(&self, pct: usize, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        if pct >= 85 {
            Color::Yellow.bold().paint(s).to_string()
        } else if pct >= 60 {
            Color::Green.paint(s).to_string()
        } else {
            Color::DarkGray.paint(s).to_string()
        }
    }

    pub fn divider(&self, width: usize) -> String {
        let w = width.clamp(8, 120);
        let ch = if self.unicode { '─' } else { '-' };
        let line: String = std::iter::repeat_n(ch, w).collect();
        self.muted(&line)
    }

    pub fn center_pad(&self, text: &str, width: usize) -> String {
        let len = text.chars().count();
        if len >= width {
            return text.to_string();
        }
        let pad = (width - len) / 2;
        format!("{}{}", " ".repeat(pad), text)
    }

    pub fn ok(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Green.paint(s).to_string()
    }

    pub fn warn(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Yellow.paint(s).to_string()
    }

    pub fn danger(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::Red.paint(s).to_string()
    }

    pub fn muted(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        Color::DarkGray.paint(s).to_string()
    }

    pub fn bold(&self, s: &str) -> String {
        if !self.color {
            return s.to_string();
        }
        nu_ansi_term::Style::new().bold().paint(s).to_string()
    }

    /// Premium brand badge: cyan ZAI block, stable width, no wrap.
    pub fn brand(&self) -> String {
        if !self.color {
            return "ZAI".to_string();
        }
        Color::Black
            .on(Color::Cyan)
            .bold()
            .paint(" ZAI ")
            .to_string()
    }

    /// Professional section title with rule: "── Title ──...".
    pub fn section(&self, title: &str) -> String {
        let w = self
            .width
            .min(100)
            .saturating_sub(title.len() + 6)
            .clamp(8, 80);
        let rule: String = std::iter::repeat_n(if self.unicode { '─' } else { '-' }, w).collect();
        format!(
            "{} {} {}",
            self.muted(&rule),
            self.bold(title),
            self.muted(&rule)
        )
    }

    /// Compact context meter bar: 10 cells, color shifts at 60/85 pct.
    /// Example: "[####------] 1840/4096 (44 pct)".
    pub fn ctx_bar(&self, used: usize, total: u32) -> String {
        let total = total.max(1) as usize;
        let pct = (used * 100 / total).min(999);
        let fill = (pct.min(100) * 10 / 100).clamp(0, 10);
        let bar: String = (0..10)
            .map(|i| {
                if i < fill {
                    if self.unicode {
                        '█'
                    } else {
                        '#'
                    }
                } else if self.unicode {
                    '░'
                } else {
                    '-'
                }
            })
            .collect();
        let label = format!("[{bar}] {used}/{total} ({pct} pct)");
        self.ctx_style(pct, &label)
    }

    /// One-line professional header for non-TUI commands.
    pub fn header_line(&self, version: &str, model: &str, profile: &str) -> String {
        format!(
            "{} {} {} {}",
            self.brand(),
            self.muted(&format!("v{version}")),
            self.accent(&truncate_left(model, 30)),
            self.muted(&format!("· {profile} · offline"))
        )
    }

    /// Gradient logo lines for welcome banners. Cyan to magenta top-bottom.
    pub fn logo(&self, art: &[&str]) -> String {
        if !self.color {
            return art.join("\n");
        }
        let shades = [
            Color::Cyan,
            Color::LightBlue,
            Color::Blue,
            Color::Purple,
            Color::Magenta,
        ];
        art.iter()
            .enumerate()
            .map(|(i, line)| shades[i % shades.len()].bold().paint(*line).to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Success with check mark, warning with triangle, error with cross.
    /// Keeps plain fallback identical text without glyphs shifting tests.
    pub fn success(&self, s: &str) -> String {
        self.ok(&format!("✓ {s}"))
    }

    pub fn failure(&self, s: &str) -> String {
        self.danger(&format!("✗ {s}"))
    }
}

fn truncate_left(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!(
        "...{}",
        s.chars()
            .skip(s.chars().count() - (max - 3))
            .collect::<String>()
    )
}

pub fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(40, 160)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_disables_color() {
        let t = Theme {
            mode: ThemeMode::Plain,
            color: false,
            unicode: false,
            width: 100,
        };
        assert_eq!(t.accent("x"), "x");
    }
}
