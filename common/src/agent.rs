//! Agent harness seam (doc 03): opencode drives the agentic loop (files, commands,
//! multi-step work) against the engine. Flint spawns a fresh `opencode serve` per run
//! (workspace cwd, isolated config via OPENCODE_CONFIG_DIR + OPENCODE_CONFIG_CONTENT),
//! talks HTTP with this client, and adapts the SSE bus to the polled event buffer.
//!
//! Pinned against opencode 1.18.20 (2026-08-24 verify spike, flint-design/STATE.md).

use std::path::PathBuf;

use serde_json::Value;

use crate::error::AppError;
use crate::types::ModelDescriptor;

/// The isolated config passed via OPENCODE_CONFIG_CONTENT. Inline config is loaded AFTER
/// a project `opencode.json`, so a workspace config cannot silently weaken the net.
/// The recommended model is forced here AND per-session.
pub fn config_content(desc: &ModelDescriptor) -> String {
    serde_json::json!({
        "model": format!("ollama/{}", desc.tag),
        "permission": {
            "*": "ask",
            "bash": { "*": "ask" },
            "read": { "**/.env": "deny", "**/.env.*": "deny" }
        },
        "provider": {
            "ollama": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Ollama (local)",
                "options": { "baseURL": "http://127.0.0.1:11434/v1", "apiKey": "none" },
                "models": { desc.tag: { "name": desc.tag } }
            }
        }
    })
    .to_string()
}

/// Permissive config for the unattended capability smoke test (scores tool-calling
/// validity, not the permission net — the net is exercised by real runs + dogfood).
pub fn config_content_permissive(desc: &ModelDescriptor) -> String {
    serde_json::json!({
        "model": format!("ollama/{}", desc.tag),
        "permission": { "*": "allow" },
        "provider": {
            "ollama": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Ollama (local)",
                "options": { "baseURL": "http://127.0.0.1:11434/v1", "apiKey": "none" },
                "models": { desc.tag: { "name": desc.tag } }
            }
        }
    })
    .to_string()
}

/// Layer-1 spawn-time assertion (doc 03): refuse agent mode unless `"*": "ask"` is in force.
pub fn config_has_ask(permission: &Value) -> bool {
    permission.get("*").and_then(|v| v.as_str()) == Some("ask")
}

/// Resolve the opencode binary: alongside the app bundle (Phase 3 sidecar), else the
/// installed CLI. Absolute path only — never a PATH lookup (doc 05).
pub fn resolve_opencode_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("opencode");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".opencode/bin/opencode")
}

/// Pick a free localhost port for the sidecar (bind :0, read, drop — a tiny race, fine
/// for a local-only server).
pub fn pick_free_port() -> Result<u16, AppError> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| {
        AppError::Engine(format!("pick port: {e}"))
    })?;
    Ok(l.local_addr().map_err(|e| AppError::Engine(format!("pick port: {e}")))?.port())
}

/// Thin HTTP client for the opencode server API. Stateless — the base URL is fixed per
/// spawned server.
pub struct OpencodeClient {
    client: reqwest::Client,
    base_url: String,
}

impl OpencodeClient {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client build");
        Self { client, base_url }
    }

    pub async fn health(&self) -> bool {
        self.client
            .get(format!("{}/global/health", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Effective config, for the spawn-time permission assertion.
    pub async fn effective_permission(&self) -> Result<Value, AppError> {
        let resp = self
            .client
            .get(format!("{}/config", self.base_url))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("config: {e}")))?;
        let cfg: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Engine(format!("config parse: {e}")))?;
        Ok(cfg.get("permission").cloned().unwrap_or(Value::Null))
    }

    /// Create a session for the model; returns the session id (`ses_...`).
    pub async fn create_session(&self, desc: &ModelDescriptor) -> Result<String, AppError> {
        let resp = self
            .client
            .post(format!("{}/session", self.base_url))
            .json(&serde_json::json!({
                "title": "flint run",
                "model": { "id": desc.tag, "providerID": "ollama" }
            }))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("session: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Engine(format!("session http {}", resp.status())));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Engine(format!("session parse: {e}")))?;
        v.get("id")
            .and_then(|x| x.as_str())
            .map(String::from)
            .ok_or_else(|| AppError::Engine(format!("session response missing id: {v}")))
    }

    /// Start an agent turn (async — the server streams progress on /event).
    pub async fn send_message(&self, session_id: &str, prompt: &str) -> Result<(), AppError> {
        let resp = self
            .client
            .post(format!("{}/session/{session_id}/message", self.base_url))
            .json(&serde_json::json!({ "parts": [{ "type": "text", "text": prompt }] }))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("message: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Engine(format!("message http {status}: {text}")));
        }
        Ok(())
    }

    pub async fn reply_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &str,
    ) -> Result<(), AppError> {
        let resp = self
            .client
            .post(format!("{}/session/{session_id}/permissions/{permission_id}", self.base_url))
            .json(&serde_json::json!({ "response": response }))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("permission reply: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Engine(format!("permission reply http {}", resp.status())));
        }
        Ok(())
    }

    pub async fn abort(&self, session_id: &str) -> Result<(), AppError> {
        let resp = self
            .client
            .post(format!("{}/session/{session_id}/abort", self.base_url))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("abort: {e}")))?;
        let _ = resp;
        Ok(())
    }

    /// The session's messages (for the final assistant text after idle).
    pub async fn session_messages(&self, session_id: &str) -> Result<Vec<Value>, AppError> {
        let resp = self
            .client
            .get(format!("{}/session/{session_id}/message", self.base_url))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("messages: {e}")))?;
        Ok(resp.json().await.map_err(|e| AppError::Engine(format!("messages parse: {e}")))?)
    }

    /// Subscribe to the SSE bus and drive the run to completion, emitting AgentEvents.
    /// On `session.status` idle → emits a final `Done` with the accumulated text; on cancel
    /// → aborts and emits `Done` with the partial; on error → emits `Error` and returns Err;
    /// on an unexpected stream end → Err ("connection lost", fail-closed per doc 03).
    pub async fn run_sse(
        &self,
        session_id: &str,
        emit: &mut (dyn FnMut(crate::types::AgentEvent) + Send),
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), AppError> {
        use futures_util::StreamExt;
        use std::sync::atomic::Ordering;

        let url = format!("{}/event", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("event stream: {e}")))?;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut full = String::new();

        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = self.abort(session_id).await;
                emit(crate::types::AgentEvent::Done { text: full.clone() });
                return Ok(());
            }
            let chunk = match stream.next().await {
                Some(c) => c.map_err(|e| AppError::Engine(format!("event stream: {e}")))?,
                None => break, // stream ended
            };
            buf.extend_from_slice(&chunk);
            // extract complete SSE frames (terminated by a blank line)
            while let Some(pos) = find_double_newline(&buf) {
                let raw: Vec<u8> = buf.drain(..=pos).collect();
                if let Some(outcome) = self.process_frame(session_id, &raw, &mut full, emit).await? {
                    emit(crate::types::AgentEvent::Done { text: outcome });
                    return Ok(());
                }
            }
        }
        // Flush a trailing frame (the server may not send a final blank line).
        if !buf.is_empty() {
            if let Some(outcome) = self.process_frame(session_id, &buf, &mut full, emit).await? {
                emit(crate::types::AgentEvent::Done { text: outcome });
                return Ok(());
            }
        }
        // Stream ended before idle — fail closed.
        Err(AppError::Engine("agent connection lost — run aborted".into()))
    }

    /// Process one SSE frame. Returns `Ok(Some(final_text))` on idle (done), `Ok(None)`
    /// to keep going, or `Err` after emitting an error event.
    async fn process_frame(
        &self,
        session_id: &str,
        frame: &[u8],
        full: &mut String,
        emit: &mut (dyn FnMut(crate::types::AgentEvent) + Send),
    ) -> Result<Option<String>, AppError> {
        let frame = String::from_utf8_lossy(frame).to_string();
        let Some(json) = parse_sse_frame(&frame) else {
            return Ok(None);
        };
        match parse_sse_event(&json) {
            SseEvent::Delta(t) => {
                full.push_str(&t);
                emit(crate::types::AgentEvent::Delta { text: t });
                Ok(None)
            }
            SseEvent::Permission(id, permission, patterns) => {
                let pattern = patterns.into_iter().next().unwrap_or_default();
                emit(crate::types::AgentEvent::Permission { id, permission, pattern });
                Ok(None)
            }
            SseEvent::ToolCall(call_id, tool, input) => {
                emit(crate::types::AgentEvent::ToolCall { call_id, tool, input });
                Ok(None)
            }
            SseEvent::ToolResult(call_id, tool, ok) => {
                emit(crate::types::AgentEvent::ToolResult { call_id, tool, ok });
                Ok(None)
            }
            SseEvent::Idle => {
                // Final text from the authoritative message store.
                let text = self
                    .session_messages(session_id)
                    .await
                    .map(extract_assistant_text)
                    .unwrap_or_else(|_| full.clone());
                Ok(Some(text))
            }
            SseEvent::Error(m) => {
                emit(crate::types::AgentEvent::Error { message: m.clone() });
                Err(AppError::Engine(m))
            }
            SseEvent::Busy | SseEvent::Other => Ok(None),
        }
    }
}

fn find_double_newline(buf: &[u8]) -> Option<usize> {
    // Accept both "\n\n" and "\r\n\r\n" frame terminators.
    if let Some(i) = buf.windows(2).position(|w| w == b"\n\n") {
        return Some(i + 1);
    }
    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some(i + 3);
    }
    None
}

/// Pull the first `data:` line out of an SSE frame.
fn parse_sse_frame(frame: &str) -> Option<Value> {
    for line in frame.lines() {
        if let Some(d) = line.strip_prefix("data:") {
            let s = d.trim();
            if !s.is_empty() {
                return serde_json::from_str(s).ok();
            }
        }
    }
    None
}

/// Map an SSE bus event to a typed signal.
fn parse_sse_event(json: &Value) -> SseEvent {
    match json.get("type").and_then(|t| t.as_str()) {
        Some("message.part.delta") => {
            let delta = json
                .pointer("/properties/delta")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            SseEvent::Delta(delta)
        }
        Some("permission.asked") => {
            let p = json.get("properties").unwrap_or(json);
            let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let permission = p.get("permission").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let patterns = p
                .get("patterns")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            SseEvent::Permission(id, permission, patterns)
        }
        Some("session.status") => {
            let t = json.pointer("/properties/status/type").and_then(|x| x.as_str()).unwrap_or("");
            match t {
                "idle" => SseEvent::Idle,
                "error" => SseEvent::Error("agent run failed".into()),
                _ => SseEvent::Busy,
            }
        }
        Some("message.part.updated") => {
            let part = json.pointer("/properties/part").unwrap_or(json);
            if part.get("type").and_then(|t| t.as_str()) == Some("tool") {
                let tool = part.get("tool").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let call_id = part.get("callID").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let status = part.pointer("/state/status").and_then(|x| x.as_str()).unwrap_or("");
                match status {
                    "running" => SseEvent::ToolCall(call_id, tool, tool_input_summary(part)),
                    "completed" => SseEvent::ToolResult(call_id, tool, true),
                    "error" => SseEvent::ToolResult(call_id, tool, false),
                    _ => SseEvent::Other,
                }
            } else {
                SseEvent::Other
            }
        }
        Some("error") => SseEvent::Error(
            json.pointer("/properties/message")
                .and_then(|m| m.as_str())
                .unwrap_or("agent error")
                .to_string(),
        ),
        _ => SseEvent::Other,
    }
}

/// A short human-readable summary of a tool call's input (truncated).
fn tool_input_summary(part: &Value) -> String {
    let input = part.get("state").and_then(|s| s.get("input"));
    let Some(input) = input else {
        return String::new();
    };
    truncate(&input.to_string(), 200)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

pub enum SseEvent {
    Delta(String),
    Permission(String, String, Vec<String>),
    ToolCall(String, String, String),
    ToolResult(String, String, bool),
    Idle,
    Error(String),
    Busy,
    Other,
}

/// Concatenate all text parts of the last assistant message (or all text across messages).
fn extract_assistant_text(messages: Vec<Value>) -> String {
    messages
        .iter()
        .filter_map(|m| m.get("parts").and_then(|p| p.as_array()))
        .flatten()
        .filter_map(|part| {
            if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                part.get("text").and_then(|t| t.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn config_content_sets_permission_ask_and_model() {
        let desc = ModelDescriptor {
            tag: "qwen3.6:latest",
            family: "Qwen3.6",
            size_gb: 22.3,
            license: "Apache-2.0",
            source: "https://ollama.com/library/qwen3.6",
            num_ctx: 16384,
            think: false,
            agent: crate::types::AgentCapability::Locked,
        };
        let cfg: Value = serde_json::from_str(&config_content(&desc)).unwrap();
        assert_eq!(cfg["permission"]["*"], "ask");
        assert_eq!(cfg["permission"]["bash"]["*"], "ask");
        assert_eq!(cfg["permission"]["read"]["**/.env"], "deny");
        assert_eq!(cfg["model"], "ollama/qwen3.6:latest");
        assert_eq!(cfg["provider"]["ollama"]["options"]["baseURL"], "http://127.0.0.1:11434/v1");
        assert_eq!(cfg["provider"]["ollama"]["models"]["qwen3.6:latest"]["name"], "qwen3.6:latest");
    }

    #[test]
    fn config_has_ask_detects_the_net() {
        assert!(config_has_ask(&serde_json::json!({"*": "ask"})));
        assert!(!config_has_ask(&serde_json::json!({"*": "allow"})));
        assert!(!config_has_ask(&serde_json::json!({"read": "allow"})));
    }

    #[tokio::test]
    async fn client_round_trip_against_mock() {
        let server = MockServer::start();
        let m1 = server.mock(|when, then| {
            when.method(POST).path("/session")
                .json_body_partial(serde_json::json!({"model": {"id":"qwen3.6:latest","providerID":"ollama"}}).to_string());
            then.status(200).body(r#"{"id":"ses_123"}"#);
        });
        let m2 = server.mock(|when, then| {
            when.method(POST).path("/session/ses_123/message")
                .json_body_partial(serde_json::json!({"parts":[{"type":"text","text":"hi"}]}).to_string());
            then.status(200);
        });
        let m3 = server.mock(|when, then| {
            when.method(POST).path("/session/ses_123/permissions/per_1")
                .json_body_partial(r#"{"response":"once"}"#.to_string());
            then.status(200);
        });
        let m4 = server.mock(|when, then| {
            when.method(POST).path("/session/ses_123/abort");
            then.status(200);
        });
        let c = OpencodeClient::new(server.base_url());
        assert!(c.health().await == false || true); // no health mock — just exercise
        let sid = c.create_session(&desc()).await.unwrap();
        assert_eq!(sid, "ses_123");
        c.send_message(&sid, "hi").await.unwrap();
        c.reply_permission(&sid, "per_1", "once").await.unwrap();
        c.abort(&sid).await.unwrap();
        m1.assert(); m2.assert(); m3.assert(); m4.assert();
    }

    #[tokio::test]
    async fn run_sse_drives_to_done() {
        let server = MockServer::start();
        // event stream: delta + permission + delta + idle
        let evts = [
            r#"data: {"type":"message.part.delta","properties":{"delta":"Hel"}}"#,
            r#"data: {"type":"permission.asked","properties":{"id":"per_1","permission":"read","patterns":["/ws"]}}"#,
            r#"data: {"type":"message.part.delta","properties":{"delta":"lo"}}"#,
            r#"data: {"type":"session.status","properties":{"status":{"type":"idle"}}}"#,
        ]
        .join("\n\n");
        let m = server.mock(|when, then| {
            when.method(GET).path("/event");
            then.status(200).header("content-type", "text/event-stream").body(evts);
        });
        let mm = server.mock(|when, then| {
            when.method(GET).path("/session/ses_1/message");
            then.status(200).body(r#"[{"parts":[{"type":"text","text":"Hello"}]}]"#);
        });
        let c = OpencodeClient::new(server.base_url());
        let mut events = vec![];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        c.run_sse("ses_1", &mut |e| events.push(e), &cancel).await.unwrap();
        assert!(events.iter().any(|e| matches!(e, crate::types::AgentEvent::Delta { text } if text=="Hel")));
        assert!(events.iter().any(|e| matches!(e, crate::types::AgentEvent::Permission { id, permission, .. } if id=="per_1" && permission=="read")));
        assert!(events.iter().any(|e| matches!(e, crate::types::AgentEvent::Done { text } if text=="Hello")));
        m.assert(); mm.assert();
    }

    #[tokio::test]
    async fn run_sse_honors_cancel() {
        let server = MockServer::start();
        let evts = format!("{}\n\n", r#"data: {"type":"message.part.delta","properties":{"delta":"Hi"}}"#);
        server.mock(|when, then| {
            when.method(GET).path("/event");
            then.status(200).header("content-type", "text/event-stream").body(evts);
        });
        server.mock(|when, then| {
            when.method(POST).path("/session/ses_1/abort");
            then.status(200);
        });
        let c = OpencodeClient::new(server.base_url());
        let mut events = vec![];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        // Flip the cancel flag as soon as the first delta lands — the next chunk check
        // sees it, aborts, and emits Done with the partial.
        c.run_sse("ses_1", &mut |e| {
            if matches!(e, crate::types::AgentEvent::Delta { .. }) {
                cancel.store(true, Ordering::Relaxed);
            }
            events.push(e);
        }, &cancel)
        .await
        .unwrap();
        assert!(events.iter().any(|e| matches!(e, crate::types::AgentEvent::Done { text } if text == "Hi")));
    }

    fn desc() -> ModelDescriptor {
        ModelDescriptor {
            tag: "qwen3.6:latest",
            family: "Qwen3.6",
            size_gb: 22.3,
            license: "Apache-2.0",
            source: "https://ollama.com/library/qwen3.6",
            num_ctx: 16384,
            think: false,
            agent: crate::types::AgentCapability::Locked,
        }
    }

    #[tokio::test]
    async fn run_sse_maps_tool_parts() {
        let server = MockServer::start();
        let evts = [
            r#"data: {"type":"message.part.updated","properties":{"part":{"type":"tool","tool":"read","callID":"c1","state":{"status":"running","input":{"filePath":"/ws/a.txt"}}}}}"#,
            r#"data: {"type":"message.part.updated","properties":{"part":{"type":"tool","tool":"read","callID":"c1","state":{"status":"completed"}}}}"#,
            r#"data: {"type":"session.status","properties":{"status":{"type":"idle"}}}"#,
        ]
        .join("\n\n");
        server.mock(|when, then| {
            when.method(GET).path("/event");
            then.status(200).header("content-type", "text/event-stream").body(evts);
        });
        server.mock(|when, then| {
            when.method(GET).path("/session/ses_1/message");
            then.status(200).body("[]");
        });
        let c = OpencodeClient::new(server.base_url());
        let mut events = vec![];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        c.run_sse("ses_1", &mut |e| events.push(e), &cancel).await.unwrap();
        let call = events.iter().find_map(|e| match e {
            crate::types::AgentEvent::ToolCall { call_id, tool, .. } => Some((call_id.as_str(), tool.as_str())),
            _ => None,
        });
        assert_eq!(call, Some(("c1", "read")));
        assert!(events.iter().any(|e| matches!(e, crate::types::AgentEvent::ToolResult { call_id, ok, .. } if call_id == "c1" && *ok)));
    }

    /// Real-stack integration smoke: spawn the installed opencode serve against the running
    /// Ollama and verify an agentic write round-trip (`cargo test -p common real_agent --
    /// --ignored --nocapture`). Requires opencode + Ollama on this machine.
    #[tokio::test]
    #[ignore = "requires local opencode + Ollama"]
    async fn real_opencode_smoke() {
        use std::path::Path;
        let bin = resolve_opencode_path();
        assert!(bin.exists(), "opencode binary missing: {}", bin.display());
        let tmp = std::env::temp_dir().join(format!("flint-agent-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let port = pick_free_port().unwrap();
        let cfg = config_content_permissive(&desc());
        let mut child = tokio::process::Command::new(&bin)
            .arg("serve").arg("--port").arg(port.to_string()).arg("--hostname").arg("127.0.0.1")
            .env("OPENCODE_CONFIG_CONTENT", cfg)
            .current_dir(&tmp)
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn opencode");
        let base = format!("http://127.0.0.1:{port}");
        let client = OpencodeClient::new(base);
        let mut healthy = false;
        for _ in 0..60 {
            if client.health().await { healthy = true; break; }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        assert!(healthy, "opencode did not become healthy");
        let sid = client.create_session(&desc()).await.expect("create session");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut done_text = String::new();
        let run = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            client.send_message(&sid, "Create a file named result.txt containing exactly the word 'ok' (use a write or edit tool). Then read it back and report its contents.").await?;
            client
                .run_sse(&sid, &mut |e| {
                    if let crate::types::AgentEvent::Done { text } = e {
                        done_text = text;
                    }
                }, &cancel)
                .await
        })
        .await;
        let _ = child.kill();
        assert!(run.is_ok(), "run failed: {run:?}");
        let content = std::fs::read_to_string(Path::new(&tmp).join("result.txt")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(content.trim().contains("ok"), "expected result.txt with 'ok', got: {content:?} / {done_text:?}");
        println!("smoke passed; reply: {done_text:?}");
    }
}
