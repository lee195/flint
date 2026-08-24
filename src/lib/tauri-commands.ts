import { invoke } from "@tauri-apps/api/core";
import type {
  AgentEvent,
  AgentStatusView,
  ChatEvent,
  ChatHistoryView,
  EngineStatusView,
  InstallState,
  ProbeResult,
  Recommendation,
  SmokeResult,
} from "./types";

/**
 * Typed wrappers around Tauri `invoke()` calls. One function per Rust command
 * registered in src-tauri/src/lib.rs — keeps the frontend/backend contract in one
 * place, mirroring skills-manager's `lib/tauri-commands.ts` pattern.
 *
 * Add a new command: declare it here, then add the `#[tauri::command]` fn AND
 * register it in `generate_handler![]` in src-tauri/src/lib.rs.
 */

/** Hardware probe (local only — the honesty screen + tier gate). */
export async function probeMachine(): Promise<ProbeResult> {
  return invoke<ProbeResult>("probe_machine");
}

/** The single recommendation for this machine (probe + cookbook + install state). */
export async function getRecommendation(): Promise<Recommendation> {
  return invoke<Recommendation>("get_recommendation");
}

/** Engine reachability + on-disk models + whether the recommended model is installed. */
export async function engineStatus(): Promise<EngineStatusView> {
  return invoke<EngineStatusView>("engine_status");
}

/** Consent-gated install — kicks off the pull; poll `getInstallProgress`. */
export async function startInstall(model: string): Promise<void> {
  return invoke<void>("start_install", { model });
}

export async function getInstallProgress(): Promise<InstallState> {
  return invoke<InstallState>("get_install_progress");
}

export async function cancelInstall(): Promise<void> {
  return invoke<void>("cancel_install");
}

export async function deleteModel(tag: string): Promise<void> {
  return invoke<void>("delete_model", { tag });
}

/** Launch Ollama.app (`/usr/bin/open` — absolute path, doc 05). */
export async function launchOllama(): Promise<void> {
  return invoke<void>("launch_ollama");
}

/** Chat history + the model it runs on (loads the persisted conversation). */
export async function getChat(): Promise<ChatHistoryView> {
  return invoke<ChatHistoryView>("get_chat");
}

/** Send a user message and start the streaming reply. */
export async function sendChat(message: string): Promise<void> {
  return invoke<void>("send_chat", { message });
}

/** Drain buffered streamed events (poll ~150ms while streaming). */
export async function pollChatOutput(): Promise<ChatEvent[]> {
  return invoke<ChatEvent[]>("poll_chat_output");
}

export async function cancelChat(): Promise<void> {
  return invoke<void>("cancel_chat");
}

export async function newChat(): Promise<void> {
  return invoke<void>("new_chat");
}

/** Choose the folder the agent may work in (validated on the Rust side). */
export async function setWorkspace(path: string): Promise<void> {
  return invoke<void>("set_workspace", { path });
}

/** Agent view state: run status, workspace, and the capability gate. */
export async function getAgentStatus(): Promise<AgentStatusView> {
  return invoke<AgentStatusView>("get_agent_status");
}

/** Start an agent run (gated by tier + smoke test; pre-run snapshot taken). */
export async function runAgent(prompt: string): Promise<void> {
  return invoke<void>("run_agent", { prompt });
}

export async function pollAgentOutput(): Promise<AgentEvent[]> {
  return invoke<AgentEvent[]>("poll_agent_output");
}

export async function respondPermission(id: string, allow: boolean): Promise<void> {
  return invoke<void>("respond_permission", { id, allow });
}

export async function cancelAgent(): Promise<void> {
  return invoke<void>("cancel_agent");
}

export async function runSmokeTest(): Promise<SmokeResult> {
  return invoke<SmokeResult>("run_smoke_test");
}

export async function restoreSnapshot(): Promise<void> {
  return invoke<void>("restore_snapshot");
}
