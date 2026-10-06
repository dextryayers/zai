use anyhow::{Context, Result};
use std::path::PathBuf;

/// Resolved directories for one profile.
/// Layout follows plan.md Section 9.
#[derive(Debug, Clone)]
pub struct ProfilePaths {
    pub profile: String,
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub sessions_dir: PathBuf,
    pub patches_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub history_file: PathBuf,
    pub cache_dir: PathBuf,
    pub models_dir: PathBuf,
}

impl ProfilePaths {
    pub fn new(profile: &str) -> Result<Self> {
        // Test override: ZAI_TEST_HOME points to a temp dir (AICLI_TEST_HOME kept as fallback).
        if let Ok(test_home) =
            std::env::var("ZAI_TEST_HOME").or_else(|_| std::env::var("AICLI_TEST_HOME"))
        {
            let base = PathBuf::from(test_home);
            let data_dir = base.join("profiles").join(profile);
            return Ok(Self {
                profile: profile.to_string(),
                config_file: base.join("config.toml"),
                db_file: data_dir.join("db.sqlite"),
                sessions_dir: data_dir.join("sessions"),
                patches_dir: data_dir.join("patches"),
                logs_dir: data_dir.join("logs"),
                history_file: data_dir.join("history.txt"),
                data_dir,
                cache_dir: base.join("cache"),
                models_dir: base.join("cache").join("models"),
            });
        }

        let config_base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                directories::BaseDirs::new()
                    .map(|b| b.config_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("~/.config"))
            });
        let data_base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                directories::BaseDirs::new()
                    .map(|b| b.data_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            });
        let cache_base = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                directories::BaseDirs::new()
                    .map(|b| b.cache_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("~/.cache"))
            });

        let config_file = config_base.join("zai").join("config.toml");
        let data_dir = data_base.join("zai").join("profiles").join(profile);
        let cache_dir = cache_base.join("zai");
        Ok(Self {
            profile: profile.to_string(),
            config_file,
            db_file: data_dir.join("db.sqlite"),
            sessions_dir: data_dir.join("sessions"),
            patches_dir: data_dir.join("patches"),
            logs_dir: data_dir.join("logs"),
            history_file: data_dir.join("history.txt"),
            data_dir,
            models_dir: cache_dir.join("models"),
            cache_dir,
        })
    }

    /// Create all dirs with 0700 for data dir. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        for d in [
            &self.data_dir,
            &self.sessions_dir,
            &self.patches_dir,
            &self.logs_dir,
            &self.cache_dir,
            &self.models_dir,
        ] {
            std::fs::create_dir_all(d).with_context(|| format!("create dir {}", d.display()))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perm = std::fs::Permissions::from_mode(0o700);
            let _ = std::fs::set_permissions(&self.data_dir, perm);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_override_is_hermetic() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("ZAI_TEST_HOME", tmp.path());
        let p = ProfilePaths::new("default").unwrap();
        assert!(p.db_file.starts_with(tmp.path()));
        p.ensure().unwrap();
        assert!(p.sessions_dir.is_dir());
        std::env::remove_var("ZAI_TEST_HOME");
        std::env::remove_var("AICLI_TEST_HOME");
    }
}
