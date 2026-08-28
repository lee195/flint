# Flint

A standalone macOS app that gives a non-technical user working local AI with zero
dev-tool gates. It probes the machine's hardware, recommends **one** model that fits
(the "cookbook"), downloads it on consent, and offers a plain **chat** plus — on capable
hardware — an **agent mode** (files, commands, multi-step work) — all local, no cloud,
no vendor, **no second installs**. Its reason to exist is **vendor resilience** (a
single-point-of-failure hedge must be independent of any vendor's cloud). Design docs:
`../flint-design/` (`STATE.md` → `PLAN.md` → `docs/00`–`06`).

**Status: Phases 0–3b shipped.** Probe + cookbook + engine + install + chat + agent mode
with a safety net, and the **standalone engine** — the app now runs on its own bundled
`llama-server` (llama.cpp) over GGUF models it downloads itself (resumable, sha256-pinned)
with **no Ollama dependency**. Agent mode asks permission for every action (inline banners)
and offers snapshot restore. What remains of Phase 3: 3a (signing/notarization + bundling
opencode) is paused on a Developer ID certificate; the signing runbook is in
`docs/signing-runbook.md`.

## Architecture

![Flint architecture](docs/architecture.svg)

The Vue frontend (in a WKWebView) talks to the Tauri Rust backend over typed `invoke()`
wrappers, polling for state rather than listening for events. Shared logic lives in the
`common` crate — hardware probe, cookbook tier table, the GGUF downloader, and the
`EngineBackend` trait, which the app satisfies with **`LlamaCppBackend`** (an
`llama-server` sidecar spawned with the installed model by `engine_mgr`, localhost only;
the frontend never does HTTP). Local data lives under `~/.flint/` (`models/`, `chat/`,
`snapshots/`, `bin/`, `opencode/`).

## Stack

- **Tauri v2** — desktop shell; Rust backend (`src-tauri/`).
- **Vue 3 + TypeScript** — `<script setup>` SFCs (`src/`).
- **Vite + Deno** — bundler + JS toolchain (`deno.json` tasks; deps in `package.json`,
  installed with `deno install`).
- **Tailwind CSS v4** — via `@tailwindcss/vite`.
- **vue-router v4** — hash history (WKWebView-safe).
- **vue-i18n** — all chrome strings in the `en` catalog (i18n-ready from day 1).

## Commands

```bash
deno install                    # install npm deps into node_modules
deno task tauri dev             # run the full desktop app — primary dev loop
deno task dev                   # Vite frontend only (no Rust; `invoke` calls will fail)
deno task build                 # type-check (vue-tsc) + build frontend to dist/
deno task tauri build           # produce a distributable native binary/installer (unsigned; signing runbook in docs/)
deno task fetch-llama-server    # vendor the pinned llama.cpp release (sha256-verified) to ~/.flint/bin/
cargo build --workspace         # build the Rust workspace (common + src-tauri)
cargo test -p common            # shared-crate unit tests
```

## Structure

- `common/` — shared no-Tauri crate: canonical data-dir owner (`~/.flint/`), error type,
  hardware probe, cookbook tier table (incl. `hf_repo`/`hf_file`/`sha256` GGUF refs), the
  `EngineBackend` trait with `OllamaBackend` (dev fallback) + `LlamaCppBackend` impls, the
  resumable GGUF downloader, chat transcripts, the opencode client, the capability smoke
  record, and the git2 snapshot.
- `src-tauri/` — Tauri app backend: commands + plugins, `engine_mgr.rs` (the llama-server
  sidecar lifecycle), and `agent.rs` (the opencode harness); `main.rs` is a thin shim.
- `src/` — Vue frontend (`main.ts`, `App.vue`, `router.ts`, `i18n.ts`, `views/`, `lib/`,
  `composables/`).

See `CLAUDE.md` for the Tauri bridge pattern, the Deno toolchain rules, and the binding
WKWebView rules.
