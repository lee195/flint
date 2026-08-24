//! Shared crate for Flint — no Tauri deps (serde only). The future home of the
//! hardware probe, the cookbook tier table, and the engine backend trait (Phase 0).
//! Pre-Phase 0 ships only the error type, the canonical data-dir owner, and a types
//! placeholder (per flint-design/PLAN.md).

pub mod config;
pub mod error;
pub mod types;

pub use error::AppError;
