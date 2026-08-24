/**
 * IPC types shared between the frontend and the Rust backend (mirroring
 * `common::types`). IPC structs are serde-renamed to camelCase; anything that is a
 * *stored payload* stays snake_case on both sides — a mismatch reads as `undefined`
 * and renders blank (a footgun that hit single-brain-cell in Phase 1/2).
 */

export type Tier = "GB8" | "GB16" | "GB32" | "GB64";

export interface ProbeResult {
  chip: string;
  ramGb: number;
  tier: Tier | null;
  supported: boolean;
  unsupportedReason?: string;
}

export interface ModelDescriptor {
  tag: string;
  family: string;
  sizeGb: number;
  license: string;
  source: string;
  numCtx: number;
  think: boolean;
  agent: "locked";
}

export interface Recommendation {
  probe: ProbeResult;
  model: ModelDescriptor | null;
  asOf: string;
  alreadyInstalled: boolean;
}

export type EngineHealth =
  | { state: "Running"; version: string }
  | { state: "NotRunning" };

export interface ModelInfo {
  name: string;
  sizeBytes: number;
  modifiedAt: string;
}

export interface EngineStatusView {
  health: EngineHealth;
  models: ModelInfo[];
  recommendedInstalled: boolean;
}

export interface PullProgress {
  status: string;
  completed: number;
  total: number;
  percent: number;
}

export type InstallState =
  | { state: "Idle" }
  | { state: "Running"; model: string; progress: PullProgress }
  | { state: "Done"; model: string }
  | { state: "Failed"; model: string; error: string }
  | { state: "Cancelled"; model: string };
