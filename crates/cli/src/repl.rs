use crate::{output, Ctx};
use aicli_ui::{markdown, progress, status};
use serde::Serialize;

const REPL_ART: &[&str] = &[
    "███████╗ █████╗ ██╗",
    "╚══███╔╝██╔══██╗██║",
    "  ███╔╝ ███████║██║",
    " ███╔╝  ██╔══██║██║",
    "███████╗██║  ██║██║",
    "╚══════╝╚═╝  ╚═╝╚═╝",
];

fn print_centered(theme: &aicli_ui::theme::Theme, text: &str) {
    let w = theme.width.min(100);
    for line in text.lines() {
        let len = line.chars().count();
        let pad = if len >= w { 0 } else { (w - len) / 2 };
        println!("{}{}", " ".repeat(pad), theme.banner(line));
    }
}

fn print_welcome_repl(theme: &aicli_ui::theme::Theme) {
    for line in REPL_ART {
        print_centered(theme, line);
    }
    print_centered(theme, "Welcome to Zai");
    println!(
        "{}",
        theme.center_pad(
            "Local-first offline assistant - chat, code, recall",
            theme.width.min(100)
        )
    );
    println!(
        "{}",
        theme.muted(
            "Type a message or / for commands - /new-chat starts fresh - /clear wipes the view"
        )
    );
    println!(
        "{}",
        theme.muted("Shortcuts: Ctrl+A/E line start/end - Ctrl+U clear line - Ctrl+L clear screen - Ctrl+R history - Tab completes /")
    );
}

struct SlashCompleter;

impl rustyline::completion::Completer for SlashCompleter {
    type Candidate = String;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        if !line[..pos.min(line.len())].starts_with('/') {
            return Ok((0, vec![]));
        }
        let prefix: String = line[..pos.min(line.len())]
            .split_whitespace()
            .next()
            .unwrap_or("/")
            .to_string();
        if prefix.contains(' ') {
            return Ok((0, vec![]));
        }
        let out: Vec<String> = crate::slash::SLASHES
            .iter()
            .filter(|m| m.name.starts_with(prefix.as_str()))
            .map(|m| format!("{} ", m.name))
            .collect();
        Ok((0, out))
    }
}

impl rustyline::hint::Hinter for SlashCompleter {
    type Hint = String;
    fn hint(&self, line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        if !line.starts_with('/') || line.contains(' ') {
            return None;
        }
        crate::slash::complete(line)
            .first()
            .map(|m| format!("  [{} - {}]", m.name, m.desc))
    }
}

impl rustyline::highlight::Highlighter for SlashCompleter {}
impl rustyline::validate::Validator for SlashCompleter {}
impl rustyline::Helper for SlashCompleter {}

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
    let seed_val = aicli_infer::sampler::resolve_seed(seed, ctx.config.model.seed);

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
    let mut session_id = sess.id.clone();

    print_welcome_repl(theme);
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
    let last_line = existing
        .last()
        .map(|t| {
            let first: String = t
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(72)
                .collect();
            format!("{}: {first}", t.role)
        })
        .unwrap_or_else(|| "no exchanges yet".to_string());
    println!(
        "{}",
        aicli_ui::panel::render_panel(
            theme,
            &format!("Session {session_id}"),
            &format!(
                "Title: {}\nTurns: {} {rec}\nLast: {last_line}\nModel {model_id} effort {effort} temp {:.1} seed {}\n/new-chat starts fresh - /clear wipes the view - /history lists exchanges - /help lists all",
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
    println!(
        "{}",
        theme.muted("Keys: Up/Down history - Tab completes / - Ctrl+L clear - Ctrl+R search - Ctrl+C stop - Ctrl+D exit")
    );

    let mut rl = rustyline::Editor::<SlashCompleter, rustyline::history::DefaultHistory>::new()?;
    rl.set_helper(Some(SlashCompleter));
    let _ = rl.load_history(ctx.paths.history_file.as_path());

    loop {
        let prompt = format!("{} ", theme.accent("❯ zai>"));
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
                    &mut session_id,
                    &input,
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
                    println!(
                        "{}",
                        theme.muted(&format!(
                            "generating with {model_id} (timeout {}s)...",
                            ctx.config.model.timeout_s.max(10)
                        ))
                    );
                    let parts = crate::answer::AskParts {
                        system: "You are Zai, a local assistant. Answer concisely.",
                        chunks: &[],
                        history: &hist,
                    };
                    crate::answer::compose_answer(
                        ctx,
                        &input,
                        &model_id,
                        &sampler,
                        &usage.prompt,
                        &parts,
                        n_ctx,
                        &crate::answer::NO_CANCEL,
                    )
                };
                let answer = routed.text;
                println!("{}", theme.muted(&routed.backend_note));
                println!(
                    "{} {} {}",
                    theme.user_tag("● YOU"),
                    theme.muted("said:"),
                    theme.muted(&input.chars().take(120).collect::<String>())
                );
                println!(
                    "{} {}",
                    theme.zai_tag("◆ ZAI"),
                    theme.muted(&format!("[{model_id}]"))
                );

                // Single clean render, no fake spinner: generation above
                // already took the real time.
                print!("\r");
                println!("{}", theme.muted("────────────────"));
                println!("{}", markdown::render_markdown(theme, &answer));
                println!("{}", theme.muted("────────────────"));
                let full = answer;

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

async fn handle_slash(
    ctx: &Ctx,
    conn: &rusqlite::Connection,
    session_id: &mut String,
    input: &str,
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
                    "/help /new [title] /new-chat [title] /sessions /open <id> /model [id]\n/manage /ollama <list|pull|rm|show> /insert <file.gguf> [more...] /history [n] /setting [set k v] /effort [level]\n/budget /compact /export [md|json] /sources /plain /clear /quit\nKeys: Up/Down history - Tab completes / - Ctrl+A/E line - Ctrl+U clear line - Ctrl+L clear - Ctrl+R history - Ctrl+C stop - Ctrl+D exit"
                )
            );
            println!(
                "{}",
                theme.muted("Tip: type / then press Tab to see all commands with hints.")
            );
            Ok(true)
        }
        ["/quit"] | ["/exit"] => std::process::exit(0),
        ["/clear"] => {
            print!("\x1B[2J\x1B[1;1H");
            print_welcome_repl(theme);
            println!(
                "{}",
                theme.ok("chat log view cleared, history kept in database")
            );
            Ok(true)
        }
        ["/new", rest @ ..]
        | ["/new-chat", rest @ ..]
        | ["/newchat", rest @ ..]
        | ["/nc", rest @ ..] => {
            let title = rest.join(" ");
            let s = aicli_core::sessions::create_session(conn, &title, model_id)?;
            *session_id = s.id.clone();
            print!("\x1B[2J\x1B[1;1H");
            print_welcome_repl(theme);
            println!("{}", theme.ok(&format!("new chat started: {}", s.id)));
            Ok(true)
        }
        ["/open", rest @ ..] => {
            if rest.is_empty() {
                println!("{}", theme.muted("usage: /open <id>"));
                return Ok(true);
            }
            let id = rest.join(" ");
            match aicli_core::sessions::get_session(conn, id.trim())? {
                Some(s) => {
                    *session_id = s.id.clone();
                    println!("{}", theme.ok(&format!("opened session {}", s.id)));
                }
                None => println!("{}", theme.warn(&format!("unknown session {}", id.trim()))),
            }
            Ok(true)
        }
        ["/sessions"] => {
            let list = aicli_core::sessions::list_sessions(conn, 20)?;
            for s in list {
                println!("{}  {}  {}", s.id, s.title, s.updated_at);
            }
            Ok(true)
        }
        ["/sources"] => {
            println!(
                "{}",
                theme.muted("this chat has no index attached, use: zai ask \"q\" --show-sources")
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
        ["/history", rest @ ..] => {
            let want: usize = rest
                .first()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15)
                .clamp(1, 100);
            let turns = aicli_core::sessions::list_turns(conn, session_id).unwrap_or_default();
            if turns.is_empty() {
                println!("{}", theme.muted("no exchanges yet in this chat"));
                return Ok(true);
            }
            let start = turns.len().saturating_sub(want);
            for t in turns.iter().skip(start) {
                let who = if t.role == "user" { "YOU" } else { "ZAI" };
                let first: String = t
                    .content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(64)
                    .collect();
                let hm = t.created_at.get(11..16).unwrap_or("--:--");
                println!("{hm} {who}: {first}");
            }
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
        ["/insert", rest @ ..] | ["/add", rest @ ..] | ["/import", rest @ ..] => {
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
            let set_default = args.iter().any(|a| a == "--default");
            let recursive = args.iter().any(|a| a == "--recursive");
            args.retain(|a| a != "--default" && a != "--recursive");
            if args.is_empty() {
                let mut body = String::from(
                    "usage: /insert <file.gguf> [more...] [--name id] [--ctx n] [--default] [--recursive]\nPass a folder to import every .gguf inside it. Alias: /add.",
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
                    body.push_str(
                        "\nno .gguf found in ./models ~/models, try: /insert ~/models/tiny.gguf",
                    );
                } else {
                    body.push_str("\nfound nearby:");
                    for p in cands {
                        body.push_str(&format!("\n  {}", p.display()));
                    }
                }
                println!("{}", theme.muted(&body));
                return Ok(true);
            }
            if args.len() > 1 && name.is_some() {
                println!(
                    "{}",
                    theme.warn("--name only works with a single file, drop it for multi insert")
                );
                return Ok(true);
            }
            // Expand folders into file lists.
            let mut files: Vec<std::path::PathBuf> = Vec::new();
            let mut failed: Vec<String> = Vec::new();
            for raw in &args {
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
                    failed.push(format!("{p}: file not found", p = p.display()));
                }
            }
            if files.is_empty() {
                for f in &failed {
                    println!("{}", theme.danger(&format!("insert failed: {f}")));
                }
                return Ok(true);
            }
            let mut inserted: Vec<aicli_models::ModelEntry> = Vec::new();
            for src_path in &files {
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
                    src_path,
                    name.as_deref(),
                    nctx,
                    |_, _| {},
                ) {
                    Ok(entry) => {
                        println!(
                            "{}",
                            theme.ok(&format!(
                                "saved {} ({} MB, ctx {})",
                                entry.id, entry.size_mb, entry.n_ctx
                            ))
                        );
                        inserted.push(entry);
                    }
                    Err(e) => failed.push(format!("{}: {e}", src_path.display())),
                }
            }
            for f in &failed {
                println!("{}", theme.danger(&format!("skipped: {f}")));
            }
            if inserted.is_empty() {
                println!("{}", theme.danger("nothing inserted"));
                return Ok(true);
            }
            let cached = aicli_models::list_all(&ctx.paths.models_dir)
                .iter()
                .filter(|m| aicli_models::local_path(&ctx.paths.models_dir, m).exists())
                .count();
            if set_default || cached <= 1 {
                if let Some(last) = inserted.last() {
                    let mut cfg = ctx.config.clone();
                    cfg.model.default = last.id.clone();
                    if cfg.save(&ctx.paths.config_file).is_ok() {
                        *model_id = last.id.clone();
                        println!("{}", theme.ok(&format!("now default: {}", last.id)));
                    }
                }
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
                "/add",
                "/history",
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
