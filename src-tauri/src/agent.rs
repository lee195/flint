//! Agent orchestration (doc 03/04): spawns a fresh `opencode serve` per run scoped to the
//! chosen workspace, drives it over HTTP, and buffers `AgentEvent`s for the polled
//! frontend. Also owns the capability smoke test (doc 01) and the pre-run snapshot hook
//! (doc 04). Permission prompts are buffered and answered via `respond_permission`;
//! unanswered asks auto-deny after 5 minutes (hackathon pattern, ported — not a modal).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use common::agent::config_content_permissive;
use common::agent::OpencodeClient;
use common::types::{AgentEvent, AgentStatusView, ModelDescriptor, SmokeResult};
use common::{agent as agentcfg, cookbook, probe, smoke, snapshot, AppError};

const AUTO_DENY_MS: i64 = 5 * 60 * 1000;
const SMOKE_TIMEOUT: Duration = Duration::from_secs(120);

pub struct PendingPermission {
    pub id: String,
    pub asked_at_ms: i64,
}

pub struct AgentState {
    pub workspace: PathBuf,
    pub running: bool,
    pub output: VecDeque<AgentEvent>,
    pub pending: Option<PendingPermission>,
    pub base_url: Option<String>,
    pub session_id: Option<String>,
    pub child: Option<tokio::process::Child>,
    pub cancel: Arc<AtomicBool>,
    /// The pre-run snapshot commit for the most recent run (review/undo).
    pub last_snapshot: Option<String>,
    pub last_reply: String,
}

static AGENT: OnceLock<Mutex<AgentState>> = OnceLock::new();

pub fn init() -> Result<(), String> {
    AGENT
        .set(Mutex::new(AgentState {
            workspace: PathBuf::new(),
            running: false,
            output: VecDeque::new(),
            pending: None,
            base_url: None,
            session_id: None,
            child: None,
            cancel: Arc::new(AtomicBool::new(false)),
            last_snapshot: None,
            last_reply: String::new(),
        }))
        .map_err(|_| "agent state already initialized".to_string())
}

pub fn state() -> MutexGuard<'static, AgentState> {
    AGENT
        .get()
        .expect("agent not initialized")
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn recommended_desc() -> Result<ModelDescriptor, AppError> {
    let p = probe::probe()?;
    let tier = p.tier.ok_or_else(|| AppError::Engine("no tier for this machine".into()))?;
    Ok(cookbook::model_for_tier(tier).clone())
}

/// Validate + store the chosen workspace (folder picker / paste / drag-drop).
pub fn set_workspace(path: String) -> Result<(), AppError> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(AppError::Engine(format!("not a directory: {path}")));
    }
    state().workspace = p;
    Ok(())
}

/// Compose the agent view: run status + workspace + the capability gate.
pub fn status() -> Result<AgentStatusView, AppError> {
    let g = state();
    let p = probe::probe()?;
    let tier_ok = smoke::tier_unlocks_agent(p.ram_gb);
    let last = smoke::load();
    let passed = last.as_ref().map(|r| r.passed).unwrap_or(false);
    let unlocked = tier_ok && passed;
    let reason = if !tier_ok {
        Some("Agent mode needs 32 GB or more RAM.".to_string())
    } else if !passed {
        Some("Run the capability smoke test to unlock agent mode.".to_string())
    } else {
        None
    };
    Ok(AgentStatusView {
        running: g.running,
        workspace: Some(g.workspace.display().to_string()).filter(|s| !s.is_empty()),
        unlocked,
        locked_reason: reason,
        last_smoke: last,
    })
}

/// Drain buffered events; auto-deny any permission unanswered for 5 minutes (async — the
/// reject reply is an HTTP call).
pub async fn drain_output() -> Vec<AgentEvent> {
    let auto = {
        let g = state();
        g.pending
            .as_ref()
            .filter(|p| now_ms() - p.asked_at_ms > AUTO_DENY_MS)
            .map(|p| (p.id.clone(), g.base_url.clone(), g.session_id.clone()))
    };
    if let Some((id, base, sid)) = auto {
        let _ = state().pending.take();
        if let (Some(base), Some(sid)) = (base, sid) {
            let _ = OpencodeClient::new(base).reply_permission(&sid, &id, "reject").await;
        }
        state().output.push_back(AgentEvent::Error {
            message: "Permission auto-denied after 5 minutes.".into(),
        });
    }
    let mut out = Vec::new();
    while let Some(e) = state().output.pop_front() {
        out.push(e);
    }
    out
}

/// Start an agent run: gate → pre-run snapshot → spawn → drive to completion.
pub async fn run_agent(prompt: String) -> Result<(), AppError> {
    {
        let g = state();
        if g.running {
            return Err(AppError::Engine("an agent run is already active".into()));
        }
        if g.workspace.as_os_str().is_empty() {
            return Err(AppError::Engine("choose a workspace first".into()));
        }
    }
    let status = status()?;
    if !status.unlocked {
        return Err(AppError::Engine(
            status.locked_reason.unwrap_or_else(|| "agent mode locked".into()),
        ));
    }
    let desc = recommended_desc()?;
    let workspace = state().workspace.clone();

    // Pre-run snapshot — non-negotiable before a file-writing run (doc 04).
    let commit = snapshot::before_run(&workspace, &format!("pre-run {}", now_ms()))?;
    {
        let mut g = state();
        g.last_snapshot = Some(commit.to_string());
        g.last_reply = String::new();
        g.output.clear();
        g.running = true;
        g.cancel.store(false, Ordering::Relaxed);
    }

    let cancel = state().cancel.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_driver(workspace, desc, prompt, cancel).await;
    });
    Ok(())
}

async fn run_driver(
    workspace: PathBuf,
    desc: ModelDescriptor,
    prompt: String,
    cancel: Arc<AtomicBool>,
) -> Result<(), AppError> {
    // The engine (llama-server) must be up before opencode starts — it targets the
    // engine's OpenAI-compatible endpoint (Phase 3b full swap).
    if let Err(e) = crate::engine_mgr().ensure_running(&desc).await {
        state().output.push_back(AgentEvent::Error { message: e.to_string() });
        finish(None);
        return Err(e);
    }
    let (base_url, child) = match spawn_server(&workspace, &desc).await {
        Ok(v) => v,
        Err(e) => {
            state().output.push_back(AgentEvent::Error { message: e.to_string() });
            finish(None);
            return Err(e);
        }
    };
    let client = OpencodeClient::new(base_url.clone());
    let sid = match client.create_session(&desc).await {
        Ok(s) => s,
        Err(e) => {
            let mut c = child;
            let _ = c.kill();
            state().output.push_back(AgentEvent::Error { message: e.to_string() });
            finish(None);
            return Err(e);
        }
    };
    {
        let mut g = state();
        g.base_url = Some(base_url);
        g.session_id = Some(sid.clone());
        g.child = Some(child);
    }
    let result = client.send_message(&sid, &prompt).await;
    let result = match result {
        Ok(()) => {
            let mut emit = |ev: AgentEvent| {
                if let AgentEvent::Permission { id, .. } = &ev {
                    state().pending = Some(PendingPermission { id: id.clone(), asked_at_ms: now_ms() });
                }
                state().output.push_back(ev);
            };
            client.run_sse(&sid, &mut emit, &cancel).await
        }
        Err(e) => Err(e),
    };
    finish(Some(result));
    Ok(())
}

/// Finalize a run: capture the reply, kill the sidecar, clear session state.
fn finish(result: Option<Result<(), AppError>>) {
    let mut g = state();
    g.running = false;
    if let Some(mut c) = g.child.take() {
        let _ = c.kill();
    }
    g.base_url = None;
    g.session_id = None;
    g.pending = None;
    if let Some(Err(e)) = result {
        // run_sse already emits an Error event on failure; avoid a duplicate.
        let dup = matches!(g.output.back(), Some(AgentEvent::Error { .. }));
        if !dup {
            g.output.push_back(AgentEvent::Error { message: e.to_string() });
        }
    }
}

/// Spawn a fresh `opencode serve` scoped to the workspace with the isolated (ask) config,
/// wait for health, and assert the permission net is in force (fail-closed, doc 03).
async fn spawn_server(
    workspace: &Path,
    desc: &ModelDescriptor,
) -> Result<(String, tokio::process::Child), AppError> {
    let bin = agentcfg::resolve_opencode_path();
    if !bin.exists() {
        return Err(AppError::Engine(format!("opencode not found at {}", bin.display())));
    }
    let port = agentcfg::pick_free_port()?;
    let config_dir = common::config::data_dir().join("opencode");
    let content = agentcfg::config_content(desc, &crate::engine_mgr().openai_base_url());
    let mut child = tokio::process::Command::new(&bin)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .env("OPENCODE_CONFIG_DIR", &config_dir)
        .env("OPENCODE_CONFIG_CONTENT", content)
        .current_dir(workspace)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Engine(format!("spawn opencode: {e}")))?;

    let base_url = format!("http://127.0.0.1:{port}");
    let client = OpencodeClient::new(base_url.clone());
    let mut healthy = false;
    for _ in 0..60 {
        if client.health().await {
            healthy = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if !healthy {
        let _ = child.kill();
        return Err(AppError::Engine("opencode didn't become healthy".into()));
    }
    let permission = client.effective_permission().await?;
    if !agentcfg::config_has_ask(&permission) {
        let _ = child.kill();
        return Err(AppError::Engine(
            "permission net not in force — agent refused (fail-closed)".into(),
        ));
    }
    Ok((base_url, child))
}

/// The capability smoke test (doc 01): a scripted write/read run in a temp dir, scored on
/// the file effect. Persists the result (the agent gate).
pub async fn run_smoke_test() -> Result<SmokeResult, AppError> {
    let desc = recommended_desc()?;
    // The smoke scores tool-calling on the real engine — bring it up first (3b swap).
    crate::engine_mgr().ensure_running(&desc).await?;
    let tmp = std::env::temp_dir().join(format!("flint-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| AppError::Io { path: tmp.clone(), source: e })?;

    let bin = agentcfg::resolve_opencode_path();
    if !bin.exists() {
        return Err(AppError::Engine(format!("opencode not found at {}", bin.display())));
    }
    let port = agentcfg::pick_free_port()?;
    let config_dir = common::config::data_dir().join("opencode");
    let content = config_content_permissive(&desc, &crate::engine_mgr().openai_base_url());
    let mut child = tokio::process::Command::new(&bin)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .env("OPENCODE_CONFIG_DIR", &config_dir)
        .env("OPENCODE_CONFIG_CONTENT", content)
        .current_dir(&tmp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Engine(format!("spawn opencode: {e}")))?;

    let base_url = format!("http://127.0.0.1:{port}");
    let client = OpencodeClient::new(base_url.clone());
    let mut healthy = false;
    for _ in 0..60 {
        if client.health().await {
            healthy = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if !healthy {
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(AppError::Engine("opencode didn't become healthy".into()));
    }

    let sid = match client.create_session(&desc).await {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    let prompt = "In this directory: create a file named result.txt containing exactly the word 'ok' (use a write or edit tool). Then read it back and report its contents.";
    let cancel = Arc::new(AtomicBool::new(false));
    let mut reply = String::new();
    let run = tokio::time::timeout(SMOKE_TIMEOUT, async {
        client.send_message(&sid, prompt).await?;
        client
            .run_sse(
                &sid,
                &mut |e| {
                    if let AgentEvent::Done { text } = e {
                        reply = text;
                    }
                },
                &cancel,
            )
            .await
    })
    .await;
    let _ = child.kill();

    let file_ok = std::fs::read_to_string(tmp.join("result.txt"))
        .map(|c| c.trim().contains("ok"))
        .unwrap_or(false);
    let detail = match &run {
        Ok(Ok(())) => format!("reply: {}", truncate(&reply, 160)),
        Ok(Err(e)) => format!("run error: {e}"),
        Err(_) => "timeout".to_string(),
    };
    let _ = std::fs::remove_dir_all(&tmp);

    let result = SmokeResult {
        passed: file_ok,
        model: desc.tag.to_string(),
        ts_ms: now_ms(),
        detail,
    };
    smoke::save(&result);
    Ok(result)
}

/// Answer a pending permission (Allow → once, Deny → reject).
pub async fn respond_permission(id: String, allow: bool) -> Result<(), AppError> {
    let (base, sid) = {
        let g = state();
        (g.base_url.clone(), g.session_id.clone())
    };
    let (Some(base), Some(sid)) = (base, sid) else {
        return Err(AppError::Engine("no active agent run".into()));
    };
    state().pending = None;
    let response = if allow { "once" } else { "reject" };
    OpencodeClient::new(base).reply_permission(&sid, &id, response).await
}

pub async fn cancel_agent() -> Result<(), AppError> {
    state().cancel.store(true, Ordering::Relaxed);
    let (base, sid) = {
        let g = state();
        (g.base_url.clone(), g.session_id.clone())
    };
    if let (Some(base), Some(sid)) = (base, sid) {
        let _ = OpencodeClient::new(base).abort(&sid).await;
    }
    Ok(())
}

/// Restore the workspace to the most recent run's pre-run snapshot (review/undo).
pub fn restore_snapshot() -> Result<(), AppError> {
    let (workspace, commit) = {
        let g = state();
        (
            g.workspace.clone(),
            g.last_snapshot
                .clone()
                .ok_or_else(|| AppError::Engine("no snapshot for this workspace".into()))?,
        )
    };
    snapshot::restore(&workspace, &commit)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// Defense-in-depth plugin (doc 03): `tool.execute.before` throws for file-tool args that
/// resolve outside the workspace root (the serve cwd). Layer 1 (permission "ask") is the
/// primary gate; if this plugin fails to load, layer 1 still gates everything — the
/// layers fail independently.
const CONFINEMENT_PLUGIN: &str = r#"// Flint path confinement — defense-in-depth (layer 2).
// Throws for file-tool args that resolve outside the workspace root (the serve cwd),
// so even a "allowed" tool call can't escape the chosen folder.
const ROOT = process.cwd();

function isPathish(v) {
  return typeof v === "string" && (v.startsWith("/") || v.includes(".."));
}

function collectPaths(value, out) {
  if (Array.isArray(value)) {
    for (const x of value) collectPaths(x, out);
  } else if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) {
      if (["filePath", "file_path", "path", "cwd", "directory", "root"].includes(k) && isPathish(v)) {
        out.push(v);
      }
      collectPaths(v, out);
    }
  }
  return out;
}

function withinRoot(p) {
  const abs = p.startsWith("/") ? p : ROOT + "/" + p;
  return abs === ROOT || abs.startsWith(ROOT + "/");
}

export const flintConfinement = {
  name: "flint-confinement",
  "tool.execute.before": async (input, output) => {
    const tool = input.tool;
    const fileTools = ["read", "write", "edit", "patch", "grep", "glob", "list"];
    if (fileTools.includes(tool)) {
      const args = output.args ?? {};
      for (const p of collectPaths(args, [])) {
        if (!withinRoot(p)) {
          throw new Error("path outside workspace blocked by Flint: " + p);
        }
      }
    }
  },
};
"#;

/// Write the confinement plugin into `~/.flint/opencode/plugin/` (idempotent). Called on
/// startup — if the write fails, layer 1 still gates (fail-open for the net is NOT
/// possible: `"*": "ask"` is asserted at spawn).
pub fn ensure_confinement_plugin() -> Result<(), AppError> {
    let dir = common::config::data_dir().join("opencode").join("plugin");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io { path: dir.clone(), source: e })?;
    let path = dir.join("flint-confinement.js");
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, CONFINEMENT_PLUGIN).map_err(|e| AppError::Io { path, source: e })?;
    Ok(())
}
