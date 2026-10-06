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
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SPIN: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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
    let code_bg = Style::default().fg(Color::Gray).bg(Color::Black);
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

fn load_messages(ctx: &Ctx, session_id: &str) -> Vec<Msg> {
    let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) else {
        return vec![];
    };
    aicli_core::sessions::list_turns(&conn, session_id)
        .unwrap_or_default()
        .into_iter()
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
            Msg::full(role, &text)
        })
        .collect()
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
    let mut session = aicli_core::sessions::ensure_session(&conn0, None, &model_id)?;
    drop(conn0);
    let mut sessions = load_sessions(&ctx);
    let mut sess_sel: usize = sessions
        .iter()
        .position(|s| s.id == session.id)
        .unwrap_or(0);
    let mut messages = load_messages(&ctx, &session.id);
    messages.push(Msg::full(
        Role::Sys,
        "Welcome to Zai. Type a message or /help for commands.",
    ));

    let mut input = String::new();
    let mut cursor: usize = 0;
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
                persist_turns(&ctx, &session.id, q, &routed.text, "done");
                messages.push(Msg::streaming(Role::Zai, &routed.text));
                pending = Some(Pending::Streaming);
                follow = true;
            }
        }
        // Advance streaming reveal.
        let mut stream_done = false;
        for m in messages.iter_mut().rev().take(1) {
            if !m.done {
                m.shown = (m.shown + 8).min(m.flat.len());
                if m.shown >= m.flat.len() {
                    m.done = true;
                    stream_done = true;
                }
                break;
            }
        }
        if stream_done {
            pending = None;
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
                    InsertMsg::Done(Ok(entry)) => {
                        overlay = Overlay::None;
                        toast = Some((
                            format!("saved model {} ({} MB)", entry.id, entry.size_mb),
                            Instant::now(),
                        ));
                        model_id = entry.id.clone();
                        persist_model(&ctx, &entry.id);
                    }
                    InsertMsg::Done(Err(e)) => {
                        overlay = Overlay::Insert {
                            label: "insert failed".to_string(),
                            done: 0,
                            total: 1,
                            finished: Some(format!("error: {e}")),
                        };
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

        let (used, total) = ctx_usage(&ctx, &session.id, &input);
        terminal.draw(|f| {
            draw(
                f, &ctx, version, &model_id, &effort, used, total, &sessions, sess_sel, &messages,
                &input, cursor, &overlay, &toast, &pending, scroll, follow, focus_left, tick,
                boot_until,
            );
        })?;

        if crossterm::event::poll(Duration::from_millis(40))? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    use crossterm::event::{KeyCode, KeyModifiers};
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                            if pending.is_some() {
                                mark_stopped(&ctx, &session.id);
                                pending = None;
                                messages.push(Msg::full(Role::Sys, "stopped, partial kept"));
                            }
                            continue;
                        }
                        (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => break,
                        _ => {}
                    }
                    if !handle_key(
                        &ctx,
                        key.code,
                        &mut input,
                        &mut cursor,
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
    overlay: &Overlay,
    toast: &Option<(String, Instant)>,
    pending: &Option<Pending>,
    scroll: u16,
    follow: bool,
    focus_left: bool,
    tick: usize,
    boot_until: Instant,
) {
    let area = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    // Status bar.
    let pct = if total > 0 {
        used * 100 / total.max(1) as usize
    } else {
        0
    };
    let status = format!(
        "zai {version} | model {model_id} | effort {effort} | ctx {used}/{total} {pct}% | offline | {}",
        ctx.paths.profile
    );
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        rows[0],
    );

    // Body columns.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(1)])
        .split(rows[1]);

    // Left: sessions.
    let items: Vec<ListItem> = sessions
        .iter()
        .map(|s| {
            let head: String = s.title.chars().take(22).collect();
            ListItem::new(vec![
                Line::from(Span::styled(head, Style::default().fg(Color::White))),
                Line::from(Span::styled(
                    format!("{} turns", s.turns),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();
    let left_block = Block::default()
        .borders(Borders::ALL)
        .title(if focus_left {
            " Sessions (focused) "
        } else {
            " Sessions "
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

    // Main: messages.
    let mut lines: Vec<Line<'static>> = Vec::new();
    for m in messages {
        let tag = match m.role {
            Role::User => ("You", Color::Green),
            Role::Zai => ("Zai", Color::Cyan),
            Role::Sys => ("-", Color::DarkGray),
        };
        lines.push(Line::from(vec![
            Span::styled(
                tag.0.to_string(),
                Style::default().fg(tag.1).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));
        lines.extend(m.visible());
        lines.push(Line::from(""));
    }
    if let Some(Pending::Thinking { start, .. }) = pending {
        let frame = SPIN[tick % SPIN.len()];
        lines.push(Line::from(vec![
            Span::styled(
                format!("{frame} thinking"),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!(" {:.1}s", start.elapsed().as_secs_f32()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    let total_lines = lines.len() as u16;
    let view_h = cols[1].height.saturating_sub(2);
    let offset = if follow {
        total_lines.saturating_sub(view_h)
    } else {
        scroll.min(total_lines.saturating_sub(1))
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Chat ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        cols[1],
    );

    // Input with slash completion popup.
    let before: String = input.chars().take(cursor).collect();
    let after: String = input.chars().skip(cursor).collect();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(before),
            Span::styled("▍", Style::default().fg(Color::Cyan)),
            Span::styled(after, Style::default().fg(Color::DarkGray)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if focus_left {
                    " Input (Tab to focus) "
                } else {
                    " Input "
                })
                .border_style(Style::default().fg(if focus_left {
                    Color::DarkGray
                } else {
                    Color::Cyan
                })),
        ),
        rows[2],
    );
    if input.starts_with('/') && !input.contains(' ') {
        let matches = slash::complete(input);
        if !matches.is_empty() {
            let items: Vec<ListItem> = matches
                .iter()
                .take(8)
                .map(|m| {
                    ListItem::new(Line::from(vec![
                        Span::styled(m.name.to_string(), Style::default().fg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(m.desc.to_string(), Style::default().fg(Color::DarkGray)),
                    ]))
                })
                .collect();
            let w = 56u16.min(area.width.saturating_sub(4));
            let h = (items.len() as u16 + 2).min(10);
            let popup = Rect::new(area.x + 2, rows[2].y.saturating_sub(h), w, h);
            f.render_widget(Clear, popup);
            f.render_widget(
                List::new(items).block(Block::default().borders(Borders::ALL).title(" Commands ")),
                popup,
            );
        }
    }

    // Overlays.
    match overlay {
        Overlay::None => {}
        Overlay::Help => {
            let rows: Vec<ListItem> = SLASHES
                .iter()
                .map(|m| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<10}", m.name), Style::default().fg(Color::Cyan)),
                        Span::styled(m.desc.to_string(), Style::default().fg(Color::White)),
                    ]))
                })
                .collect();
            popup_list(f, area, " Help - Esc to close ", &rows, None);
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
            f.render_widget(Clear, popup);
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
            f.render_widget(Clear, popup);
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
        f.render_widget(Clear, popup);
        f.render_widget(
            Paragraph::new(msg.clone()).block(Block::default().borders(Borders::ALL)),
            popup,
        );
    }

    // Boot splash.
    if Instant::now() < boot_until {
        let frame = SPIN[tick % SPIN.len()];
        let popup = centered(area, 40, 5);
        f.render_widget(Clear, popup);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "zai",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{frame} warming up local workspace"),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL)),
            popup,
        );
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn popup_list(f: &mut Frame, area: Rect, title: &str, rows: &[ListItem], sel: Option<usize>) {
    let h = (rows.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(4);
    let w = 64u16.min(area.width.saturating_sub(4));
    let popup = centered(area, w, h);
    f.render_widget(Clear, popup);
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

fn persist_turns(ctx: &Ctx, session_id: &str, input: &str, answer: &str, status: &str) {
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

fn mark_stopped(ctx: &Ctx, session_id: &str) {
    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
        if let Ok(turns) = aicli_core::sessions::list_turns(&conn, session_id) {
            if let Some(last) = turns.last() {
                let _ = conn.execute(
                    "UPDATE turns SET status='stopped' WHERE id=?1",
                    [last.id.clone()],
                );
            }
        }
    }
}

enum InsertMsg {
    Progress(u64, u64),
    Done(Result<aicli_models::ModelEntry, String>),
    DonePull(Result<String, String>),
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    ctx: &Ctx,
    code: crossterm::event::KeyCode,
    input: &mut String,
    cursor: &mut usize,
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
    use crossterm::event::KeyCode;
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

    match code {
        KeyCode::Esc => {
            input.clear();
            *cursor = 0;
            Ok(true)
        }
        KeyCode::Enter => {
            if *focus_left {
                if let Some(row) = sessions.get(*sess_sel).cloned() {
                    if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                        if let Ok(Some(s)) = aicli_core::sessions::get_session(&conn, &row.id) {
                            *session = s;
                            *messages = load_messages(ctx, &session.id);
                            *follow = true;
                            *focus_left = false;
                        }
                    }
                }
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
            *follow = true;
            submit(
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
            )
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                let mut chars: Vec<char> = input.chars().collect();
                chars.remove(*cursor - 1);
                *input = chars.into_iter().collect();
                *cursor -= 1;
            }
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
            }
            Ok(true)
        }
        KeyCode::Tab => {
            // Complete slash command or toggle panel focus.
            if input.starts_with('/') && !input.contains(' ') {
                let matches = slash::complete(input);
                if matches.len() == 1 {
                    *input = format!("{} ", matches[0].name);
                    *cursor = input.chars().count();
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
            let mut chars: Vec<char> = input.chars().collect();
            chars.insert(*cursor, c);
            *input = chars.into_iter().collect();
            *cursor += 1;
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
                messages.clear();
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
                let t = title.unwrap_or_default();
                if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    if let Ok(s) = aicli_core::sessions::create_session(&conn, &t, model_id) {
                        *session = s;
                        *sessions = load_sessions(ctx);
                        messages.clear();
                        messages.push(Msg::full(Role::Sys, &format!("New session {}", session.id)));
                    }
                }
            }
            Slash::Open(id) => {
                if id.trim().is_empty() {
                    messages.push(Msg::full(Role::Sys, "usage: /open <id>"));
                } else if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                    match aicli_core::sessions::get_session(&conn, id.trim()) {
                        Ok(Some(s)) => {
                            *session = s;
                            *messages = load_messages(ctx, &session.id);
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
                path,
                name,
                ctx: nctx,
            } => {
                if path.is_empty() {
                    messages.push(Msg::full(
                        Role::Sys,
                        "usage: /insert <file.gguf> [--name id] [--ctx n]",
                    ));
                } else {
                    start_insert(ctx, &path, name, nctx, overlay, insert_rx);
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

fn start_insert(
    ctx: &Ctx,
    path: &str,
    name: Option<String>,
    n_ctx: Option<u32>,
    overlay: &mut Overlay,
    insert_rx: &mut Option<mpsc::Receiver<InsertMsg>>,
) {
    let src = std::path::PathBuf::from(shellexpand(path));
    let models_dir = ctx.paths.models_dir.clone();
    let (tx, rx) = mpsc::channel();
    *insert_rx = Some(rx);
    *overlay = Overlay::Insert {
        label: format!("copy {}", src.display()),
        done: 0,
        total: 1,
        finished: None,
    };
    std::thread::spawn(move || {
        let res = aicli_models::insert_gguf(&models_dir, &src, name.as_deref(), n_ctx, |d, t| {
            let _ = tx.send(InsertMsg::Progress(d, t));
        });
        let _ = tx.send(InsertMsg::Done(res.map_err(|e| e.to_string())));
    });
}

fn shellexpand(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
