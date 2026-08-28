//! Engine lifecycle for the llama.cpp swap (Phase 3b): a long-lived `llama-server`
//! sidecar spawned with the installed model, killed on quit. The `EngineBackend` is
//! constructed against the server's port, and the same OpenAI-compatible base URL is
//! what opencode's provider targets for agent mode. Full swap (per session decision):
//! this is THE engine; Ollama remains only as a dev fallback (not wired here).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::agent::pick_free_port;
use common::engine::{EngineBackend, LlamaCppBackend};
use common::types::{EngineHealth, ModelDescriptor};
use common::{config, AppError};

pub struct EngineManager {
    pub port: u16,
    pub backend: Arc<dyn EngineBackend>,
    child: Mutex<Option<tokio::process::Child>>,
}

impl EngineManager {
    pub fn new() -> Result<Arc<Self>, AppError> {
        let port = pick_free_port()?;
        let backend: Arc<dyn EngineBackend> =
            Arc::new(LlamaCppBackend::new(format!("http://127.0.0.1:{port}")));
        Ok(Arc::new(Self { port, backend, child: Mutex::new(None) }))
    }

    /// The OpenAI-compatible endpoint opencode's provider points at (agent mode).
    pub fn openai_base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    fn resolve_server_bin() -> PathBuf {
        // Release: bundled externalBin sits beside the app binary (3a-M2 recipe).
        // Dev: `deno task fetch-llama-server` installs to ~/.flint/bin/.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("llama-server");
                if candidate.exists() {
                    return candidate;
                }
            }
        }
        config::bin_dir().join("llama-server")
    }

    /// Spawn (or reuse) the llama-server with `desc`'s model loaded, waiting for health.
    /// Absolute paths only (doc 05) — no PATH lookups.
    pub async fn ensure_running(&self, desc: &ModelDescriptor) -> Result<(), AppError> {
        // Already healthy? (No MutexGuard held across the health await — std guards are
        // !Send, which would make this future unspawnable.)
        let healthy = self.backend.health().await? != EngineHealth::NotRunning;
        let alive = self
            .child
            .lock()
            .unwrap()
            .as_mut()
            .map(|c| c.try_wait().map(|s| s.is_none()).unwrap_or(false))
            .unwrap_or(false);
        if alive && healthy {
            return Ok(());
        }
        self.stop();

        let model_path = config::models_dir().join(desc.hf_file);
        if !model_path.exists() {
            return Err(AppError::Engine(format!(
                "model not installed: {}",
                desc.hf_file
            )));
        }
        let bin = Self::resolve_server_bin();
        if !bin.exists() {
            return Err(AppError::Engine(format!(
                "llama-server not found at {} — run `deno task fetch-llama-server`",
                bin.display()
            )));
        }
        let child = tokio::process::Command::new(&bin)
            .arg("--model")
            .arg(&model_path)
            .arg("--port")
            .arg(self.port.to_string())
            .arg("--ctx-size")
            .arg(desc.num_ctx.to_string())
            .arg("--n-gpu-layers")
            .arg("999")
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--no-webui")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AppError::Engine(format!("spawn llama-server: {e}")))?;
        *self.child.lock().unwrap() = Some(child);

        for _ in 0..120 {
            if self.backend.health().await? != EngineHealth::NotRunning {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        self.stop();
        Err(AppError::Engine("llama-server didn't become healthy".into()))
    }

    /// Kill the sidecar (on quit, on model switch, on delete). `start_kill` is sync so
    /// the exit handler can call this; the kernel reaps the dead process.
    pub fn stop(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dogfood of the exact spawn path — the app's engine startup, headlessly.
    /// Requires the recommended GGUF installed + llama-server in ~/.flint/bin.
    /// Run: `cargo test -p flint real_engine_spawn -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "requires the recommended GGUF + llama-server installed"]
    async fn real_engine_spawn() {
        let p = common::probe::probe().expect("probe");
        let desc = p
            .tier
            .map(common::cookbook::model_for_tier)
            .expect("tier for this machine")
            .clone();
        let mgr = EngineManager::new().expect("manager");
        mgr.ensure_running(&desc).await.expect("engine should come up");
        let health = mgr.backend.health().await.expect("health");
        assert!(matches!(health, common::types::EngineHealth::Running { .. }));
        println!("engine up: {health:?} on port {}", mgr.port);
        mgr.stop();
    }
}
