/**
 * IPC types shared between the frontend and the Rust backend (mirroring
 * `common::types`). IPC structs are serde-renamed to camelCase; anything that is a
 * *stored payload* stays snake_case on both sides — a mismatch reads as `undefined`
 * and renders blank (a footgun that hit single-brain-cell in Phase 1/2).
 */

export interface ProbeResult {
  chip: string;
  ramGb: number;
  tier: string;
  supported: boolean;
  unsupportedReason?: string;
}
