use crate::theme::Theme;

/// Streaming state for chat tokens.
pub struct StreamState {
    pub buffer: String,
    pub done: bool,
    pub stopped: bool,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            done: false,
            stopped: false,
        }
    }

    pub fn push(&mut self, delta: &str) {
        self.buffer.push_str(delta);
    }

    /// Render current buffer with caret when streaming.
    pub fn render(&self, theme: &Theme) -> String {
        if self.done {
            return crate::markdown::render_markdown(theme, &self.buffer);
        }
        let mut out = crate::markdown::render_markdown(theme, &self.buffer);
        out.push_str(&theme.accent("▍"));
        out
    }
}

impl Default for StreamState {
    fn default() -> Self {
        Self::new()
    }
}
