use crate::theme::Theme;
use std::io::Write;

/// Elegant spinner frames, low noise, 80 ms per frame.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_ASCII: &[&str] = &["-", "\\", "|", "/"];

/// Render one spinner line for tests and for non indicatif paths.
pub fn spinner_line(theme: &Theme, frame_idx: usize, msg: &str) -> String {
    let frames = if theme.unicode {
        SPINNER_FRAMES
    } else {
        SPINNER_ASCII
    };
    let f = frames[frame_idx % frames.len()];
    if theme.color {
        format!("{} {msg}", theme.accent(f))
    } else {
        format!("{f} {msg}")
    }
}

/// Animated progress bar string, deterministic for tests.
/// width is bar width in chars.
pub fn progress_bar(pct: f64, width: usize) -> String {
    let pct = pct.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "[{}{}] {:5.1} pct",
        "=".repeat(filled),
        " ".repeat(empty),
        pct
    )
}

/// Live download line with speed and ETA, single row, no wrap.
/// Premium: block bar + aligned columns, still contains "pct" for tests.
pub fn download_line(label: &str, done_mb: f64, total_mb: f64, speed_mbs: f64) -> String {
    let pct = if total_mb > 0.0 {
        done_mb / total_mb * 100.0
    } else {
        0.0
    };
    let eta_s = if speed_mbs > 0.0 {
        ((total_mb - done_mb) / speed_mbs).max(0.0) as u64
    } else {
        0
    };
    format!(
        "{label} ▓ {done_mb:.1}/{total_mb:.1} MB │ {speed_mbs:.1} MB/s │ ETA {:02}:{:02} │ {}",
        eta_s / 60,
        eta_s % 60,
        progress_bar(pct, 20)
    )
}

/// Premium step line: "● 2/5 drafting diff".
pub fn step_line(current: usize, total: usize, msg: &str) -> String {
    format!("● {current}/{total} {msg}")
}

/// Stream text with typewriter pacing to stdout. Used for mock demo and
/// for real token stream when UI wants paced output.
pub fn stream_text_paced(text: &str, flush_ms: u64) {
    let mut stdout = std::io::stdout();
    for chunk in text.as_bytes().chunks(8) {
        let _ = stdout.write_all(chunk);
        let _ = stdout.flush();
        if flush_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(flush_ms.min(30)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_endpoints() {
        assert!(progress_bar(0.0, 10).contains("0.0 pct"));
        assert!(progress_bar(100.0, 10).contains("100.0 pct"));
    }
}
