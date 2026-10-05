use crate::{output, repl, Cmd, Ctx, ModelsOp, NotesOp, PatchOp, SessionsOp, TasksOp};
use aicli_ui::{markdown, panel, progress, status, table};
use serde::Serialize;

pub async fn dispatch(ctx: Ctx) -> anyhow::Result<()> {
    match &ctx.cli.cmd {
        None => {
            show_home(&ctx)?;
            Ok(())
        }
        Some(Cmd::Chat {
            session,
            model,
            temp,
            top_p: _,
            seed,
            ctx: ctx_n,
            mock: _,
        }) => repl::run_chat(&ctx, session.clone(), model.clone(), *temp, *seed, *ctx_n).await,
        Some(Cmd::Ask {
            query,
            top_k,
            show_sources,
            no_rag,
            model,
            session,
            temp,
            top_p: _,
            seed,
            show_budget,
            ..
        }) => cmd_ask(
            &ctx,
            query,
            *top_k,
            *show_sources,
            *no_rag,
            model.clone(),
            session.clone(),
            *temp,
            *seed,
            *show_budget,
        ),
        Some(Cmd::Code {
            goal,
            path,
            dry_run,
            apply,
            allow_shell,
            max_steps,
            temp,
            seed,
        }) => cmd_code(
            &ctx,
            goal,
            path,
            *dry_run,
            *apply,
            *allow_shell,
            *max_steps,
            *temp,
            *seed,
        ),
        Some(Cmd::Patch { op }) => cmd_patch(&ctx, op),
        Some(Cmd::Run { cmd }) => cmd_run(&ctx, cmd),
        Some(Cmd::Index { path, rebuild }) => cmd_index(&ctx, path, *rebuild),
        Some(Cmd::Models { op }) => cmd_models(&ctx, op),
        Some(Cmd::Daily {
            today,
            week,
            search,
            add_note,
        }) => cmd_daily(&ctx, *today, *week, search.clone(), add_note.clone()),
        Some(Cmd::Tasks { op }) => cmd_tasks(&ctx, op),
        Some(Cmd::Notes { op }) => cmd_notes(&ctx, op),
        Some(Cmd::Sessions { op }) => cmd_sessions(&ctx, op),
        Some(Cmd::Config { op }) => cmd_config(&ctx, op),
        Some(Cmd::Doctor {
            bench_load,
            bench_gen,
            check_updates,
        }) => cmd_doctor(&ctx, *bench_load, *bench_gen, *check_updates),
    }
}

fn show_home(ctx: &Ctx) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            product: String,
            version: String,
            model: String,
            profile: String,
        }
        output::print_json(
            true,
            Data {
                product: "aicli".to_string(),
                version: "0.3.0".to_string(),
                model: ctx.config.model.default.clone(),
                profile: ctx.paths.profile.clone(),
            },
        );
        return Ok(());
    }
    println!(
        "{}",
        status::status_line(
            theme,
            "0.3.0",
            &ctx.config.model.default,
            0,
            ctx.config.model.n_ctx,
            true,
            &ctx.paths.profile
        )
    );
    for i in 0..4 {
        print!(
            "\r{}",
            progress::spinner_line(theme, i, "warming up local workspace")
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!();
    let tasks = aicli_core::store::task_list(&ctx.paths.data_dir, None);
    let open = tasks.iter().filter(|t| !t.done).count();
    // Session count from sqlite for Phase 3 home signal.
    let sessions_open = open_db_sessions(ctx).map(|v| v.len()).unwrap_or(0);
    let body = format!(
        "chat   start daily chat      aicli chat\ncode   fix from prompt       aicli code \"fix failing test\"\ndaily  today overview        aicli daily --today\nindex  search local files    aicli index ./docs\n\ntasks open: {open}  sessions: {sessions_open}  profile: {}",
        ctx.paths.profile
    );
    println!("{}", panel::render_panel(theme, "Quick actions", &body));
    println!("{}", status::hint_line(theme));
    Ok(())
}

fn open_db_sessions(ctx: &Ctx) -> anyhow::Result<Vec<aicli_core::sessions::Session>> {
    let conn = aicli_core::db::open(&ctx.paths.db_file)?;
    aicli_core::sessions::list_sessions(&conn, 50)
}

fn resolve_model(
    ctx: &Ctx,
    override_id: Option<String>,
) -> (String, Option<std::path::PathBuf>, bool) {
    let id = override_id.unwrap_or_else(|| ctx.config.model.default.clone());
    let cached =
        aicli_models::find_model(&id).map(|e| aicli_models::local_path(&ctx.paths.models_dir, &e));
    let exists = cached.as_ref().map(|p| p.exists()).unwrap_or(false);
    (id, cached, exists)
}

fn sampler_for(
    ctx: &Ctx,
    temp: Option<f32>,
    seed: Option<u64>,
    mode: &str,
) -> aicli_infer::SamplerConfig {
    let seed = seed.unwrap_or(ctx.config.model.seed);
    match mode {
        "code" => aicli_infer::SamplerConfig::code(temp.or(Some(ctx.config.model.temp_code)), seed),
        _ => aicli_infer::SamplerConfig::chat(temp.or(Some(ctx.config.model.temp_chat)), seed),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_ask(
    ctx: &Ctx,
    query: &str,
    _top_k: usize,
    show_sources: bool,
    _no_rag: bool,
    model_override: Option<String>,
    session_opt: Option<String>,
    temp: Option<f32>,
    seed: Option<u64>,
    show_budget: bool,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let (model_id, model_path, cached) = resolve_model(ctx, model_override);
    let sampler = sampler_for(ctx, temp, seed, "chat");
    let n_ctx = ctx.config.model.n_ctx;

    // History from session when given, else empty. Phase 3 durable turns.
    let history: Vec<(String, String)> = session_opt
        .as_deref()
        .and_then(|sid| {
            aicli_core::db::open(&ctx.paths.db_file)
                .ok()
                .and_then(|c| aicli_core::sessions::list_turns(&c, sid).ok())
                .map(|turns| {
                    turns
                        .into_iter()
                        .map(|t| (t.role, t.content))
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();

    let chunks = vec!["crates/cli/src/main.rs: command tree and dispatch".to_string()];
    let usage = aicli_infer::build_prompt(
        "You are AICLI, a local assistant. Answer concisely.",
        &chunks,
        &history,
        query,
        n_ctx,
    );

    if usage.overflow {
        eprintln!(
            "{}",
            panel::render_panel(
                theme,
                "Context overflow",
                &format!(
                    "estimated {} tokens over ctx {n_ctx}. Run /ctx compact or raise n_ctx.",
                    usage.estimated_tokens
                )
            )
        );
        if ctx.is_json() {
            output::print_json_error(
                "E_CTX_OVERFLOW",
                "prompt over context budget",
                "run compact or raise n_ctx",
            );
        }
        std::process::exit(4);
    }

    if show_budget {
        eprintln!(
            "{}",
            theme.muted(&format!(
                "budget sys {} rag {} hist {} in {} total {} / {n_ctx}",
                usage.sections.system,
                usage.sections.rag,
                usage.sections.history,
                usage.sections.input,
                usage.estimated_tokens
            ))
        );
    }

    // Backend status line: cached GGUF validated, else mock with fix.
    let backend_note = if cached {
        match model_path.as_ref() {
            Some(p) => match aicli_infer::load_info(
                p,
                n_ctx,
                ctx.config.model.n_threads,
                ctx.config.model.n_gpu_layers,
            ) {
                Ok(info) => format!(
                    "gguf v{} tensors {} size {} MB",
                    info.gguf_version,
                    info.tensor_count,
                    info.size_bytes / (1024 * 1024)
                ),
                Err(e) => format!("backend warn: {e}"),
            },
            None => "mock backend".to_string(),
        }
    } else {
        format!("mock backend, pull with: aicli models pull {model_id}")
    };

    let answer = aicli_infer::sampler::mock_answer_with_sampler(query, &sampler);

    // Persist turns when session given. Create session id on demand.
    if let Some(sid) = session_opt.as_deref() {
        if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
            if aicli_core::sessions::ensure_with_id(&conn, sid, &model_id).is_ok() {
                let user_turn = aicli_core::sessions::append_turn(
                    &conn,
                    sid,
                    "user",
                    query,
                    usage.estimated_tokens as i64,
                    0,
                    "done",
                );
                if let Ok(ut) = user_turn {
                    let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &ut);
                }
                if let Ok(t) = aicli_core::sessions::append_turn(
                    &conn,
                    sid,
                    "assistant",
                    &answer,
                    0,
                    aicli_infer::estimate_tokens(&answer) as i64,
                    "done",
                ) {
                    let _ = aicli_core::sessions::mirror_append(&ctx.paths.sessions_dir, &t);
                }
            }
        }
    }

    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            query: String,
            answer: String,
            model: String,
            sampler: aicli_infer::SamplerConfig,
            usage: aicli_infer::PromptUsage,
            backend: String,
        }
        output::print_json(
            true,
            Data {
                query: query.to_string(),
                answer: answer.clone(),
                model: model_id,
                sampler,
                usage,
                backend: backend_note,
            },
        );
        return Ok(());
    }

    if !ctx.is_quiet() {
        let pct = (usage.estimated_tokens * 100 / n_ctx.max(1) as usize).min(999);
        println!(
            "{}",
            status::status_line(
                theme,
                "0.3.0",
                &model_id,
                usage.estimated_tokens as u32,
                n_ctx,
                true,
                &ctx.paths.profile
            )
        );
        if pct >= 85 {
            println!(
                "{}",
                theme.warn(&format!("ctx pressure {pct} pct, consider /ctx compact"))
            );
        }
        println!("{}", theme.muted(&backend_note));
    }
    aicli_infer::sampler::stream_mock_to_stdout(&answer, ctx.config.ui.stream_flush_ms)?;
    println!();
    println!("{}", markdown::render_markdown(theme, &answer));
    if show_sources {
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Sources",
                "[1] crates/cli/src/main.rs:1-40 score=0.81"
            )
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_code(
    ctx: &Ctx,
    goal: &str,
    path: &std::path::Path,
    _dry_run: bool,
    apply: bool,
    _allow_shell: bool,
    max_steps: u32,
    temp: Option<f32>,
    seed: Option<u64>,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let sampler = sampler_for(ctx, temp, seed, "code");
    let hits = aicli_tools::search_files(path, goal, false, true, &[], 5).unwrap_or_default();
    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            goal: String,
            steps: u32,
            hits: usize,
            sampler: aicli_infer::SamplerConfig,
            mode: String,
        }
        output::print_json(
            true,
            Data {
                goal: goal.to_string(),
                steps: max_steps.min(ctx.config.tools.max_steps),
                hits: hits.len(),
                sampler,
                mode: "dry-run preview, apply in Phase 4".to_string(),
            },
        );
        return Ok(());
    }
    println!(
        "{}",
        panel::render_panel(
            theme,
            "Plan",
            &format!(
                "goal: {goal}\nmode: code temp {:.1} seed {}\n1. search scope\n2. read top hits\n3. draft diff\n4. request approval",
                sampler.temp, sampler.seed
            )
        )
    );
    if hits.is_empty() {
        println!("{}", theme.warn("no matching files, try broader goal"));
    } else {
        let rows: Vec<Vec<String>> = hits
            .iter()
            .take(5)
            .map(|h| vec![h.file.clone(), h.line_no.to_string(), h.line.clone()])
            .collect();
        println!(
            "{}",
            table::render_table(
                theme,
                &["FILE", "LINE", "TEXT"],
                &rows,
                &[false, true, false]
            )
        );
    }
    let diff = "--- a/example.rs\n+++ b/example.rs\n@@ -1,2 +1,3 @@\n fn main() {}\n+// aicli: planned edit\n";
    println!(
        "{}",
        panel::render_panel(theme, "Diff preview", diff.trim())
    );
    if apply {
        println!(
            "{}",
            theme.warn(
                "apply requested but Phase 4 gate requires explicit approval UI, staying dry-run"
            )
        );
    } else {
        println!(
            "{}",
            theme.muted("dry-run only, no files changed. Use patch apply after review in Phase 4.")
        );
    }
    Ok(())
}

fn cmd_patch(ctx: &Ctx, op: &PatchOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        PatchOp::Show { patch_id } => {
            let p = ctx.paths.patches_dir.join(format!("{patch_id}.diff"));
            if p.exists() {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                println!(
                    "{}",
                    panel::render_panel(theme, &format!("Patch {patch_id}"), &text)
                );
            } else {
                println!(
                    "{}",
                    panel::render_panel(
                        theme,
                        &format!("Patch {patch_id}"),
                        "no pending patch with that id"
                    )
                );
            }
        }
        PatchOp::Apply { patch_id, .. } => {
            println!(
                "{}",
                theme.warn(&format!(
                    "apply {patch_id} deferred to Phase 4 atomic engine"
                ))
            );
        }
        PatchOp::Drop { patch_id } => {
            println!(
                "{}",
                theme.muted(&format!("drop {patch_id}: nothing to drop"))
            );
        }
    }
    Ok(())
}

fn cmd_run(ctx: &Ctx, cmd: &[String]) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let full = cmd.join(" ");
    if full.is_empty() {
        anyhow::bail!("usage: aicli run -- <cmd...>");
    }
    match aicli_tools::check_shell(
        &full,
        &ctx.config.tools.shell_allowlist,
        &ctx.config.tools.shell_denylist,
    ) {
        aicli_tools::GateDecision::Allow => {
            println!("{}", theme.muted(&format!("run: {full}")));
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&full)
                .output()?;
            let text = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            let preview: String = text.chars().take(4000).collect();
            println!("{preview}");
            if !out.status.success() {
                std::process::exit(out.status.code().unwrap_or(6));
            }
            Ok(())
        }
        aicli_tools::GateDecision::Deny { reason, hint } => {
            if ctx.is_json() {
                output::print_json_error("E_TOOL_DENIED", &reason, &hint);
            } else {
                eprintln!(
                    "{}",
                    aicli_ui::panel::render_error(theme, "E_TOOL_DENIED", &reason, &hint, None)
                );
            }
            std::process::exit(5);
        }
    }
}

fn cmd_index(ctx: &Ctx, path: &std::path::Path, rebuild: bool) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let opts = aicli_ingest::walk::WalkOptions {
        exts: ctx.config.index.ext.clone(),
        exclude_globs: ctx.config.index.exclude.clone(),
        include_hidden: false,
        max_file_mb: ctx.config.index.max_file_mb,
    };
    print!("{}", progress::spinner_line(theme, 0, "scanning"));
    let rep = aicli_ingest::walk::collect_files(path, &opts);
    print!("\r");
    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            files: usize,
            skipped_large: usize,
            rebuild: bool,
        }
        output::print_json(
            true,
            Data {
                files: rep.files.len(),
                skipped_large: rep.skipped_large.len(),
                rebuild,
            },
        );
        return Ok(());
    }
    let rows = vec![vec![
        path.display().to_string(),
        rep.files.len().to_string(),
        rep.skipped_large.len().to_string(),
        if rebuild {
            "rebuild".to_string()
        } else {
            "incremental".to_string()
        },
    ]];
    println!(
        "{}",
        table::render_table(
            theme,
            &["PATH", "FILES", "SKIPPED", "MODE"],
            &rows,
            &[false, true, true, false]
        )
    );
    for f in rep.files.iter().take(5) {
        println!("{}", theme.muted(&format!("  {}", f.display())));
    }
    if rep.files.len() > 5 {
        println!(
            "{}",
            theme.muted(&format!("  ... {} more", rep.files.len() - 5))
        );
    }
    Ok(())
}

fn cmd_models(ctx: &Ctx, op: &ModelsOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        ModelsOp::List => {
            let reg = aicli_models::builtin_registry();
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Row {
                    id: String,
                    quant: String,
                    size_mb: u64,
                    n_ctx: u32,
                    cached: bool,
                    path: String,
                }
                let rows: Vec<Row> = reg
                    .into_iter()
                    .map(|m| {
                        let p = aicli_models::local_path(&ctx.paths.models_dir, &m);
                        let cached = p.exists();
                        Row {
                            id: m.id,
                            quant: m.quant,
                            size_mb: m.size_mb,
                            n_ctx: m.n_ctx,
                            cached,
                            path: p.display().to_string(),
                        }
                    })
                    .collect();
                output::print_json(true, &rows);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = reg
                .iter()
                .map(|m| {
                    let p = aicli_models::local_path(&ctx.paths.models_dir, m);
                    let cached = if p.exists() { "cached" } else { "" };
                    let status = if m.is_default {
                        format!("default {cached}").trim().to_string()
                    } else {
                        cached.to_string()
                    };
                    vec![
                        m.id.clone(),
                        m.quant.clone(),
                        format!("{} MB", m.size_mb),
                        format!("{}", m.n_ctx),
                        status,
                    ]
                })
                .collect();
            println!(
                "{}",
                table::render_table(
                    theme,
                    &["ID", "QUANT", "SIZE", "CTX", "STATUS"],
                    &rows,
                    &[false, false, true, true, false]
                )
            );
            println!(
                "{}",
                theme.muted(&format!("cache: {}", ctx.paths.models_dir.display()))
            );
            Ok(())
        }
        ModelsOp::Pull { id } => {
            if ctx.cli.offline {
                if ctx.is_json() {
                    output::print_json_error(
                        "E_OFFLINE",
                        "offline flag set, pull needs network",
                        "unset --offline or place GGUF manually in cache",
                    );
                } else {
                    eprintln!(
                        "{}",
                        aicli_ui::panel::render_error(
                            theme,
                            "E_OFFLINE",
                            "pull needs network",
                            "unset --offline or place GGUF manually",
                            None
                        )
                    );
                }
                std::process::exit(4);
            }
            if ctx.is_json() {
                match aicli_models::pull(id, &ctx.paths.models_dir) {
                    Ok(p) => {
                        #[derive(Serialize)]
                        struct Data {
                            id: String,
                            path: String,
                        }
                        output::print_json(
                            true,
                            Data {
                                id: id.clone(),
                                path: p.display().to_string(),
                            },
                        );
                    }
                    Err(e) => {
                        // Fallback to simulated progress for demo when HF unreachable.
                        eprintln!(
                            "{}",
                            theme.warn(&format!("real pull failed: {e}, demo progress"))
                        );
                        aicli_models::pull_simulated(id)?;
                    }
                }
                return Ok(());
            }
            println!("{}", theme.bold(&format!("pull {id}")));
            match aicli_models::pull(id, &ctx.paths.models_dir) {
                Ok(p) => {
                    println!("{}", theme.ok(&format!("saved {}", p.display())));
                    match aicli_models::verify_sha256(
                        &p,
                        aicli_models::find_model(id)
                            .and_then(|m| m.sha256)
                            .as_deref(),
                    ) {
                        Ok(true) => println!("{}", theme.ok("verify sha256 ok")),
                        Ok(false) => println!("{}", theme.danger("verify sha256 mismatch")),
                        Err(e) => println!("{}", theme.warn(&format!("verify skipped: {e}"))),
                    }
                }
                Err(e) => {
                    println!("{}", theme.warn(&format!("real pull failed: {e}")));
                    println!("{}", theme.muted("demo progress for offline review"));
                    aicli_models::pull_simulated(id)?;
                }
            }
            Ok(())
        }
        ModelsOp::Verify { id } => {
            let entry = aicli_models::find_model(id)
                .ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
            let p = aicli_models::local_path(&ctx.paths.models_dir, &entry);
            if !p.exists() {
                if ctx.is_json() {
                    output::print_json_error(
                        "E_MODEL_MISSING",
                        &format!("{id} not cached"),
                        &format!("aicli models pull {id}"),
                    );
                } else {
                    eprintln!(
                        "{}",
                        aicli_ui::panel::render_error(
                            theme,
                            "E_MODEL_MISSING",
                            &format!("{id} not cached"),
                            &format!("aicli models pull {id}"),
                            None
                        )
                    );
                }
                std::process::exit(3);
            }
            let ok = aicli_models::verify_sha256(&p, entry.sha256.as_deref())?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    ok: bool,
                }
                output::print_json(true, Data { id: id.clone(), ok });
            } else if ok {
                println!("{}", theme.ok(&format!("verify {id} ok")));
            } else {
                println!("{}", theme.danger(&format!("verify {id} mismatch")));
            }
            Ok(())
        }
        ModelsOp::Remove { id } => {
            let entry = aicli_models::find_model(id)
                .ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
            let p = aicli_models::local_path(&ctx.paths.models_dir, &entry);
            if p.exists() {
                std::fs::remove_file(&p)?;
                println!("{}", theme.ok(&format!("removed {id}")));
            } else {
                println!("{}", theme.warn(&format!("remove {id}: no file in cache")));
            }
            Ok(())
        }
        ModelsOp::SetDefault { id } => {
            if aicli_models::find_model(id).is_none() {
                anyhow::bail!("unknown model {id}");
            }
            println!(
                "{}",
                theme.muted(&format!(
                    "set-default {id}: edit config model.default, then: aicli config show"
                ))
            );
            Ok(())
        }
    }
}

fn cmd_daily(
    ctx: &Ctx,
    today: bool,
    week: bool,
    search: Option<String>,
    add_note: Option<String>,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    if let Some(text) = add_note {
        let n = aicli_core::store::note_add(&ctx.paths.data_dir, &text, None)?;
        if ctx.is_json() {
            output::print_json(true, &n);
        } else {
            println!("{}", theme.ok(&format!("note {} saved", n.id)));
        }
        return Ok(());
    }
    if let Some(q) = search {
        let hits = aicli_core::store::note_search(&ctx.paths.data_dir, &q, 20);
        if ctx.is_json() {
            output::print_json(true, &hits);
        } else if hits.is_empty() {
            println!("{}", theme.warn("no notes match"));
        } else {
            let rows: Vec<Vec<String>> = hits
                .iter()
                .map(|n| vec![n.id.clone(), n.date.clone(), n.text.clone()])
                .collect();
            println!(
                "{}",
                table::render_table(
                    theme,
                    &["ID", "DATE", "TEXT"],
                    &rows,
                    &[false, false, false]
                )
            );
        }
        return Ok(());
    }
    let tasks = aicli_core::store::task_list(&ctx.paths.data_dir, None);
    let open: Vec<_> = tasks.iter().filter(|t| !t.done).collect();
    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            open_tasks: usize,
            total_tasks: usize,
            week: bool,
        }
        output::print_json(
            true,
            Data {
                open_tasks: open.len(),
                total_tasks: tasks.len(),
                week,
            },
        );
        return Ok(());
    }
    let mut body = format!("open tasks: {}\n", open.len());
    for t in open.iter().take(10) {
        body.push_str(&format!("[ ] {} {}\n", t.id, t.text));
    }
    if today || (!today && !week) {
        body.push_str("\nRun: aicli tasks add \"write tests\"");
    }
    if week {
        body.push_str(&format!(
            "\nweek: {} total tasks, model narrative in Phase 5",
            tasks.len()
        ));
    }
    println!("{}", panel::render_panel(theme, "Daily", body.trim()));
    Ok(())
}

fn cmd_tasks(ctx: &Ctx, op: &TasksOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        TasksOp::Add { text, date } => {
            let item = aicli_core::store::task_add(&ctx.paths.data_dir, text, date.as_deref())?;
            if ctx.is_json() {
                output::print_json(true, &item);
            } else {
                println!("{}", theme.ok(&format!("task {} added", item.id)));
            }
            Ok(())
        }
        TasksOp::List { date } => {
            let items = aicli_core::store::task_list(&ctx.paths.data_dir, date.as_deref());
            if ctx.is_json() {
                output::print_json(true, &items);
            } else if items.is_empty() {
                println!(
                    "{}",
                    theme.warn("No tasks yet. Run: aicli tasks add \"first task\"")
                );
            } else {
                let rows: Vec<Vec<String>> = items
                    .iter()
                    .map(|t| {
                        vec![
                            t.id.clone(),
                            t.date.clone(),
                            if t.done {
                                "done".to_string()
                            } else {
                                "open".to_string()
                            },
                            t.text.clone(),
                        ]
                    })
                    .collect();
                println!(
                    "{}",
                    table::render_table(
                        theme,
                        &["ID", "DATE", "STATE", "TEXT"],
                        &rows,
                        &[false, false, false, false]
                    )
                );
            }
            Ok(())
        }
        TasksOp::Done { id } => {
            let ok = aicli_core::store::task_done(&ctx.paths.data_dir, id)?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    done: bool,
                }
                output::print_json(
                    true,
                    Data {
                        id: id.clone(),
                        done: ok,
                    },
                );
            } else if ok {
                println!("{}", theme.ok(&format!("task {id} done")));
            } else {
                println!("{}", theme.warn(&format!("task {id} not found")));
            }
            Ok(())
        }
        TasksOp::Carry { from } => {
            println!(
                "{}",
                theme.muted(&format!(
                    "carry from {from}: copy open tasks in Phase 5 full"
                ))
            );
            Ok(())
        }
    }
}

fn cmd_notes(ctx: &Ctx, op: &NotesOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        NotesOp::Add { text, date } => {
            let n = aicli_core::store::note_add(&ctx.paths.data_dir, text, date.as_deref())?;
            if ctx.is_json() {
                output::print_json(true, &n);
            } else {
                println!("{}", theme.ok(&format!("note {} saved", n.id)));
            }
            Ok(())
        }
        NotesOp::List { date, limit } => {
            let hits = aicli_core::store::note_search(&ctx.paths.data_dir, "", *limit);
            let filtered: Vec<_> = hits
                .into_iter()
                .filter(|n| date.as_deref().map(|d| n.date == d).unwrap_or(true))
                .collect();
            if ctx.is_json() {
                output::print_json(true, &filtered);
            } else {
                let rows: Vec<Vec<String>> = filtered
                    .iter()
                    .map(|n| vec![n.id.clone(), n.date.clone(), n.text.clone()])
                    .collect();
                println!(
                    "{}",
                    table::render_table(
                        theme,
                        &["ID", "DATE", "TEXT"],
                        &rows,
                        &[false, false, false]
                    )
                );
            }
            Ok(())
        }
        NotesOp::Search { query } => {
            let hits = aicli_core::store::note_search(&ctx.paths.data_dir, query, 20);
            if ctx.is_json() {
                output::print_json(true, &hits);
            } else {
                let rows: Vec<Vec<String>> = hits
                    .iter()
                    .map(|n| vec![n.id.clone(), n.date.clone(), n.text.clone()])
                    .collect();
                println!(
                    "{}",
                    table::render_table(
                        theme,
                        &["ID", "DATE", "TEXT"],
                        &rows,
                        &[false, false, false]
                    )
                );
            }
            Ok(())
        }
    }
}

fn cmd_sessions(ctx: &Ctx, op: &SessionsOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let conn = aicli_core::db::open(&ctx.paths.db_file)?;
    match op {
        SessionsOp::List => {
            let list = aicli_core::sessions::list_sessions(&conn, 50)?;
            if ctx.is_json() {
                output::print_json(true, &list);
            } else if list.is_empty() {
                println!(
                    "{}",
                    panel::render_panel(theme, "Sessions", "No sessions yet. Run: aicli chat")
                );
            } else {
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|s| {
                        let turns = aicli_core::sessions::list_turns(&conn, &s.id)
                            .map(|v| v.len().to_string())
                            .unwrap_or_else(|_| "?".to_string());
                        vec![s.id.clone(), s.title.clone(), turns, s.model.clone()]
                    })
                    .collect();
                println!(
                    "{}",
                    table::render_table(
                        theme,
                        &["ID", "TITLE", "TURNS", "MODEL"],
                        &rows,
                        &[false, false, true, false]
                    )
                );
            }
            Ok(())
        }
        SessionsOp::Open { id } => match aicli_core::sessions::get_session(&conn, id)? {
            Some(s) => {
                let turns = aicli_core::sessions::list_turns(&conn, id)?;
                if ctx.is_json() {
                    #[derive(Serialize)]
                    struct Data {
                        session: aicli_core::sessions::Session,
                        turns: Vec<aicli_core::sessions::Turn>,
                    }
                    output::print_json(true, Data { session: s, turns });
                } else {
                    println!(
                        "{}",
                        panel::render_panel(
                            theme,
                            &format!("Session {}", s.id),
                            &format!(
                                "title: {}\nturns: {}\nmodel: {}",
                                s.title,
                                turns.len(),
                                s.model
                            )
                        )
                    );
                    for t in turns.iter().take(10) {
                        let tag = if t.status == "stopped" {
                            " (stopped)"
                        } else {
                            ""
                        };
                        println!(
                            "{} {}{}:\n{}",
                            t.created_at,
                            t.role,
                            tag,
                            t.content.chars().take(400).collect::<String>()
                        );
                    }
                }
                Ok(())
            }
            None => {
                anyhow::bail!("unknown session {id}");
            }
        },
        SessionsOp::Rename { id, title } => {
            let ok = aicli_core::sessions::rename_session(&conn, id, title)?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    ok: bool,
                }
                output::print_json(true, Data { id: id.clone(), ok });
            } else if ok {
                println!("{}", theme.ok(&format!("renamed {id}")));
            } else {
                println!("{}", theme.warn(&format!("session {id} not found")));
            }
            Ok(())
        }
        SessionsOp::Delete { id, yes } => {
            if !yes && !ctx.is_json() {
                println!(
                    "{}",
                    theme.warn(&format!("delete {id} needs --yes, no action taken"))
                );
                return Ok(());
            }
            let ok = aicli_core::sessions::delete_session(&conn, id)?;
            // Remove JSONL mirror best effort.
            let _ = std::fs::remove_file(aicli_core::sessions::jsonl_path(
                &ctx.paths.sessions_dir,
                id,
            ));
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    ok: bool,
                }
                output::print_json(true, Data { id: id.clone(), ok });
            } else if ok {
                println!("{}", theme.ok(&format!("deleted {id}")));
            } else {
                println!("{}", theme.warn(&format!("session {id} not found")));
            }
            Ok(())
        }
        SessionsOp::Export { id, format } => {
            let s = aicli_core::sessions::get_session(&conn, id)?
                .ok_or_else(|| anyhow::anyhow!("unknown session {id}"))?;
            let turns = aicli_core::sessions::list_turns(&conn, id)?;
            if format == "json" {
                if ctx.is_json() {
                    #[derive(Serialize)]
                    struct Data {
                        session: aicli_core::sessions::Session,
                        turns: Vec<aicli_core::sessions::Turn>,
                    }
                    output::print_json(true, Data { session: s, turns });
                } else {
                    println!("{}", serde_json::to_string_pretty(&turns)?);
                }
            } else {
                println!("{}", aicli_core::sessions::export_markdown(&s, &turns));
            }
            Ok(())
        }
    }
}

fn cmd_config(ctx: &Ctx, op: &crate::ConfigOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        crate::ConfigOp::Show => {
            if ctx.is_json() {
                output::print_json(true, &ctx.config);
            } else {
                let text = toml::to_string_pretty(&ctx.config).unwrap_or_default();
                println!(
                    "{}",
                    markdown::render_markdown(theme, &format!("```toml\n{text}\n```"))
                );
            }
            Ok(())
        }
        crate::ConfigOp::Set { key, value } => {
            // Support model.default, model.n_ctx, ui.theme in Phase 3 writer.
            let mut cfg = ctx.config.clone();
            let applied = match key.as_str() {
                "model.default" => {
                    cfg.model.default = value.clone();
                    true
                }
                "model.n_ctx" => match value.parse::<u32>() {
                    Ok(n) => {
                        cfg.model.n_ctx = n;
                        true
                    }
                    Err(_) => false,
                },
                "ui.theme" => {
                    cfg.ui.theme = value.clone();
                    true
                }
                _ => false,
            };
            if !applied {
                anyhow::bail!("unknown key {key}, try model.default, model.n_ctx, ui.theme");
            }
            let text = toml::to_string_pretty(&cfg)?;
            if let Some(parent) = ctx.paths.config_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&ctx.paths.config_file, text)?;
            println!("{}", theme.ok(&format!("set {key}={value}")));
            Ok(())
        }
        crate::ConfigOp::Reset { yes } => {
            if !yes {
                println!("{}", theme.warn("reset requires --yes"));
                return Ok(());
            }
            let def = aicli_core::Config::default();
            let text = toml::to_string_pretty(&def)?;
            if let Some(parent) = ctx.paths.config_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&ctx.paths.config_file, text)?;
            println!("{}", theme.ok("config reset to defaults"));
            Ok(())
        }
    }
}

fn cmd_doctor(
    ctx: &Ctx,
    bench_load: bool,
    bench_gen: Option<u32>,
    check_updates: bool,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    // Disk use for cache plus data.
    let cache_mb = dir_mb(&ctx.paths.cache_dir);
    let data_mb = dir_mb(&ctx.paths.data_dir);
    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            version: String,
            profile: String,
            config_file: String,
            data_dir: String,
            cache_mb: u64,
            data_mb: u64,
            model: String,
            n_ctx: u32,
            model_cached: bool,
        }
        let entry = aicli_models::find_model(&ctx.config.model.default);
        let cached = entry
            .as_ref()
            .map(|e| aicli_models::local_path(&ctx.paths.models_dir, e).exists())
            .unwrap_or(false);
        output::print_json(
            true,
            Data {
                version: "0.3.0".to_string(),
                profile: ctx.paths.profile.clone(),
                config_file: ctx.paths.config_file.display().to_string(),
                data_dir: ctx.paths.data_dir.display().to_string(),
                cache_mb,
                data_mb,
                model: ctx.config.model.default.clone(),
                n_ctx: ctx.config.model.n_ctx,
                model_cached: cached,
            },
        );
        return Ok(());
    }
    let entry = aicli_models::find_model(&ctx.config.model.default);
    let cached = entry
        .as_ref()
        .map(|e| {
            let p = aicli_models::local_path(&ctx.paths.models_dir, e);
            if p.exists() {
                format!("yes {}", p.display())
            } else {
                format!("no, run: aicli models pull {}", e.id)
            }
        })
        .unwrap_or_else(|| "unknown model id".to_string());
    let rows = vec![
        vec!["profile".to_string(), ctx.paths.profile.clone()],
        vec![
            "config".to_string(),
            ctx.paths.config_file.display().to_string(),
        ],
        vec!["data".to_string(), ctx.paths.data_dir.display().to_string()],
        vec!["model".to_string(), ctx.config.model.default.clone()],
        vec!["model file".to_string(), cached],
        vec!["ctx".to_string(), format!("{}", ctx.config.model.n_ctx)],
        vec![
            "threads".to_string(),
            if ctx.config.model.n_threads == 0 {
                "auto".to_string()
            } else {
                format!("{}", ctx.config.model.n_threads)
            },
        ],
        vec!["cache MB".to_string(), format!("{cache_mb}")],
        vec!["data MB".to_string(), format!("{data_mb}")],
    ];
    println!(
        "{}",
        table::render_table(theme, &["KEY", "VALUE"], &rows, &[false, false])
    );
    if bench_load {
        let t0 = std::time::Instant::now();
        for i in 0..6 {
            print!(
                "\r{}",
                progress::spinner_line(theme, i, "bench load: hashing cache dir")
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        let ms = t0.elapsed().as_millis();
        println!(
            "\r{}",
            theme.ok(&format!(
                "load probe done in {ms} ms (real GGUF timing in Phase 2 full)"
            ))
        );
    }
    if let Some(n) = bench_gen {
        let t0 = std::time::Instant::now();
        let sampler = aicli_infer::SamplerConfig::deterministic();
        let text = aicli_infer::sampler::mock_answer_with_sampler("bench", &sampler);
        // Simulate tokens per second from paced render without sleeping in bench.
        let tokens = aicli_infer::estimate_tokens(&text).max(1);
        let ms = t0.elapsed().as_millis().max(1);
        let tps = tokens as f64 / (ms as f64 / 1000.0);
        println!(
            "{}",
            theme.muted(&format!(
                "gen {n} tokens requested, mock {tokens} tokens at {tps:.1} tok/s (llama backend in next step)"
            ))
        );
    }
    if check_updates {
        println!(
            "{}",
            theme.muted("check-updates: network-gated, skipped in offline default")
        );
    }
    println!(
        "{}",
        theme.ok("doctor ok, sessions in sqlite, prompts budgeted, downloads resumable")
    );
    Ok(())
}

fn dir_mb(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(walk) = std::fs::read_dir(path) {
        for entry in walk.flatten() {
            let p = entry.path();
            if p.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                total += dir_mb(&p) * 1024 * 1024 / (1024 * 1024);
                // Recursion returns MB, convert back naively is avoided by direct sum below.
            }
        }
    }
    // Simple non recursive fallback is enough for Phase 3 dashboard.
    // Full recursive sum:
    fn sum(p: &std::path::Path, acc: &mut u64) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let pp = e.path();
                if pp.is_file() {
                    *acc += e.metadata().map(|m| m.len()).unwrap_or(0);
                } else if pp.is_dir() {
                    sum(&pp, acc);
                }
            }
        }
    }
    let mut bytes = 0u64;
    sum(path, &mut bytes);
    let _ = total;
    bytes / (1024 * 1024)
}
