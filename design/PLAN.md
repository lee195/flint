# Flint — implementation plan

> The implementation plan that follows the design docs. The docs (`design/docs/00`–`06`)
> are the source of truth; this plan is the sequencing, the shippable criteria per phase, and
> the verification. Written 2026-08-24, after the v0 scoping session (decisions below);
> updated through Phase 3b.
>
> **v0 scope was Phase 0 + 1** (scaffold + probe + cookbook + engine interface + install + chat);
> Phase 2 (agent + safety net) and Phase 3 (standalone shell/engine) were planned additions and
> have shipped. App repo: this repo root, with `design/` holding the design docs.

## Session decisions (2026-08-24)

- **v0 = Phase 0 + 1.** The first vertical slice that validates the thesis: a runnable app that
  probes the hardware, recommends **one** model for the tier, installs it on consent, and chats
  with it locally. Agent mode is out of v0 (Phase 2).
- **Engine: Ollama backend first**, behind the engine interface (doc 02's decision trigger
  fired — Ollama 0.32.15 is installed and running on this machine, with `qwen3.6` (36B) and
  `qwen3-coder` (30.5B) already on disk). The llama.cpp sidecar is the Phase 2 swap, not v0.
- **Toolchain: Deno** as the JS runtime + task runner (in place of Bun). Tauri 2 + Rust stays —
  the Deno + Vue + Vite toolchain pattern (package.json for deps, deno.json for tasks) is
  documented in `CLAUDE.md`; the experimental `deno desktop` shell is explicitly not used
  (experimental, ~130 MB bundle, would fight the standalone + signing thesis).
- **Author dogfoods the top tier.** This machine is an Apple M5 Pro / 64 GB → Flint's top tier
  (64 GB+). The other tiers ship as pinned-but-unvalidated entries with an honest "as of" date;
  they become blocking only when non-author hardware appears (doc 01).
- **Artifacts:** this `PLAN.md` + a `STATE.md` ledger update.
- **Phase 0 execution decisions (2026-08-24):** engine HTTP client = **reqwest + tokio**
  (async, streaming — pull progress + chat NDJSON); top tier = **`qwen3.6` with
  `think:false`** (measured — see Phase 0 spike); Phase 0 only (chat stays Phase 1).

## How to read this plan

- Each phase has: **goal** (one sentence), **builds** (concrete work, doc-referenced),
  **shippable** (what a user can do when the phase lands), **verification** (commands /
  dogfood tests that prove the phase), **dependencies** (what must be done first), and
  **out of scope** (what a phase explicitly does not do — to resist scope creep).
- The one **implementation spike** from STATE.md's "What's next" that gates v0 (the model/tier
  spike) is folded into Phase 0 as its *first concrete work*, not a separate pre-phase.
- Phase ordering is strict: a phase's verification must pass before the next begins. No
  parallel phases.
- All Rust work happens in the Cargo workspace (`common/` + `src-tauri/`). All frontend work
  happens in the `flint/src/` scaffold. All Tauri commands edit `src-tauri/src/lib.rs`
  (declare + register) and `src/lib/tauri-commands.ts` (typed wrapper) per the scaffold's
  CLAUDE.md rule.

---

## Pre-Phase 0 — workspace bootstrap

**Goal:** a runnable `flint/` scaffold that conforms to the inherited rules (WKWebView, i18n,
data-dir, Deno toolchain) and is ready to absorb real features.

**Builds:**
- **Scaffold** Tauri 2 + Vue 3 + TS + Vite, hash router, port 1420, Tailwind v4 +
  lucide-vue-next + clsx, `@tauri-apps/api` — a clean Tauri 2 layout (frontend layout,
  `tauri.conf.json`, capabilities, `CLAUDE.md` rules). `greet` stays as
  the bridge-works test (removed in Phase 0 once the probe command round-trips).
- **Deno toolchain** (this session's decision): JS runtime + task runner is **Deno**.
  - npm packages (Vue, vue-router, vue-i18n, Vite, vue-tsc, Tailwind v4, lucide-vue-next,
    clsx, `@tauri-apps/api`, `@tauri-apps/cli`) declared in **`package.json`** (the
    established pattern — package.json for deps, `deno.json` for tasks).
  - Installed with `deno install` (fallback `npm install` only if a native-postinstall
    package misbehaves under Deno).
  - Tasks in **`deno.json`**: `dev` → `vite`, `build` → `vue-tsc --noEmit && vite build`,
    `preview` → `vite preview`, `tauri` → `tauri`. `deno task` resolves `node_modules/.bin`
    the way npm scripts do, so `vite`/`vue-tsc`/`tauri` run unchanged.
  - `tauri.conf.json`: `beforeDevCommand: "deno task dev"`,
    `beforeBuildCommand: "deno task build"` (shell-level — no Tauri-side change).
- **Cargo workspace**: root `Cargo.toml` with members `common/` + `src-tauri/`.
  `common/` is a library crate holding `AppError` + a `types` module placeholder (no Tauri
  deps — `serde` only, the future home of probe/cookbook/engine). `src-tauri/` is the Tauri
  app crate depending on `common`. Verify the workspace builds both.
- **i18n-ready from the first commit** (doc 00's en-first/i18n-ready rule — a deliberate,
  documented deviation from a bilingual-day-1 stance): `vue-i18n` wired up with an `en`
  catalog; **all chrome strings pass through i18n** so `de` is a catalog pass, not a
  refactor.
- **`common::config::data_dir()`** owns `~/.flint/` — never Tauri's `app_data_dir`
  (the canonical-dir lesson: two paths = zero shared data).
- **`CLAUDE.md`** in the repo carrying the binding rules (WKWebView: hash router, inline
  banners, polling not events, no top-level await, virtualized lists; no modals/portals, no
  `tauri::Window::emit`; Deno toolchain; data-dir rule).
- **Capabilities:** `core:default` only for now.

**Shippable:** `deno task tauri dev` opens an empty window; the workspace builds; `greet`
works end-to-end.

**Verification:**
```bash
workdir=flint
cargo build --workspace             # common + src-tauri build
deno task build                     # vue-tsc --noEmit && vite build
deno task tauri dev                 # scaffold window launches
# in the app: greet round-trips (dev console)
```

**Dependencies:** none (this is the start).

**Out of scope:** probe, cookbook, engine, chat (all Phase 0/1); agent mode (Phase 2);
packaging/signing (Phase 3).

---

## Phase 0 — probe + cookbook + engine interface + install ("recommend one model")

**Goal:** the app probes the machine, shows the honesty screen, recommends **one** model for
the tier, and installs it on consent. No chat yet.

**Builds:**
- **Model/tier spike — RESOLVED (2026-08-24, measured on this M5 Pro / 64 GB):** top tier →
  **`qwen3.6:latest` with `think:false`** — 357 ms on a short chat vs 14.8 s / 325 thinking
  chunks with thinking on (Qwen3's chain-of-thought dominates latency; thinking off is the
  right default for a no-parameters chat pane). `qwen3-coder` (30.5B, 4.2 s) rejected for a
  layperson's general chat. Lower tiers seeded **pinned-but-untested** (doc 01's maintenance
  stance): 8 GB → `qwen3:4b` (chat-only), 16 GB → `qwen3:8b`, 32 GB → `qwen3:14b`. Every row
  carries `agent: locked` (agent capability is Phase 2). Remaining spike work at
  implementation: confirm the lower-tier tags resolve + license strings (Qwen family leans
  Apache-2.0), finalize per-tier `num_ctx`.
- **Probe** (`common/probe`): `sysctl` (`machdep.cpu.brand_string`, `hw.memsize`) → chip, RAM,
  tier; non-Apple-Silicon → honest "Flint needs Apple Silicon" message (doc 01 lean). Runs on
  launch.
- **Cookbook** (`common/cookbook`): tier table (4 tiers → one descriptor each: tag, family,
  size, license, source, capability flags, "as of" date), embedded at build time, not fetched
  (doc 01's no-silent-remote posture); `recommendation(probe)`.
- **Engine backend trait** (`common/engine`): `EngineBackend { health(), list_models(),
  ensure_model(desc, on_progress) }` behind which `OllamaBackend` is the v0 implementation
  (localhost:11434; `/api/version`, `/api/tags`, `/api/pull` with streamed progress). **All
  HTTP in Rust — the frontend never does HTTP** (established boundary).
- **Install flow:** `/` onboarding: probe → honesty screen (doc 01 copy: what local models
  can/can't do, per-tier expectation) → recommendation card ("your Mac: M5 Pro, 64 GB → we
  recommend X") → consent-gated Install (name, size, source, license; streamed progress via
  polled pull ~500 ms; resumable by Ollama) → ready-to-chat state.
- **Commands + typed wrappers** (`tauri-commands.ts`): `probe`, `engine_status`,
  `recommendation`, `install_model` (progress polled), `list_models`. Remove `greet` once
  `probe` round-trips (the bridge is proven).
- **Routes:** `/` (onboarding) + `/settings` (engine status pill green/amber/grey, model
  on-disk + delete affordance — doc 02's disk hygiene).

**Shippable:** the author launches the app, sees the honesty screen, installs (or confirms,
if already on disk) one recommended model with consent, and reaches a ready-to-chat state;
settings shows engine status + the on-disk model with delete.

**Verification:**
```bash
workdir=flint
cargo test -p common                  # probe→tier mapping, cookbook recommendation
# Manual dogfood (author, this machine):
# 1. launch: probe reports M5 Pro / 64 GB → tier 64 GB+ → top-tier recommendation
# 2. honesty screen renders; recommendation card names exactly one model + size + license
# 3. install step: top-tier pick is already on disk → verify "confirm" path is a no-op;
#    then delete the model in settings and reinstall via the consent gate (exercises
#    streamed progress + resume)
# 4. settings status pill is green; "Launch Ollama" affordance appears if stopped
# 5. greet is gone from the codebase
```

**Dependencies:** Pre-Phase 0.

**Out of scope:** chat (Phase 1); agent mode, smoke test, permission net, snapshot/undo,
workspace scoping (Phase 2); llama.cpp sidecar + HF downloader (Phase 2); packaging/signing
(Phase 3); `de` locale (post-validation, doc 00).

---

## Phase 1 — chat ("talk to it") — **SHIPPED 2026-08-24**

**Goal:** the user chats with the installed local model in a plain pane; conversations
persist across restarts.

**Execution decisions:** direct Ollama `/api/chat` streaming (opencode is the Phase 2
harness, not v0 chat); **single conversation** persisted at `~/.flint/chat/current.jsonl`
+ New chat (multi-conversation list out of v0); 150 ms poll of a backend-buffered
`VecDeque<ChatEvent>`; cancel keeps the partial assistant reply (honest).

**Builds:**
- **Chat surface** (`common/engine`): extend `EngineBackend` with
  `chat(session_id, messages)` + a buffered `Mutex<VecDeque<ChatEvent>>` (the established
  buffered-output pattern — backend buffers, frontend pulls). Commands `send_chat`,
  `poll_chat_output` (~150 ms poll), `cancel_chat`. Ollama
  `/api/chat` with the tier's `num_ctx`, default temperature. Chat is a no-tools session
  (doc 03 lean — the permission net is never triggered in chat).
- **Chat pane `/chat`** (doc 04's plain chat): message list, streaming bubble, cancel; model
  identity + "runs on this device" line; no parameters, no system-prompt editing; virtualized
  long history (WKWebView rule 5).
- **Transcript persistence:** JSONL per conversation under `~/.flint/chat/` (doc 04's lean —
  JSONL for validation; SQLite only if history UX ever matters). Best-effort, silent failure.
- **Engine health surfacing:** status pill (green/amber/grey) + "Launch Ollama" affordance
  when not running (`open -a Ollama` — `/usr/bin/open`, PATH-safe under doc 05's absolute-path
  rule; **no PATH lookups anywhere**).
- **Settings:** model delete + on-disk disk-usage row (doc 02).

**Shippable:** the author asks the recommended model a question and gets a streamed local
answer; the conversation survives a restart.

**Verification:**
```bash
workdir=flint
cargo test -p common                  # chat buffering, JSONL round-trip, engine mock
# Manual dogfood (author, this machine):
# 1. ask the top-tier model a question → streamed answer renders in the bubble
# 2. cancel mid-stream → run stops, no partial write
# 3. quit + relaunch → conversation loads from ~/.flint/chat/
# 4. stop Ollama → status pill goes amber; "Launch Ollama" relaunches it; chat resumes
```

**Dependencies:** Phase 0.

**Out of scope:** agent mode, smoke test, permission net, snapshot/undo, workspace scoping
(all Phase 2); packaging/signing (Phase 3); `de` locale (post-validation, doc 00).

---

## Phase 2 — agent + safety net — **SHIPPED 2026-08-24**

**Goal:** the author picks a workspace, asks the agent to do file work on the recommended
model, watches typed tool cards, approves/denies each action via inline banners, cancels,
and can restore the pre-run snapshot. Agent mode is gated by tier floor (GB32+) + a passed
smoke test.

**Execution decisions:** **keep Ollama** for the agent (opencode targets
`http://127.0.0.1:11434/v1`; the **llama.cpp sidecar swap + HF downloader moved to
Phase 3**); **installed opencode** (`~/.opencode/bin/opencode`) for validation (bundling is
Phase 3); **fresh `opencode serve` per run** (workspace cwd, isolated config) with
**abort-on-SSE-drop** (fail-closed — resolves doc 03's resync concern; `/permission`
listing routes are non-functional on 1.18.20); config isolation via **`OPENCODE_CONFIG_DIR`
+ `OPENCODE_CONFIG_CONTENT`** (verified: overrides a hostile project `opencode.json`), with
`GET /config` assertion at spawn; **git2 detached snapshot + "Restore snapshot"** (doc 04
staging); path-confinement `tool.execute.before` plugin + `.env` deny defaults as
defense-in-depth; 5-minute auto-deny on unanswered permissions. Verify spike results
recorded in `STATE.md`.

## Phase 3 — standalone shell + engine — **planned 2026-08-24**

**Goal:** a signed, notarized, self-updating `.dmg` that a non-technical user double-clicks
and works with zero second installs — no Ollama, no PATH binaries. Split into **3a (standalone
shell)** and **3b (standalone engine)**: two independent, individually-shippable workstreams.

**Session decisions (2026-08-24):**
- **3a/3b split** (user decision) — packaging/signing/updater + opencode bundling is 3a; the
  llama.cpp swap + HF downloader (removes the Ollama second-install) is 3b.
- **Signing identity: none available yet → blocked.** Zero code-signing identities on this
  machine (`security find-identity`). Phase 3 ships the **unsigned `.dmg` + a signing runbook**
  first; signing/notarization runs the moment a Developer ID Application cert lands (personal).
  Doc 05's rule stands: signing is a day-1 rule once a build leaves
  this machine.
- **llama.cpp provenance: vendor the official release** (user decision) — `llama-server`
  macOS arm64 from llama.cpp GitHub releases, **pinned by SHA-256** (no cmake needed;
  consistent with doc 05's opencode lean over doc 02's build-from-source lean for now).
- **Updater: keys + config now, hosting later** (user decision) — generate the signing
  keypair + wire the updater plugin/pubkey; the hosted update manifest is deferred until a
  real distribution target exists (author rebuilds manually meanwhile).

### Phase 3a — standalone shell (packaging/signing/updater + opencode bundling)

**Goal:** a double-clickable `.dmg` (Gatekeeper-clean once signed) that runs chat + agent with
no PATH lookups — opencode bundled in-app.

**Builds:**
- **M1 signing+notarization spike (do first; doc 05's highest-risk unknown):** sign the app +
  nested sidecar binaries (hardened runtime + entitlements) and notarize/staple with
  `xcrun notarytool`. **Blocked on the cert** → deliver the **signing runbook**
  (`docs/signing-runbook.md`): exact codesign/entitlements/notarytool commands so the
  spike runs the moment the cert lands; produce the unsigned `.dmg` meanwhile.
- **M2 bundle opencode as `externalBin`:** `deno task bundle-opencode` copies the Mach-O arm64
  binary to `src-tauri/binaries/opencode-aarch64-apple-darwin` (the standard externalBin
  recipe); `common::agent::resolve_opencode_path` falls back to the bundle
  (bundle first, then `~/.opencode/bin` for dev); kill the sidecar on quit; status pill
  covers the sidecar.
- **M3 updater keys + config:** `tauri signer generate` → commit the pubkey to
  `tauri.conf.json` updater block, wire the `tauri-plugin-updater` (JS + Rust), build signed
  update artifacts. **Hosted manifest endpoint deferred** until a distribution target.
- **M4 validate:** unsigned `.dmg` double-click on a clean path; once cert lands, the runbook
  run proves the signed+notarized+stapled path.

**Shippable:** a `.dmg` that installs a self-contained app; agent mode works with the bundled
opencode (no `~/.opencode` dependency). Signed path proven via runbook the day the cert lands.

**Verification:**
```bash
workdir=flint
deno task bundle-opencode && cargo build --workspace && deno task build
deno task tauri build                    # produces unsigned .dmg
# manual dogfood: double-click the .dmg → app runs chat + agent with no ~/.opencode/bin
# signing path (once cert exists): follow docs/signing-runbook.md end-to-end → notarized .dmg
```

**Dependencies:** Phase 2. **Out of scope:** the llama.cpp engine swap (3b); HF downloader (3b);
live updater hosting.

### Phase 3b — standalone engine (llama.cpp swap + HF downloader) — **in progress 2026-08-28**

**Goal:** the app's engine is its own bundled `llama-server` over GGUF models it downloads
itself — **no Ollama second-install** (doc 00's standalone thesis, doc 02's swap behind the
same `EngineBackend` trait).

**Execution decisions (2026-08-28):**
- **Full swap to llama.cpp** (user decision) — LlamaCpp is THE active engine for chat +
  install + agent; Ollama's code stays only as a dev fallback (not wired into the app).
- **Engine becomes a lifecycle**, not a daemon: a long-lived `llama-server` sidecar spawned
  with the installed model (`common` has the backend; `src-tauri/src/engine_mgr.rs` owns the
  process: pick-free-port, spawn on demand, `--ctx-size num_ctx`, full Metal offload
  `--n-gpu-layers 999`, kill on quit + on model switch/delete).
- **Agent wiring:** the active engine's OpenAI-compat URL is threaded into opencode's
  provider (`config_content(desc, base_url)`); llama-server **accepts any model id** (verified
  empirically), so opencode's `ollama/<tag>` id works unchanged — no `--alias` needed.
- **llama.cpp pinned: `b10667`** (`deno task fetch-llama-server`, sha256-verified → `~/.flint/bin/`).

**Builds:**
- **M1 provenance/feasibility spike — VERIFIED (2026-08-28):** vendored llama.cpp
  `b10667` (macos-arm64 release, pinned sha256); llama-server loads + serves
  `/v1/models` + streaming `/v1/chat/completions` with `data: [DONE]`;
  **`chat_template_kwargs: {"enable_thinking": false}` works** (no reasoning_content);
  **any model id accepted** (full path, basename, arbitrary tag). **Finding:** the Ollama
  GGUF blob is NOT loadable by llama.cpp (`qwen35moe.rope.dimension_sections` wrong length —
  Ollama's GGUF diverges) → the product downloads bartowski GGUFs. **Tool-calling gate
  PASSED:** the real-stack smoke (opencode → llama-server → Qwen3.5-35B-A3B) wrote
  `result.txt` with "ok" (78 s); top-tier chat answers in ~135 ms with thinking off.
- **M2 HF downloader (`common/downloader`) — DONE:** pure-Rust (reqwest) GGUF download to
  `~/.flint/models/`, **resumable (HTTP Range — verified against the real HF CDN, `206` +
  Content-Range)**, **sha256-verified** (deletes corrupt `.part` on mismatch), streamed
  `PullProgress`, cancel-aware. 5 unit tests (fresh/resume/corrupt/noop/cancel).
- **M3 cookbook HF refs — DONE:** `ModelDescriptor` gains `hf_repo`/`hf_file`/`sha256`;
  all four tiers mapped to **bartowski Q4_K_M GGUFs** pinned by digest (top tier
  `Qwen/Qwen3.5-35B-A3B-Q4_K_M.gguf` = 22,285,080,384 B = the 22.3 GB cookbook size).
- **M4 `LlamaCppBackend` + engine lifecycle + agent wiring — DONE (built):** new
  `EngineBackend` impl (health `/health`, store-scan `list_models`, downloader-backed
  `ensure_model`, file `delete_model`, OpenAI-compat SSE `chat`); `engine_mgr` spawns/kills
  the sidecar and exposes the OpenAI-compat URL for opencode; commands rewired
  (`launch_ollama` → `start_engine`; auto-start before chat/agent/smoke; install completes
  with engine start). Frontend: i18n copy + `hf*` type mirror. **42 common tests green**,
  workspace + frontend builds clean, app boots with the new wiring.
- **M5 validate — headless verification PASSED; UI dogfood pending (author):** the
  uninstall-Ollama dogfood was proven headlessly: **Ollama stopped** → the exact spawn path
  (`engine_mgr::ensure_running`) brings llama-server up with the top-tier model (integration
  test, ~4 s), health green, sidecar killed on quit (no strays); app boots clean with no
  errors. What remains is the human UI pass (install via consent gate on a fresh machine,
  chat + agent + permission net + snapshot restore in the window, restart persistence).

**Shippable:** the author uninstalls Ollama and chat + agent still work — the app is fully
standalone. **Verification:**
```bash
workdir=flint
cargo test --workspace                  # downloader resume/sha256, LlamaCppBackend vs mock
deno task fetch-llama-server            # vendor the pinned engine (sha256-verified)
# manual dogfood: uninstall Ollama → install a tier model via consent gate (resumable) →
# chat + agent run against bundled llama-server; kill app → sidecar dies
```

**Dependencies:** 3a (bundling recipe + status-pill pattern apply to `llama-server` too).
**Out of scope:** multiple models on disk concurrently; GPU-specific tuning UI; provider
abstraction beyond the `EngineBackend` trait.

## Future phases (out of the current plan — sketched, not planned)

- **Phase 4 — distribution:** live updater hosting, wider audience rollout, `de` locale, the
  multi-conversation list if history UX matters (doc 04 lean says JSONL suffices for
  validation).

## Cross-phase rules

- **Doc references are binding.** Every `doc NN` reference points to the source of truth in
  `design/docs/`. If the plan and a doc disagree, the doc wins; this plan revises. The
  plan is sequencing, not design.
- **One phase at a time.** No phase starts until the previous phase's verification passes.
  The model/tier spike (Phase 0) is the only "run this first within the phase" item.
- **WKWebView rules are binding (scaffold CLAUDE.md).** Hash router, no modals/portals
  (inline banners), polling not events (no `tauri::Window::emit`), no top-level `await` in
  `<script setup>`, virtualized lists. Every frontend change in every phase honors these.
- **Deno is the JS toolchain.** `deno.json` tasks + `package.json` deps (`deno install`); no
  Bun, no npm scripts. Rust/Cargo is untouched by this. The pattern is documented in
  `CLAUDE.md` (its `deno desktop` shell is not used; Tauri stays).
- **Data-dir rule.** `~/.flint/` is owned by exactly one function
  (`common::config::data_dir()`); never resolve the store path outside `common`, never use
  Tauri's `app_data_dir`.
- **Absolute paths only.** No PATH lookups anywhere (doc 05); the engine is reached by
  localhost URL, never a PATH binary.
- **Engine behind an interface from day one** (doc 02). Ollama is the v0 implementation; the
  llama.cpp sidecar is the Phase 2 swap behind the same trait.
- **i18n-ready from the first commit** (doc 00): all chrome strings through vue-i18n (en
  catalog); `de` is a later catalog pass, not a refactor.
- **No scope creep.** Each phase's "out of scope" list is binding. If a phase surfaces a need
  for something out of scope, it goes into a future phase or a fresh design pass.
- **Tests per phase.** `cargo test` per crate + manual dogfood. Frontend component tests are
  not in v1 (the WKWebView rules make a real webview test harness expensive; the dogfood is
  the frontend test).
- **The dogfood is permanent.** The author is the first user on this M5 Pro / 64 GB (top
  tier). v0's validation is the author using the recommended model for real work; the
  three-outcomes judgment (absorbed / stays / retires) is made from that usage, not from
  feature count.

## What's next, in order

1. ~~Pre-Phase 0~~ / ~~Phase 0~~ / ~~Phase 1~~ **shipped 2026-08-24** (scaffold → probe →
   cookbook → engine → install → chat).
2. ~~Phase 2 — agent + safety net~~ **shipped 2026-08-24** (opencode harness, smoke-test gate,
   permission net, git2 snapshot/restore, `/agent` view).
3. **Phase 3 — standalone shell + engine** (planned 2026-08-24): **3a** = signing/
   notarization (blocked on cert → unsigned `.dmg` + runbook first) + opencode bundling +
   updater keys; **3b** = llama.cpp swap + HF downloader (no Ollama). Starts with 3a-M1.

As each phase ships, this plan's "What's next" and `STATE.md` should be updated to reflect
the new state.
