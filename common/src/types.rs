//! IPC-shared types (frontend mirror: `src/lib/types.ts`). IPC structs are serde-renamed
//! to camelCase; stored payloads (chat transcripts, Phase 1) stay snake_case — a mismatch
//! reads as `undefined` and renders blank.

use serde::{Deserialize, Serialize};

/// Hardware capability tier, named after the RAM class it serves (doc 01: ~4 tiers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    #[serde(rename = "GB8")]
    Gb8,
    #[serde(rename = "GB16")]
    Gb16,
    #[serde(rename = "GB32")]
    Gb32,
    #[serde(rename = "GB64")]
    Gb64,
}

impl Tier {
    /// Map physical RAM (GB) to a tier. Below 8 GB has no tier (too weak for a satisfying
    /// local model — the honest "use a chat website" answer, doc 01).
    pub fn of_ram_gb(ram_gb: f64) -> Option<Tier> {
        match ram_gb {
            r if r < 8.0 => None,
            r if r < 12.0 => Some(Tier::Gb8),
            r if r < 24.0 => Some(Tier::Gb16),
            r if r < 48.0 => Some(Tier::Gb32),
            _ => Some(Tier::Gb64),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tier::Gb8 => "8 GB",
            Tier::Gb16 => "16 GB",
            Tier::Gb32 => "32 GB",
            Tier::Gb64 => "64 GB+",
        }
    }
}

/// Result of the hardware probe (`common::probe`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub chip: String,
    pub ram_gb: f64,
    pub tier: Option<Tier>,
    pub supported: bool,
    pub unsupported_reason: Option<String>,
}

/// What a tier's single recommended model looks like (doc 01: radical opinionation).
/// String fields are `&'static str` because the cookbook table is a build-time-pinned const.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDescriptor {
    pub tag: &'static str,
    pub family: &'static str,
    pub size_gb: f64,
    pub license: &'static str,
    pub source: &'static str,
    pub num_ctx: u32,
    /// `think:false` — Qwen3's chain-of-thought dominates latency; off by default for a
    /// no-parameters chat pane (measured: 357 ms vs 14.8 s on this machine).
    pub think: bool,
    pub agent: AgentCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentCapability {
    /// Agent mode is Phase 2 (tier floor + smoke test). All rows ship locked in v0.
    Locked,
}

/// Full recommendation for the current machine (probe + descriptor + install state).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub probe: ProbeResult,
    pub model: Option<ModelDescriptor>,
    /// "recommendations as of <month>" — staleness shown honestly (doc 01), never hidden.
    pub as_of: &'static str,
    pub already_installed: bool,
}

/// Engine reachability (Ollama `/api/version`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum EngineHealth {
    Running { version: String },
    NotRunning,
}

/// A model present in the engine's store (Ollama `/api/tags`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// Engine status for the settings view: reachability + on-disk models + whether the
/// recommended model for this machine is installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatusView {
    pub health: EngineHealth,
    pub models: Vec<ModelInfo>,
    pub recommended_installed: bool,
}

/// One pull-progress sample (Ollama `/api/pull` NDJSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub percent: f64,
}

/// Install lifecycle, shared between the backend task and the polled frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum InstallState {
    Idle,
    Running {
        model: String,
        progress: PullProgress,
    },
    Done {
        model: String,
    },
    Failed {
        model: String,
        error: String,
    },
    Cancelled {
        model: String,
    },
}

/// One chat message. Single-word fields (`role`/`content`/`ts`) — no camelCase/snake_case
/// footgun between the IPC surface and the stored JSONL payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub ts: i64,
}

/// A streamed chat event, buffered backend-side and drained by the frontend poll.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ChatEvent {
    Delta {
        content: String,
    },
    Done {
        full: String,
    },
    Error {
        message: String,
    },
}

/// The chat pane's starting state: persisted history + the model it runs on.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryView {
    pub messages: Vec<ChatMessage>,
    pub model: Option<String>,
}

/// A buffered agent-run event, drained by the frontend poll (like chat events).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentEvent {
    Delta {
        text: String,
    },
    ToolCall {
        call_id: String,
        tool: String,
        input: String,
    },
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
    },
    Permission {
        id: String,
        permission: String,
        pattern: String,
    },
    Done {
        text: String,
    },
    Error {
        message: String,
    },
}

/// The agent view's state: run status, the chosen workspace, and the capability gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusView {
    pub running: bool,
    pub workspace: Option<String>,
    /// Tier floor (GB32+) AND a passed smoke test (doc 01).
    pub unlocked: bool,
    pub locked_reason: Option<String>,
    pub last_smoke: Option<SmokeResult>,
}

/// Result of the capability smoke test (doc 01), persisted across launches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeResult {
    pub passed: bool,
    pub model: String,
    pub ts_ms: i64,
    pub detail: String,
}

/// One reviewable agent run: the pre-run snapshot marker + the run's final text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunView {
    pub run_id: String,
    pub text: String,
    pub snapshot_ts_ms: i64,
    pub changed_files: Vec<String>,
}
