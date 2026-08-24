//! App-wide error type. Kept in `common` so both the app and any future sidecar
//! (llama.cpp/opencode wrappers, Phase 2) share one error vocabulary.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("engine error: {0}")]
    Engine(String),

    #[error("unknown: {0}")]
    Unknown(String),
}

/// Serializes as the Display message so Tauri commands can return `Result<T, AppError>`
/// and the frontend receives a plain-language error string (never an internal type shape).
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
