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
