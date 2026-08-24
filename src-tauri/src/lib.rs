use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use common::cookbook;
use common::engine::{EngineBackend, OllamaBackend};
use common::probe;
use common::types::{EngineStatusView, InstallState, ProbeResult, PullProgress, Recommendation};
use common::{config, AppError};

/// The engine, created once in `setup()`. Ollama is the v0 backend behind the
/// `EngineBackend` trait (llama.cpp becomes a second impl in Phase 2).
static ENGINE: OnceLock<OllamaBackend> = OnceLock::new();

/// Shared install lifecycle: the background pull task writes it, the frontend polls it.
static INSTALL: OnceLock<Mutex<InstallState>> = OnceLock::new();

/// Cancellation flag for the in-flight install (checked between pull chunks in `common`).
static CANCEL: AtomicBool = AtomicBool::new(false);

fn engine() -> &'static OllamaBackend {
    ENGINE.get().expect("engine not initialized")
}

fn install_state() -> MutexGuard<'static, InstallState> {
    INSTALL
        .get()
        .expect("install state not initialized")
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Hardware probe (local only — no engine, no network). The honesty screen + tier gate.
#[tauri::command]
fn probe_machine() -> Result<ProbeResult, AppError> {
    probe::probe()
}

/// The single recommendation for this machine (probe + cookbook + install state).
#[tauri::command]
async fn get_recommendation() -> Result<Recommendation, AppError> {
    let p = probe::probe()?;
    let models = engine().list_models().await?;
    let tags: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
    Ok(cookbook::recommendation(&p, &tags).unwrap_or(Recommendation {
        probe: p,
        model: None,
        as_of: cookbook::COOKBOOK_AS_OF,
        already_installed: false,
    }))
}

/// Engine reachability + on-disk models + whether the recommended model is installed.
#[tauri::command]
async fn engine_status() -> Result<EngineStatusView, AppError> {
    let health = engine().health().await?;
    let models = engine().list_models().await?;
    let recommended_installed = probe::probe()
        .ok()
        .and_then(|p| p.tier)
        .map(cookbook::model_for_tier)
        .is_some_and(|d| models.iter().any(|m| m.name == d.tag));
    Ok(EngineStatusView {
        health,
        models,
        recommended_installed,
    })
}

/// Consent-gated install (doc 01/02): kicks off the pull in the background; progress is
/// polled via `get_install_progress`. Resumable by Ollama; cancelled installs abort early.
#[tauri::command]
async fn start_install(model: String) -> Result<(), AppError> {
    let desc = cookbook::find_by_tag(&model)
        .ok_or_else(|| AppError::Engine(format!("unknown model: {model}")))?;
    *install_state() = InstallState::Running {
        model: model.clone(),
        progress: PullProgress { status: "starting".into(), completed: 0, total: 0, percent: 0.0 },
    };
    CANCEL.store(false, Ordering::Relaxed);
    let desc = desc.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = {
            let mut update = |p: PullProgress| {
                if !CANCEL.load(Ordering::Relaxed) {
                    *install_state() = InstallState::Running { model: model.clone(), progress: p };
                }
            };
            engine().ensure_model(&desc, &mut update, &CANCEL).await
        };
        let cancelled = CANCEL.load(Ordering::Relaxed);
        *install_state() = match (cancelled, outcome) {
            (true, _) => InstallState::Cancelled { model },
            (false, Ok(())) => InstallState::Done { model },
            (false, Err(e)) => InstallState::Failed { model, error: e.to_string() },
        };
    });
    Ok(())
}

/// Polled by the frontend (~500 ms) while installing.
#[tauri::command]
async fn get_install_progress() -> Result<InstallState, AppError> {
    Ok(install_state().clone())
}

/// Cancel the in-flight install (abandons the task; the pull is resumable by Ollama).
#[tauri::command]
async fn cancel_install() -> Result<(), AppError> {
    CANCEL.store(true, Ordering::Relaxed);
    Ok(())
}

/// Remove a model from the engine's store (settings delete affordance).
#[tauri::command]
async fn delete_model(tag: String) -> Result<(), AppError> {
    engine().delete_model(&tag).await
}

/// Launch Ollama.app. `/usr/bin/open` is on the minimal launchd PATH (doc 05's
/// absolute-paths rule — no PATH lookups anywhere).
#[tauri::command]
fn launch_ollama() -> Result<(), AppError> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg("Ollama")
        .status()
        .map_err(|e| AppError::Engine(format!("launch Ollama: {e}")))?;
    if !status.success() {
        return Err(AppError::Engine(
            "couldn't open Ollama — is Ollama.app installed? (Phase 2 bundles llama.cpp instead)".into(),
        ));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            let _ = std::fs::create_dir_all(config::data_dir());
            ENGINE
                .set(OllamaBackend::new("http://127.0.0.1:11434".to_string()))
                .map_err(|_| tauri::Error::Anyhow(anyhow::anyhow!("engine already initialized")))?;
            INSTALL
                .set(Mutex::new(InstallState::Idle))
                .map_err(|_| tauri::Error::Anyhow(anyhow::anyhow!("install state already initialized")))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            probe_machine,
            get_recommendation,
            engine_status,
            start_install,
            get_install_progress,
            cancel_install,
            delete_model,
            launch_ollama,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
