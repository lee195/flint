//! Canonical data-dir owner. Rule (design/STATE.md + PLAN.md): `~/.flint/` is
//! resolved by EXACTLY ONE function and nothing else — never Tauri's `app_data_dir()`
//! (the canonical-dir lesson: two paths = zero shared data).

use std::path::PathBuf;

/// `~/.flint/` — the single data dir for Flint (models, chat transcripts, config).
pub fn data_dir() -> PathBuf {
    home_dir().join(".flint")
}

/// `~/.flint/models/` — the GGUF model store for the llama.cpp engine (Phase 3b).
/// Downloader writes `<file>.gguf.part` here and renames on a passing digest check.
pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// `~/.flint/bin/` — dev location for sidecar binaries (llama-server, fetched by
/// `deno task fetch-llama-server`). Release bundles them as Tauri externalBins (3a-M2).
pub fn bin_dir() -> PathBuf {
    data_dir().join("bin")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
