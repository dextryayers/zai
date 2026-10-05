use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Full CLI config. Precedence: defaults < file < env AICLI_* < CLI flags.
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
        if let Ok(v) = std::env::var("AICLI_MODEL") {
            if !v.is_empty() {
                self.model.default = v;
            }
        }
        if let Ok(v) = std::env::var("AICLI_CTX") {
            if let Ok(n) = v.parse::<u32>() {
                self.model.n_ctx = n;
            }
        }
        if let Ok(v) = std::env::var("AICLI_THEME") {
            if !v.is_empty() {
                self.ui.theme = v;
            }
        }
        if let Ok(v) = std::env::var("AICLI_PROFILE") {
            if !v.is_empty() {
                self.profile.name = v;
            }
        }
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
}
