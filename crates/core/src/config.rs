use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| std::env::var(k).ok())
}

/// Full CLI config. Precedence: defaults < file < env ZAI_* < CLI flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub profile: ProfileCfg,
    pub model: ModelCfg,
    pub index: IndexCfg,
    pub tools: ToolsCfg,
    pub ui: UiCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileCfg {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCfg {
    pub default: String,
    pub n_ctx: u32,
    pub n_threads: u32,
    pub n_gpu_layers: u32,
    pub temp_chat: f32,
    pub temp_code: f32,
    pub top_p: f32,
    pub seed: u64,
    pub timeout_s: u64,
    #[serde(default = "default_effort")]
    pub effort: String,
}

/// Effort levels. Each maps to temp, top_p, max output tokens, max agent steps.
pub const EFFORT_LEVELS: &[&str] = &["Default", "Low", "Medium", "High", "XHigh", "Expert"];

fn default_effort() -> String {
    "Default".to_string()
}

pub fn effort_profile(level: &str) -> Option<(f32, f32, usize, u32)> {
    match level {
        "Default" => Some((0.6, 0.9, 1024, 12)),
        "Low" => Some((0.3, 0.8, 256, 4)),
        "Medium" => Some((0.5, 0.85, 512, 8)),
        "High" => Some((0.6, 0.9, 1024, 16)),
        "XHigh" => Some((0.7, 0.95, 2048, 20)),
        "Expert" => Some((0.8, 0.95, 4096, 24)),
        _ => None,
    }
}

/// Effective temp: explicit flag wins, then a customized config value,
/// then the effort profile. Stock defaults are 0.6 chat and 0.2 code.
pub fn resolve_temp(flag: Option<f32>, cfg_temp: f32, stock: f32, effort: &str) -> f32 {
    if let Some(t) = flag {
        return t;
    }
    if (cfg_temp - stock).abs() > f32::EPSILON {
        return cfg_temp;
    }
    effort_profile(effort)
        .map(|(t, _, _, _)| t)
        .unwrap_or(cfg_temp)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexCfg {
    pub paths: Vec<String>,
    pub ext: Vec<String>,
    pub exclude: Vec<String>,
    pub chunk_tokens: usize,
    pub overlap_tokens: usize,
    pub top_k: usize,
    pub max_file_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCfg {
    pub shell_allowlist: Vec<String>,
    pub shell_denylist: Vec<String>,
    pub auto_apply: bool,
    pub confirm_shell: bool,
    pub max_steps: u32,
    /// Shell permission: deny blocks all, ask uses the allowlist with prompt,
    /// allow runs any command without prompt. Only the user can set allow.
    #[serde(default = "default_shell_mode")]
    pub shell_mode: String,
}

fn default_shell_mode() -> String {
    "ask".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiCfg {
    pub theme: String,
    pub right_rail: bool,
    pub stream_flush_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: ProfileCfg {
                name: "default".to_string(),
            },
            model: ModelCfg {
                default: "qwen2.5-3b-instruct-q4_k_m".to_string(),
                n_ctx: 4096,
                n_threads: 0,
                n_gpu_layers: 0,
                temp_chat: 0.6,
                temp_code: 0.2,
                top_p: 0.9,
                seed: 0,
                timeout_s: 120,
                effort: "Default".to_string(),
            },
            index: IndexCfg {
                paths: vec!["./docs".to_string(), "./src".to_string()],
                ext: vec![
                    "md".to_string(),
                    "rs".to_string(),
                    "toml".to_string(),
                    "txt".to_string(),
                ],
                exclude: vec![
                    "target/**".to_string(),
                    ".git/**".to_string(),
                    "node_modules/**".to_string(),
                    "dist/**".to_string(),
                ],
                chunk_tokens: 512,
                overlap_tokens: 64,
                top_k: 5,
                max_file_mb: 5,
            },
            tools: ToolsCfg {
                shell_allowlist: vec![
                    "cargo test".to_string(),
                    "cargo test --quiet".to_string(),
                    "cargo fmt --check".to_string(),
                    "git status".to_string(),
                    "git diff".to_string(),
                ],
                shell_denylist: vec!["rm -rf".to_string(), "sudo".to_string(), "mkfs".to_string()],
                auto_apply: false,
                confirm_shell: true,
                max_steps: 12,
                shell_mode: default_shell_mode(),
            },
            ui: UiCfg {
                theme: "auto".to_string(),
                right_rail: true,
                stream_flush_ms: 16,
            },
        }
    }
}

impl Config {
    pub fn load(config_file: &Path) -> Result<Self> {
        let mut cfg = Self::default();
        if config_file.exists() {
            let text = std::fs::read_to_string(config_file)
                .with_context(|| format!("read {}", config_file.display()))?;
            let file_cfg: Self = toml::from_str(&text)
                .with_context(|| format!("parse {}", config_file.display()))?;
            cfg = file_cfg;
        }
        cfg.apply_env();
        cfg.validate()?;
        Ok(cfg)
    }

    fn apply_env(&mut self) {
        if let Some(v) = env_first(&["ZAI_MODEL", "AICLI_MODEL"]) {
            if !v.is_empty() {
                self.model.default = v;
            }
        }
        if let Some(v) = env_first(&["ZAI_CTX", "AICLI_CTX"]) {
            if let Ok(n) = v.parse::<u32>() {
                self.model.n_ctx = n;
            }
        }
        if let Some(v) = env_first(&["ZAI_THEME", "AICLI_THEME"]) {
            if !v.is_empty() {
                self.ui.theme = v;
            }
        }
        if let Some(v) = env_first(&["ZAI_PROFILE", "AICLI_PROFILE"]) {
            if !v.is_empty() {
                self.profile.name = v;
            }
        }
        if let Some(v) = env_first(&["ZAI_EFFORT"]) {
            if !v.is_empty() {
                self.model.effort = v;
            }
        }
        if let Some(v) = env_first(&["ZAI_SHELL"]) {
            if !v.is_empty() {
                self.tools.shell_mode = v;
            }
        }
    }

    /// Write config back to file. Creates parent dirs.
    pub fn save(&self, config_file: &Path) -> Result<()> {
        self.validate()?;
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = config_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(config_file, text)?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            [1024, 2048, 4096, 8192, 16384, 32768].contains(&self.model.n_ctx),
            "n_ctx must be one of 1024 2048 4096 8192 16384 32768, got {}",
            self.model.n_ctx
        );
        anyhow::ensure!(
            (0.0..=2.0).contains(&self.model.temp_chat),
            "temp_chat out of range 0.0-2.0"
        );
        anyhow::ensure!(
            ["auto", "dark", "light", "plain"].contains(&self.ui.theme.as_str()),
            "theme must be auto, dark, light, plain"
        );
        anyhow::ensure!(
            EFFORT_LEVELS.contains(&self.model.effort.as_str()),
            "effort must be one of Default Low Medium High XHigh Expert, got {}",
            self.model.effort
        );
        anyhow::ensure!(
            ["deny", "ask", "allow"].contains(&self.tools.shell_mode.as_str()),
            "shell_mode must be deny, ask, or allow"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn bad_ctx_rejected() {
        let mut c = Config::default();
        c.model.n_ctx = 999;
        assert!(c.validate().is_err());
    }

    #[test]
    fn effort_levels_map() {
        assert_eq!(effort_profile("Default"), Some((0.6, 0.9, 1024, 12)));
        assert_eq!(effort_profile("Expert"), Some((0.8, 0.95, 4096, 24)));
        assert_eq!(effort_profile("Nope"), None);
        assert_eq!(resolve_temp(Some(0.1), 0.6, 0.6, "Expert"), 0.1);
        assert_eq!(resolve_temp(None, 0.4, 0.6, "Expert"), 0.4);
        assert_eq!(resolve_temp(None, 0.6, 0.6, "Low"), 0.3);
        let mut c = Config::default();
        c.model.effort = "Nope".to_string();
        assert!(c.validate().is_err());
    }

    #[test]
    fn old_config_without_effort_still_loads() {
        let text = "[profile]\nname = \"default\"\n[model]\ndefault = \"m\"\nn_ctx = 4096\nn_threads = 0\nn_gpu_layers = 0\ntemp_chat = 0.6\ntemp_code = 0.2\ntop_p = 0.9\nseed = 0\ntimeout_s = 120\n[index]\npaths = []\next = []\nexclude = []\nchunk_tokens = 512\noverlap_tokens = 64\ntop_k = 5\nmax_file_mb = 5\n[tools]\nshell_allowlist = []\nshell_denylist = []\nauto_apply = false\nconfirm_shell = true\nmax_steps = 12\n[ui]\ntheme = \"auto\"\nright_rail = true\nstream_flush_ms = 16\n";
        let cfg: Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.model.effort, "Default");
    }
}
