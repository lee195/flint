//! Shared crate for Flint — no Tauri deps. The hardware probe, the cookbook tier table,
//! the engine backend trait (+ the v0 Ollama implementation), and the canonical data-dir
//! owner live here so both the app and any future sidecar share one vocabulary.

pub mod agent;
pub mod chat_store;
pub mod config;
pub mod cookbook;
pub mod engine;
pub mod error;
pub mod probe;
pub mod smoke;
pub mod snapshot;
pub mod types;

pub use error::AppError;
