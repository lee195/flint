//! The engine seam (doc 02): a backend interface so the serving engine is swappable, and
//! the v0 implementation, `OllamaBackend`. All HTTP happens here in Rust — the frontend
//! never does HTTP. `llama.cpp` becomes a second implementation behind this same trait in
//! Phase 2.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::StreamExt;

use crate::error::AppError;
use crate::types::{ChatEvent, ChatMessage, EngineHealth, ModelDescriptor, ModelInfo, PullProgress};

#[async_trait]
pub trait EngineBackend: Send + Sync {
    /// Reachability + version. `NotRunning` (never an error) when the engine is absent —
    /// the UI surfaces the amber pill + Launch affordance.
    async fn health(&self) -> Result<EngineHealth, AppError>;

    /// Models currently in the engine's store.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, AppError>;

    /// Ensure the model is present, streaming pull progress to `progress`. Checks `cancel`
    /// between chunks so a cancelled install aborts the download early (resumable anyway).
    async fn ensure_model(
        &self,
        desc: &ModelDescriptor,
        progress: &mut (dyn FnMut(PullProgress) + Send),
        cancel: &AtomicBool,
    ) -> Result<(), AppError>;

    async fn delete_model(&self, tag: &str) -> Result<(), AppError>;

    /// Stream one chat turn (a no-tools session — the permission net is never involved).
    /// Emits `ChatEvent::Delta` per token; returns the full assistant reply for
    /// persistence. Checks `cancel` between chunks.
    async fn chat(
        &self,
        desc: &ModelDescriptor,
        messages: &[ChatMessage],
        emit: &mut (dyn FnMut(ChatEvent) + Send),
        cancel: &AtomicBool,
    ) -> Result<String, AppError>;
}

/// v0 backend: Ollama's OpenAI-compatible server on localhost. Bound to 127.0.0.1 only —
/// never 0.0.0.0 (doc 02).
pub struct OllamaBackend {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaBackend {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("reqwest client build");
        Self { client, base_url }
    }
}

#[async_trait]
impl EngineBackend for OllamaBackend {
    async fn health(&self) -> Result<EngineHealth, AppError> {
        let url = format!("{}/api/version", self.base_url);
        match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r
                    .json()
                    .await
                    .map_err(|e| AppError::Engine(format!("version parse: {e}")))?;
                let version = v
                    .get("version")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?")
                    .to_string();
                Ok(EngineHealth::Running { version })
            }
            _ => Ok(EngineHealth::NotRunning),
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, AppError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("tags: {e}")))?;
        if !resp.status().is_success() {
            // Engine reachable but no tags — treat as an empty store rather than an error
            // so a stopped-but-present engine degrades to "no models" not "broken".
            return Ok(vec![]);
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Engine(format!("tags parse: {e}")))?;
        let mut out = Vec::new();
        if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
            for m in models {
                out.push(ModelInfo {
                    name: m.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    size_bytes: m.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                    modified_at: m
                        .get("modified_at")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
        Ok(out)
    }

    async fn ensure_model(
        &self,
        desc: &ModelDescriptor,
        progress: &mut (dyn FnMut(PullProgress) + Send),
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        let url = format!("{}/api/pull", self.base_url);
        let body = serde_json::json!({ "name": desc.tag, "stream": true });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("pull start: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Engine(format!("pull http {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Engine("install cancelled".into()));
            }
            let chunk = chunk.map_err(|e| AppError::Engine(format!("pull stream: {e}")))?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw[..raw.len().saturating_sub(1)]).trim().to_string();
                match process_pull_line(&line, progress)? {
                    PullLine::Done => return Ok(()),
                    PullLine::Continue => {}
                }
            }
        }
        // Flush a trailing partial line (the server may not send a final newline).
        if !buf.is_empty() {
            let line = String::from_utf8_lossy(&buf).trim().to_string();
            if !line.is_empty() {
                if matches!(process_pull_line(&line, progress)?, PullLine::Done) {
                    return Ok(());
                }
            }
        }
        Err(AppError::Engine("pull stream ended before success".into()))
    }

    async fn delete_model(&self, tag: &str) -> Result<(), AppError> {
        let url = format!("{}/api/delete", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .json(&serde_json::json!({ "name": tag }))
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("delete: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Engine(format!("delete http {}", resp.status())));
        }
        Ok(())
    }

    async fn chat(
        &self,
        desc: &ModelDescriptor,
        messages: &[ChatMessage],
        emit: &mut (dyn FnMut(ChatEvent) + Send),
        cancel: &AtomicBool,
    ) -> Result<String, AppError> {
        let url = format!("{}/api/chat", self.base_url);
        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
            .collect();
        let body = serde_json::json!({
            "model": desc.tag,
            "messages": api_messages,
            "stream": true,
            "think": desc.think,
            "options": { "num_ctx": desc.num_ctx }
        });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("chat start: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Engine(format!("chat http {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Engine("chat cancelled".into()));
            }
            let chunk = chunk.map_err(|e| AppError::Engine(format!("chat stream: {e}")))?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw[..raw.len().saturating_sub(1)]).trim().to_string();
                match process_chat_line(&line, emit, &mut full)? {
                    ChatLine::Done => return Ok(full),
                    ChatLine::Continue => {}
                }
            }
        }
        // Flush a trailing partial line (server may not send a final newline).
        if !buf.is_empty() {
            let line = String::from_utf8_lossy(&buf).trim().to_string();
            if !line.is_empty() {
                process_chat_line(&line, emit, &mut full)?;
            }
        }
        Ok(full)
    }
}

/// Outcome of one `/api/pull` NDJSON line.
enum PullLine {
    Continue,
    Done,
}

fn process_pull_line(
    line: &str,
    progress: &mut dyn FnMut(PullProgress),
) -> Result<PullLine, AppError> {
    if line.is_empty() {
        return Ok(PullLine::Continue);
    }
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| AppError::Engine(format!("pull line: {e}")))?;
    let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("").to_string();
    match status.as_str() {
        "error" => {
            let msg = v.get("error").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
            Err(AppError::Engine(msg))
        }
        "success" => Ok(PullLine::Done),
        _ => {
            let completed = v.get("completed").and_then(|x| x.as_u64()).unwrap_or(0);
            let total = v.get("total").and_then(|x| x.as_u64()).unwrap_or(0);
            let percent = if total > 0 { (completed as f64 / total as f64) * 100.0 } else { 0.0 };
            progress(PullProgress {
                status,
                completed,
                total,
                percent: (percent * 10.0).round() / 10.0,
            });
            Ok(PullLine::Continue)
        }
    }
}

/// Outcome of one `/api/chat` NDJSON line.
enum ChatLine {
    Continue,
    Done,
}

fn process_chat_line(
    line: &str,
    emit: &mut dyn FnMut(ChatEvent),
    full: &mut String,
) -> Result<ChatLine, AppError> {
    if line.is_empty() {
        return Ok(ChatLine::Continue);
    }
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| AppError::Engine(format!("chat line: {e}")))?;
    if let Some(err) = v.get("error").and_then(|x| x.as_str()) {
        return Err(AppError::Engine(err.to_string()));
    }
    let done = v.get("done").and_then(|x| x.as_bool()).unwrap_or(false);
    let delta = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    if !delta.is_empty() {
        full.push_str(delta);
        emit(ChatEvent::Delta { content: delta.to_string() });
    }
    Ok(if done { ChatLine::Done } else { ChatLine::Continue })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::atomic::AtomicBool;

    fn desc(tag: &'static str) -> ModelDescriptor {
        ModelDescriptor {
            tag,
            family: "Qwen3",
            size_gb: 22.3,
            license: "Apache-2.0",
            source: "https://ollama.com/library/qwen3.6",
            num_ctx: 16384,
            think: false,
            agent: crate::types::AgentCapability::Locked,
        }
    }

    #[tokio::test]
    async fn health_running() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/version");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"version":"0.32.15"}"#);
        });
        let backend = OllamaBackend::new(server.base_url());
        let health = backend.health().await.unwrap();
        assert_eq!(health, EngineHealth::Running { version: "0.32.15".into() });
        mock.assert();
    }

    #[tokio::test]
    async fn health_not_running_when_unreachable() {
        // Bind a listener, grab its port, then drop it so nothing is listening.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let backend = OllamaBackend::new(format!("http://127.0.0.1:{port}"));
        let health = backend.health().await.unwrap();
        assert_eq!(health, EngineHealth::NotRunning);
    }

    #[tokio::test]
    async fn list_models_parses_tags() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"models":[{"name":"qwen3.6:latest","size":23938333577,"modified_at":"2026-04-20T19:15:52Z"}]}"#);
        });
        let backend = OllamaBackend::new(server.base_url());
        let models = backend.list_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "qwen3.6:latest");
        assert_eq!(models[0].size_bytes, 23938333577);
        mock.assert();
    }

    #[tokio::test]
    async fn ensure_model_streams_progress_then_succeeds() {
        let server = MockServer::start();
        let ndjson = [
            r#"{"status":"pulling manifest"}"#,
            r#"{"status":"downloading","digest":"sha256:abc","total":100,"completed":30,"progress":"30%"}"#,
            r#"{"status":"downloading","digest":"sha256:abc","total":100,"completed":80,"progress":"80%"}"#,
            r#"{"status":"verifying sha256 digest"}"#,
            r#"{"status":"success"}"#,
        ]
        .join("\n");
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/pull").json_body(serde_json::json!({"name":"qwen3.6:latest","stream":true}));
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(ndjson);
        });
        let backend = OllamaBackend::new(server.base_url());
        let mut progress: Vec<PullProgress> = Vec::new();
        let cancel = AtomicBool::new(false);
        let res = backend
            .ensure_model(&desc("qwen3.6:latest"), &mut |p| progress.push(p), &cancel)
            .await;
        assert!(res.is_ok(), "expected success, got {res:?}");
        // Every non-terminal line emits a progress sample: manifest, 30%, 80%, verifying.
        assert_eq!(progress.len(), 4);
        assert_eq!(progress[0].status, "pulling manifest");
        assert_eq!(progress[1].percent, 30.0);
        assert_eq!(progress[2].percent, 80.0);
        assert_eq!(progress[3].status, "verifying sha256 digest");
        mock.assert();
    }

    #[tokio::test]
    async fn ensure_model_reports_pull_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/pull");
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(r#"{"status":"error","error":"unknown model"}"#);
        });
        let backend = OllamaBackend::new(server.base_url());
        let cancel = AtomicBool::new(false);
        let res = backend
            .ensure_model(&desc("nope:latest"), &mut |_| {}, &cancel)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("unknown model"));
        mock.assert();
    }

    #[tokio::test]
    async fn ensure_model_honors_cancel() {
        let server = MockServer::start();
        let lines = vec![
            r#"{"status":"downloading","total":100,"completed":10}"#,
            r#"{"status":"downloading","total":100,"completed":20}"#,
        ]
        .join("\n");
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/pull");
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(lines);
        });
        let backend = OllamaBackend::new(server.base_url());
        let cancel = AtomicBool::new(true);
        let res = backend
            .ensure_model(&desc("qwen3.6:latest"), &mut |_| {}, &cancel)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cancelled"));
        mock.assert();
    }

    #[tokio::test]
    async fn delete_model_calls_delete() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(DELETE).path("/api/delete");
            then.status(200);
        });
        let backend = OllamaBackend::new(server.base_url());
        backend.delete_model("qwen3.6:latest").await.unwrap();
        mock.assert();
    }

    /// Dogfood integration smoke — run with `cargo test -p common real_ollama --
    /// --ignored --nocapture` on a machine with Ollama running on :11434.
    #[tokio::test]
    #[ignore = "requires local Ollama running on 127.0.0.1:11434"]
    async fn real_ollama_health_and_tags() {
        let backend = OllamaBackend::new("http://127.0.0.1:11434".into());
        let health = backend.health().await.expect("health should resolve");
        assert!(
            matches!(health, EngineHealth::Running { .. }),
            "Ollama should be running, got {health:?}"
        );
        let models = backend.list_models().await.expect("tags should resolve");
        assert!(!models.is_empty(), "expected at least one model on disk");
        println!("health: {health:?}\nmodels: {models:?}");
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into(), ts: 0 }
    }

    #[tokio::test]
    async fn chat_streams_deltas_and_returns_full() {
        let server = MockServer::start();
        let ndjson = [
            r#"{"model":"qwen3.6:latest","message":{"role":"assistant","content":"Hel"},"done":false}"#,
            r#"{"model":"qwen3.6:latest","message":{"role":"assistant","content":"lo"},"done":false}"#,
            r#"{"model":"qwen3.6:latest","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#,
        ]
        .join("\n");
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/chat")
                .json_body_partial(
                    serde_json::json!({
                        "model": "qwen3.6:latest",
                        "think": false,
                        "options": { "num_ctx": 16384 }
                    })
                    .to_string(),
                );
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(ndjson);
        });
        let backend = OllamaBackend::new(server.base_url());
        let mut events: Vec<ChatEvent> = Vec::new();
        let cancel = AtomicBool::new(false);
        let history = vec![msg("user", "hi")];
        let full = backend
            .chat(&desc("qwen3.6:latest"), &history, &mut |e| events.push(e), &cancel)
            .await
            .expect("chat should succeed");
        assert_eq!(full, "Hello");
        assert_eq!(
            events,
            vec![
                ChatEvent::Delta { content: "Hel".into() },
                ChatEvent::Delta { content: "lo".into() },
            ]
        );
        mock.assert();
    }

    #[tokio::test]
    async fn chat_reports_engine_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(r#"{"error":"model not found"}"#);
        });
        let backend = OllamaBackend::new(server.base_url());
        let cancel = AtomicBool::new(false);
        let res = backend
            .chat(&desc("nope:latest"), &[], &mut |_| {}, &cancel)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("model not found"));
        mock.assert();
    }

    #[tokio::test]
    async fn chat_honors_cancel() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(r#"{"message":{"role":"assistant","content":"hi"},"done":false}"#);
        });
        let backend = OllamaBackend::new(server.base_url());
        let cancel = AtomicBool::new(true);
        let res = backend.chat(&desc("qwen3.6:latest"), &[], &mut |_| {}, &cancel).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cancelled"));
        mock.assert();
    }

    /// Dogfood integration smoke — a real streamed reply from the running Ollama
    /// (`cargo test -p common real_chat -- --ignored --nocapture`).
    #[tokio::test]
    #[ignore = "requires local Ollama running on 127.0.0.1:11434 with the recommended model"]
    async fn real_ollama_chat() {
        let backend = OllamaBackend::new("http://127.0.0.1:11434".into());
        let cancel = AtomicBool::new(false);
        let mut events: Vec<ChatEvent> = Vec::new();
        let history = vec![msg(
            "user",
            "Reply with exactly one word: hello.",
        )];
        let reply = backend
            .chat(&desc("qwen3.6:latest"), &history, &mut |e| events.push(e), &cancel)
            .await
            .expect("chat should stream");
        assert!(!reply.is_empty());
        let joined: String = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::Delta { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(joined, reply, "deltas must reassemble the full reply");
        println!("reply: {reply:?}");
    }
}
