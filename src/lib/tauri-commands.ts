import { invoke } from "@tauri-apps/api/core";

/**
 * Typed wrappers around Tauri `invoke()` calls. One function per Rust command
 * registered in src-tauri/src/lib.rs — keeps the frontend/backend contract in one
 * place, mirroring skills-manager's `lib/tauri-commands.ts` pattern.
 *
 * Add a new command: declare it here, then add the `#[tauri::command]` fn AND
 * register it in `generate_handler![]` in src-tauri/src/lib.rs.
 */

/** Bridge-works test — removed in Phase 0 once the probe command round-trips. */
export async function greet(name: string): Promise<string> {
  return invoke<string>("greet", { name });
}
