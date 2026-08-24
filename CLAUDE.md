# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Product

**Flint** is a standalone macOS app that gives a non-technical user working local AI with
zero dev-tool gates: it probes the machine's hardware, recommends **one** model that fits
(the "cookbook"), installs it on consent, and offers a plain **chat** — all local, no
cloud, no vendor. Its reason to exist is **vendor resilience** (flint-design doc 00). The
agent mode (opencode harness + safety net) is Phase 2 — **not in v0**. Design docs live in
the sibling `../flint-design/` (`STATE.md` → `PLAN.md` → `docs/00`–`05`); the v0 plan
(`PLAN.md`) is Pre-Phase 0 bootstrap + Phase 0 (probe/cookbook/engine/install) + Phase 1
(chat).

**Current state: Pre-Phase 0.** The scaffold is a Tauri 2 + Vue 3 app with a `greet`
bridge-works test. No probe, no cookbook, no chat yet. Phase 0 starts with the model/tier
spike (top-tier pick from the on-disk Ollama candidates `qwen3.6` / `qwen3-coder`).

**v0 engine: Ollama backend first** behind the `EngineBackend` trait (`common/`, Phase 0).
The llama.cpp sidecar is the Phase 2 swap behind the same interface. All HTTP happens in
Rust — the frontend never does HTTP.

## Overview

`flint` is a two-crate Cargo workspace plus a Vue frontend:

- `common/` — shared, no-Tauri library: the canonical data-dir owner (`common::config`),
  error type, and (from Phase 0) the probe, cookbook tier table, and engine backend trait.
- `src-tauri/` — the Tauri 2 app backend: commands + plugins.
- `src/` — Vue 3 + TypeScript + Vite frontend. JS toolchain is **Deno** (not Bun/npm
  scripts): deps in `package.json` (installed with `deno install`), tasks in `deno.json`.

**Toolchain (binding, this repo's decision):** Deno is the JS runtime + task runner.
`deno.json` tasks: `dev`, `build` (`vue-tsc --noEmit && vite build`), `check`, `preview`,
`tauri`. `deno task` resolves `node_modules/.bin` the way npm scripts do. No Bun, no npm
scripts. Rust/Cargo is untouched by this. `ember-deno` is the in-house Deno + Vue + Vite
toolchain reference (pattern only — its `deno desktop` shell is not used; Tauri stays).

**Data dir:** `~/.flint/`, owned by `common::config::data_dir()` — resolved by that one
function only, never Tauri's `app_data_dir()` (a path split silently breaks shared state).
On first launch the app ensures the dir exists.

## Commands

- `deno task tauri dev` — run the full desktop app (boots Vite, then launches the Tauri
  window). This is the primary dev loop.
- `deno task dev` — Vite frontend only in a browser (no Rust/native APIs; `invoke` calls
  will fail).
- `deno task build` — type-check (`vue-tsc --noEmit`) and build the frontend to `dist/`.
- `deno task check` — type-check only.
- `deno task tauri build` — produce a distributable native binary/installer (unsigned in
  v0; signing/notarization is Phase 3).
- `cargo build --workspace` / `cargo test -p common` — Rust workspace build/tests.

The Vite dev server is locked to port **1420** (`strictPort: true`) because Tauri expects
a fixed port.

## Architecture

### Frontend ↔ backend bridge

The frontend calls Rust functions over Tauri's IPC, not HTTP:

1. A Rust function is annotated `#[tauri::command]` in `src-tauri/src/lib.rs`.
2. It must be registered in the `invoke_handler(tauri::generate_handler![...])` list in
   the same file.
3. The frontend calls it through a typed wrapper in `src/lib/tauri-commands.ts`, which
   wraps `invoke` from `@tauri-apps/api/core`.

Adding a new backend command requires editing `src-tauri/src/lib.rs` in both places (the
function and the `generate_handler!` macro) **and** adding a typed wrapper in
`src/lib/tauri-commands.ts`.

**Entry points.** `src-tauri/src/main.rs` is a thin shim that calls `run()` in `lib.rs`;
almost all backend setup lives in `lib.rs`. The frontend mounts from `src/main.ts` →
`App.vue`.

**Permissions.** Tauri 2 gates native API access through capabilities.
`src-tauri/capabilities/default.json` lists the permissions granted to the main window —
new plugin APIs must be added there or calls are denied at runtime. Currently only
`core:default`.

**Case convention.** IPC structs (`common/src/types.rs` ↔ `src/lib/types.ts`) are
serde-renamed to **camelCase**; stored payloads (chat transcripts, Phase 1) stay
**snake_case**. TS interfaces for parsed payloads must be snake_case — a camelCase field
reads as `undefined` and renders blank (a footgun that hit single-brain-cell).

### Routing

`src/router.ts` uses **hash history** (`createWebHashHistory`) — required for Tauri's
webview. Do not switch to HTML history mode. Views are lazy-loaded.

### Polling

Backend → frontend state flows via **polling**, never Tauri events. The primitive is
`src/composables/use-polling.ts` (5s cadence, spark doc 02). Chat streaming polls its
buffered run-loop at ~150ms (skills-manager's `poll_skill_output` copy source), not the
5s composable.

### i18n

All chrome strings live in the vue-i18n `en` catalog in `src/i18n.ts` — nothing is
hard-coded in components (flint doc 00's en-first, i18n-ready rule; `de` is later a
catalog pass). Use `useI18n()` → `t("path.to.key")`.

## WKWebView rules (binding)

macOS runs the frontend in WKWebView, which has frozen on past projects when certain UI
patterns are used. These rules come from `spark-design/docs/02-wkwebview-constraints.md`
and apply to every change in this app:

1. **Hash router only** (already set). No HTML history mode.
2. **No portal dialogs.** All prompts and confirmations are **inline banners** rendered
   in normal page flow — no modal dialogs, no `window.prompt`. At most **one**
   portal-using component mounted per route.
3. **No `tauri::Window::emit` for run state.** Backend → frontend state flows via
   **polling**, not Tauri events. Don't add a second code path.
4. **No top-level `await` in `<script setup>`** — defer async setup to `onMounted`.
5. **Virtualize or paginate heavy lists** (long chat transcripts) — don't render
   thousands of DOM nodes.

If a freeze recurs, narrow the allowed UI primitive set further and document the offender.

## Absolute paths only

No PATH lookups anywhere (flint doc 05). The engine is reached by localhost URL, never a
PATH binary; anything we spawn is resolved to an absolute path inside the app bundle.
