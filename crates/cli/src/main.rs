pub mod answer;
pub mod handlers;
pub mod output;
pub mod repl;
pub mod settings;
pub mod slash;
pub mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "zai",
    version,
    about = "Zai AI offline coding and daily assistant"
)]
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

#[derive(Subcommand, Debug, Clone)]
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
        #[arg(long, default_value_t = false)]
        yes: bool,
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
        /// Skip the approval prompt (still gated by shell mode + denylist).
        #[arg(long, default_value_t = false)]
        yes: bool,
        /// Run with full output (no 4000-char preview truncation in JSON).
        #[arg(long, default_value_t = false)]
        full: bool,
    },
    /// Index a folder for RAG
    Index {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        rebuild: bool,
        #[arg(long, default_value_t = false)]
        eval: bool,
        #[arg(long, default_value_t = false)]
        status: bool,
    },
    /// Manage GGUF models
    Models {
        #[command(subcommand)]
        op: ModelsOp,
    },
    /// Ollama daemon models
    Ollama {
        #[command(subcommand)]
        op: OllamaOp,
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
    /// User owned long memory
    Memory {
        #[command(subcommand)]
        op: MemoryOp,
    },
    /// Insert local GGUF file(s) into the model cache. Shortcut for models insert.
    Insert {
        /// GGUF file(s) or folder(s). Folders are scanned for *.gguf.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Custom model id. Only allowed with a single file.
        #[arg(long)]
        name: Option<String>,
        /// Context size. Defaults to a size-based suggestion (2048/4096/8192).
        #[arg(long)]
        ctx: Option<u32>,
        /// Set the inserted model as default after insert.
        #[arg(long, default_value_t = false)]
        default: bool,
        /// Scan folders recursively.
        #[arg(long, default_value_t = false)]
        recursive: bool,
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
    /// Local inference backend (llama.cpp): status and setup
    Backend {
        #[command(subcommand)]
        op: BackendOp,
    },
}

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
pub enum ModelsOp {
    List,
    Pull {
        id: String,
    },
    #[command(aliases = ["add", "import"])]
    Insert {
        /// GGUF file(s) or folder(s). Folders are scanned for *.gguf.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Custom model id. Only allowed with a single file.
        #[arg(long)]
        name: Option<String>,
        /// Context size. Defaults to a size-based suggestion (2048/4096/8192).
        #[arg(long)]
        ctx: Option<u32>,
        /// Set the inserted model as default after insert.
        #[arg(long, default_value_t = false)]
        default: bool,
        /// Scan folders recursively.
        #[arg(long, default_value_t = false)]
        recursive: bool,
    },
    Verify {
        id: String,
    },
    /// Scan folders for GGUF files. Defaults to cwd, ./models, ~/models.
    Scan {
        /// Folders to scan. Empty means the default folders.
        paths: Vec<PathBuf>,
        /// Scan folders recursively.
        #[arg(long, default_value_t = false)]
        recursive: bool,
    },
    /// Read GGUF header metadata: version, tensors, arch, quant, ctx hint.
    Inspect {
        path: PathBuf,
    },
    Remove {
        id: String,
    },
    SetDefault {
        id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum OllamaOp {
    Status,
    List,
    Pull { name: String },
    Rm { name: String },
    Show { name: String },
}

#[derive(Subcommand, Debug, Clone)]
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
    Clear {
        #[arg(long)]
        date: Option<String>,
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
pub enum MemoryOp {
    Show,
    Add { text: String },
    Promote { session: String, turn: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum BackendOp {
    Status,
    Setup {
        /// llama.cpp tag to build, defaults to the pinned release.
        #[arg(long)]
        tag: Option<String>,
    },
    Serve {
        /// Model id to serve. Defaults to the active model.
        #[arg(long)]
        model: Option<String>,
    },
    Stop,
}

#[derive(Debug, Clone)]
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
