pub mod handlers;
pub mod output;
pub mod repl;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aicli", version, about = "Offline coding and daily assistant")]
pub struct Cli {
    #[arg(long, default_value = "default")]
    pub profile: String,
    #[arg(long, default_value_t = false)]
    pub plain: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
    #[arg(long, default_value_t = false)]
    pub offline: bool,
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Start or resume interactive chat
    Chat {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        temp: Option<f32>,
        #[arg(long)]
        top_p: Option<f32>,
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long)]
        ctx: Option<u32>,
        #[arg(long, default_value_t = false)]
        mock: bool,
    },
    /// One shot question with optional RAG
    Ask {
        query: String,
        #[arg(long)]
        index: Option<String>,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long, default_value_t = false)]
        show_sources: bool,
        #[arg(long, default_value_t = false)]
        no_rag: bool,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        temp: Option<f32>,
        #[arg(long)]
        top_p: Option<f32>,
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long, default_value_t = false)]
        show_budget: bool,
    },
    /// Coding agent, dry run by default
    Code {
        goal: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = true)]
        dry_run: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
        #[arg(long, default_value_t = false)]
        allow_shell: bool,
        #[arg(long, default_value_t = 12)]
        max_steps: u32,
        #[arg(long)]
        temp: Option<f32>,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Inspect pending patches
    Patch {
        #[command(subcommand)]
        op: PatchOp,
    },
    /// Run allowlisted shell command
    Run {
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Index a folder for RAG
    Index {
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        rebuild: bool,
    },
    /// Manage GGUF models
    Models {
        #[command(subcommand)]
        op: ModelsOp,
    },
    /// Daily overview
    Daily {
        #[arg(long, default_value_t = false)]
        today: bool,
        #[arg(long, default_value_t = false)]
        week: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        add_note: Option<String>,
    },
    /// Tasks by date
    Tasks {
        #[command(subcommand)]
        op: TasksOp,
    },
    /// Notes search and capture
    Notes {
        #[command(subcommand)]
        op: NotesOp,
    },
    /// Sessions list and export
    Sessions {
        #[command(subcommand)]
        op: SessionsOp,
    },
    /// Config show and set
    Config {
        #[command(subcommand)]
        op: ConfigOp,
    },
    /// Diagnostics and benchmarks
    Doctor {
        #[arg(long, default_value_t = false)]
        bench_load: bool,
        #[arg(long)]
        bench_gen: Option<u32>,
        #[arg(long, default_value_t = false)]
        check_updates: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum PatchOp {
    Show {
        #[arg(long)]
        patch_id: String,
    },
    Apply {
        #[arg(long)]
        patch_id: String,
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    Drop {
        #[arg(long)]
        patch_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModelsOp {
    List,
    Pull { id: String },
    Verify { id: String },
    Remove { id: String },
    SetDefault { id: String },
}

#[derive(Subcommand, Debug)]
pub enum TasksOp {
    Add {
        text: String,
        #[arg(long)]
        date: Option<String>,
    },
    List {
        #[arg(long)]
        date: Option<String>,
    },
    Done {
        id: String,
    },
    Carry {
        #[arg(long, default_value = "yesterday")]
        from: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum NotesOp {
    Add {
        text: String,
        #[arg(long)]
        date: Option<String>,
    },
    List {
        #[arg(long)]
        date: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Search {
        query: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionsOp {
    List,
    Open {
        id: String,
    },
    Rename {
        id: String,
        title: String,
    },
    Delete {
        id: String,
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    Export {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "md")]
        format: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigOp {
    Show,
    Set {
        key: String,
        value: String,
    },
    Reset {
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
}

pub struct Ctx {
    pub cli: Cli,
    pub paths: aicli_core::ProfilePaths,
    pub config: aicli_core::Config,
    pub theme: aicli_ui::Theme,
}

impl Ctx {
    pub fn build(cli: Cli) -> Result<Self> {
        let paths = aicli_core::ProfilePaths::new(&cli.profile)?;
        paths.ensure()?;
        let config = aicli_core::Config::load(&paths.config_file)?;
        let mode = if cli.plain {
            aicli_ui::ThemeMode::Plain
        } else {
            aicli_ui::ThemeMode::parse(&config.ui.theme)
        };
        let theme = aicli_ui::Theme::new(mode);
        Ok(Self {
            cli,
            paths,
            config,
            theme,
        })
    }

    pub fn is_json(&self) -> bool {
        self.cli.json
    }

    pub fn is_quiet(&self) -> bool {
        self.cli.quiet
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let ctx = Ctx::build(cli)?;
    handlers::dispatch(ctx).await
}
