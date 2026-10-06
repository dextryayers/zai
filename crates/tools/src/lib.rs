pub mod events;
pub mod fs;
pub mod gate;
pub mod git;
pub mod patch;
pub mod search;
pub mod shell;

pub use events::log_event;
pub use fs::{fs_list, fs_read, resolve_inside};
pub use gate::{check_shell, check_shell_full, GateDecision};
pub use git::{git_diff, git_status};
pub use patch::{apply_patch_set, parse_patch, validate_patch, word_diff};
pub use search::{search_files, SearchHit};
pub use shell::run_blocking;
