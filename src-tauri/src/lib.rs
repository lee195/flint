mod agent;
mod engine_mgr;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use common::chat_store;
use common::cookbook;
use common::engine::EngineBackend;
use common::probe;
use common::types::{
    AgentEvent, AgentStatusView, ChatEvent, ChatHistoryView, ChatMessage, EngineStatusView,
    InstallState, ProbeResult, PullProgress, Recommendation, SmokeResult,
};
use common::{config, AppError};

/// The engine manager, created once in `setup()`. Phase 3b full swap: it owns the
/// llama-server sidecar and the `EngineBackend` (Ollama is the dev fallback only).
static ENGINE_MGR: OnceLock<Arc<engine_mgr::EngineManager>> = OnceLock::new();

/// Shared install lifecycle: the background download task writes it, the frontend polls it.
static INSTALL: OnceLock<Mutex<InstallState>> = OnceLock::new();

/// Cancellation flag for the in-flight install (checked between chunks in `common`).
static CANCEL: AtomicBool = AtomicBool::new(false);

/// The single active chat: message history + a streamed-event buffer the frontend polls.
struct ChatState {
    messages: Vec<ChatMessage>,
    output: VecDeque<ChatEvent>,
    streaming: bool,
}

static CHAT: OnceLock<Mutex<ChatState>> = OnceLock::new();

/// Cancellation flag for the in-flight chat turn (checked between chunks in `common`).
static CANCEL_CHAT: AtomicBool = AtomicBool::new(false);

fn engine_mgr() -> &'static engine_mgr::EngineManager {
    ENGINE_MGR.get().expect("engine manager not initialized")
}

fn engine() -> &'static dyn EngineBackend {
    engine_mgr().backend.as_ref()
}

fn install_state() -> MutexGuard<'static, InstallState> {
    INSTALL
        .get()
        .expect("install state not initialized")
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn chat_state() -> MutexGuard<'static, ChatState> {
    CHAT
        .get()
        .expect("chat state not initialized")
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The recommended model tag for this machine (chat runs on the recommendation, doc 01).
fn recommended_model_tag() -> Option<String> {
    probe::probe()
        .ok()
        .and_then(|p| p.tier)
        .map(cookbook::model_for_tier)
        .map(|d| d.tag.to_string())
}

/// The recommended model descriptor for this machine.
fn recommended_desc() -> Result<common::types::ModelDescriptor, AppError> {
    let tag = recommended_model_tag()
        .ok_or_else(|| AppError::Engine("no model for this machine".into()))?;
    cookbook::find_by_tag(&tag)
        .cloned()
        .ok_or_else(|| AppError::Engine(format!("unknown model: {tag}")))
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
        .is_some_and(|d| models.iter().any(|m| m.name == d.hf_file));
    Ok(EngineStatusView {
        health,
        models,
        recommended_installed,
    })
}

/// Consent-gated install (doc 01/02): kicks off the download in the background; progress is
/// polled via `get_install_progress`. Resumable (HTTP Range) + sha256-verified; cancelled
/// installs abort early. On success the engine is started with the new model.
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
        // Model on disk → bring the engine up with it (a failure here still counts as
        // installed; the status pill shows amber + Start engine).
        if outcome.is_ok() {
            let _ = engine_mgr().ensure_running(&desc).await;
        }
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

/// Remove a model from the engine's store (settings delete affordance). Stops the
/// sidecar first — the next start re-spawns with whatever model remains.
#[tauri::command]
async fn delete_model(tag: String) -> Result<(), AppError> {
    let res = engine().delete_model(&tag).await;
    engine_mgr().stop();
    res
}

/// Start the bundled engine (llama-server) with the recommended model, if installed.
#[tauri::command]
async fn start_engine() -> Result<(), AppError> {
    let desc = recommended_desc()?;
    engine_mgr().ensure_running(&desc).await
}

/// The chat pane's starting state: persisted history + the model it runs on. Loads the
/// store on first call in a session (single conversation, doc 04).
#[tauri::command]
async fn get_chat() -> Result<ChatHistoryView, AppError> {
    let model = recommended_model_tag();
    let mut g = chat_state();
    if g.messages.is_empty() {
        g.messages = chat_store::load_messages();
    }
    Ok(ChatHistoryView { messages: g.messages.clone(), model })
}

/// Send a user message and start the streaming reply. Appends + persists the user
/// message; the assistant reply is streamed into the buffer and persisted on completion.
#[tauri::command]
async fn send_chat(message: String) -> Result<(), AppError> {
    let model = recommended_model_tag()
        .ok_or_else(|| AppError::Engine("no model for this machine".into()))?;
    let desc = cookbook::find_by_tag(&model)
        .ok_or_else(|| AppError::Engine(format!("unknown model: {model}")))?;
    let user_msg = ChatMessage { role: "user".into(), content: message, ts: now_ms() };
    {
        let mut g = chat_state();
        if g.streaming {
            return Err(AppError::Engine("a reply is already streaming — wait or cancel".into()));
        }
        g.messages.push(user_msg.clone());
        g.streaming = true;
    }
    chat_store::append_message(&user_msg);
    CANCEL_CHAT.store(false, Ordering::Relaxed);
    let desc = desc.clone();
    tauri::async_runtime::spawn(async move {
        // Clone the history so the request doesn't hold the lock for the whole stream
        // (the emit callback needs it per-event).
        let history = chat_state().messages.clone();
        let result = {
            let mut emit = |e: ChatEvent| {
                if !CANCEL_CHAT.load(Ordering::Relaxed) {
                    chat_state().output.push_back(e);
                }
            };
            if let Err(e) = engine_mgr().ensure_running(&desc).await {
                Err(e)
            } else {
                engine().chat(&desc, &history, &mut emit, &CANCEL_CHAT).await
            }
        };
        let mut g = chat_state();
        g.streaming = false;
        match (CANCEL_CHAT.load(Ordering::Relaxed), result) {
            (_, Err(e)) => {
                g.output.push_back(ChatEvent::Error { message: e.to_string() });
            }
            (_, Ok(reply)) => {
                // Keep the partial on cancel too — honest: the user watched it stop.
                if !reply.is_empty() {
                    let m = ChatMessage { role: "assistant".into(), content: reply.clone(), ts: now_ms() };
                    g.messages.push(m.clone());
                    chat_store::append_message(&m);
                }
                g.output.push_back(ChatEvent::Done { full: reply });
            }
        }
    });
    Ok(())
}

/// Drained by the frontend poll (~150 ms) while streaming.
#[tauri::command]
async fn poll_chat_output() -> Result<Vec<ChatEvent>, AppError> {
    let mut g = chat_state();
    let mut out = Vec::new();
    while let Some(e) = g.output.pop_front() {
        out.push(e);
    }
    Ok(out)
}

#[tauri::command]
async fn cancel_chat() -> Result<(), AppError> {
    CANCEL_CHAT.store(true, Ordering::Relaxed);
    Ok(())
}

/// Clear the conversation (New chat) — truncates `current.jsonl`.
#[tauri::command]
async fn new_chat() -> Result<(), AppError> {
    let mut g = chat_state();
    if g.streaming {
        return Err(AppError::Engine("wait for the current reply before starting a new chat".into()));
    }
    g.messages.clear();
    g.output.clear();
    chat_store::clear_conversation();
    Ok(())
}

// ============================ Agent (Phase 2) ============================

/// Choose the folder the agent may work in (folder picker / paste-path / drag-drop).
#[tauri::command]
fn set_workspace(path: String) -> Result<(), AppError> {
    agent::set_workspace(path)
}

/// Agent view state: run status, workspace, and the capability gate (tier + smoke).
#[tauri::command]
fn get_agent_status() -> Result<AgentStatusView, AppError> {
    agent::status()
}

/// Start an agent run (gated by tier floor + passed smoke test; pre-run snapshot).
#[tauri::command]
async fn run_agent(prompt: String) -> Result<(), AppError> {
    agent::run_agent(prompt).await
}

/// Drain buffered agent events (poll ~150 ms while running; auto-denies stale permissions).
#[tauri::command]
async fn poll_agent_output() -> Result<Vec<AgentEvent>, AppError> {
    Ok(agent::drain_output().await)
}

/// Answer a permission banner (Allow → once, Deny → reject).
#[tauri::command]
async fn respond_permission(id: String, allow: bool) -> Result<(), AppError> {
    agent::respond_permission(id, allow).await
}

#[tauri::command]
async fn cancel_agent() -> Result<(), AppError> {
    agent::cancel_agent().await
}

/// Run the capability smoke test (doc 01) and persist the result.
#[tauri::command]
async fn run_smoke_test() -> Result<SmokeResult, AppError> {
    agent::run_smoke_test().await
}

/// Restore the workspace to the pre-run snapshot (review/undo).
#[tauri::command]
fn restore_snapshot() -> Result<(), AppError> {
    agent::restore_snapshot()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            let _ = std::fs::create_dir_all(config::data_dir());
            let _ = std::fs::create_dir_all(config::models_dir());
            agent::init().map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
            let _ = agent::ensure_confinement_plugin();
            ENGINE_MGR
                .set(engine_mgr::EngineManager::new()?)
                .map_err(|_| tauri::Error::Anyhow(anyhow::anyhow!("engine manager already initialized")))?;
            INSTALL
                .set(Mutex::new(InstallState::Idle))
                .map_err(|_| tauri::Error::Anyhow(anyhow::anyhow!("install state already initialized")))?;
            CHAT
                .set(Mutex::new(ChatState {
                    messages: vec![],
                    output: VecDeque::new(),
                    streaming: false,
                }))
                .map_err(|_| tauri::Error::Anyhow(anyhow::anyhow!("chat state already initialized")))?;
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
            start_engine,
            get_chat,
            send_chat,
            poll_chat_output,
            cancel_chat,
            new_chat,
            set_workspace,
            get_agent_status,
            run_agent,
            poll_agent_output,
            respond_permission,
            cancel_agent,
            run_smoke_test,
            restore_snapshot,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        // Kill the llama-server sidecar on quit (Phase 3b lifecycle).
        if let tauri::RunEvent::Exit = event {
            if let Some(mgr) = ENGINE_MGR.get() {
                mgr.stop();
            }
        }
    });
}
