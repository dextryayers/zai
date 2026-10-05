use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Token stream events for UI.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Start,
    Delta(String),
    Done { elapsed_ms: u128, tokens: usize },
    Stopped { partial: String },
}

/// Simulate token stream from full answer with flush cadence, cancel, timeout.
/// Real llama backend will call the same callback shape.
pub fn stream_with_cancel(
    answer: &str,
    flush_ms: u64,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
    mut on_event: impl FnMut(StreamEvent),
) {
    on_event(StreamEvent::Start);
    let t0 = Instant::now();
    let mut tokens = 0;
    let mut buf = String::new();
    let mut partial = String::new();
    for word in answer.split_inclusive([' ', '\n']) {
        if cancel.load(Ordering::Relaxed) {
            on_event(StreamEvent::Stopped {
                partial: partial.clone(),
            });
            return;
        }
        if t0.elapsed() > timeout {
            on_event(StreamEvent::Stopped {
                partial: partial.clone(),
            });
            return;
        }
        buf.push_str(word);
        partial.push_str(word);
        tokens += 1;
        if buf.len() >= 24 {
            on_event(StreamEvent::Delta(buf.clone()));
            buf.clear();
            if flush_ms > 0 {
                std::thread::sleep(Duration::from_millis(flush_ms.min(20)));
            }
        }
    }
    if !buf.is_empty() {
        on_event(StreamEvent::Delta(buf));
    }
    on_event(StreamEvent::Done {
        elapsed_ms: t0.elapsed().as_millis(),
        tokens,
    });
}
