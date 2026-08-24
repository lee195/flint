//! Canonical data-dir owner. Rule (flint-design/STATE.md + PLAN.md): `~/.flint/` is
//! resolved by EXACTLY ONE function and nothing else — never Tauri's `app_data_dir()`
//! (single-brain-cell's canonical-dir lesson: two paths = zero shared data).

use std::path::PathBuf;

/// `~/.flint/` — the single data dir for Flint (models, chat transcripts, config).
pub fn data_dir() -> PathBuf {
    home_dir().join(".flint")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
