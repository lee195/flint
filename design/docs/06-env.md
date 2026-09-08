# Doc 06 — Environment & toolchain facts (canonical)

> **Purpose:** the single source of truth for machine-specific facts so sessions can start lean
> without re-deriving them. Created 2026-08-24 (Phase 3 planning); updated 2026-08-28
> (Phase 3b shipped). Read `STATE.md` → `PLAN.md` → docs `00`–`06` for design; this file is
> operational, not design.

## Development machine (author's)

- **Apple M5 Pro / 64 GB RAM** — Flint's top tier; the author dogfoods the top tier. Only
  Apple Silicon (`aarch64-apple-darwin`) is supported (doc 00). No Intel/x86 build.
- **OS:** macOS 26.6.2. Toolchain: Xcode CommandLineTools only (no full Xcode);
  `xcrun notarytool` is available.

## Toolchain versions (pinned in use)

- **Deno 2.9.0** — JS runtime + task runner (this repo's binding decision; see `../../CLAUDE.md`).
- **Rust 1.90** — Cargo workspace `common/` + `src-tauri/`, host `aarch64-apple-darwin`.
- **cargo-tauri** (Tauri 2) — app shell; Vite dev server locked to port **1420**.
- **llama.cpp b10667** — the app's engine. `deno task fetch-llama-server` vendors the pinned
  macos-arm64 release (sha256-verified) to `~/.flint/bin/llama-server` + its dylibs. Version
  0.3.0-dev, build 10667, AppleClang 21, arm64. OpenAI-compat `/v1` on localhost.
- **opencode 1.18.20** — installed at `~/.opencode/bin/opencode` (Mach-O arm64, Bun-built,
  single self-contained binary). Used for agent mode (bundling is Phase 3a).
- **Ollama 0.32.15** — still installed on this machine with `qwen3.6` + `qwen3-coder` on disk
  (~45 GB at `~/.ollama/models`), but **the app no longer uses it** (Phase 3b full swap) —
  it is the dev fallback backend only, not wired into the app. Its GGUF blobs are NOT
  loadable by llama.cpp (GGUF divergence), so models come from HF bartowski GGUFs instead.
- **cmake: NOT installed** — vendoring the official llama.cpp release means it's not needed.
- **git** (`/opt/homebrew/bin/git`), **make** present.

## Key paths

- Design docs: in-repo `design/` (`STATE.md`, `PLAN.md`, `docs/00`–`06`).
- App: this repo — Cargo workspace (`common/`, `src-tauri/`) + Vue frontend (`src/`).
  (Remote re-point is pending — the repo moved off its original host.)
- **Data dir: `~/.flint/`** — owned exclusively by `common::config::data_dir()`; never Tauri's
  `app_data_dir()`. Subdirs:
  - `models/` — GGUF model store (Phase 3b). Top-tier model installed:
    `Qwen_Qwen3.5-35B-A3B-Q4_K_M.gguf` (22,285,080,384 B). Downloads write `.part` + rename
    on a passing digest check.
  - `bin/` — dev vendor of `llama-server` + dylibs (`deno task fetch-llama-server`).
  - `chat/current.jsonl` — the single persisted conversation.
  - `snapshots/` — git2 safety-net repos (created on demand).
  - `opencode/` — Flint's `OPENCODE_CONFIG_DIR`; `plugin/flint-confinement.js` written at startup.
  - `smoke.json` — capability smoke test result when it has run.
- External prior art: `github.com/pewdiepie-archdaemon/odysseus` (AGPL — UX reference only).

## Signing & notarization status (Phase 3, 3a — paused)

- **0 valid code-signing identities** on this machine (`security find-identity -v -p codesigning`
  → none). **Signing/notarization is blocked until a Developer ID Application cert is
  provided** (personal). 3a (signing +
  bundling opencode + updater keys) is **paused** per the 2026-08-28 session decision.
- The **signing runbook** (`flint/docs/signing-runbook.md`) pins the exact
  codesign/entitlements/notarytool commands so the spike runs the moment a cert lands.
- Current builds are **unsigned** (`deno task tauri build`; doc 05: author-only validation runs
  may be unsigned, but signing is a day-1 rule the moment a build leaves this machine).

## Cookbook / model facts

- **Top tier (this machine):** `qwen3.6:latest` (Ollama tag) = **Qwen3.5-35B-A3B** (qwen35moe,
  36B MoE, Q4_K_M). The app now downloads the bartowski GGUF
  `bartowski/Qwen_Qwen3.5-35B-A3B-GGUF/Qwen_Qwen3.5-35B-A3B-Q4_K_M.gguf` (22.3 GB, sha256
  `2f2df1e8…4751ab`, Apache-2.0). Thinking off via `chat_template_kwargs.enable_thinking:false`
  — measured **~135 ms** per short reply via llama.cpp (vs 357 ms via Ollama).
- Lower tiers pinned-but-untested (doc 01): `qwen3:4b` / `qwen3:8b` / `qwen3:14b` → bartowski
  Qwen3-4B/8B/14B Q4_K_M GGUFs with pinned sha256. `num_ctx` 8192/8192/16384/16384.
- Tier table lives in `common::cookbook` with `hf_repo`/`hf_file`/`sha256` per row.

## Key commands

```bash
# Dev loop / builds
deno task tauri dev          # full desktop app (Vite :1420 + Tauri window)
deno task build              # vue-tsc --noEmit && vite build -> dist/
deno task check              # type-check only
deno task tauri build        # distributable (unsigned until cert lands)
deno task fetch-llama-server # vendor the pinned llama.cpp release to ~/.flint/bin/

# Rust
cargo test --workspace       # all crates (42 common tests green, 3 ignored + engine spawn)
cargo test -p flint real_engine_spawn -- --ignored --nocapture   # app engine startup (~4s)
OPENCODE_BASE_URL=http://127.0.0.1:<port>/v1 cargo test -p common real_opencode_smoke -- --ignored --nocapture  # agent tool-calling gate (~78s)

# Phase 3 signing (once a cert exists)
security find-identity -v -p codesigning   # list Developer ID identities
xcrun notarytool <...>                      # notarize + staple (see runbook, 3a-M1)
```

## Shipped state (as of 2026-08-28)

- **Phases 0–3b shipped.** Probe + cookbook + engine + install + chat + agent mode with
  safety net + **standalone engine**: the app runs on its own `llama-server` (llama.cpp)
  over self-downloaded GGUF models (resumable, sha256-pinned) with **no Ollama dependency**
  (verified: Ollama stopped, `engine_mgr::ensure_running` brings the engine up, sidecar
  killed on quit). Tool-calling gate passed (opencode → llama-server → Qwen3.5-35B-A3B).
- **Paused: Phase 3a** — signing/notarization (cert-blocked; runbook ready), bundling
  opencode as externalBin, updater keys. See `PLAN.md` → Phase 3.
