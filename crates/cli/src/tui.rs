/// Full screen TUI launched by bare `zai`.
/// Left panel lists history sessions, main column streams chat,
/// every command starts with `/`. Overlays cover help, models,
/// settings, effort, and GGUF insert progress.
use crate::{
    slash::{self, Slash, SLASHES},
    Ctx,
};
use anyhow::Result;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SPIN: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Dark theme takeover. Every surface uses these, never the terminal
/// default, so light terminals can not leak white into the UI.
const DARK_BG: Color = Color::Rgb(13, 17, 23);
const DARK_PANEL: Color = Color::Rgb(22, 27, 34);

fn dark_bg() -> Style {
    Style::default().bg(DARK_BG)
}

/// Centered welcome art with a unique block font. Rendered on boot
/// splash and as the first chat banner. Pure ASCII plus box blocks
/// so it stays sharp on all modern terminals.
const WELCOME_ART: &[&str] = &[
    "███████╗ █████╗ ██╗",
    "╚══███╔╝██╔══██╗██║",
    "  ███╔╝ ███████║██║",
    " ███╔╝  ██╔══██║██║",
    "███████╗██║  ██║██║",
    "╚══════╝╚═╝  ╚═╝╚═╝",
];

const WELCOME_TITLE: &str = "Welcome to Zai";
const WELCOME_SUB: &str = "Local-first offline assistant - chat, code, recall";

fn welcome_banner_msg() -> String {
    "Fresh chat ready - pick an action above, or just type and press Enter.".to_string()
}

/// Quick start card for the minimal first-run stage. Never plain text:
/// a bordered panel with the fastest actions plus the ready model line.
fn quick_card_lines(model_id: &str, n_ctx: u32) -> Vec<Line<'static>> {
    let row = |a: &str, b: &str| {
        Line::from(vec![
            Span::styled(
                format!(" {a:<9}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(b.to_string(), Style::default().fg(Color::Gray)),
        ])
    };
    vec![
        row("type", "a message, Enter opens your tabs"),
        row("/model", "switch the AI model"),
        row("/insert", "add a GGUF file"),
        row("/help", "all commands, ? works too"),
        Line::from(vec![
            Span::styled(
                format!(" ◆ {} ready", truncate_model(model_id, 30)),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · ctx {n_ctx}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ]
}

/// Centered logo stage for the minimal first-run view.
/// Pads every line so the block font sits in the middle of the width.
fn logo_lines(width: u16) -> Vec<Line<'static>> {
    let w = (width as usize).max(24);
    let mut out: Vec<Line<'static>> = Vec::new();
    for art in WELCOME_ART {
        let pad = (w.saturating_sub(art.chars().count())) / 2;
        out.push(Line::from(vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(
                art.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    let centered = |t: &str, st: Style| {
        let pad = (w.saturating_sub(t.chars().count())) / 2;
        Line::from(vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(t.to_string(), st),
        ])
    };
    out.push(Line::from(""));
    out.push(centered(
        WELCOME_TITLE,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));
    out.push(centered(WELCOME_SUB, Style::default().fg(Color::DarkGray)));
    out
}

/// Shorten a model id from the left so the filename stays visible.
fn truncate_model(id: &str, max: usize) -> String {
    let len = id.chars().count();
    if len <= max {
        return id.to_string();
    }
    format!(
        "...{}",
        id.chars().skip(len - (max - 3)).collect::<String>()
    )
}

fn new_chat_session(
    ctx: &Ctx,
    session: &mut aicli_core::sessions::Session,
    sessions: &mut Vec<SessionRow>,
    messages: &mut Vec<Msg>,
    model_id: &str,
    toast: &mut Option<(String, Instant)>,
    title: Option<String>,
) {
    let t = title.unwrap_or_default();
    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
        if let Ok(s) = aicli_core::sessions::create_session(&conn, &t, model_id) {
            *session = s;
            *sessions = load_sessions(ctx);
            messages.clear();
            messages.push(Msg::full(Role::Sys, &welcome_banner_msg()));
            messages.push(Msg::full(
                Role::Sys,
                &format!("New chat started: {}", session.id),
            ));
            *toast = Some((format!("new chat {}", session.id), Instant::now()));
        }
    }
}

fn clear_chat_log(messages: &mut Vec<Msg>, toast: &mut Option<(String, Instant)>) {
    messages.clear();
    messages.push(Msg::full(Role::Sys, &welcome_banner_msg()));
    messages.push(Msg::full(Role::Sys, "Chat log cleared."));
    *toast = Some(("chat cleared".to_string(), Instant::now()));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    User,
    Zai,
    Sys,
}

struct Msg {
    role: Role,
    styled: Vec<Line<'static>>,
    flat: Vec<(char, Style)>,
    shown: usize,
    done: bool,
    time: String,
}

/// Current clock as HH:MM for fresh message headers.
fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Short HH:MM from an RFC3339 timestamp. Falls back to --:--.
fn short_hm(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[11..16].to_string()
    } else {
        "--:--".to_string()
    }
}

impl Msg {
    fn full(role: Role, text: &str) -> Self {
        let styled = md_to_lines(text);
        let flat = flatten(&styled);
        let shown = flat.len();
        Self {
            role,
            styled,
            flat,
            shown,
            done: true,
            time: now_hm(),
        }
    }

    fn streaming(role: Role, text: &str) -> Self {
        let styled = md_to_lines(text);
        let flat = flatten(&styled);
        Self {
            role,
            styled,
            flat,
            shown: 0,
            done: false,
            time: now_hm(),
        }
    }

    fn visible(&self) -> Vec<Line<'static>> {
        if self.done || self.shown >= self.flat.len() {
            return self.styled.clone();
        }
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut cur: Vec<Span<'static>> = Vec::new();
        let mut style = Style::default();
        for (i, (ch, st)) in self.flat.iter().enumerate() {
            if i >= self.shown {
                break;
            }
            if *ch == '\n' {
                out.push(Line::from(std::mem::take(&mut cur)));
                style = Style::default();
                continue;
            }
            if *st != style {
                style = *st;
            }
            cur.push(Span::styled(ch.to_string(), style));
        }
        if !cur.is_empty() || out.is_empty() {
            cur.push(Span::styled("▍", Style::default().fg(Color::Cyan)));
            out.push(Line::from(cur));
        }
        out
    }
}

fn flatten(lines: &[Line<'static>]) -> Vec<(char, Style)> {
    let mut out = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        for span in &line.spans {
            let st = span.style;
            for ch in span.content.chars() {
                out.push((ch, st));
            }
        }
        if li + 1 < lines.len() {
            out.push(('\n', Style::default()));
        }
    }
    out
}

/// Minimal markdown to styled lines. Never panics on malformed input.
fn md_to_lines(input: &str) -> Vec<Line<'static>> {
    let cyan_bold = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(Color::DarkGray);
    let cyan = Style::default().fg(Color::Cyan);
    let code_bg = Style::default().fg(Color::Gray).bg(DARK_PANEL);
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut in_code = false;
    for raw in input.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            let tag = line.trim_start().trim_start_matches("```").trim();
            let tag = if tag.is_empty() { "code" } else { tag };
            out.push(Line::from(vec![
                Span::styled(if in_code { "┌─ " } else { "└─" }, muted),
                Span::styled(tag.to_string(), muted),
            ]));
            continue;
        }
        if in_code {
            out.push(Line::from(vec![
                Span::styled("│ ", muted),
                Span::styled(line.to_string(), code_bg),
            ]));
            continue;
        }
        let t = line.trim_start();
        if let Some(h) = t
            .strip_prefix("### ")
            .or_else(|| t.strip_prefix("## ").or_else(|| t.strip_prefix("# ")))
        {
            out.push(Line::from(Span::styled(h.to_string(), cyan_bold)));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let mut spans = vec![Span::styled("  • ", muted)];
            spans.extend(inline_spans(item, cyan, bold));
            out.push(Line::from(spans));
        } else if t.starts_with("> ") {
            if let Some(q) = t.strip_prefix("> ") {
                out.push(Line::from(vec![
                    Span::styled("│ ", muted),
                    Span::styled(q.to_string(), muted),
                ]));
            }
        } else if line.trim().is_empty() {
            out.push(Line::from(""));
        } else {
            out.push(Line::from(inline_spans(line, cyan, bold)));
        }
    }
    if out.is_empty() {
        out.push(Line::from(""));
    }
    out
}

fn inline_spans(s: &str, cyan: Style, bold: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '`') {
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push(Span::styled(code, cyan));
                i += end + 2;
                continue;
            }
        }
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(end) = rest.find("**") {
                let b: String = chars[i + 2..i + 2 + end].iter().collect();
                out.push(Span::styled(b, bold));
                i += end + 4;
                continue;
            }
        }
        out.push(Span::raw(chars[i].to_string()));
        i += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    Models {
        items: Vec<ModelRow>,
        sel: usize,
    },
    Settings,
    Effort {
        sel: usize,
    },
    Manage {
        ollama_tab: bool,
        items: Vec<ManageRow>,
        sel: usize,
        confirm: Option<String>,
    },
    Insert {
        label: String,
        done: u64,
        total: u64,
        finished: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManageRow {
    id: String,
    detail: String,
    ollama: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelRow {
    id: String,
    quant: String,
    size: String,
    status: String,
    active: bool,
}

#[derive(Debug, Clone)]
struct SessionRow {
    id: String,
    title: String,
    turns: usize,
}

enum Pending {
    Thinking { input: String, start: Instant },
    Streaming,
}

pub fn launch(ctx: Ctx) -> Result<()> {
    if !console_is_tty() {
        anyhow::bail!("zai TUI needs a TTY. Try: zai ask \"hello\" or zai --help");
    }
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let res = run_app(ctx, &mut terminal);
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    res
}

fn console_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn load_sessions(ctx: &Ctx) -> Vec<SessionRow> {
    let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) else {
        return vec![];
    };
    aicli_core::sessions::list_sessions(&conn, 50)
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            let turns = aicli_core::sessions::list_turns(&conn, &s.id)
                .map(|v| v.len())
                .unwrap_or(0);
            SessionRow {
                id: s.id,
                title: s.title,
                turns,
            }
        })
        .collect()
}

/// Strip legacy mock artifacts from old stored turns on display.
/// Early mock answers embedded a fake fenced block with fn apply_patch
/// and a fake Sources section citing repo paths. The database keeps the
/// original bytes untouched, but the chat view renders them cleaned so
/// old history never looks like internal code. Returns cleaned text plus
/// whether anything was removed.
fn clean_legacy_mock(text: &str) -> (String, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut removed = false;
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("```") {
            let mut j = i + 1;
            let mut is_legacy = false;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                if lines[j].contains("fn apply_patch") {
                    is_legacy = true;
                }
                j += 1;
            }
            if is_legacy {
                removed = true;
                i = (j + 1).min(lines.len());
                continue;
            }
            out.push(lines[i]);
            i += 1;
            continue;
        }
        // Legacy fake citation tail: a Sources: line whose remaining
        // non blank lines all look like the old citation format.
        if t == "Sources:" {
            let tail_legacy = lines[i + 1..].iter().all(|l| {
                let s = l.trim();
                s.is_empty() || s.contains("crates/") || s.contains("(score") || s.starts_with('-')
            }) && lines[i + 1..]
                .iter()
                .any(|l| l.contains("crates/") || l.contains("(score"));
            if tail_legacy {
                removed = true;
                break;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    // Collapse runs of blank lines and trim the tail.
    let mut clean: Vec<&str> = Vec::new();
    let mut blanks = 0;
    for l in out {
        if l.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 {
                clean.push(l);
            } else {
                removed = true;
            }
        } else {
            blanks = 0;
            clean.push(l);
        }
    }
    while clean.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        clean.pop();
    }
    (clean.join("\n"), removed)
}

/// Recent turns per session shown on open. Old sessions can hold hundreds
/// of turns, so only the tail is rendered and the cut is labeled.
const HISTORY_WINDOW: usize = 20;

fn load_messages(ctx: &Ctx, session_id: &str) -> Vec<Msg> {
    let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) else {
        return vec![];
    };
    let turns = aicli_core::sessions::list_turns(&conn, session_id).unwrap_or_default();
    let total = turns.len();
    let start = total.saturating_sub(HISTORY_WINDOW);
    let mut out = Vec::new();
    if start > 0 {
        out.push(Msg::full(
            Role::Sys,
            &format!(
                "history: showing last {} of {total} turns - older stays in the database, /export to review",
                total - start,
            ),
        ));
    }
    let mut cleaned = 0;
    let mut rendered: Vec<Msg> = turns
        .into_iter()
        .skip(start)
        .map(|t| {
            let role = if t.role == "user" {
                Role::User
            } else {
                Role::Zai
            };
            let text = if t.status == "stopped" {
                format!("{}\n\n(stopped)", t.content)
            } else {
                t.content
            };
            let (text, was) = clean_legacy_mock(&text);
            if was {
                cleaned += 1;
            }
            let mut m = Msg::full(role, &text);
            m.time = short_hm(&t.created_at);
            m
        })
        .collect();
    if cleaned > 0 {
        out.push(Msg::full(
            Role::Sys,
            &format!(
                "cleaned {cleaned} legacy mock block(s) from old history view - database untouched, new answers never contain them"
            ),
        ));
    }
    out.append(&mut rendered);
    out
}

fn ctx_usage(ctx: &Ctx, session_id: &str, pending_input: &str) -> (usize, u32) {
    let n_ctx = ctx.config.model.n_ctx;
    let history: Vec<(String, String)> = aicli_core::db::open(&ctx.paths.db_file)
        .ok()
        .and_then(|c| aicli_core::sessions::list_turns(&c, session_id).ok())
        .map(|v| v.into_iter().map(|t| (t.role, t.content)).collect())
        .unwrap_or_default();
    let u = aicli_infer::build_prompt(
        "You are Zai, a local assistant. Answer concisely.",
        &[],
        &history,
        pending_input,
        n_ctx,
    );
    (u.estimated_tokens, n_ctx)
}

#[allow(clippy::too_many_lines)]
fn run_app(ctx: Ctx, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let mut model_id = ctx.config.model.default.clone();
    let mut effort = ctx.config.model.effort.clone();
    let conn0 = aicli_core::db::open(&ctx.paths.db_file)?;
    // Every fresh launch opens a totally clean chat: reuse the last session
    // only when it never got a turn, otherwise start a brand new one.
    // Older chats stay untouched and reachable from the sessions tabs.
    let mut session = match aicli_core::sessions::last_active(&conn0)? {
        Some(s) if count_turns(&ctx, &s.id) == 0 => s,
        _ => aicli_core::sessions::create_session(&conn0, "", &model_id)?,
    };
    drop(conn0);
    let mut sessions = load_sessions(&ctx);
    let mut sess_sel: usize = sessions
        .iter()
        .position(|s| s.id == session.id)
        .unwrap_or(0);
    let mut messages = load_messages(&ctx, &session.id);
    if messages.is_empty() {
        messages.push(Msg::full(Role::Sys, &welcome_banner_msg()));
    } else {
        messages.push(Msg::full(
            Role::Sys,
            "Welcome back. Press / for commands, Ctrl+N for new chat, F1 for help.",
        ));
    }

    let mut input = String::new();
    let mut cursor: usize = 0;
    let mut slash_sel: Option<usize> = None;
    let mut history: Vec<String> = Vec::new();
    let mut hist_idx: Option<usize> = None;
    let mut overlay = Overlay::None;
    let mut toast: Option<(String, Instant)> = None;
    let mut show_sources = true;
    let mut pending: Option<Pending> = None;
    let mut scroll: u16 = 0;
    let mut follow = true;
    let mut focus_left = false;
    let mut insert_rx: Option<mpsc::Receiver<InsertMsg>> = None;
    let boot_until = Instant::now() + Duration::from_millis(600);
    let mut tick: usize = 0;
    let mut chat_turns = count_turns(&ctx, &session.id);
    let mut usage_session = session.id.clone();
    // Context meter cache: recomputed roughly once a second or on input.
    let mut usage_cache: Option<(usize, u32, usize, String)> = None;

    loop {
        tick += 1;
        // Expire toast.
        if let Some((_, t)) = &toast {
            if t.elapsed() > Duration::from_secs(4) {
                toast = None;
            }
        }
        // Thinking phase completes into streaming answer.
        if let Some(Pending::Thinking {
            input: ref q,
            start,
        }) = pending
        {
            if start.elapsed() > Duration::from_millis(450) {
                let sampler = sampler_for_ctx(&ctx, &effort, None);
                let history: Vec<(String, String)> = aicli_core::db::open(&ctx.paths.db_file)
                    .ok()
                    .and_then(|c| aicli_core::sessions::list_turns(&c, &session.id).ok())
                    .map(|v| v.into_iter().map(|t| (t.role, t.content)).collect())
                    .unwrap_or_default();
                let prompt = aicli_infer::build_prompt(
                    "You are Zai, a local assistant. Answer concisely.",
                    &[],
                    &history,
                    q,
                    ctx.config.model.n_ctx,
                )
                .prompt;
                let routed = crate::answer::compose_answer(
                    &ctx,
                    q,
                    &model_id,
                    &sampler,
                    &prompt,
                    ctx.config.model.n_ctx,
                );
                messages.push(Msg::streaming(Role::Zai, &routed.text));
                // Truthful provenance line instead of fake citations.
                // Only shown when the sources rail is on.
                if show_sources {
                    messages.push(Msg::full(
                        Role::Sys,
                        &format!("via {}", routed.backend_note),
                    ));
                }
                pending = Some(Pending::Streaming);
                follow = true;
            }
        }
        // Advance streaming reveal on the newest unfinished message.
        // Scans from the tail so trailing done notes never block it.
        // The user turn is already persisted at submit time, so an
        // interrupted stream only needs the assistant side saved.
        let mut stream_done = false;
        let mut finished_text: Option<String> = None;
        for m in messages.iter_mut().rev() {
            if !m.done {
                m.shown = (m.shown + 8).min(m.flat.len());
                if m.shown >= m.flat.len() {
                    m.done = true;
                    stream_done = true;
                    if m.role == Role::Zai {
                        finished_text = Some(flat_text(m));
                    }
                }
                break;
            }
        }
        if stream_done {
            pending = None;
            if let Some(t) = finished_text {
                persist_assistant(&ctx, &session.id, &t, "done");
                chat_turns += 2;
            }
        }
        // Insert progress pump.
        if let Some(rx) = insert_rx.as_ref() {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    InsertMsg::Progress(d, t) => {
                        if let Overlay::Insert {
                            label, done, total, ..
                        } = &mut overlay
                        {
                            let _ = label;
                            *done = d;
                            *total = t;
                        }
                    }
                    InsertMsg::DoneMany(inserted, failed, default_id) => {
                        overlay = Overlay::None;
                        if inserted.len() == 1 {
                            // Single insert takes over as the active model.
                            if let Some(e) = inserted.first() {
                                model_id = e.id.clone();
                                persist_model(&ctx, &e.id);
                            }
                        } else if let Some(d) = &default_id {
                            model_id = d.clone();
                        }
                        if inserted.is_empty() {
                            messages.push(Msg::full(
                                Role::Sys,
                                &format!(
                                    "insert failed:\n{}",
                                    failed.join("\n").chars().take(800).collect::<String>()
                                ),
                            ));
                            toast = Some(("insert failed".to_string(), Instant::now()));
                        } else {
                            let mut body = inserted
                                .iter()
                                .map(|e| {
                                    format!("saved {} ({} MB, ctx {})", e.id, e.size_mb, e.n_ctx)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            if !failed.is_empty() {
                                body.push_str(&format!(
                                    "\nskipped:\n{}",
                                    failed.join("\n").chars().take(500).collect::<String>()
                                ));
                            }
                            if let Some(d) = &default_id {
                                body.push_str(&format!("\nnow default: {d}"));
                            }
                            messages.push(Msg::full(Role::Sys, &body));
                            toast = Some((
                                format!("saved {} model(s)", inserted.len()),
                                Instant::now(),
                            ));
                            sessions = load_sessions(&ctx);
                        }
                    }
                    InsertMsg::DonePull(Ok(name)) => {
                        overlay = Overlay::None;
                        toast = Some((format!("pulled ollama model {name}"), Instant::now()));
                    }
                    InsertMsg::DonePull(Err(e)) => {
                        overlay = Overlay::Insert {
                            label: "pull failed".to_string(),
                            done: 0,
                            total: 1,
                            finished: Some(format!("error: {e}")),
                        };
                    }
                }
            }
        }

        // Session switch detection: refresh the turn counter and drop
        // the meter cache so the new chat reads exact from the database.
        if usage_session != session.id {
            usage_session = session.id.clone();
            chat_turns = count_turns(&ctx, &session.id);
            usage_cache = None;
        }
        // Context meter is cached: full history scan roughly once a
        // second or when the input changes, never every frame.
        let stale = match &usage_cache {
            Some((_, _, at, snap)) => tick - *at > 24 || *snap != input,
            None => true,
        };
        if stale {
            let (u, t) = ctx_usage(&ctx, &session.id, &input);
            usage_cache = Some((u, t, tick, input.clone()));
        }
        let (used, total) = match &usage_cache {
            Some((u, t, _, _)) => (*u, *t),
            None => (0, ctx.config.model.n_ctx),
        };
        terminal.draw(|f| {
            draw(
                f,
                &ctx,
                version,
                &model_id,
                &effort,
                used,
                total,
                &sessions,
                sess_sel,
                &messages,
                &input,
                cursor,
                slash_sel,
                &overlay,
                &toast,
                &pending,
                scroll,
                follow,
                focus_left,
                tick,
                boot_until,
                &session.id,
                chat_turns,
            );
        })?;

        if crossterm::event::poll(Duration::from_millis(40))? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    use crossterm::event::{KeyCode, KeyModifiers};
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            match &pending {
                                // Cancelled while thinking: the question is
                                // already persisted, nothing partial exists.
                                Some(Pending::Thinking { .. }) => {
                                    pending = None;
                                    messages.push(Msg::full(
                                        Role::Sys,
                                        "cancelled before the answer started",
                                    ));
                                }
                                // Cancelled mid stream: save the real visible
                                // prefix as a stopped turn, then freeze it.
                                Some(Pending::Streaming) => {
                                    if let Some(m) = messages.iter_mut().rev().find(|x| !x.done) {
                                        let part = partial_text(m);
                                        m.shown = m.flat.len();
                                        m.done = true;
                                        persist_assistant(&ctx, &session.id, &part, "stopped");
                                        chat_turns += 1;
                                    }
                                    pending = None;
                                    messages.push(Msg::full(Role::Sys, "stopped, partial kept"));
                                }
                                None => {}
                            }
                            continue;
                        }
                        (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => break,
                        (KeyCode::Char('n'), m) if m.contains(KeyModifiers::CONTROL) => {
                            new_chat_session(
                                &ctx,
                                &mut session,
                                &mut sessions,
                                &mut messages,
                                &model_id,
                                &mut toast,
                                None,
                            );
                            sess_sel = sessions
                                .iter()
                                .position(|s| s.id == session.id)
                                .unwrap_or(0);
                            input.clear();
                            cursor = 0;
                            slash_sel = None;
                            follow = true;
                            continue;
                        }
                        (KeyCode::Char('l'), m) if m.contains(KeyModifiers::CONTROL) => {
                            clear_chat_log(&mut messages, &mut toast);
                            follow = true;
                            continue;
                        }
                        (KeyCode::Char('p'), m) if m.contains(KeyModifiers::CONTROL) => {
                            overlay = Overlay::Help;
                            continue;
                        }
                        (KeyCode::Char('o'), m) if m.contains(KeyModifiers::CONTROL) => {
                            overlay = Overlay::Models {
                                items: model_rows(&ctx, &model_id),
                                sel: 0,
                            };
                            continue;
                        }
                        (KeyCode::Char('e'), m) if m.contains(KeyModifiers::CONTROL) => {
                            let sel = aicli_core::config::EFFORT_LEVELS
                                .iter()
                                .position(|l| l == &effort)
                                .unwrap_or(0);
                            overlay = Overlay::Effort { sel };
                            continue;
                        }
                        (KeyCode::Char('b'), m) if m.contains(KeyModifiers::CONTROL) => {
                            let (u, t) = ctx_usage(&ctx, &session.id, "");
                            messages.push(Msg::full(
                                Role::Sys,
                                &format!("budget {u}/{t} model {model_id} effort {effort}"),
                            ));
                            follow = true;
                            continue;
                        }
                        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                            overlay = Overlay::Settings;
                            continue;
                        }
                        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => {
                            input.clear();
                            cursor = 0;
                            slash_sel = None;
                            continue;
                        }
                        _ => {}
                    }
                    if !handle_key(
                        &ctx,
                        key,
                        &mut input,
                        &mut cursor,
                        &mut slash_sel,
                        &mut history,
                        &mut hist_idx,
                        &mut overlay,
                        &mut sessions,
                        &mut sess_sel,
                        &mut session,
                        &mut messages,
                        &mut show_sources,
                        &mut pending,
                        &mut scroll,
                        &mut follow,
                        &mut focus_left,
                        &mut toast,
                        &mut model_id,
                        &mut effort,
                        &mut insert_rx,
                    )? {
                        break;
                    }
                }
                crossterm::event::Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw(
    f: &mut Frame,
    ctx: &Ctx,
    version: &str,
    model_id: &str,
    effort: &str,
    used: usize,
    total: u32,
    sessions: &[SessionRow],
    sess_sel: usize,
    messages: &[Msg],
    input: &str,
    cursor: usize,
    slash_sel: Option<usize>,
    overlay: &Overlay,
    toast: &Option<(String, Instant)>,
    pending: &Option<Pending>,
    scroll: u16,
    follow: bool,
    focus_left: bool,
    tick: usize,
    boot_until: Instant,
    session_id: &str,
    session_turns: usize,
) {
    let area = f.area();
    // Black stage base so the whole UI sits on black in every terminal.
    f.buffer_mut().set_style(area, dark_bg());
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    // View mode: minimal logo plus chat column until the first user message
    // appears, full tabbed view afterwards. Derived every frame, no flag.
    let minimal = !messages.iter().any(|m| m.role == Role::User);
    let show_rail = !minimal || focus_left;

    // Slim brand line on top. Model facts live in the model bar below chat.
    let top_line = Line::from(vec![
        Span::styled(
            " ZAI ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(format!("v{version}"), Style::default().fg(Color::DarkGray)),
        Span::styled("  ● offline ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("▣ {}", ctx.paths.profile),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("  F1 help", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(top_line), rows[0]);

    // Context meter shared by the model bar below the chat column.
    let pct = if total > 0 {
        used * 100 / total.max(1) as usize
    } else {
        0
    };
    let ctx_color = if pct >= 85 {
        Color::Yellow
    } else if pct >= 60 {
        Color::Green
    } else {
        Color::DarkGray
    };
    let bar_fill = pct.min(100) * 10 / 100;
    let bar: String = (0..10)
        .map(|i| if i < bar_fill { '#' } else { '-' })
        .collect();

    // Content column: centered like OpenCode, max 124 wide with gutters
    // on wide terminals. Top brand line stays full width.
    let content_w = area.width.min(124);
    let content_x = area.x + area.width.saturating_sub(content_w) / 2;
    let content = Rect::new(
        content_x,
        rows[1].y,
        content_w,
        area.height.saturating_sub(rows[1].y),
    );
    let crows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(content);

    // Body: tabbed columns in full mode, logo stage plus full width chat
    // column in minimal mode.
    let chat_area: Rect = if show_rail {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(30), Constraint::Min(1)])
            .split(crows[0]);

        // Left: sessions with premium active marker.
        let items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let head: String = s.title.chars().take(20).collect();
                let head = if head.trim().is_empty() {
                    s.id.clone()
                } else {
                    head
                };
                let active = i == sess_sel.min(sessions.len().saturating_sub(1));
                let marker = if active { "● " } else { "○ " };
                let title_style = if active {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            marker.to_string(),
                            Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }),
                        ),
                        Span::styled(head, title_style),
                    ]),
                    Line::from(Span::styled(
                        format!("  {} turns", s.turns),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();
        let left_block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(if focus_left {
                " ● Sessions - Alt+2 to chat "
            } else {
                " ○ Sessions - Alt+1 to focus "
            })
            .border_style(Style::default().fg(if focus_left {
                Color::Cyan
            } else {
                Color::DarkGray
            }));
        let mut state = ListState::default();
        state.select(Some(sess_sel.min(sessions.len().saturating_sub(1))));
        f.render_stateful_widget(
            List::new(items)
                .block(left_block)
                .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
                .highlight_symbol("> "),
            cols[0],
            &mut state,
        );
        cols[1]
    } else {
        // Minimal stage: hero logo plus a quick start card above a full
        // width chat column. Never a plain screen. Short terminals fall
        // back to the boxed logo, tiny ones to chat only.
        let body_h = crows[0].height;
        if body_h >= 19 {
            let stage = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(9),
                    Constraint::Length(7),
                    Constraint::Min(1),
                ])
                .split(crows[0]);
            f.render_widget(Paragraph::new(logo_lines(stage[0].width)), stage[0]);
            let card_w = stage[1].width.min(62);
            let card_x = stage[1].x + stage[1].width.saturating_sub(card_w) / 2;
            let card = Rect::new(card_x, stage[1].y, card_w, 7.min(stage[1].height));
            clear_black(f, card);
            f.render_widget(
                Paragraph::new(quick_card_lines(model_id, ctx.config.model.n_ctx)).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(" Quick start ")
                        .border_style(Style::default().fg(Color::Cyan)),
                ),
                card,
            );
            stage[2]
        } else if body_h >= 13 {
            let stage = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(11), Constraint::Min(1)])
                .split(crows[0]);
            f.render_widget(
                Paragraph::new(logo_lines(stage[0].width)).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(" ZAI ")
                        .border_style(Style::default().fg(Color::Cyan)),
                ),
                stage[0],
            );
            stage[1]
        } else {
            crows[0]
        }
    };

    // Main: premium message cards. User input uses green YOU card,
    // AI answer uses cyan ZAI card with unique styling, system is muted.
    // Each card header carries the turn time so long histories stay clear.
    let mut lines: Vec<Line<'static>> = Vec::new();
    for m in messages {
        match m.role {
            Role::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "● YOU ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(m.time.clone(), Style::default().fg(Color::DarkGray)),
                    Span::styled(" ────────────────", Style::default().fg(Color::DarkGray)),
                ]));
                for l in m.visible() {
                    let mut spans = vec![Span::styled(
                        "┃ ".to_string(),
                        Style::default().fg(Color::Green),
                    )];
                    spans.extend(l.spans);
                    lines.push(Line::from(spans));
                }
                lines.push(Line::from(""));
            }
            Role::Zai => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "◆ ZAI ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(m.time.clone(), Style::default().fg(Color::DarkGray)),
                    Span::styled(" ────────────────", Style::default().fg(Color::DarkGray)),
                ]));
                for l in m.visible() {
                    let mut spans = vec![Span::styled(
                        "│ ".to_string(),
                        Style::default().fg(Color::Cyan),
                    )];
                    spans.extend(l.spans);
                    lines.push(Line::from(spans));
                }
                lines.push(Line::from(""));
            }
            Role::Sys => {
                lines.push(Line::from(vec![Span::styled(
                    "· ─ ─ ─".to_string(),
                    Style::default().fg(Color::DarkGray),
                )]));
                lines.extend(m.visible());
                lines.push(Line::from(""));
            }
        }
    }
    if let Some(Pending::Thinking { start, .. }) = pending {
        let frame = SPIN[tick % SPIN.len()];
        lines.push(Line::from(vec![
            Span::styled(
                format!("{frame} ZAI is thinking"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {:.1}s - Ctrl+C stops", start.elapsed().as_secs_f32()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    let total_lines = lines.len() as u16;
    let view_h = chat_area.height.saturating_sub(2);
    let offset = if follow {
        total_lines.saturating_sub(view_h)
    } else {
        scroll.min(total_lines.saturating_sub(1))
    };
    let scroll_tag = if follow { "" } else { " - PgDn to follow " };
    let chat_title = if minimal {
        " Chat - type a message, Enter opens tabs ".to_string()
    } else {
        format!(" ◆ Chat{scroll_tag} ")
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .title(chat_title)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        chat_area,
    );

    // Premium input with placeholder and shortcut hints.
    let before: String = input.chars().take(cursor).collect();
    let after: String = input.chars().skip(cursor).collect();
    let input_line = if input.is_empty() {
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("▍", Style::default().fg(Color::Cyan)),
            Span::styled(
                " Type a message or press / for commands...".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(before, Style::default().fg(Color::White)),
            Span::styled("▍", Style::default().fg(Color::Cyan)),
            Span::styled(after, Style::default().fg(Color::Gray)),
        ])
    };
    f.render_widget(
        Paragraph::new(input_line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(if focus_left {
                    " Input - Tab returns to chat ".to_string()
                } else {
                    format!(
                        " Input {}/{} - / commands - Ctrl+N new - Ctrl+L clear - F1 help ",
                        cursor,
                        input.chars().count()
                    )
                })
                .border_style(Style::default().fg(if focus_left {
                    Color::DarkGray
                } else {
                    Color::Cyan
                })),
        ),
        crows[1],
    );
    // Model bar: always below the chat column, first frame to last.
    // The active AI model never leaves this strip. Rich on wide screens,
    // compact on narrow ones, never overflowing.
    let sess_short: String = session_id.chars().take(8).collect();
    let model_meta = aicli_models::find_any(&ctx.paths.models_dir, model_id)
        .map(|e| format!("{} · {} MB", e.quant, e.size_mb))
        .unwrap_or_default();
    let wide = crows[2].width >= 100;
    let model_bar = if wide {
        Line::from(vec![
            Span::styled(
                " ◆ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate_model(model_id, 28),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {model_meta}"), Style::default().fg(Color::Gray)),
            Span::styled(
                format!("  ⚡ {effort}"),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("  "),
            Span::styled(
                format!("ctx {used}/{total} {pct}% [{bar}]"),
                Style::default().fg(ctx_color),
            ),
            Span::styled(
                format!("  ▣ {} {session_turns}t {sess_short}", ctx.paths.profile),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ◆ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate_model(model_id, 20),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("ctx {pct}%"), Style::default().fg(ctx_color)),
            Span::styled(
                format!("  {session_turns}t"),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };
    f.render_widget(Paragraph::new(model_bar), crows[2]);
    if input.starts_with('/') && !input.contains(' ') {
        let matches = slash::complete(input);
        if !matches.is_empty() {
            let sel = slash_sel.unwrap_or(0).min(matches.len().saturating_sub(1));
            let visible: Vec<(usize, &&slash::SlashMeta)> =
                matches.iter().enumerate().take(10).collect();
            let items: Vec<ListItem> = visible
                .iter()
                .map(|(i, m)| {
                    let active = *i == sel;
                    let marker = if active { "> " } else { "  " };
                    let style = if active {
                        Style::default()
                            .bg(Color::Cyan)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{marker}{:<10}", m.name),
                            if active {
                                style
                            } else {
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ),
                        Span::raw(" "),
                        Span::styled(
                            m.usage.to_string(),
                            if active {
                                Style::default().fg(Color::Black).bg(Color::Cyan)
                            } else {
                                Style::default().fg(Color::DarkGray)
                            },
                        ),
                    ]))
                })
                .collect();
            let w = 68u16.min(area.width.saturating_sub(4));
            let h = (items.len() as u16 + 2).min(12);
            let popup = Rect::new(area.x + 2, crows[1].y.saturating_sub(h), w, h);
            clear_black(f, popup);
            f.render_widget(
                List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(" Commands - Up/Down select - Tab/Enter apply - Esc dismiss "),
                ),
                popup,
            );
        }
    }

    // Overlays.
    match overlay {
        Overlay::None => {}
        Overlay::Help => {
            let mut rows: Vec<ListItem> = SLASHES
                .iter()
                .map(|m| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<10}", m.name), Style::default().fg(Color::Cyan)),
                        Span::styled(m.desc.to_string(), Style::default().fg(Color::White)),
                    ]))
                })
                .collect();
            rows.push(ListItem::new(Line::from("")));
            for (k, d) in [
                ("Ctrl+N", "new chat"),
                ("Ctrl+L", "clear log"),
                ("Ctrl+P/F1", "this palette"),
                ("?", "help when input empty"),
                ("Ctrl+O/F2", "models"),
                ("Ctrl+E", "effort"),
                ("Ctrl+B", "budget"),
                ("Ctrl+S", "settings"),
                ("Ctrl+U", "clear input"),
                ("Ctrl+A/Home", "line start"),
                ("Ctrl+K", "kill to cursor end"),
                ("Ctrl+W", "delete word back"),
                ("End/Delete", "line end / forward delete"),
                ("Up/Down", "pick / command or history"),
                ("Tab/Enter", "apply command"),
                ("PgUp/PgDn", "scroll chat"),
                ("Alt+1/Alt+2", "sessions/chat focus"),
                ("Ctrl+C/D", "stop/exit"),
                ("F3", "toggle rail focus"),
            ] {
                rows.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{k:<12}"),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(d.to_string(), Style::default().fg(Color::Gray)),
                ])));
            }
            popup_list(
                f,
                area,
                " Help - Commands plus Shortcuts - Esc to close ",
                &rows,
                None,
            );
        }
        Overlay::Models { items, sel } => {
            let rows: Vec<ListItem> = items
                .iter()
                .map(|m| {
                    let mark = if m.active { "*" } else { " " };
                    ListItem::new(Line::from(vec![
                        Span::raw(format!("{mark} ")),
                        Span::styled(m.id.clone(), Style::default().fg(Color::White)),
                        Span::raw(" "),
                        Span::styled(
                            format!("{} {} {}", m.quant, m.size, m.status),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();
            popup_list(
                f,
                area,
                " Models - Enter to switch, Esc to close ",
                &rows,
                Some(*sel),
            );
        }
        Overlay::Settings => {
            let body = crate::settings::settings_text(ctx, model_id);
            let h = body.lines().count() as u16 + 4;
            let w = 62u16.min(area.width.saturating_sub(4));
            let popup = centered(area, w, h.min(area.height.saturating_sub(4)));
            clear_black(f, popup);
            f.render_widget(
                Paragraph::new(body).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Settings - /setting set <key> <value> "),
                ),
                popup,
            );
        }
        Overlay::Effort { sel } => {
            let rows: Vec<ListItem> = effort_rows(effort)
                .into_iter()
                .map(|(name, desc, active)| {
                    let mark = if active { "*" } else { " " };
                    ListItem::new(Line::from(vec![
                        Span::raw(format!("{mark} ")),
                        Span::styled(name, Style::default().fg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(desc, Style::default().fg(Color::DarkGray)),
                    ]))
                })
                .collect();
            popup_list(
                f,
                area,
                " Effort - Enter to apply, Esc to close ",
                &rows,
                Some(*sel),
            );
        }
        Overlay::Manage {
            ollama_tab,
            items,
            sel,
            confirm,
        } => {
            let tab = if *ollama_tab {
                "Ollama (install + delete)"
            } else {
                "GGUF (delete only, no download)"
            };
            let mut rows: Vec<ListItem> = items
                .iter()
                .map(|m| {
                    ListItem::new(Line::from(vec![
                        Span::styled(m.id.clone(), Style::default().fg(Color::White)),
                        Span::raw(" "),
                        Span::styled(m.detail.clone(), Style::default().fg(Color::DarkGray)),
                    ]))
                })
                .collect();
            if rows.is_empty() {
                rows.push(ListItem::new(Line::from(Span::styled(
                    if *ollama_tab {
                        "no ollama models, press i then: /ollama pull <name>"
                    } else {
                        "no gguf files, press i then: /insert <file.gguf>"
                    },
                    Style::default().fg(Color::DarkGray),
                ))));
            }
            if let Some(c) = confirm {
                rows.push(ListItem::new(Line::from(vec![Span::styled(
                    format!("delete {c}? press y to confirm, n to abort"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )])));
            }
            popup_list(
                f,
                area,
                &format!(" Manage {tab} - Tab switch, i install, d delete, r refresh, Esc close "),
                &rows,
                Some(*sel),
            );
        }
        Overlay::Insert {
            label,
            done,
            total,
            finished,
        } => {
            let ratio = if *total > 0 {
                *done as f64 / *total as f64
            } else {
                0.0
            };
            let msg = if let Some(e) = finished {
                e.clone()
            } else {
                format!("{label}\n{done}/{total} bytes")
            };
            let popup = centered(area, 56.min(area.width.saturating_sub(4)), 7);
            clear_black(f, popup);
            let inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(2)])
                .split(popup);
            let title = if label.starts_with("pull ") {
                " Pull Ollama "
            } else {
                " Insert GGUF "
            };
            f.render_widget(
                Paragraph::new(msg).block(Block::default().borders(Borders::ALL).title(title)),
                inner[0],
            );
            f.render_widget(
                Gauge::default()
                    .block(Block::default().borders(Borders::ALL))
                    .style(dark_bg())
                    .gauge_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .bg(DARK_BG)
                            .add_modifier(Modifier::BOLD),
                    )
                    .ratio(ratio.clamp(0.0, 1.0)),
                inner[1],
            );
        }
    }

    // Toast.
    if let Some((msg, _)) = toast {
        let w = (msg.len() as u16 + 4).min(area.width.saturating_sub(4));
        let popup = Rect::new(
            area.width.saturating_sub(w + 2),
            area.height.saturating_sub(4),
            w,
            3,
        );
        clear_black(f, popup);
        f.render_widget(
            Paragraph::new(msg.clone()).block(Block::default().borders(Borders::ALL)),
            popup,
        );
    }

    // Centered premium boot splash with unique Welcome to Zai font.
    if Instant::now() < boot_until {
        let frame = SPIN[tick % SPIN.len()];
        let popup = centered(area, 52.min(area.width.saturating_sub(4)), 13);
        clear_black(f, popup);
        let mut splash: Vec<Line> = WELCOME_ART
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            })
            .collect();
        splash.push(Line::from(Span::styled(
            WELCOME_TITLE.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        splash.push(Line::from(Span::styled(
            WELCOME_SUB.to_string(),
            Style::default().fg(Color::DarkGray),
        )));
        splash.push(Line::from(Span::styled(
            format!("{frame} warming up local workspace"),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(
            Paragraph::new(splash).alignment(Alignment::Center).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .title(" ZAI ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            popup,
        );
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

/// Clear a popup area and force it black, so overlays never flash the
/// terminal default background in the middle of the black stage.
fn clear_black(f: &mut Frame, area: Rect) {
    f.buffer_mut().set_style(area, dark_bg());
}

fn popup_list(f: &mut Frame, area: Rect, title: &str, rows: &[ListItem], sel: Option<usize>) {
    let h = (rows.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(4);
    let w = 64u16.min(area.width.saturating_sub(4));
    let popup = centered(area, w, h);
    clear_black(f, popup);
    let mut state = ListState::default();
    state.select(sel);
    f.render_stateful_widget(
        List::new(rows.to_vec())
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::DarkGray)),
        popup,
        &mut state,
    );
}

/// Delete one row from the manage page. Ollama models via daemon, GGUF files locally.
fn manage_delete(ctx: &Ctx, id: &str, ollama_tab: bool, toast: &mut Option<(String, Instant)>) {
    if ollama_tab {
        match aicli_models::ollama::rm(id) {
            Ok(()) => *toast = Some((format!("deleted ollama model {id}"), Instant::now())),
            Err(e) => *toast = Some((format!("delete failed: {e}"), Instant::now())),
        }
        return;
    }
    match aicli_models::remove_custom(&ctx.paths.models_dir, id) {
        Ok(true) => *toast = Some((format!("deleted gguf {id}"), Instant::now())),
        Ok(false) => {
            // Builtin entry: drop the cached file, registry stays for re pull.
            if let Some(entry) = aicli_models::find_any(&ctx.paths.models_dir, id) {
                let p = aicli_models::local_path(&ctx.paths.models_dir, &entry);
                if p.exists() && std::fs::remove_file(&p).is_ok() {
                    *toast = Some((format!("deleted gguf file {id}"), Instant::now()));
                } else {
                    *toast = Some((format!("nothing cached for {id}"), Instant::now()));
                }
            } else {
                *toast = Some((format!("unknown model {id}"), Instant::now()));
            }
        }
        Err(e) => *toast = Some((format!("delete failed: {e}"), Instant::now())),
    }
}

/// Manage rows split by tab. Ollama rows need a live daemon, GGUF rows are local only.
fn refresh_manage(ctx: &Ctx) -> (Vec<ManageRow>, Vec<ManageRow>) {
    let ollama_rows: Vec<ManageRow> = aicli_models::ollama::list()
        .unwrap_or_default()
        .into_iter()
        .map(|m| ManageRow {
            id: m.name.clone(),
            detail: format!(
                "{} MB modified {}",
                aicli_models::ollama::size_mb(m.size),
                m.modified.get(..10).unwrap_or(m.modified.as_str())
            ),
            ollama: true,
        })
        .collect();
    let gguf_rows: Vec<ManageRow> = aicli_models::list_all(&ctx.paths.models_dir)
        .into_iter()
        .map(|m| {
            let p = aicli_models::local_path(&ctx.paths.models_dir, &m);
            let detail = if p.exists() {
                format!(
                    "{} MB cached{}",
                    m.size_mb,
                    if m.repo == "local" { " local" } else { "" }
                )
            } else {
                "not downloaded, GGUF has no download".to_string()
            };
            ManageRow {
                id: m.id,
                detail,
                ollama: false,
            }
        })
        .collect();
    (ollama_rows, gguf_rows)
}

fn effort_rows(current: &str) -> Vec<(String, String, bool)> {
    aicli_core::config::EFFORT_LEVELS
        .iter()
        .map(|name| {
            let (temp, top_p, tokens, steps) =
                aicli_core::config::effort_profile(name).unwrap_or((0.6, 0.9, 1024, 12));
            (
                name.to_string(),
                format!("temp {temp} top_p {top_p} tokens {tokens} steps {steps}"),
                *name == current,
            )
        })
        .collect()
}

fn sampler_for_ctx(ctx: &Ctx, effort: &str, temp: Option<f32>) -> aicli_infer::SamplerConfig {
    let seed = ctx.config.model.seed;
    let (_, etop_p, _, _) =
        aicli_core::config::effort_profile(effort).unwrap_or((0.6, 0.9, 1024, 12));
    aicli_infer::SamplerConfig {
        temp: aicli_core::config::resolve_temp(temp, ctx.config.model.temp_chat, 0.6, effort),
        top_p: etop_p,
        seed,
        mode: "chat".to_string(),
    }
}

fn persist_model(ctx: &Ctx, id: &str) {
    let mut cfg = ctx.config.clone();
    cfg.model.default = id.to_string();
    let _ = cfg.save(&ctx.paths.config_file);
}

/// Persist the user turn the moment Enter is pressed, so an interrupted
/// answer never loses the question from history.
fn persist_user(ctx: &Ctx, session_id: &str, input: &str) {
    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
        if let Ok(t) = aicli_core::sessions::append_turn(
            &conn,
            session_id,
            "user",
            input,
            aicli_infer::estimate_tokens(input) as i64,
            0,
            "done",
        ) {
            let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &t);
        }
    }
}

/// Persist the assistant turn with its final status, done or stopped.
fn persist_assistant(ctx: &Ctx, session_id: &str, answer: &str, status: &str) {
    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
        if let Ok(t) = aicli_core::sessions::append_turn(
            &conn,
            session_id,
            "assistant",
            answer,
            0,
            aicli_infer::estimate_tokens(answer) as i64,
            status,
        ) {
            let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &t);
        }
    }
}

/// Full text of a message from its flattened chars.
fn flat_text(m: &Msg) -> String {
    m.flat.iter().map(|(ch, _)| *ch).collect()
}

/// Visible prefix of a streaming message, used for real partial saves.
fn partial_text(m: &Msg) -> String {
    m.flat.iter().take(m.shown).map(|(ch, _)| *ch).collect()
}

/// Turn count for one session, used for the model bar tag.
fn count_turns(ctx: &Ctx, session_id: &str) -> usize {
    aicli_core::db::open(&ctx.paths.db_file)
        .ok()
        .and_then(|c| aicli_core::sessions::list_turns(&c, session_id).ok())
        .map(|v| v.len())
        .unwrap_or(0)
}

enum InsertMsg {
    Progress(u64, u64),
    DoneMany(Vec<aicli_models::ModelEntry>, Vec<String>, Option<String>),
    DonePull(Result<String, String>),
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    ctx: &Ctx,
    key: crossterm::event::KeyEvent,
    input: &mut String,
    cursor: &mut usize,
    slash_sel: &mut Option<usize>,
    history: &mut Vec<String>,
    hist_idx: &mut Option<usize>,
    overlay: &mut Overlay,
    sessions: &mut Vec<SessionRow>,
    sess_sel: &mut usize,
    session: &mut aicli_core::sessions::Session,
    messages: &mut Vec<Msg>,
    show_sources: &mut bool,
    pending: &mut Option<Pending>,
    scroll: &mut u16,
    follow: &mut bool,
    focus_left: &mut bool,
    toast: &mut Option<(String, Instant)>,
    model_id: &mut String,
    effort: &mut String,
    insert_rx: &mut Option<mpsc::Receiver<InsertMsg>>,
) -> Result<bool> {
    use crossterm::event::{KeyCode, KeyModifiers};
    let code = key.code;
    let mods = key.modifiers;
    // Global premium shortcuts work even with overlays closed.
    if *overlay == Overlay::None {
        if mods.contains(KeyModifiers::ALT) {
            match code {
                KeyCode::Char('1') => {
                    *focus_left = true;
                    return Ok(true);
                }
                KeyCode::Char('2') => {
                    *focus_left = false;
                    return Ok(true);
                }
                _ => {}
            }
        }
        match code {
            KeyCode::F(1) => {
                *overlay = Overlay::Help;
                return Ok(true);
            }
            KeyCode::F(2) => {
                *overlay = Overlay::Models {
                    items: model_rows(ctx, model_id),
                    sel: 0,
                };
                return Ok(true);
            }
            KeyCode::F(3) => {
                *focus_left = !*focus_left;
                *toast = Some((
                    format!("focus {}", if *focus_left { "sessions" } else { "chat" }),
                    Instant::now(),
                ));
                return Ok(true);
            }
            // Bare ? opens help, mirroring the palette shortcut.
            KeyCode::Char('?') if input.is_empty() => {
                *overlay = Overlay::Help;
                return Ok(true);
            }
            _ => {}
        }
    }
    // Overlay navigation first.
    match overlay {
        Overlay::Manage {
            ollama_tab,
            items,
            sel,
            confirm,
        } => match code {
            KeyCode::Esc => {
                if confirm.is_some() {
                    *confirm = None;
                } else {
                    *overlay = Overlay::None;
                }
                return Ok(true);
            }
            KeyCode::Tab => {
                *ollama_tab = !*ollama_tab;
                let (o, g) = refresh_manage(ctx);
                *items = if *ollama_tab { o } else { g };
                *sel = 0;
                *confirm = None;
                return Ok(true);
            }
            KeyCode::Up => {
                *sel = sel.saturating_sub(1);
                return Ok(true);
            }
            KeyCode::Down => {
                *sel = (*sel + 1).min(items.len().saturating_sub(1));
                return Ok(true);
            }
            KeyCode::Char('r') => {
                let (o, g) = refresh_manage(ctx);
                *items = if *ollama_tab { o } else { g };
                *confirm = None;
                return Ok(true);
            }
            KeyCode::Char('i') => {
                *input = if *ollama_tab {
                    "/ollama pull ".to_string()
                } else {
                    "/insert ".to_string()
                };
                *cursor = input.chars().count();
                *overlay = Overlay::None;
                *focus_left = false;
                return Ok(true);
            }
            KeyCode::Char('d') => {
                if let Some(row) = items.get(*sel) {
                    *confirm = Some(row.id.clone());
                }
                return Ok(true);
            }
            KeyCode::Char('y') => {
                if let Some(id) = confirm.take() {
                    manage_delete(ctx, &id, *ollama_tab, toast);
                    let (o, g) = refresh_manage(ctx);
                    *items = if *ollama_tab { o } else { g };
                    *sel = 0;
                }
                return Ok(true);
            }
            KeyCode::Char('n') => {
                *confirm = None;
                return Ok(true);
            }
            _ => return Ok(true),
        },
        Overlay::Models { items, sel } => match code {
            KeyCode::Esc => {
                *overlay = Overlay::None;
                return Ok(true);
            }
            KeyCode::Up => {
                *sel = sel.saturating_sub(1);
                return Ok(true);
            }
            KeyCode::Down => {
                *sel = (*sel + 1).min(items.len().saturating_sub(1));
                return Ok(true);
            }
            KeyCode::Enter => {
                if let Some(row) = items.get(*sel) {
                    *model_id = row.id.clone();
                    persist_model(ctx, &row.id);
                    *toast = Some((format!("model {}", row.id), Instant::now()));
                    *sessions = load_sessions(ctx);
                }
                *overlay = Overlay::None;
                return Ok(true);
            }
            _ => return Ok(true),
        },
        Overlay::Effort { sel } => match code {
            KeyCode::Esc => {
                *overlay = Overlay::None;
                return Ok(true);
            }
            KeyCode::Up => {
                *sel = sel.saturating_sub(1);
                return Ok(true);
            }
            KeyCode::Down => {
                *sel = (*sel + 1).min(aicli_core::config::EFFORT_LEVELS.len().saturating_sub(1));
                return Ok(true);
            }
            KeyCode::Enter => {
                let level = aicli_core::config::EFFORT_LEVELS[*sel].to_string();
                *effort = level.clone();
                let mut cfg = ctx.config.clone();
                cfg.model.effort = level.clone();
                let _ = cfg.save(&ctx.paths.config_file);
                *toast = Some((format!("effort {level}"), Instant::now()));
                *overlay = Overlay::None;
                return Ok(true);
            }
            _ => return Ok(true),
        },
        Overlay::Help | Overlay::Settings => match code {
            KeyCode::Esc | KeyCode::Enter => {
                *overlay = Overlay::None;
                return Ok(true);
            }
            _ => return Ok(true),
        },
        Overlay::Insert { finished, .. } => match code {
            KeyCode::Esc if finished.is_some() => {
                *overlay = Overlay::None;
                *insert_rx = None;
                return Ok(true);
            }
            _ => return Ok(true),
        },
        Overlay::None => {}
    }

    // Slash popup navigation has priority when typing a slash command.
    // Up/Down moves selection, Tab/Enter applies it, Esc dismisses.
    let slash_active = input.starts_with('/') && !input.contains(' ') && !*focus_left;
    if slash_active {
        let matches = slash::complete(input);
        if !matches.is_empty() {
            let len = matches.len();
            match code {
                KeyCode::Up => {
                    let cur = slash_sel.unwrap_or(0);
                    *slash_sel = Some(if cur == 0 { len - 1 } else { cur - 1 });
                    return Ok(true);
                }
                KeyCode::Down => {
                    let cur = slash_sel.unwrap_or(0);
                    *slash_sel = Some((cur + 1) % len);
                    return Ok(true);
                }
                KeyCode::Tab => {
                    let sel = slash_sel.unwrap_or(0).min(len - 1);
                    *input = format!("{} ", matches[sel].name);
                    *cursor = input.chars().count();
                    *slash_sel = None;
                    return Ok(true);
                }
                KeyCode::Enter => {
                    // If input is only a prefix, complete it instead of submitting.
                    let sel = slash_sel.unwrap_or(0).min(len - 1);
                    let picked = matches[sel].name.to_string();
                    if input.trim() != picked {
                        *input = format!("{picked} ");
                        *cursor = input.chars().count();
                        *slash_sel = None;
                        return Ok(true);
                    }
                    // Exact match falls through to normal submit below.
                }
                KeyCode::Esc => {
                    *slash_sel = None;
                    input.clear();
                    *cursor = 0;
                    return Ok(true);
                }
                _ => {}
            }
        }
    }

    match code {
        KeyCode::Esc => {
            input.clear();
            *cursor = 0;
            *slash_sel = None;
            Ok(true)
        }
        KeyCode::Enter => {
            if *focus_left {
                if let Some(row) = sessions.get(*sess_sel).cloned() {
                    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                        if let Ok(Some(s)) = aicli_core::sessions::get_session(&conn, &row.id) {
                            *session = s;
                            *messages = load_messages(ctx, &session.id);
                            if messages.is_empty() {
                                messages.push(Msg::full(Role::Sys, &welcome_banner_msg()));
                            }
                            *follow = true;
                            *focus_left = false;
                        }
                    }
                }
                return Ok(true);
            }
            // Never swallow a message while an answer streams. Keep the
            // input intact and say why instead of dropping it silently.
            if pending.is_some() {
                *toast = Some((
                    "busy - answer streaming, Enter queues nothing, Ctrl+C stops".to_string(),
                    Instant::now(),
                ));
                return Ok(true);
            }
            let text = input.trim().to_string();
            if text.is_empty() {
                return Ok(true);
            }
            history.push(text.clone());
            *hist_idx = None;
            input.clear();
            *cursor = 0;
            *slash_sel = None;
            *follow = true;
            let cont = submit(
                ctx,
                &text,
                session,
                sessions,
                messages,
                show_sources,
                pending,
                toast,
                model_id,
                effort,
                overlay,
                insert_rx,
            )?;
            // Session switching commands replace the active session.
            // Resync the highlight so the rail never points elsewhere.
            *sess_sel = sessions
                .iter()
                .position(|s| s.id == session.id)
                .unwrap_or(*sess_sel);
            Ok(cont)
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                let mut chars: Vec<char> = input.chars().collect();
                chars.remove(*cursor - 1);
                *input = chars.into_iter().collect();
                *cursor -= 1;
                *slash_sel = Some(0);
            }
            Ok(true)
        }
        KeyCode::Delete => {
            let len = input.chars().count();
            if *cursor < len {
                let mut chars: Vec<char> = input.chars().collect();
                chars.remove(*cursor);
                *input = chars.into_iter().collect();
            }
            Ok(true)
        }
        KeyCode::Home => {
            *cursor = 0;
            Ok(true)
        }
        KeyCode::End => {
            *cursor = input.chars().count();
            Ok(true)
        }
        KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => {
            *cursor = 0;
            Ok(true)
        }
        KeyCode::Char('k') if mods.contains(KeyModifiers::CONTROL) => {
            let kept: String = input.chars().take(*cursor).collect();
            *input = kept;
            Ok(true)
        }
        KeyCode::Char('w') if mods.contains(KeyModifiers::CONTROL) => {
            let mut chars: Vec<char> = input.chars().collect();
            let mut end = (*cursor).min(chars.len());
            while end > 0 && chars[end - 1] == ' ' {
                end -= 1;
            }
            while end > 0 && chars[end - 1] != ' ' {
                end -= 1;
            }
            chars.drain(end..*cursor);
            *input = chars.into_iter().collect();
            *cursor = end;
            Ok(true)
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            Ok(true)
        }
        KeyCode::Right => {
            *cursor = (*cursor + 1).min(input.chars().count());
            Ok(true)
        }
        KeyCode::Up => {
            if *focus_left {
                if *sess_sel > 0 {
                    *sess_sel -= 1;
                }
                return Ok(true);
            }
            if !history.is_empty() {
                let next = hist_idx
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(history.len() - 1);
                *hist_idx = Some(next.min(history.len() - 1));
                *input = history[hist_idx.unwrap()].clone();
                *cursor = input.chars().count();
                *slash_sel = None;
            }
            Ok(true)
        }
        KeyCode::Down => {
            if *focus_left {
                *sess_sel = (*sess_sel + 1).min(sessions.len().saturating_sub(1));
                return Ok(true);
            }
            if let Some(i) = *hist_idx {
                if i + 1 < history.len() {
                    *hist_idx = Some(i + 1);
                    *input = history[i + 1].clone();
                } else {
                    *hist_idx = None;
                    input.clear();
                }
                *cursor = input.chars().count();
                *slash_sel = None;
            }
            Ok(true)
        }
        KeyCode::Tab => {
            // Slash selection already handled above. Here Tab toggles focus
            // or completes a common prefix when popup is visible.
            if input.starts_with('/') && !input.contains(' ') {
                let matches = slash::complete(input);
                if matches.len() == 1 {
                    *input = format!("{} ", matches[0].name);
                    *cursor = input.chars().count();
                    *slash_sel = None;
                } else if let Some(common) = common_prefix(&matches) {
                    if common.len() > input.len() {
                        *input = common;
                        *cursor = input.chars().count();
                    }
                }
            } else {
                *focus_left = !*focus_left;
            }
            Ok(true)
        }
        KeyCode::PageUp => {
            *follow = false;
            *scroll = scroll.saturating_add(5);
            Ok(true)
        }
        KeyCode::PageDown => {
            *scroll = scroll.saturating_sub(5);
            if *scroll == 0 {
                *follow = true;
            }
            Ok(true)
        }
        KeyCode::Char(c) => {
            // Alt combos already handled. Plain chars edit the input.
            if mods.contains(KeyModifiers::ALT) || mods.contains(KeyModifiers::CONTROL) {
                return Ok(true);
            }
            let mut chars: Vec<char> = input.chars().collect();
            chars.insert(*cursor, c);
            *input = chars.into_iter().collect();
            *cursor += 1;
            if input.starts_with('/') && !input.contains(' ') {
                *slash_sel = Some(0);
            }
            Ok(true)
        }
        _ => Ok(true),
    }
}

fn common_prefix(matches: &[&crate::slash::SlashMeta]) -> Option<String> {
    if matches.is_empty() {
        return None;
    }
    let first = matches[0].name;
    let mut len = first.len();
    for m in &matches[1..] {
        len = first
            .chars()
            .zip(m.name.chars())
            .take_while(|(a, b)| a == b)
            .count()
            .min(len);
    }
    Some(first[..len].to_string())
}

/// Execute /run inside the TUI under the shell mode gate.
/// Approval prompts cannot block the event loop, so ask mode without
/// auto yes points at the terminal command instead of hanging.
fn run_in_tui(ctx: &Ctx, cmd: &str, messages: &mut Vec<Msg>) {
    let full = cmd.trim();
    if full.is_empty() {
        messages.push(Msg::full(Role::Sys, "usage: /run <cmd>"));
        return;
    }
    if let aicli_tools::GateDecision::Deny { reason, hint } = aicli_tools::check_shell_full(
        full,
        &ctx.config.tools.shell_mode,
        &ctx.config.tools.shell_allowlist,
        &ctx.config.tools.shell_denylist,
    ) {
        messages.push(Msg::full(Role::Sys, &format!("denied: {reason}. {hint}")));
        return;
    }
    let auto_yes = std::env::var("ZAI_AUTO_YES")
        .or_else(|_| std::env::var("AICLI_AUTO_YES"))
        .is_ok();
    if ctx.config.tools.shell_mode != "allow" && ctx.config.tools.confirm_shell && !auto_yes {
        messages.push(Msg::full(
            Role::Sys,
            &format!("approval needed, run in terminal: zai run -- {full}\nor set: /setting set shell allow"),
        ));
        return;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match aicli_tools::run_blocking(
        full,
        &cwd,
        &ctx.config.tools.shell_mode,
        &ctx.config.tools.shell_allowlist,
        &ctx.config.tools.shell_denylist,
        std::time::Duration::from_secs(60),
        Some(&ctx.paths.logs_dir.join("run.log")),
    ) {
        Ok((preview, _, code)) => {
            let _ = aicli_tools::log_event(
                &ctx.paths.data_dir,
                "shell.run",
                None,
                &format!("cmd={full} exit={code}"),
            );
            messages.push(Msg::full(
                Role::Sys,
                &format!("$ {full} (exit {code})\n{preview}"),
            ));
        }
        Err(e) => messages.push(Msg::full(Role::Sys, &format!("run failed: {e}"))),
    }
}

/// Execute /ollama in the TUI. Pull runs in a thread with gauge progress.
fn run_ollama_slash(
    _ctx: &Ctx,
    op: &str,
    arg: Option<&str>,
    messages: &mut Vec<Msg>,
    toast: &mut Option<(String, Instant)>,
    overlay: &mut Overlay,
    insert_rx: &mut Option<mpsc::Receiver<InsertMsg>>,
) {
    match op {
        "list" => match aicli_models::ollama::list() {
            Ok(models) if models.is_empty() => {
                messages.push(Msg::full(
                    Role::Sys,
                    "no ollama models installed. Run: /ollama pull <name>",
                ));
            }
            Ok(models) => {
                let body: String = models
                    .iter()
                    .map(|m| format!("{} ({} MB)", m.name, aicli_models::ollama::size_mb(m.size)))
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(Msg::full(Role::Sys, &format!("Ollama models:\n{body}")));
            }
            Err(e) => messages.push(Msg::full(Role::Sys, &format!("ollama list failed: {e}"))),
        },
        "pull" => {
            let Some(name) = arg.filter(|s| !s.trim().is_empty()) else {
                messages.push(Msg::full(Role::Sys, "usage: /ollama pull <name>"));
                return;
            };
            let name = name.trim().to_string();
            let (tx, rx) = mpsc::channel();
            *insert_rx = Some(rx);
            *overlay = Overlay::Insert {
                label: format!("pull {name}"),
                done: 0,
                total: 1,
                finished: None,
            };
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                let res = aicli_models::ollama::pull(&name, move |p| {
                    let (d, t) = p.map(|(d, t, _)| (d, t)).unwrap_or((0, 1));
                    let _ = tx2.send(InsertMsg::Progress(d, t));
                });
                let _ = tx.send(InsertMsg::DonePull(
                    res.map(|()| name).map_err(|e| e.to_string()),
                ));
            });
        }
        "rm" | "delete" | "remove" => {
            let Some(name) = arg.filter(|s| !s.trim().is_empty()) else {
                messages.push(Msg::full(Role::Sys, "usage: /ollama rm <name>"));
                return;
            };
            match aicli_models::ollama::rm(name.trim()) {
                Ok(()) => {
                    *toast = Some((
                        format!("deleted ollama model {}", name.trim()),
                        Instant::now(),
                    ))
                }
                Err(e) => messages.push(Msg::full(Role::Sys, &format!("ollama rm failed: {e}"))),
            }
        }
        "show" => {
            let Some(name) = arg.filter(|s| !s.trim().is_empty()) else {
                messages.push(Msg::full(Role::Sys, "usage: /ollama show <name>"));
                return;
            };
            match aicli_models::ollama::show(name.trim()) {
                Ok(v) => messages.push(Msg::full(
                    Role::Sys,
                    &serde_json::to_string_pretty(&v).unwrap_or_default(),
                )),
                Err(e) => messages.push(Msg::full(Role::Sys, &format!("ollama show failed: {e}"))),
            }
        }
        _ => {
            let up = if aicli_models::ollama::daemon_reachable() {
                "reachable"
            } else {
                "down, start with: ollama serve"
            };
            let bin = if aicli_models::ollama::binary_exists() {
                "installed"
            } else {
                "missing, see https://ollama.com"
            };
            messages.push(Msg::full(
                Role::Sys,
                &format!("Ollama daemon {up}, binary {bin}.\nusage: /ollama <list|pull|rm|show|status> [name]"),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn submit(
    ctx: &Ctx,
    text: &str,
    session: &mut aicli_core::sessions::Session,
    sessions: &mut Vec<SessionRow>,
    messages: &mut Vec<Msg>,
    show_sources: &mut bool,
    pending: &mut Option<Pending>,
    toast: &mut Option<(String, Instant)>,
    model_id: &mut String,
    effort: &mut String,
    overlay: &mut Overlay,
    insert_rx: &mut Option<mpsc::Receiver<InsertMsg>>,
) -> Result<bool> {
    if let Some(cmd) = slash::parse(text) {
        match cmd {
            Slash::Quit => return Ok(false),
            Slash::Help => {
                *overlay = Overlay::Help;
            }
            Slash::Clear => {
                clear_chat_log(messages, toast);
            }
            Slash::Sessions => {
                *sessions = load_sessions(ctx);
                let body: String = sessions
                    .iter()
                    .map(|s| format!("{}  {} ({} turns)", s.id, s.title, s.turns))
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(Msg::full(Role::Sys, &format!("Sessions:\n{body}")));
            }
            Slash::New(title) => {
                new_chat_session(ctx, session, sessions, messages, model_id, toast, title);
            }
            Slash::Open(id) => {
                if id.trim().is_empty() {
                    messages.push(Msg::full(Role::Sys, "usage: /open <id>"));
                } else if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    match aicli_core::sessions::get_session(&conn, id.trim()) {
                        Ok(Some(s)) => {
                            *session = s;
                            *messages = load_messages(ctx, &session.id);
                            if messages.is_empty() {
                                messages.push(Msg::full(Role::Sys, &welcome_banner_msg()));
                            }
                        }
                        _ => messages.push(Msg::full(
                            Role::Sys,
                            &format!("unknown session {}", id.trim()),
                        )),
                    }
                }
            }
            Slash::Model(arg) => {
                if let Some(id) = arg {
                    if aicli_models::find_any(&ctx.paths.models_dir, id.trim()).is_some() {
                        *model_id = id.trim().to_string();
                        persist_model(ctx, model_id);
                        *toast = Some((format!("model {model_id}"), Instant::now()));
                    } else {
                        messages.push(Msg::full(
                            Role::Sys,
                            &format!("unknown model {}. Saved models: /model", id.trim()),
                        ));
                    }
                } else {
                    *overlay = Overlay::Models {
                        items: model_rows(ctx, model_id),
                        sel: 0,
                    };
                }
            }
            Slash::Insert {
                paths,
                name,
                ctx: nctx,
                default,
                recursive,
            } => {
                if paths.is_empty() {
                    messages.push(Msg::full(Role::Sys, &insert_help()));
                } else if paths.len() > 1 && name.is_some() {
                    messages.push(Msg::full(
                        Role::Sys,
                        "--name only works with a single file, drop it for multi insert",
                    ));
                } else {
                    start_insert(
                        ctx, &paths, name, nctx, default, recursive, overlay, insert_rx,
                    );
                }
            }
            Slash::Setting { key, value } => {
                if let (Some(k), Some(v)) = (key, value) {
                    match crate::settings::apply_setting(ctx, &k, &v) {
                        Ok(msg) => *toast = Some((msg, Instant::now())),
                        Err(e) => {
                            messages.push(Msg::full(Role::Sys, &format!("setting error: {e}")))
                        }
                    }
                } else {
                    *overlay = Overlay::Settings;
                }
            }
            Slash::Effort(level) => {
                if let Some(l) = level {
                    match crate::settings::apply_effort(ctx, &l) {
                        Ok(msg) => {
                            *effort = l.clone();
                            *toast = Some((msg, Instant::now()));
                        }
                        Err(e) => {
                            messages.push(Msg::full(Role::Sys, &format!("effort error: {e}")))
                        }
                    }
                } else {
                    let sel = aicli_core::config::EFFORT_LEVELS
                        .iter()
                        .position(|l| l == effort)
                        .unwrap_or(0);
                    *overlay = Overlay::Effort { sel };
                }
            }
            Slash::Manage => {
                let (o, g) = refresh_manage(ctx);
                *overlay = Overlay::Manage {
                    ollama_tab: true,
                    items: o,
                    sel: 0,
                    confirm: None,
                };
                let _ = g;
            }
            Slash::Ollama { op, arg } => {
                run_ollama_slash(
                    ctx,
                    &op,
                    arg.as_deref(),
                    messages,
                    toast,
                    overlay,
                    insert_rx,
                );
            }
            Slash::Budget => {
                let (used, total) = ctx_usage(ctx, &session.id, "");
                messages.push(Msg::full(
                    Role::Sys,
                    &format!("budget {used}/{total} model {model_id} effort {effort}"),
                ));
            }
            Slash::Compact => {
                if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    if let Ok(turns) = aicli_core::sessions::list_turns(&conn, &session.id) {
                        let (compacted, notice) = aicli_core::sessions::compact_history(&turns);
                        messages.push(Msg::full(
                            Role::Sys,
                            &format!(
                                "{}; showing {} of {} turns",
                                notice.unwrap_or_else(|| "no compact needed".to_string()),
                                compacted.len(),
                                turns.len()
                            ),
                        ));
                    }
                }
            }
            Slash::History(n) => {
                let want = n.unwrap_or(15).clamp(1, 100);
                if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    if let Ok(turns) = aicli_core::sessions::list_turns(&conn, &session.id) {
                        if turns.is_empty() {
                            messages.push(Msg::full(Role::Sys, "no exchanges yet in this chat"));
                        } else {
                            let start = turns.len().saturating_sub(want);
                            let body: String = turns
                                .iter()
                                .skip(start)
                                .map(|t| {
                                    let who = if t.role == "user" { "YOU" } else { "ZAI" };
                                    let first: String = t
                                        .content
                                        .lines()
                                        .next()
                                        .unwrap_or("")
                                        .chars()
                                        .take(64)
                                        .collect();
                                    let (clean, _) = clean_legacy_mock(&first);
                                    format!("{} {who}: {}", short_hm(&t.created_at), clean)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            messages
                                .push(Msg::full(Role::Sys, &format!("recent exchanges:\n{body}")));
                        }
                    }
                }
            }
            Slash::Export(fmt) => {
                let f = fmt.unwrap_or_else(|| "md".to_string());
                if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    if let Ok(Some(s)) = aicli_core::sessions::get_session(&conn, &session.id) {
                        if let Ok(turns) = aicli_core::sessions::list_turns(&conn, &session.id) {
                            if f == "json" {
                                messages.push(Msg::full(
                                    Role::Sys,
                                    &serde_json::to_string_pretty(&turns).unwrap_or_default(),
                                ));
                            } else {
                                messages.push(Msg::full(
                                    Role::Sys,
                                    &aicli_core::sessions::export_markdown(&s, &turns),
                                ));
                            }
                        }
                    }
                }
            }
            Slash::Run(cmd) => {
                run_in_tui(ctx, &cmd, messages);
            }
            Slash::Sources => {
                *show_sources = !*show_sources;
                *toast = Some((
                    format!("sources {}", if *show_sources { "on" } else { "off" }),
                    Instant::now(),
                ));
            }
            Slash::Plain => {
                messages.push(Msg::full(
                    Role::Sys,
                    "plain mode: restart with --plain for ASCII output",
                ));
            }
            Slash::Unknown(head) => {
                if let Some(hint) = slash::closest(&head) {
                    messages.push(Msg::full(
                        Role::Sys,
                        &format!("unknown command {head}. Did you mean {hint}"),
                    ));
                } else {
                    messages.push(Msg::full(
                        Role::Sys,
                        &format!("unknown command {head}. Try /help"),
                    ));
                }
            }
        }
        return Ok(true);
    }
    if pending.is_some() {
        return Ok(true);
    }
    // The question is stored before the answer exists, so history never
    // loses it even when the stream is cancelled halfway.
    persist_user(ctx, &session.id, text);
    messages.push(Msg::full(Role::User, text));
    *pending = Some(Pending::Thinking {
        input: text.to_string(),
        start: Instant::now(),
    });
    Ok(true)
}

fn model_rows(ctx: &Ctx, active: &str) -> Vec<ModelRow> {
    let mut rows: Vec<ModelRow> = aicli_models::list_all(&ctx.paths.models_dir)
        .into_iter()
        .map(|m| {
            let p = aicli_models::local_path(&ctx.paths.models_dir, &m);
            let status = if p.exists() {
                "cached".to_string()
            } else if m.repo == "local" {
                "missing".to_string()
            } else {
                "download".to_string()
            };
            ModelRow {
                id: m.id.clone(),
                quant: m.quant.clone(),
                size: format!("{} MB", m.size_mb),
                status,
                active: m.id == active,
            }
        })
        .collect();
    // Installed Ollama models appear as ollama/<name> when the daemon is up.
    for m in aicli_models::ollama::list().unwrap_or_default() {
        let id = aicli_models::ollama::to_id(&m.name);
        rows.push(ModelRow {
            id: id.clone(),
            quant: "ollama".to_string(),
            size: format!("{} MB", aicli_models::ollama::size_mb(m.size)),
            status: "daemon".to_string(),
            active: id == active,
        });
    }
    rows
}

/// Smart help for /insert with no args. Lists nearby *.gguf candidates
/// from the current dir and ~/models so the user can copy a path.
fn insert_help() -> String {
    let mut body = String::from(
        "usage: /insert <file.gguf> [more...] [--name id] [--ctx n] [--default] [--recursive]\n\
         Pass a folder to import every .gguf inside it. Alias: /add.",
    );
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    for dir in aicli_models::default_scan_dirs() {
        for p in aicli_models::find_gguf_files(&dir, false)
            .into_iter()
            .take(4)
        {
            if !cands.contains(&p) {
                cands.push(p);
            }
            if cands.len() >= 6 {
                break;
            }
        }
        if cands.len() >= 6 {
            break;
        }
    }
    if cands.is_empty() {
        body.push_str("\nno .gguf found in ./models ~/models, try: /insert ~/models/tiny.gguf");
    } else {
        body.push_str("\nfound nearby:");
        for p in cands {
            body.push_str(&format!("\n  {}", p.display()));
        }
    }
    body
}

#[allow(clippy::too_many_arguments)]
fn start_insert(
    ctx: &Ctx,
    paths: &[String],
    name: Option<String>,
    n_ctx: Option<u32>,
    set_default: bool,
    recursive: bool,
    overlay: &mut Overlay,
    insert_rx: &mut Option<mpsc::Receiver<InsertMsg>>,
) {
    if paths.len() > 1 && name.is_some() {
        // Fail fast without a thread so the message shows instantly.
        return;
    }
    let raws: Vec<String> = paths.to_vec();
    let models_dir = ctx.paths.models_dir.clone();
    let config_file = ctx.paths.config_file.clone();
    let (tx, rx) = mpsc::channel();
    *insert_rx = Some(rx);
    let label = if raws.len() == 1 {
        format!("copy {}", raws[0])
    } else {
        format!("insert {} paths", raws.len())
    };
    *overlay = Overlay::Insert {
        label,
        done: 0,
        total: 1,
        finished: None,
    };
    std::thread::spawn(move || {
        if raws.len() > 1 && name.is_some() {
            let _ = tx.send(InsertMsg::DoneMany(
                vec![],
                vec!["--name only works with a single file".to_string()],
                None,
            ));
            return;
        }
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for raw in &raws {
            let p = aicli_models::resolve_insert_path(raw);
            if p.is_dir() {
                let found = aicli_models::find_gguf_files(&p, recursive);
                if found.is_empty() {
                    failed.push(format!(
                        "{}: no .gguf files found, retry with --recursive",
                        p.display()
                    ));
                } else {
                    files.extend(found);
                }
            } else if p.exists() {
                files.push(p);
            } else {
                failed.push(format!("{}: file not found", p.display()));
            }
        }
        let mut inserted: Vec<aicli_models::ModelEntry> = Vec::new();
        for src in &files {
            match aicli_models::insert_gguf(&models_dir, src, name.as_deref(), n_ctx, |d, t| {
                let _ = tx.send(InsertMsg::Progress(d, t));
            }) {
                Ok(e) => inserted.push(e),
                Err(e) => failed.push(format!("{}: {e}", src.display())),
            }
        }
        let mut default_id: Option<String> = None;
        if !inserted.is_empty() {
            let cached = aicli_models::list_all(&models_dir)
                .iter()
                .filter(|m| aicli_models::local_path(&models_dir, m).exists())
                .count();
            if set_default || cached <= 1 {
                if let Some(last) = inserted.last() {
                    // Best effort default switch from the worker thread.
                    if let Ok(mut cfg) = aicli_core::Config::load(&config_file) {
                        cfg.model.default = last.id.clone();
                        let _ = cfg.save(&config_file);
                    }
                    default_id = Some(last.id.clone());
                }
            }
        }
        let _ = tx.send(InsertMsg::DoneMany(inserted, failed, default_id));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_MOCK: &str = "## Answer v1\n\nYou asked: hello\n\nMock stream with temp 0.6.\n\n```rust\n// preview of coding agent output\nfn apply_patch() -> bool {\n    true\n}\n```\n\n- point one\n\nSources:\n- `crates/cli/src/main.rs:1-40` (score 0.81)\n";

    #[test]
    fn legacy_mock_is_stripped_on_display() {
        let (clean, was) = clean_legacy_mock(LEGACY_MOCK);
        assert!(was);
        assert!(!clean.contains("```"));
        assert!(!clean.contains("apply_patch"));
        assert!(!clean.contains("crates/"));
        assert!(!clean.contains("Sources:"));
        assert!(clean.contains("You asked: hello"));
    }

    #[test]
    fn real_user_code_is_preserved() {
        let real = "help me review:\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\nend";
        let (clean, was) = clean_legacy_mock(real);
        assert!(!was);
        assert_eq!(clean, real);
    }

    #[test]
    fn genuine_sources_tail_is_preserved() {
        let text = "answer here\n\nSources:\n- docs/guide.md:10-20 score=0.91";
        let (clean, was) = clean_legacy_mock(text);
        assert!(!was, "real citations must survive: {clean}");
    }

    #[test]
    fn short_hm_slices_rfc3339() {
        assert_eq!(short_hm("2026-10-06T10:10:18Z"), "10:10");
        assert_eq!(short_hm("x"), "--:--");
    }

    #[test]
    fn logo_lines_are_centered() {
        let lines = logo_lines(60);
        assert_eq!(lines.len(), WELCOME_ART.len() + 3);
        for l in &lines {
            let width: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(width <= 60, "logo line overflows: {width}");
        }
        // Art block is wider than the title, so its padding is smaller.
        let art_pad = lines[0].spans[0].content.len();
        let title_pad = lines[WELCOME_ART.len() + 1].spans[0].content.len();
        assert!(art_pad < title_pad);
    }

    #[test]
    fn flat_and_partial_text_roundtrip() {
        let m = Msg::full(Role::Zai, "hello\nworld");
        assert_eq!(flat_text(&m), "hello\nworld");
        let mut s = Msg::streaming(Role::Zai, "hello\nworld");
        s.shown = 5;
        assert_eq!(partial_text(&s), "hello");
        assert_eq!(partial_text(&Msg::streaming(Role::Zai, "abc")), "");
    }

    #[test]
    fn dark_theme_is_actually_dark() {
        for c in [DARK_BG, DARK_PANEL] {
            if let Color::Rgb(r, g, b) = c {
                assert!(r < 40 && g < 40 && b < 40, "theme bg must stay dark");
            } else {
                panic!("theme bg must be explicit rgb, never terminal default");
            }
        }
        assert_ne!(DARK_BG, DARK_PANEL);
    }

    #[test]
    fn quick_card_has_actions_and_ready_model() {
        let lines = quick_card_lines("tiny-q4-k-m", 2048);
        assert_eq!(lines.len(), 5);
        let flat: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(flat.contains("/model"));
        assert!(flat.contains("/insert"));
        assert!(flat.contains("tiny-q4-k-m"));
        assert!(flat.contains("2048"));
    }

    #[test]
    fn truncate_model_keeps_filename_visible() {
        assert_eq!(truncate_model("tiny", 32), "tiny");
        let long = "qwen2.5-coder-3b-instruct-q6_k-very-long-name";
        let short = truncate_model(long, 20);
        assert!(short.starts_with("..."));
        assert!(short.ends_with("long-name"));
    }
}
