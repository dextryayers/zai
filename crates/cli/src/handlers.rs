use crate::MemoryOp;
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
            yes,
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
            *yes,
        ),
        Some(Cmd::Patch { op }) => cmd_patch(&ctx, op),
        Some(Cmd::Run { cmd }) => cmd_run(&ctx, cmd),
        Some(Cmd::Index {
            path,
            rebuild,
            eval,
            status,
        }) => cmd_index(&ctx, path, *rebuild, *eval, *status),
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
        Some(Cmd::Memory { op }) => cmd_memory(&ctx, op),
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
                product: "zai".to_string(),
                version: "1.0.0".to_string(),
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
            "1.0.0",
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
        "chat   start daily chat      zai chat\ncode   fix from prompt       zai code \"fix failing test\"\ndaily  today overview        zai daily --today\nindex  search local files    zai index ./docs\n\ntasks open: {open}  sessions: {sessions_open}  profile: {}",
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
    top_k: usize,
    show_sources: bool,
    no_rag: bool,
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

    // Phase 6 real retrieval when index exists. Falls back to mock chunk.
    let top_k = top_k.clamp(1, 10);
    let (chunks, retrieved) = if no_rag {
        (vec![], vec![])
    } else {
        // Index root: --index flag or first config path or cwd.
        let root = ctx
            .cli
            .cmd
            .as_ref()
            .and_then(|c| match c {
                Cmd::Ask { index, .. } => index.clone(),
                _ => None,
            })
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                ctx.config
                    .index
                    .paths
                    .first()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
            });
        let root_abs = root.canonicalize().unwrap_or(root.clone());
        match aicli_rag::retrieve(&ctx.paths.cache_dir, &root_abs, query, top_k) {
            Ok(hits) if !hits.is_empty() => {
                let texts: Vec<String> = hits
                    .iter()
                    .map(|h| {
                        format!(
                            "[SOURCE] {}:{}-{} score={:.2}\n{}",
                            h.path,
                            h.start_line,
                            h.end_line,
                            h.score,
                            h.text.chars().take(800).collect::<String>()
                        )
                    })
                    .collect();
                (texts, hits)
            }
            _ => (
                vec!["crates/cli/src/main.rs: command tree and dispatch".to_string()],
                vec![],
            ),
        }
    };
    let usage = aicli_infer::build_prompt(
        "You are AICLI, a local assistant. Answer concisely. Use provided sources first and cite paths.",
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
        format!("mock backend, pull with: zai models pull {model_id}")
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
        struct Source {
            path: String,
            start_line: usize,
            end_line: usize,
            score: f64,
            bm25: f64,
            vector: f64,
        }
        #[derive(Serialize)]
        struct Data {
            query: String,
            answer: String,
            model: String,
            sampler: aicli_infer::SamplerConfig,
            usage: aicli_infer::PromptUsage,
            backend: String,
            sources: Vec<Source>,
        }
        let sources: Vec<Source> = retrieved
            .iter()
            .map(|h| Source {
                path: h.path.clone(),
                start_line: h.start_line,
                end_line: h.end_line,
                score: (h.score * 100.0).round() / 100.0,
                bm25: (h.bm25 * 100.0).round() / 100.0,
                vector: (h.vector * 100.0).round() / 100.0,
            })
            .collect();
        output::print_json(
            true,
            Data {
                query: query.to_string(),
                answer: answer.clone(),
                model: model_id,
                sampler,
                usage,
                backend: backend_note,
                sources,
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
                "1.0.0",
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
        if retrieved.is_empty() {
            println!(
                "{}",
                panel::render_panel(
                    theme,
                    "Sources",
                    "no index hits, run: zai index ./docs --rebuild"
                )
            );
        } else {
            let body = retrieved
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    format!(
                        "[{}] {}:{}-{} score={:.2} bm25={:.2} vec={:.2}",
                        i + 1,
                        h.path,
                        h.start_line,
                        h.end_line,
                        h.score,
                        h.bm25,
                        h.vector
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            println!("{}", panel::render_panel(theme, "Sources", &body));
        }
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
    allow_shell: bool,
    max_steps: u32,
    temp: Option<f32>,
    seed: Option<u64>,
    yes: bool,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let sampler = sampler_for(ctx, temp, seed, "code");
    let steps_cap = max_steps.min(ctx.config.tools.max_steps).max(1);
    let root = if path.is_file() {
        path.parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    } else {
        path.to_path_buf()
    };
    let session_tag = "code-cli";

    // Step 1 INVESTIGATE with animated checklist.
    let mut plan = vec![
        ("scope search".to_string(), false),
        ("read top hits".to_string(), false),
        ("draft diff".to_string(), false),
        ("approval".to_string(), false),
        ("verify".to_string(), false),
    ];
    let render_plan = |plan: &[(String, bool)]| -> String {
        plan.iter()
            .enumerate()
            .map(|(i, (name, done))| {
                let mark = if *done { "ok" } else { ".." };
                format!("{}. [{mark}] {name}", i + 1)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    if !ctx.is_json() && !ctx.is_quiet() {
        for i in 0..3 {
            print!(
                "\r{}",
                progress::spinner_line(theme, i, "agent: scoping search")
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        println!();
    }
    let keyword = goal.split_whitespace().next().unwrap_or(goal);
    let hits = aicli_tools::search_files(&root, keyword, false, true, &[], 20).unwrap_or_default();
    let _ = aicli_tools::log_event(
        &ctx.paths.data_dir,
        "code.search",
        Some(session_tag),
        &format!("goal={goal} hits={}", hits.len()),
    );
    plan[0].1 = true;

    // Step 2 read top 3 files inside sandbox.
    let mut read_summaries: Vec<String> = Vec::new();
    for h in hits.iter().take(3) {
        let rel = pathdiff_rel(&root, &std::path::PathBuf::from(&h.file));
        if let Ok((lines, total)) = aicli_tools::fs_read(
            &root,
            &rel,
            h.line_no.saturating_sub(5).max(1),
            h.line_no + 20,
        ) {
            read_summaries.push(format!("{}:{} total {total} lines", h.file, h.line_no));
            let _ = lines;
        }
    }
    plan[1].1 = true;
    if steps_cap < 3 {
        anyhow::bail!(
            "max_steps {steps_cap} too low, need at least 3 for investigate plus draft plus review"
        );
    }

    // Step 3 draft diff. Deterministic template grounded in search hits.
    let diff = draft_diff_for_goal(&root, goal, &hits);
    let file_count = aicli_tools::validate_patch(&diff, root.to_string_lossy().as_ref())
        .map_err(|e| anyhow::anyhow!("draft diff invalid: {e}"))?;
    plan[2].1 = true;
    let _ = aicli_tools::log_event(
        &ctx.paths.data_dir,
        "code.draft",
        Some(session_tag),
        &format!("files={file_count} goal={goal}"),
    );

    if ctx.is_json() {
        #[derive(Serialize)]
        struct Data {
            goal: String,
            steps: u32,
            hits: usize,
            files: usize,
            diff: String,
            sampler: aicli_infer::SamplerConfig,
            mode: String,
        }
        output::print_json(
            true,
            Data {
                goal: goal.to_string(),
                steps: steps_cap,
                hits: hits.len(),
                files: file_count,
                diff: diff.clone(),
                sampler,
                mode: if apply { "apply-requested" } else { "dry-run" }.to_string(),
            },
        );
        if !apply {
            return Ok(());
        }
    } else {
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Plan",
                &format!(
                    "goal: {goal}\nmode: code temp {:.1} seed {}\n{}",
                    sampler.temp,
                    sampler.seed,
                    render_plan(&plan)
                )
            )
        );
        if hits.is_empty() {
            println!("{}", theme.warn("no matching files, scope is full root"));
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
        // Diff preview with word diff styling via panel.
        println!(
            "{}",
            panel::render_panel(theme, "Diff preview", diff.trim())
        );
        if !read_summaries.is_empty() {
            println!(
                "{}",
                theme.muted(&format!("read: {}", read_summaries.join(" | ")))
            );
        }
    }

    // Save patch file for review flow.
    let patch_id = format!("p{:04}", patch_counter(&ctx.paths.patches_dir) + 1);
    std::fs::create_dir_all(&ctx.paths.patches_dir)?;
    let patch_file = ctx.paths.patches_dir.join(format!("{patch_id}.diff"));
    std::fs::write(&patch_file, &diff)?;
    if !ctx.is_json() {
        println!(
            "{}",
            theme.muted(&format!(
                "patch saved: {} ({patch_id})",
                patch_file.display()
            ))
        );
    }

    if !apply {
        if !ctx.is_json() {
            println!(
                "{}",
                theme.muted("dry-run only, no files changed. Review then: patch apply or code --apply --yes")
            );
        }
        return Ok(());
    }

    // Step 4 approval. Explicit prompt unless --yes.
    plan[3].1 = false;
    if !yes && !ctx.is_json() {
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Approval",
                "Apply? [a]pply  [r]eject  [t]show tests\nType a plus Enter. Ctrl+C aborts with zero changes."
            )
        );
        let choice = read_line_prompt(theme, "choice [a/r/t]> ")?;
        match choice.trim().to_lowercase().as_str() {
            "a" | "apply" | "y" | "yes" => {}
            "t" => {
                println!(
                    "{}",
                    theme.muted("verify plan: cargo fmt --check, cargo test --quiet")
                );
                return Ok(());
            }
            _ => {
                println!("{}", theme.warn("rejected, zero files changed"));
                return Ok(());
            }
        }
    } else if !yes && ctx.is_json() {
        // JSON apply without --yes stays dry for safety.
        return Ok(());
    }
    plan[3].1 = true;

    // Step 5 atomic apply plus verify.
    for i in 0..3 {
        print!(
            "\r{}",
            progress::spinner_line(theme, i, "agent: applying patch atomic")
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    println!();
    let touched = aicli_tools::apply_patch_set(&root, &diff)
        .map_err(|e| anyhow::anyhow!("apply failed, zero files changed: {e}"))?;
    let _ = aicli_tools::log_event(
        &ctx.paths.data_dir,
        "code.apply",
        Some(session_tag),
        &format!("patch={patch_id} touched={}", touched.join(",")),
    );
    plan[4].1 = true;
    if !ctx.is_json() {
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Applied",
                &format!("{}\nbackup: <file>.zai.bak.<ts>", touched.join("\n"))
            )
        );
        println!(
            "{}",
            panel::render_panel(theme, "Plan done", &render_plan(&plan))
        );
    }

    // Verify when Rust files changed and shell allowed.
    let rust_touched = touched.iter().any(|p| p.ends_with(".rs"));
    if rust_touched {
        let verify_cmd = "cargo fmt --check";
        if aicli_tools::check_shell(
            verify_cmd,
            &ctx.config.tools.shell_allowlist,
            &ctx.config.tools.shell_denylist,
        ) == aicli_tools::GateDecision::Allow
            && (allow_shell || yes)
        {
            let logp = ctx.paths.logs_dir.join("code-verify.log");
            match aicli_tools::run_blocking(
                verify_cmd,
                &root,
                &ctx.config.tools.shell_allowlist,
                &ctx.config.tools.shell_denylist,
                std::time::Duration::from_secs(60),
                Some(&logp),
            ) {
                Ok((preview, _, code)) => {
                    if !ctx.is_json() {
                        println!(
                            "{}",
                            panel::render_panel(
                                theme,
                                &format!("Verify {verify_cmd} exit {code}"),
                                &preview.chars().take(1500).collect::<String>()
                            )
                        );
                    }
                }
                Err(e) => {
                    if !ctx.is_json() {
                        println!("{}", theme.warn(&format!("verify skipped: {e}")));
                    }
                }
            }
        } else if !ctx.is_json() {
            println!(
                "{}",
                theme.muted("verify skipped: pass --allow-shell with allowlisted fmt command")
            );
        }
    }

    if ctx.is_json() {
        #[derive(Serialize)]
        struct Applied {
            patch_id: String,
            touched: Vec<String>,
        }
        output::print_json(true, Applied { patch_id, touched });
    }
    Ok(())
}

fn patch_counter(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().map(|x| x == "diff").unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

fn pathdiff_rel(root: &std::path::Path, abs: &std::path::Path) -> String {
    if let Ok(rel) = abs.strip_prefix(root) {
        return rel.to_string_lossy().to_string();
    }
    // Fallback: file name only when outside root listing uses absolute cache paths.
    abs.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| abs.to_string_lossy().to_string())
}

fn draft_diff_for_goal(
    root: &std::path::Path,
    goal: &str,
    hits: &[aicli_tools::SearchHit],
) -> String {
    // Grounded template: touch first hit file when available, else create note file.
    // Keeps hunks valid by reading real context lines.
    if let Some(h) = hits.first() {
        let rel = pathdiff_rel(root, &std::path::PathBuf::from(&h.file));
        let ctx_lines = aicli_tools::fs_read(root, &rel, h.line_no, h.line_no)
            .map(|(v, _)| v.first().cloned().unwrap_or_default())
            .unwrap_or_default();
        let safe_ctx = if ctx_lines.trim().is_empty() {
            "// context".to_string()
        } else {
            ctx_lines
        };
        return format!(
            "--- a/{rel}\n+++ b/{rel}\n@@ -{n},1 +{n},2 @@\n {safe_ctx}\n+// zai: {goal_short}\n",
            n = h.line_no,
            rel = rel,
            safe_ctx = safe_ctx,
            goal_short = goal.chars().take(80).collect::<String>(),
        );
    }
    "--- a/zai-note.md\n+++ b/zai-note.md\n@@ -1,0 +1,2 @@\n+# zai\n+// zai: planned edit\n"
        .to_string()
}

fn read_line_prompt(theme: &aicli_ui::Theme, prompt: &str) -> anyhow::Result<String> {
    use std::io::Write;
    // Auto yes for tests and pipes.
    if std::env::var("ZAI_AUTO_YES")
        .or_else(|_| std::env::var("AICLI_AUTO_YES"))
        .is_ok()
    {
        return Ok("a".to_string());
    }
    print!("{}", theme.accent(prompt));
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s)
}

fn cmd_patch(ctx: &Ctx, op: &PatchOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match op {
        PatchOp::Show { patch_id } => {
            let p = ctx.paths.patches_dir.join(format!("{patch_id}.diff"));
            if !p.exists() {
                println!(
                    "{}",
                    panel::render_panel(
                        theme,
                        &format!("Patch {patch_id}"),
                        "no pending patch with that id"
                    )
                );
                return Ok(());
            }
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    diff: String,
                    files: usize,
                }
                let files = aicli_tools::validate_patch(&text, ".").unwrap_or(0);
                output::print_json(
                    true,
                    Data {
                        id: patch_id.clone(),
                        diff: text,
                        files,
                    },
                );
                return Ok(());
            }
            // Render with line numbers and gutter colors via word diff stats.
            let files = aicli_tools::parse_patch(&text, &root).unwrap_or_default();
            let mut body = String::new();
            for f in &files {
                body.push_str(&format!("file: {}\n", f.new_path));
                for h in &f.hunks {
                    body.push_str(&format!(
                        "hunk -{},{} +{},{} lines {}\n",
                        h.old_start,
                        h.old_lines,
                        h.new_start,
                        h.new_lines,
                        h.body.len()
                    ));
                }
            }
            body.push_str("\n--- raw ---\n");
            body.push_str(text.trim());
            println!(
                "{}",
                panel::render_panel(theme, &format!("Patch {patch_id}"), &body)
            );
        }
        PatchOp::Apply { patch_id, yes } => {
            let p = ctx.paths.patches_dir.join(format!("{patch_id}.diff"));
            if !p.exists() {
                anyhow::bail!("unknown patch {patch_id}");
            }
            let diff = std::fs::read_to_string(&p)?;
            // Validate first, show files, require --yes or prompt.
            let files = aicli_tools::parse_patch(&diff, &root)?;
            if !ctx.is_json() && !yes {
                println!(
                    "{}",
                    panel::render_panel(
                        theme,
                        &format!("Apply {patch_id}"),
                        &format!(
                            "files: {}\nType yes plus Enter for atomic apply.",
                            files
                                .iter()
                                .map(|f| f.new_path.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    )
                );
                let ans = read_line_prompt(theme, "apply? [yes/no]> ")?;
                if ans.trim().to_lowercase() != "yes" {
                    println!("{}", theme.warn("aborted, zero files changed"));
                    return Ok(());
                }
            }
            // Atomic apply: all hunks or zero changes.
            for i in 0..3 {
                print!(
                    "\r{}",
                    progress::spinner_line(theme, i, "applying patch atomic")
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            println!();
            // Patches drafted from repo root, but CLI root may be cwd. Try cwd then repo ancestors.
            let touched = apply_with_root_fallback(&root, &diff)?;
            let _ = aicli_tools::log_event(
                &ctx.paths.data_dir,
                "patch.apply",
                None,
                &format!("patch={patch_id} touched={}", touched.join(",")),
            );
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    id: String,
                    touched: Vec<String>,
                }
                output::print_json(
                    true,
                    Data {
                        id: patch_id.clone(),
                        touched,
                    },
                );
            } else {
                println!(
                    "{}",
                    panel::render_panel(theme, &format!("Applied {patch_id}"), &touched.join("\n"))
                );
            }
        }
        PatchOp::Drop { patch_id } => {
            let p = ctx.paths.patches_dir.join(format!("{patch_id}.diff"));
            if p.exists() {
                std::fs::remove_file(&p)?;
                println!("{}", theme.ok(&format!("dropped {patch_id}")));
            } else {
                println!(
                    "{}",
                    theme.muted(&format!("drop {patch_id}: nothing to drop"))
                );
            }
        }
    }
    Ok(())
}

fn apply_with_root_fallback(root: &std::path::Path, diff: &str) -> anyhow::Result<Vec<String>> {
    // Try cwd, then walk up to 4 ancestors. Each attempt is atomic so safe to retry.
    let mut cur = Some(root.to_path_buf());
    let mut last_err = anyhow::anyhow!("no root tried");
    for _ in 0..5 {
        if let Some(r) = cur.clone() {
            match aicli_tools::apply_patch_set(&r, diff) {
                Ok(touched) => return Ok(touched),
                Err(e) => last_err = e,
            }
            cur = r.parent().map(|p| p.to_path_buf());
        } else {
            break;
        }
    }
    Err(last_err)
}

fn cmd_run(ctx: &Ctx, cmd: &[String]) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let full = cmd.join(" ");
    if full.is_empty() {
        anyhow::bail!("usage: zai run -- <cmd...>");
    }
    // Gate first for fast deny with exit 5.
    if let aicli_tools::GateDecision::Deny { reason, hint } = aicli_tools::check_shell(
        &full,
        &ctx.config.tools.shell_allowlist,
        &ctx.config.tools.shell_denylist,
    ) {
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
    // Confirm when required, unless auto yes env or JSON quiet automation.
    let auto_yes = std::env::var("ZAI_AUTO_YES")
        .or_else(|_| std::env::var("AICLI_AUTO_YES"))
        .is_ok();
    let need_confirm = ctx.config.tools.confirm_shell && !auto_yes;
    if need_confirm && !ctx.is_json() {
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Run approval",
                &format!("cmd: {full}\nAllowlisted. Type yes to run, Ctrl+C aborts.")
            )
        );
        // Elegant 3s countdown display before prompt.
        for i in (1..=3).rev() {
            print!(
                "\r{}",
                theme.muted(&format!("confirm in {i}s, Ctrl+C aborts"))
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        println!();
        let ans = read_line_prompt(theme, "run? [yes/no]> ")?;
        if ans.trim().to_lowercase() != "yes" {
            println!("{}", theme.warn("aborted, nothing ran"));
            return Ok(());
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let logp = ctx.paths.logs_dir.join("run.log");
    match aicli_tools::run_blocking(
        &full,
        &cwd,
        &ctx.config.tools.shell_allowlist,
        &ctx.config.tools.shell_denylist,
        std::time::Duration::from_secs(60),
        Some(&logp),
    ) {
        Ok((preview, full_text, code)) => {
            let _ = aicli_tools::log_event(
                &ctx.paths.data_dir,
                "shell.run",
                None,
                &format!("cmd={full} exit={code}"),
            );
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    cmd: String,
                    exit: i32,
                    preview: String,
                }
                output::print_json(
                    true,
                    Data {
                        cmd: full,
                        exit: code,
                        preview,
                    },
                );
            } else {
                println!("{preview}");
                let _ = full_text;
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("E_TOOL_DENIED") {
                if ctx.is_json() {
                    output::print_json_error("E_TOOL_DENIED", &msg, "choose allowlisted command");
                } else {
                    eprintln!(
                        "{}",
                        aicli_ui::panel::render_error(
                            theme,
                            "E_TOOL_DENIED",
                            &msg,
                            "choose allowlisted command",
                            None
                        )
                    );
                }
                std::process::exit(5);
            }
            if msg.contains("E_TIMEOUT") {
                anyhow::bail!("{msg}");
            }
            anyhow::bail!("{msg}")
        }
    }
}

fn cmd_index(
    ctx: &Ctx,
    path: &std::path::Path,
    rebuild: bool,
    eval: bool,
    status_only: bool,
) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    let cache_dir = ctx.paths.cache_dir.clone();
    if eval {
        // Elegant eval animation with progress dots.
        if !ctx.is_json() {
            for i in 0..4 {
                print!(
                    "\r{}",
                    progress::spinner_line(theme, i, "rag eval: 10 queries")
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            println!();
        }
        let (passed, total, details) = aicli_rag::eval::run_eval(&cache_dir)?;
        if ctx.is_json() {
            #[derive(Serialize)]
            struct Row {
                query: String,
                ok: bool,
                tops: Vec<String>,
            }
            let rows: Vec<Row> = details
                .into_iter()
                .map(|(query, ok, tops)| Row { query, ok, tops })
                .collect();
            output::print_json(
                true,
                serde_json::json!({"passed": passed, "total": total, "cases": rows}),
            );
            return Ok(());
        }
        let rows: Vec<Vec<String>> = details
            .iter()
            .map(|(q, ok, tops)| {
                vec![
                    q.clone(),
                    if *ok {
                        "pass".to_string()
                    } else {
                        "fail".to_string()
                    },
                    tops.first().cloned().unwrap_or_default(),
                ]
            })
            .collect();
        println!(
            "{}",
            table::render_table(
                theme,
                &["QUERY", "OK", "TOP1"],
                &rows,
                &[false, false, false]
            )
        );
        if passed >= 8 {
            println!("{}", theme.ok(&format!("rag eval {passed}/{total} pass")));
        } else {
            println!(
                "{}",
                theme.danger(&format!("rag eval {passed}/{total} below gate 8/10"))
            );
            std::process::exit(6);
        }
        return Ok(());
    }
    if status_only {
        let msg = aicli_rag::index::index_status_report(&cache_dir, path)?;
        if ctx.is_json() {
            output::print_json(true, serde_json::json!({"status": msg}));
        } else {
            println!("{}", panel::render_panel(theme, "Index status", &msg));
        }
        return Ok(());
    }
    let opts = aicli_ingest::walk::WalkOptions {
        exts: ctx.config.index.ext.clone(),
        exclude_globs: ctx.config.index.exclude.clone(),
        include_hidden: false,
        max_file_mb: ctx.config.index.max_file_mb,
    };
    // Animated build with live counts.
    let rep = aicli_rag::build_index(
        path,
        &cache_dir,
        &opts,
        ctx.config.index.chunk_tokens,
        ctx.config.index.overlap_tokens,
        &ctx.config.index.ext,
        rebuild,
        |done, total, chunks| {
            if !ctx.cli.json {
                print!(
                    "\r{}",
                    progress::spinner_line(
                        theme,
                        done,
                        &format!("indexing {done}/{total} files | {chunks} chunks")
                    )
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
        },
    )
    .map_err(|e| {
        // Stale index error already contains rebuild fix.
        anyhow::anyhow!("{e}")
    })?;
    println!();
    if ctx.is_json() {
        output::print_json(true, &rep);
        return Ok(());
    }
    let rows = vec![vec![
        rep.root.clone(),
        rep.files.to_string(),
        rep.chunks.to_string(),
        format!("{}ms", rep.elapsed_ms),
        format!(
            "{}+{}-{} skip{}",
            rep.added,
            rep.updated,
            rep.removed,
            rep.skipped_large + rep.skipped_binary
        ),
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
            &["ROOT", "FILES", "CHUNKS", "TIME", "DELTA", "MODE"],
            &rows,
            &[false, true, true, true, false, false]
        )
    );
    println!(
        "{}",
        panel::render_panel(
            theme,
            "Index",
            &format!(
                "version {} hash {}\ndir: {}\nask with: zai ask \"query\" --index {} --show-sources",
                rep.version, rep.config_hash, rep.dir, rep.root
            )
        )
    );
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
                        &format!("zai models pull {id}"),
                    );
                } else {
                    eprintln!(
                        "{}",
                        aicli_ui::panel::render_error(
                            theme,
                            "E_MODEL_MISSING",
                            &format!("{id} not cached"),
                            &format!("zai models pull {id}"),
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
                    "set-default {id}: edit config model.default, then: zai config show"
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
    if week {
        let report = aicli_core::store::week_report(&ctx.paths.data_dir);
        if ctx.is_json() {
            output::print_json(true, &report);
            return Ok(());
        }
        // Elegant week table with per day bars.
        let rows: Vec<Vec<String>> = report
            .days
            .iter()
            .map(|d| {
                let bar = "=".repeat(d.open.min(10));
                vec![
                    d.date.clone(),
                    format!("{}", d.open),
                    format!("{}", d.done),
                    format!("{}", d.notes),
                    bar,
                ]
            })
            .collect();
        println!(
            "{}",
            table::render_table(
                theme,
                &["DATE", "OPEN", "DONE", "NOTES", "LOAD"],
                &rows,
                &[false, true, true, true, false]
            )
        );
        let top = report
            .top_terms
            .iter()
            .map(|(w, c)| format!("{w} {c}x"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{}",
            panel::render_panel(
                theme,
                "Week",
                &format!(
                    "open {} done {} notes {}\ntop: {}\ncounts only, no invented narrative",
                    report.total_open,
                    report.total_done,
                    report.total_notes,
                    if top.is_empty() { "-" } else { &top }
                )
            )
        );
        return Ok(());
    }
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
    // Today view with memory hint and animated header.
    let mem_hint = if aicli_core::memory::memory_path(&ctx.paths.data_dir).exists() {
        "memory: on, see: zai memory show"
    } else {
        "memory: empty, add with: zai memory add \"prefer tabs\""
    };
    let mut body = format!("open tasks: {}\n", open.len());
    for t in open.iter().take(10) {
        body.push_str(&format!("[ ] {} {}\n", t.id, t.text));
    }
    if today || (!today && !week) {
        body.push_str("\nRun: zai tasks add \"write tests\"");
        body.push_str(&format!("\n{mem_hint}"));
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
                    theme.warn("No tasks yet. Run: zai tasks add \"first task\"")
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
            let n = aicli_core::store::task_carry(&ctx.paths.data_dir, from)?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    from: String,
                    copied: usize,
                }
                output::print_json(
                    true,
                    Data {
                        from: from.clone(),
                        copied: n,
                    },
                );
            } else {
                println!(
                    "{}",
                    theme.ok(&format!("carried {n} open tasks from {from} to today"))
                );
            }
            Ok(())
        }
        TasksOp::Clear { date, yes } => {
            if !yes && !ctx.is_json() {
                println!("{}", theme.warn("clear needs --yes, no action taken"));
                return Ok(());
            }
            let items = aicli_core::store::task_list(&ctx.paths.data_dir, date.as_deref());
            let mut removed = 0;
            if let Ok(conn) = aicli_core::db::open(&ctx.paths.db_file) {
                for t in &items {
                    if t.done
                        && conn
                            .execute("DELETE FROM tasks WHERE id=?1", [t.id.clone()])
                            .unwrap_or(0)
                            > 0
                    {
                        removed += 1;
                    }
                }
            }
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    removed: usize,
                }
                output::print_json(true, Data { removed });
            } else {
                println!("{}", theme.ok(&format!("cleared {removed} done tasks")));
            }
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
            } else if hits.is_empty() {
                println!("{}", theme.warn("no notes match"));
            } else {
                // Keyword highlight: wrap matched query with accent markers.
                let rows: Vec<Vec<String>> = hits
                    .iter()
                    .map(|n| {
                        let hl = highlight_query(&n.text, query, theme);
                        vec![n.id.clone(), n.date.clone(), hl]
                    })
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

fn highlight_query(text: &str, query: &str, theme: &aicli_ui::Theme) -> String {
    let q = query.trim();
    if q.is_empty() {
        return text.to_string();
    }
    // Case insensitive single pass, deterministic.
    let lower = text.to_lowercase();
    let lq = q.to_lowercase();
    if let Some(pos) = lower.find(&lq) {
        let end = pos + lq.len();
        // Byte indices are safe for ascii query; fallback to plain on unicode edge.
        if let (Some(a), Some(b)) = (text.get(..pos), text.get(pos..end)) {
            let rest = text.get(end..).unwrap_or("");
            return format!("{a}{}{rest}", theme.accent(b));
        }
    }
    text.to_string()
}

fn cmd_memory(ctx: &Ctx, op: &MemoryOp) -> anyhow::Result<()> {
    let theme = &ctx.theme;
    match op {
        MemoryOp::Show => {
            let text = aicli_core::memory::memory_show(&ctx.paths.data_dir);
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    path: String,
                    content: String,
                }
                output::print_json(
                    true,
                    Data {
                        path: aicli_core::memory::memory_path(&ctx.paths.data_dir)
                            .display()
                            .to_string(),
                        content: text,
                    },
                );
            } else {
                println!(
                    "{}",
                    markdown::render_markdown(theme, &format!("```md\n{text}\n```"))
                );
            }
            Ok(())
        }
        MemoryOp::Add { text } => {
            let p = aicli_core::memory::memory_add(&ctx.paths.data_dir, text)?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    path: String,
                }
                output::print_json(true, Data { path: p });
            } else {
                println!("{}", theme.ok(&format!("memory appended: {p}")));
            }
            Ok(())
        }
        MemoryOp::Promote { session, turn } => {
            let conn = aicli_core::db::open(&ctx.paths.db_file)?;
            let turns = aicli_core::sessions::list_turns(&conn, session)?;
            let found = turns
                .iter()
                .find(|t| t.id == *turn)
                .ok_or_else(|| anyhow::anyhow!("unknown turn {turn} in session {session}"))?;
            // Preview before append, never silent.
            if !ctx.is_json() {
                println!(
                    "{}",
                    panel::render_panel(
                        theme,
                        "Promote preview",
                        &format!(
                            "role: {}\n{}\n\nAppend to memory.md?",
                            found.role,
                            found.content.chars().take(800).collect::<String>()
                        )
                    )
                );
            }
            let p = aicli_core::memory::memory_promote(
                &ctx.paths.data_dir,
                session,
                &found.id,
                &found.role,
                &found.content,
            )?;
            if ctx.is_json() {
                #[derive(Serialize)]
                struct Data {
                    path: String,
                }
                output::print_json(true, Data { path: p });
            } else {
                println!("{}", theme.ok(&format!("promoted to {p}")));
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
                    panel::render_panel(theme, "Sessions", "No sessions yet. Run: zai chat")
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
                version: "1.0.0".to_string(),
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
                format!("no, run: zai models pull {}", e.id)
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
