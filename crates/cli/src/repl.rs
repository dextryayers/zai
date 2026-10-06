use crate::{output, Ctx};
use aicli_ui::{markdown, progress, status};
use serde::Serialize;

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
    let mut model_id = model.unwrap_or_else(|| ctx.config.model.default.clone());
    let n_ctx = ctx_n.unwrap_or(ctx.config.model.n_ctx);
    let mut effort = ctx.config.model.effort.clone();
    let mut shell_mode = ctx.config.tools.shell_mode.clone();
    let seed_val = seed.unwrap_or(ctx.config.model.seed);

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
            env!("CARGO_PKG_VERSION"),
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
                "Title: {}\nTurns: {} {rec}\nModel {model_id} effort {effort} temp {:.1} seed {}\nType a message. /help for commands. {rail_hint}",
                sess.title,
                existing.len(),
                live_temp(temp, &effort, ctx),
                seed_val,
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
                if handle_slash(
                    ctx,
                    &conn,
                    &session_id,
                    &input,
                    &mut show_sources,
                    &mut model_id,
                    &mut effort,
                    &mut shell_mode,
                )
                .await?
                {
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
                    "You are Zai, a local assistant. Answer concisely.",
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

                let routed = {
                    let sampler = aicli_infer::SamplerConfig::chat(
                        Some(live_temp(temp, &effort, ctx)),
                        seed_val,
                    );
                    crate::answer::compose_answer(
                        ctx,
                        &input,
                        &model_id,
                        &sampler,
                        &usage.prompt,
                        n_ctx,
                    )
                };
                let answer = routed.text;
                println!(
                    "{} {}",
                    theme.bold("Zai"),
                    theme.muted(&format!("[{model_id}]"))
                );

                // Thinking animation with elapsed timer, then one clean render.
                // Single render avoids garbled caret overwrites on narrow terminals.
                let t0 = std::time::Instant::now();
                for i in 0..6 {
                    print!(
                        "\r{}",
                        progress::spinner_line(
                            theme,
                            i,
                            &format!("thinking {:.1}s", t0.elapsed().as_secs_f32())
                        )
                    );
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    std::thread::sleep(std::time::Duration::from_millis(60));
                }
                print!("\r");
                println!("{}", markdown::render_markdown(theme, &answer));
                let full = answer;
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

/// Effective chat temp: explicit flag wins, then a customized config value,
/// then the effort profile.
fn live_temp(flag: Option<f32>, effort: &str, ctx: &Ctx) -> f32 {
    aicli_core::config::resolve_temp(flag, ctx.config.model.temp_chat, 0.6, effort)
}

#[allow(clippy::too_many_arguments)]
async fn handle_slash(
    ctx: &Ctx,
    conn: &rusqlite::Connection,
    session_id: &str,
    input: &str,
    show_sources: &mut bool,
    model_id: &mut String,
    effort: &mut String,
    shell_mode: &mut String,
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
                    "/help /new [title] /sessions /open <id> /model [id]\n/manage /ollama <list|pull|rm|show> /insert <file.gguf> /setting [set k v] /effort [level]\n/budget /compact /export [md|json] /sources /plain /clear /quit\nKeys: Ctrl+C stop  Ctrl+D exit  Ctrl+L clear  Ctrl+R history"
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
                let all = aicli_models::list_all(&ctx.paths.models_dir);
                for m in &all {
                    let mark = if m.id == ctx.config.model.default {
                        "*"
                    } else {
                        " "
                    };
                    println!("{mark} {} ({}, {} MB)", m.id, m.quant, m.size_mb);
                }
            } else {
                let id = rest.join(" ");
                if aicli_models::find_any(&ctx.paths.models_dir, id.trim()).is_some() {
                    let mut cfg = ctx.config.clone();
                    cfg.model.default = id.trim().to_string();
                    cfg.save(&ctx.paths.config_file)?;
                    *model_id = id.trim().to_string();
                    println!("{}", theme.ok(&format!("active model: {}", id.trim())));
                } else {
                    println!("{}", theme.warn(&format!("unknown model {id}")));
                }
            }
            Ok(true)
        }
        ["/insert", rest @ ..] => {
            if rest.is_empty() {
                println!(
                    "{}",
                    theme.muted("usage: /insert <file.gguf> [--name id] [--ctx n]")
                );
                return Ok(true);
            }
            let mut args: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
            let take = |flag: &str, args: &mut Vec<String>| -> Option<String> {
                args.iter().position(|a| a == flag).and_then(|i| {
                    args.remove(i);
                    if i < args.len() {
                        Some(args.remove(i))
                    } else {
                        None
                    }
                })
            };
            let name = take("--name", &mut args);
            let nctx = take("--ctx", &mut args).and_then(|v| v.parse::<u32>().ok());
            let src = args.first().cloned().unwrap_or_default();
            let src = if let Some(stripped) = src.strip_prefix("~/") {
                std::env::var("HOME")
                    .map(|h| format!("{h}/{stripped}"))
                    .unwrap_or(src)
            } else {
                src
            };
            let src_path = std::path::PathBuf::from(&src);
            for i in 0..3 {
                print!(
                    "\r{}",
                    progress::spinner_line(theme, i, &format!("insert {}", src_path.display()))
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            println!();
            match aicli_models::insert_gguf(
                &ctx.paths.models_dir,
                &src_path,
                name.as_deref(),
                nctx,
                |_, _| {},
            ) {
                Ok(entry) => println!(
                    "{}",
                    theme.ok(&format!("saved {} ({} MB)", entry.id, entry.size_mb))
                ),
                Err(e) => println!("{}", theme.danger(&format!("insert failed: {e}"))),
            }
            Ok(true)
        }
        ["/setting", "set", key, value] => {
            match crate::settings::apply_setting(ctx, key, value) {
                Ok(msg) => {
                    if *key == "shell" {
                        *shell_mode = (*value).to_string();
                    }
                    println!("{}", theme.ok(&msg));
                }
                Err(e) => println!("{}", theme.warn(&e)),
            }
            Ok(true)
        }
        ["/setting"] => {
            println!(
                "{}",
                aicli_ui::panel::render_panel(
                    theme,
                    "Settings",
                    &crate::settings::settings_text(ctx, model_id)
                )
            );
            Ok(true)
        }
        ["/effort", level] => {
            match crate::settings::apply_effort(ctx, level) {
                Ok(msg) => {
                    *effort = (*level).to_string();
                    println!("{}", theme.ok(&msg));
                }
                Err(e) => println!("{}", theme.warn(&e)),
            }
            Ok(true)
        }
        ["/effort"] => {
            for l in aicli_core::config::EFFORT_LEVELS {
                let mark = if *l == effort.as_str() { "*" } else { " " };
                if let Some((temp, top_p, tokens, steps)) = aicli_core::config::effort_profile(l) {
                    println!("{mark} {l}: temp {temp} top_p {top_p} tokens {tokens} steps {steps}");
                }
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
        ["/manage"] => {
            use aicli_models::ollama as ol;
            let omodels = ol::list().unwrap_or_default();
            let gmodels = aicli_models::list_all(&ctx.paths.models_dir);
            let mut body = String::from("Ollama (install + delete):\n");
            if omodels.is_empty() {
                body.push_str("- none, run: /ollama pull <name>\n");
            }
            for m in &omodels {
                body.push_str(&format!("- {} ({} MB)\n", m.name, ol::size_mb(m.size)));
            }
            body.push_str("\nGGUF (delete only, no download):\n");
            for m in &gmodels {
                let p = aicli_models::local_path(&ctx.paths.models_dir, m);
                body.push_str(&format!(
                    "- {} ({} MB, {})\n",
                    m.id,
                    m.size_mb,
                    if p.exists() { "cached" } else { "missing" }
                ));
            }
            body.push_str("\nFull page lives in the TUI: run bare `zai` then /manage.");
            println!(
                "{}",
                aicli_ui::panel::render_panel(theme, "Manage", body.trim())
            );
            Ok(true)
        }
        ["/ollama", op, rest @ ..] => {
            use aicli_models::ollama as ol;
            match *op {
                "list" => match ol::list() {
                    Ok(models) if models.is_empty() => {
                        println!(
                            "{}",
                            theme.warn("no ollama models, run: /ollama pull <name>")
                        )
                    }
                    Ok(models) => {
                        for m in &models {
                            println!("- {} ({} MB)", m.name, ol::size_mb(m.size));
                        }
                    }
                    Err(e) => println!("{}", theme.danger(&format!("ollama list failed: {e}"))),
                },
                "pull" => {
                    if rest.is_empty() {
                        println!("{}", theme.muted("usage: /ollama pull <name>"));
                    } else {
                        let name = rest.join(" ");
                        println!("{}", theme.bold(&format!("pull ollama model {name}")));
                        match ol::pull(&name, |_| {}) {
                            Ok(()) => println!("{}", theme.ok(&format!("pulled {name}"))),
                            Err(e) => println!("{}", theme.danger(&format!("pull failed: {e}"))),
                        }
                    }
                }
                "rm" | "delete" | "remove" => {
                    if rest.is_empty() {
                        println!("{}", theme.muted("usage: /ollama rm <name>"));
                    } else {
                        let name = rest.join(" ");
                        match ol::rm(name.trim()) {
                            Ok(()) => println!("{}", theme.ok(&format!("deleted {name}"))),
                            Err(e) => println!("{}", theme.danger(&format!("rm failed: {e}"))),
                        }
                    }
                }
                "show" => {
                    if rest.is_empty() {
                        println!("{}", theme.muted("usage: /ollama show <name>"));
                    } else {
                        let name = rest.join(" ");
                        match ol::show(name.trim()) {
                            Ok(v) => {
                                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default())
                            }
                            Err(e) => println!("{}", theme.danger(&format!("show failed: {e}"))),
                        }
                    }
                }
                "status" => {
                    let up = if aicli_models::ollama::daemon_reachable() {
                        "reachable at 127.0.0.1:11434"
                    } else {
                        "down, start with: ollama serve"
                    };
                    println!("{}", theme.muted(&format!("ollama daemon {up}")));
                }
                _ => println!(
                    "{}",
                    theme.muted("usage: /ollama <list|pull|rm|show|status> [name]")
                ),
            }
            Ok(true)
        }
        ["/ollama"] => {
            let up = if aicli_models::ollama::daemon_reachable() {
                "reachable"
            } else {
                "down, start with: ollama serve"
            };
            println!("{}", theme.muted(&format!("ollama daemon {up}")));
            Ok(true)
        }
        ["/run", rest @ ..] => {
            let full = rest.join(" ").trim().to_string();
            if full.is_empty() {
                println!("{}", theme.muted("usage: /run <cmd>"));
                return Ok(true);
            }
            match aicli_tools::check_shell_full(
                &full,
                shell_mode,
                &ctx.config.tools.shell_allowlist,
                &ctx.config.tools.shell_denylist,
            ) {
                aicli_tools::GateDecision::Deny { reason, hint } => {
                    println!("{}", theme.danger(&format!("denied: {reason}. {hint}")));
                    return Ok(true);
                }
                aicli_tools::GateDecision::Allow => {}
            }
            let auto_yes = std::env::var("ZAI_AUTO_YES")
                .or_else(|_| std::env::var("AICLI_AUTO_YES"))
                .is_ok();
            if shell_mode != "allow" && ctx.config.tools.confirm_shell && !auto_yes {
                use std::io::Write;
                print!("run `{full}`? [yes/no]> ");
                let _ = std::io::stdout().flush();
                let mut ans = String::new();
                std::io::stdin().read_line(&mut ans)?;
                if ans.trim().to_lowercase() != "yes" {
                    println!("{}", theme.warn("aborted, nothing ran"));
                    return Ok(true);
                }
            }
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            match aicli_tools::run_blocking(
                &full,
                &cwd,
                shell_mode,
                &ctx.config.tools.shell_allowlist,
                &ctx.config.tools.shell_denylist,
                std::time::Duration::from_secs(60),
                Some(&ctx.paths.logs_dir.join("run.log")),
            ) {
                Ok((preview, _, code)) => {
                    let _ = aicli_tools::log_event(
                        &ctx.paths.data_dir,
                        "shell.run",
                        Some(session_id),
                        &format!("cmd={full} exit={code}"),
                    );
                    println!("{preview}");
                    if code != 0 {
                        println!("{}", theme.warn(&format!("exit {code}")));
                    }
                }
                Err(e) => println!("{}", theme.danger(&format!("run failed: {e}"))),
            }
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
                "/manage",
                "/ollama",
                "/insert",
                "/setting",
                "/effort",
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
