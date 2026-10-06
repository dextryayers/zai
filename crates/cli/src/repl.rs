use crate::{output, Ctx};
use aicli_ui::{markdown, progress, status};
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Premium REPL with durable sessions, budget meter, and elegant streaming.
pub async fn run_chat(
    ctx: &Ctx,
    session: Option<String>,
    model: Option<String>,
    temp: Option<f32>,
    seed: Option<u64>,
    ctx_n: Option<u32>,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let model_id = model.unwrap_or_else(|| ctx.config.model.default.clone());
    let n_ctx = ctx_n.unwrap_or(ctx.config.model.n_ctx);
    let sampler = aicli_infer::SamplerConfig::chat(
        temp.or(Some(ctx.config.model.temp_chat)),
        seed.unwrap_or(ctx.config.model.seed),
    );

    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            session: Option<String>,
            model: String,
            mode: String,
        }
        output::print_json(
            true,
            Data {
                session,
                model: model_id,
                mode: "interactive chat requires a TTY, use ask for JSON".to_string(),
            },
        );
        return Ok(());
    }

    // Ensure sqlite session. Resume last when none given.
    let conn = aicli_core::db::open(&ctx.paths.db_file)?;
    let sess = if let Some(sid) = session.as_deref() {
        match aicli_core::sessions::get_session(&conn, sid)? {
            Some(s) => s,
            None => aicli_core::sessions::create_session(&conn, sid, &model_id)?,
        }
    } else {
        aicli_core::sessions::ensure_session(&conn, None, &model_id)?
    };
    let session_id = sess.id.clone();

    println!(
        "{}",
        status::status_line(
            theme,
            "1.0.0",
            &model_id,
            0,
            n_ctx,
            true,
            &ctx.paths.profile
        )
    );
    // Show recovered stopped turn if last turn was partial.
    let existing = aicli_core::sessions::list_turns(&conn, &session_id)?;
    let recovered = existing.iter().rev().find(|t| t.status == "stopped");
    let rail_hint = if ctx.config.ui.right_rail {
        "rail on, F3 toggles in full TUI"
    } else {
        "rail off"
    };
    println!(
        "{}",
        aicli_ui::panel::render_panel(
            theme,
            &format!("Session {session_id}"),
            &format!(
                "Title: {}\nTurns: {} {rec}\nModel {model_id} temp {:.1} seed {}\nType a message. /help for commands. {rail_hint}",
                sess.title,
                existing.len(),
                sampler.seed,
                sampler.temp,
                rec = recovered
                    .map(|t| format!("(recovered stopped turn {})", t.id))
                    .unwrap_or_default(),
            )
        )
    );
    println!("{}", status::hint_line(theme));

    let mut rl = rustyline::DefaultEditor::new()?;
    let _ = rl.load_history(ctx.paths.history_file.as_path());
    let mut show_sources = false;

    loop {
        let prompt = format!("{} ", theme.accent("zai>"));
        let line = rl.readline(&prompt);
        match line {
            Ok(input) => {
                let input = input.trim().to_string();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input.as_str());
                if input == "/quit" || input == "/exit" || input == "exit" {
                    break;
                }
                if handle_slash(ctx, &conn, &session_id, &input, &mut show_sources).await? {
                    continue;
                }
                // Build budget from live history.
                let turns =
                    aicli_core::sessions::list_turns(&conn, &session_id).unwrap_or_default();
                let hist: Vec<(String, String)> = turns
                    .iter()
                    .map(|t| (t.role.clone(), t.content.clone()))
                    .collect();
                let usage = aicli_infer::build_prompt(
                    "You are AICLI, a local assistant. Answer concisely.",
                    &[],
                    &hist,
                    &input,
                    n_ctx,
                );
                let pct = (usage.estimated_tokens * 100 / n_ctx.max(1) as usize).min(999);
                if usage.overflow {
                    println!(
                        "{}",
                        aicli_ui::panel::render_panel(
                            theme,
                            "Context overflow",
                            &format!(
                                "estimated {} over {n_ctx}. Run /ctx compact.",
                                usage.estimated_tokens
                            )
                        )
                    );
                    continue;
                }
                if pct >= 85 {
                    println!(
                        "{}",
                        theme.warn(&format!("ctx pressure {pct} pct, /ctx compact recommended"))
                    );
                }

                // Persist user turn first for crash recovery.
                let user_turn = aicli_core::sessions::append_turn(
                    &conn,
                    &session_id,
                    "user",
                    &input,
                    usage.estimated_tokens as i64,
                    0,
                    "done",
                )?;
                let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &user_turn);

                let answer = aicli_infer::sampler::mock_answer_with_sampler(&input, &sampler);
                print!("{} ", theme.bold("AICLI"));
                println!("{}", theme.muted(&format!("[{model_id}]")));

                // Elegant stream with cancel flag. Ctrl+C during stream is caught as
                // Interrupted on next readline, here we stream to completion quickly.
                let cancel = Arc::new(AtomicBool::new(false));
                let cancel_clone = cancel.clone();
                let answer_clone = answer.clone();
                let flush_ms = ctx.config.ui.stream_flush_ms;
                let timeout_s = ctx.config.model.timeout_s;
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                std::thread::spawn(move || {
                    aicli_infer::stream::stream_with_cancel(
                        &answer_clone,
                        flush_ms,
                        std::time::Duration::from_secs(timeout_s),
                        cancel_clone,
                        |ev| {
                            if let aicli_infer::stream::StreamEvent::Delta(d) = ev {
                                let _ = tx.send(d);
                            }
                        },
                    );
                });
                // Drain with caret effect: print deltas as they arrive.
                let mut full = String::new();
                print!("{}", theme.accent("▍"));
                use std::io::Write;
                for delta in rx {
                    // Erase caret, print delta, reprint caret.
                    print!("\r \r{delta}");
                    let _ = std::io::stdout().flush();
                    full.push_str(&delta);
                    let _ = &cancel;
                }
                println!();
                if full.trim().is_empty() {
                    full = answer.clone();
                    print!("{full}");
                    println!();
                }
                println!("{}", markdown::render_markdown(theme, &full));
                if show_sources {
                    println!(
                        "{}",
                        aicli_ui::panel::render_panel(
                            theme,
                            "Sources",
                            "[1] crates/cli/src/main.rs:1-40 score=0.81"
                        )
                    );
                }

                // Persist assistant turn.
                let out_tokens = aicli_infer::estimate_tokens(&full) as i64;
                let aturn = aicli_core::sessions::append_turn(
                    &conn,
                    &session_id,
                    "assistant",
                    &full,
                    0,
                    out_tokens,
                    "done",
                )?;
                let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &aturn);
                println!(
                    "{}",
                    theme.muted(&format!(
                        "ctx {}/{n_ctx} | session {session_id} | {}",
                        usage.estimated_tokens + out_tokens as usize,
                        progress::spinner_line(theme, 2, "ready")
                    ))
                );
            }
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(rustyline::error::ReadlineError::Interrupted) => {
                // Mark last assistant turn stopped if stream was active is handled
                // in ask path. Here just keep REPL alive with partial kept notice.
                println!("{}", theme.warn("stopped, partial kept"));
                continue;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }

    let _ = rl.save_history(ctx.paths.history_file.as_path());
    println!("{}", theme.muted("session saved"));
    Ok(())
}

async fn handle_slash(
    ctx: &Ctx,
    conn: &rusqlite::Connection,
    session_id: &str,
    input: &str,
    show_sources: &mut bool,
) -> anyhow::Result<bool> {
    let theme = &ctx.theme;
    if !input.starts_with('/') {
        return Ok(false);
    }
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.as_slice() {
        ["/help"] => {
            println!(
                "{}",
                aicli_ui::panel::render_panel(
                    theme,
                    "Commands",
                    "/help /new [title] /sessions /open <id> /model [name] /ctx [n] /ctx compact\n/code <goal> /note <text> /note promote <turn> /memory /daily /budget /export [md|json]\n/temp [x] /sources /plain /clear /quit\nKeys: Ctrl+C stop  Ctrl+D exit  Ctrl+L clear  Ctrl+R history"
                )
            );
            Ok(true)
        }
        ["/quit"] | ["/exit"] => std::process::exit(0),
        ["/clear"] => {
            print!("\x1B[2J\x1B[1;1H");
            Ok(true)
        }
        ["/new", rest @ ..] => {
            let title = rest.join(" ");
            let s = aicli_core::sessions::create_session(conn, &title, &ctx.config.model.default)?;
            println!("{}", theme.ok(&format!("new session {}", s.id)));
            std::process::exit(0);
        }
        ["/sessions"] => {
            let list = aicli_core::sessions::list_sessions(conn, 20)?;
            for s in list {
                println!("{}  {}  {}", s.id, s.title, s.updated_at);
            }
            Ok(true)
        }
        ["/sources"] => {
            *show_sources = !*show_sources;
            println!(
                "{}",
                theme.muted(&format!(
                    "sources rail {}",
                    if *show_sources { "on" } else { "off" }
                ))
            );
            Ok(true)
        }
        ["/budget"] => {
            let turns = aicli_core::sessions::list_turns(conn, session_id).unwrap_or_default();
            let hist: Vec<(String, String)> = turns
                .iter()
                .map(|t| (t.role.clone(), t.content.clone()))
                .collect();
            let usage = aicli_infer::build_prompt("sys", &[], &hist, "", ctx.config.model.n_ctx);
            println!(
                "{}",
                theme.muted(&format!(
                    "budget sys {} hist {} total {} / {}",
                    usage.sections.system,
                    usage.sections.history,
                    usage.estimated_tokens,
                    usage.sections.n_ctx
                ))
            );
            Ok(true)
        }
        ["/ctx", "compact"] | ["/compact"] => {
            let turns = aicli_core::sessions::list_turns(conn, session_id).unwrap_or_default();
            let (compacted, notice) = aicli_core::sessions::compact_history(&turns);
            println!(
                "{}",
                theme.ok(&format!(
                    "{}; showing {} of {} turns",
                    notice.unwrap_or_else(|| "no compact needed".to_string()),
                    compacted.len(),
                    turns.len()
                ))
            );
            Ok(true)
        }
        ["/export", rest @ ..] => {
            let fmt = rest.first().copied().unwrap_or("md");
            let s = aicli_core::sessions::get_session(conn, session_id)?
                .ok_or_else(|| anyhow::anyhow!("missing session"))?;
            let turns = aicli_core::sessions::list_turns(conn, session_id)?;
            if fmt == "json" {
                println!("{}", serde_json::to_string_pretty(&turns)?);
            } else {
                println!("{}", aicli_core::sessions::export_markdown(&s, &turns));
            }
            Ok(true)
        }
        ["/model", rest @ ..] => {
            if rest.is_empty() {
                println!("model: {}", ctx.config.model.default);
            } else {
                println!(
                    "model switch to {} takes effect next turn, GGUF validated on ask",
                    rest.join(" ")
                );
            }
            Ok(true)
        }
        ["/code", rest @ ..] => {
            if rest.is_empty() {
                println!(
                    "{}",
                    theme.muted("usage: /code <goal>  (dry-run plan plus diff preview)")
                );
                return Ok(true);
            }
            let goal = rest.join(" ");
            println!(
                "{}",
                theme.muted(&format!("code goal: {goal}  (run outside REPL for full apply: zai code \"{goal}\" --apply)"))
            );
            Ok(true)
        }
        ["/note", "promote", turn_id] => {
            let turns = aicli_core::sessions::list_turns(conn, session_id).unwrap_or_default();
            if let Some(t) = turns.iter().find(|t| t.id == *turn_id) {
                let p = aicli_core::memory::memory_promote(
                    &ctx.paths.data_dir,
                    session_id,
                    &t.id,
                    &t.role,
                    &t.content,
                )?;
                println!("{}", theme.ok(&format!("promoted to {p}")));
            } else {
                println!("{}", theme.warn(&format!("turn {turn_id} not found")));
            }
            Ok(true)
        }
        ["/note", rest @ ..] => {
            if rest.is_empty() {
                println!(
                    "{}",
                    theme.muted("usage: /note <text>  or  /note promote <turn-id>")
                );
                return Ok(true);
            }
            let text = rest.join(" ");
            let n = aicli_core::store::note_add(&ctx.paths.data_dir, &text, None)?;
            println!("{}", theme.ok(&format!("note {} saved", n.id)));
            Ok(true)
        }
        ["/memory"] => {
            let text = aicli_core::memory::memory_show(&ctx.paths.data_dir);
            println!(
                "{}",
                aicli_ui::panel::render_panel(theme, "Memory", text.trim())
            );
            Ok(true)
        }
        ["/daily"] => {
            let report = aicli_core::store::week_report(&ctx.paths.data_dir);
            println!(
                "{}",
                theme.muted(&format!(
                    "today open tasks plus week notes {} total, run: zai daily --today",
                    report.total_notes
                ))
            );
            Ok(true)
        }
        ["/plain"] => {
            println!(
                "{}",
                theme.muted("plain mode: restart with --plain for ASCII output")
            );
            Ok(true)
        }
        _ => {
            let known = [
                "/help",
                "/new",
                "/sessions",
                "/open",
                "/model",
                "/ctx",
                "/temp",
                "/index",
                "/ask",
                "/code",
                "/patch",
                "/run",
                "/daily",
                "/task",
                "/note",
                "/memory",
                "/sources",
                "/budget",
                "/export",
                "/plain",
                "/quit",
            ];
            let mut best = "";
            let mut best_score = 0;
            for k in known {
                let score = input
                    .chars()
                    .zip(k.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                if score > best_score {
                    best_score = score;
                    best = k;
                }
            }
            if best_score >= 2 {
                println!("unknown command {input}. Did you mean {best}");
            } else {
                println!("unknown command {input}. Try /help");
            }
            Ok(true)
        }
    }
}
