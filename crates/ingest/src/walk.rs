use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".cache",
    "__pycache__",
];

#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub exts: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub include_hidden: bool,
    pub max_file_mb: u64,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            exts: vec![],
            exclude_globs: vec![
                "target/**".to_string(),
                ".git/**".to_string(),
                "node_modules/**".to_string(),
            ],
            include_hidden: false,
            max_file_mb: 5,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WalkReport {
    pub files: Vec<PathBuf>,
    pub skipped_large: Vec<String>,
    pub skipped_binary: Vec<String>,
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

fn should_skip_dir(name: &str, include_hidden: bool) -> bool {
    if !include_hidden && is_hidden_name(name) {
        return true;
    }
    SKIP_DIRS.contains(&name)
}

fn excluded_by_glob(path: &Path, globs: &[String]) -> bool {
    // Minimal glob: supports prefix `dir/**` and suffix `*.ext`.
    // Full glob engine arrives with config exclude upgrade in Phase 6.
    let s = path.to_string_lossy().replace('\\', "/");
    for g in globs {
        if let Some(prefix) = g.strip_suffix("/**") {
            let p = prefix.trim_start_matches("./");
            if s.contains(&format!("/{p}/")) || s.starts_with(&format!("{p}/")) || s == *p {
                return true;
            }
        } else if g.starts_with("*.") {
            if s.ends_with(&g[1..]) {
                return true;
            }
        } else if s.contains(g.as_str()) {
            return true;
        }
    }
    false
}

/// Collect files under root with deterministic sorted output.
pub fn collect_files(root: &Path, opts: &WalkOptions) -> WalkReport {
    if root.is_file() {
        return WalkReport {
            files: vec![root.to_path_buf()],
            ..Default::default()
        };
    }
    let exts_lower: Vec<String> = opts.exts.iter().map(|e| e.to_lowercase()).collect();
    let mut files = Vec::new();
    let mut skipped_large = Vec::new();

    let walker = ignore::WalkBuilder::new(root)
        .hidden(!opts.include_hidden)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .build();

    for entry in walker.flatten() {
        let path = entry.path().to_path_buf();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if entry.depth() == 0 {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if should_skip_dir(name, opts.include_hidden) && !opts.include_hidden {
                    continue;
                }
            }
            continue;
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        if !opts.include_hidden {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_hidden_name(name) {
                    continue;
                }
            }
        }
        if excluded_by_glob(&path, &opts.exclude_globs) {
            continue;
        }
        if !exts_lower.is_empty() {
            let ok = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| exts_lower.contains(&e.to_lowercase()))
                .unwrap_or(false);
            if !ok {
                continue;
            }
        }
        if opts.max_file_mb > 0 {
            if let Ok(md) = std::fs::metadata(&path) {
                if md.len() > opts.max_file_mb * 1024 * 1024 {
                    skipped_large.push(path.to_string_lossy().to_string());
                    continue;
                }
            }
        }
        files.push(path);
    }

    files.sort();
    WalkReport {
        files,
        skipped_large,
        skipped_binary: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn walker_respects_ext_and_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src").join("a.rs"), "fn a() {}").unwrap();
        fs::write(root.join("src").join("b.txt"), "hi").unwrap();
        fs::write(root.join("target").join("x.rs"), "skip").unwrap();
        fs::write(root.join(".gitignore"), "src/b.txt\n").unwrap();

        let opts = WalkOptions {
            exts: vec!["rs".to_string()],
            ..Default::default()
        };
        let rep = collect_files(root, &opts);
        let names: Vec<String> = rep
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.rs".to_string()]);
    }

    #[test]
    fn single_file_passthrough() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("one.log");
        std::fs::write(&f, "x").unwrap();
        let rep = collect_files(&f, &WalkOptions::default());
        assert_eq!(rep.files.len(), 1);
    }
}
