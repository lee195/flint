# Flint — design session state

> Snapshot for picking the work back up. Created 2026-07-06 (project inception: use-case scoping,
> harness decision, codename); updated 2026-08-24 (v0 scoping session — decisions below).
> **Status 2026-09-08: Phases 0–3b shipped.** The design docs now live inside the repo
> (`design/`), and ownership of the project is with **jlee** (personal project, standalone).
> `PLAN.md` is the phased implementation plan.

## What Flint is (one paragraph)

A Vue 3 + Tauri 2 standalone macOS app that gives a layperson working local AI with zero dev-tool
gates: it probes the machine's hardware, recommends **one** model that actually fits (the
"cookbook"), downloads it on consent, and offers both a plain **chat** and — on capable hardware —
an **agentic mode** (files, commands, multi-step work) driven by an **opencode** sidecar against a
bundled local inference engine. Its reason to exist is **vendor resilience**: the AI market is
volatile, and a single-point-of-failure hedge must be independent of any vendor's cloud *and* of
`claude` end-to-end (a "private mode" of a claude-based tool addresses privacy, not vendor
independence — its engine is still the vendor). Codename **Flint** — the thing that makes fire
with no external supply. Data dir `~/.flint/`.

## Validation stance

The product is validated **through usage**, not feature count: the author using chat + agent for
real work is the test. Three outcomes are all success — the project is absorbed into a larger
home, stays as the standalone resilience app, or retires. Prior-art rule (inherited practice):
**copy/adapt, never depend** — pattern lessons are internalized into this repo's docs, never
imported as code. **odysseus is UX reference only, never code** (AGPL-3.0 — see doc 00).

## Decision ledger (2026-08-28, Phase 3b — standalone engine, in progress)

- **Full swap to llama.cpp** (user decision) — LlamaCpp is THE active engine for chat +
  install + agent; Ollama stays only as a dev fallback (not wired into the app).
- **Engine = lifecycle, not daemon:** a long-lived `llama-server` sidecar spawned with the
  installed model (port via `pick_free_port`, `--ctx-size num_ctx`, full Metal offload,
  kill on quit + model switch/delete). The `EngineBackend` trait is unchanged; the app holds
  an `Arc<dyn EngineBackend>` behind `engine_mgr` (`src-tauri/src/engine_mgr.rs`).
- **llama.cpp pinned `b10667`** — official macos-arm64 release, sha256-verified by
  `deno task fetch-llama-server` into `~/.flint/bin/` (dev; release bundles via 3a-M2
  externalBin). Vendored over build-from-source per the Phase 3 session decision.
- **M1 spike plumbing verified:** llama-server serves OpenAI-compat `/v1` with
  `data: [DONE]`; **`enable_thinking:false` via `chat_template_kwargs` works**;
  **any model id accepted** (opencode's `ollama/<tag>` needs no alias). **Finding:** the
  Ollama GGUF blob (qwen35moe) is NOT loadable by llama.cpp — `qwen35moe.rope.dimension_sections`
  wrong array length (Ollama's GGUF diverges) → the app downloads **bartowski Q4_K_M GGUFs**.
  **Tool-calling gate PASSED (2026-08-28):** real-stack smoke (opencode → llama-server →
  Qwen3.5-35B-A3B) wrote `result.txt` with "ok" (78 s); top-tier chat ~135 ms with thinking off.
- **Top-tier mapping resolved:** `qwen3.6:latest` (Ollama) = **Qwen3.5-35B-A3B** (qwen35moe,
  Q4_K_M, 22,285,080,384 B, Apache-2.0) at
  `bartowski/Qwen_Qwen3.5-35B-A3B-GGUF/Qwen_Qwen3.5-35B-A3B-Q4_K_M.gguf`, sha256
  `2f2df1e8…4751ab`. Lower tiers similarly mapped (Qwen3-4B/8B/14B Q4_K_M, pinned-but-untested,
  doc 01).
- **M2/M3/M4 built:** `common::downloader` (resumable Range — verified on the real HF CDN —
  + sha256 + `PullProgress`), cookbook HF refs, `LlamaCppBackend`, `engine_mgr`, agent base-URL
  threading, `start_engine` command. **42 common tests green; workspace + frontend build clean;
  app boots with the new wiring.** **M5 headless pass:** with **Ollama stopped**, the app's
  exact spawn path (`engine_mgr::ensure_running`) brings llama-server up with the top-tier
  model (~4 s), health green, sidecar killed on quit (no strays). Remaining: the author's UI
  dogfood (consent-gate install on a fresh machine, chat/agent/permission/snapshot in the
  window, restart persistence).
- **Signing (3a-M1/M3) skipped for now** (user decision) — unsigned `.dmg` + runbook stand;
  the signing spike runs when a Developer ID cert lands.

## Decision ledger (2026-08-24, Phase 3 — standalone shell + engine, planned)

- **Split 3a / 3b** (user decision): **3a** = packaging/signing/updater + bundle opencode as an
  `externalBin` (the standalone shell — runs chat + agent with no `~/.opencode` dependency).
  **3b** = the llama.cpp engine swap + HF downloader (removes the Ollama second-install; doc 02's
  swap behind the same `EngineBackend` trait). Individually shippable; 3a first.
- **Signing identity: none available yet → BLOCKED.** `security find-identity -v -p codesigning`
  reports 0 identities on this machine. 3a ships the **unsigned `.dmg` + a signing runbook**
  (`docs/signing-runbook.md`) first; signing/notarization runs the moment a Developer ID
  Application cert lands (personal). Doc 05 rule: signing is day-1 once a
  build leaves this machine. `xcrun notarytool` available via CLT.
- **llama.cpp provenance: vendor the official release** (user decision) — `llama-server` macOS
  arm64 pinned by SHA-256 (no cmake; supersedes doc 02's build-from-source lean for now).
- **Updater: keys + config now, hosting later** (user decision) — `tauri signer generate` +
  wire the updater plugin/pubkey; hosted manifest deferred until a distribution target exists.
- **Environment canonicalized** in `docs/06-env.md` (machine facts, versions, paths, commands,
  cert status) so sessions start lean.
- **Nested-sidecar notarization** (doc 05's highest-risk unknown) is 3a-M1, gated on the cert.

## Decision ledger (2026-08-24, Phase 2 — agent + safety net, shipped)

- **Engine:** keep **Ollama** for the agent (opencode → `127.0.0.1:11434/v1`); **llama.cpp
  sidecar swap + HF downloader moved to Phase 3**. **Installed opencode** for validation
  (bundling is Phase 3). **Fresh `opencode serve` per run** + **abort-on-SSE-drop**
  (fail-closed — resolves the doc 03 resync question: `/permission` listing routes are
  non-functional on 1.18.20).
- **Isolation:** `OPENCODE_CONFIG_DIR` (Flint plugins dir) + **`OPENCODE_CONFIG_CONTENT`**
  (inline config, **verified to override a hostile project `opencode.json`**) + `GET /config`
  assertion at spawn (refuse if `"*":"ask"` not in force). `.env` reads denied by default;
  `tool.execute.before` path-confinement plugin (defense-in-depth) written to
  `~/.flint/opencode/plugin/` at startup.
- **Gate:** tier floor (GB32+) AND a passed capability smoke test (`common::smoke`, scripted
  write/read in a temp dir, scored on file effect). Verified on this machine: qwen3.6 through
  opencode created `result.txt` (real-stack integration smoke, `cargo test -p common
  real_opencode_smoke -- --ignored`).
- **Safety net:** `git2` detached snapshot per workspace under `~/.flint/snapshots/` before
  each file-writing run + "Restore snapshot" (staging per doc 04; full diff-viewer polish
  deferred to the non-author trigger). Retention: last 10 repos, oldest pruned.
- **Permission UX:** inline banners (tool + pattern), Allow→once / Deny→reject via
  `POST /session/{id}/permissions/{pid}`; 5-min auto-deny; no modals (WKWebView rule).
- **Verified:** 30 common tests green; app boots with the dialog plugin + confinement plugin;
  snapshots use temp roots in tests (no data-dir pollution).

## Decision ledger (2026-08-24, Phase 2 M1 verify spike — opencode 1.18.20)

Pinned empirically against the installed `opencode serve` (isolated config, local Ollama):
- **API:** `POST /session` `{title, model:{id,providerID}}`; `POST /session/{id}/message`
  `{parts:[{type:"text",text}]}` (parts required, not `content`); `POST /session/{id}/abort`;
  `GET /event` (SSE bus: `message.part.delta`, `permission.asked`, `session.status`
  busy/idle/error); `GET /session/{id}/message` (parts for final text); `GET /config`
  (assertion); `GET /global/health`.
- **Permission:** `permission.asked` payload `{id, sessionID, permission, patterns,
  metadata, always, tool:{messageID,callID}}`; reply `POST /session/{id}/permissions/{pid}`
  `{"response":"once"|"always"|"reject"}` (NOT doc 03's `{response,remember?}` guess);
  held-until-answered (fail-closed by blocking) — verified: run paused until we replied.
- **Isolation:** `OPENCODE_CONFIG_CONTENT` (inline) **overrides a project `opencode.json`** —
  verified with a hostile `{"permission":{"*":"allow"}}` in the cwd; effective config still
  `{"*":"ask"}`. `OPENCODE_CONFIG_DIR` = Flint's plugins dir. User's global config merges
  (providers/plugins) — acceptable; Flint forces model per session. `GET /config` returns the
  effective permission for the spawn-time assertion.
- **Resync:** `/session/{id}/permission*` listing routes are non-functional (serve the SPA).
  Resolution by design: **fresh `opencode serve` per run + abort-on-SSE-drop** (fail-closed);
  message-poll for progress/final text.
- **Tool-calling:** **qwen3.6 through Ollama produces valid tool calls via opencode** (read
  fired, permission held→granted→result→correct answer). Resolves doc 01's "format
  compatibility ≠ capability" risk for the top tier.

## Decision ledger (2026-08-24, Phase 1 — chat, shipped)

- **Single conversation + New chat** (user decision): one persisted conversation at
  `~/.flint/chat/current.jsonl` (JSONL, `common::chat_store`, best-effort silent failure);
  multi-conversation list is out of v0.
- **Direct Ollama `/api/chat` streaming** for v0 chat (raw completion — opencode is the
  Phase 2 harness, not v0). Streamed into a backend `VecDeque<ChatEvent>`, frontend polls
  at 150 ms; `ChatEvent::Done` carries the full reply for persistence; cancel keeps the
  partial assistant reply (honest).
- **Verified:** 20 common tests green (chat streaming vs httpmock + real-Ollama smoke),
  `deno task build` clean, app boots with no errors, real streamed reply reassembles
  deltas → full.

## Decision ledger (2026-08-24, Phase 0 planning — see PLAN.md)

- **Model/tier spike resolved** (measured on this machine): top tier = **`qwen3.6:latest`
  with `think:false`** — 357 ms vs 14.8 s with thinking on; `qwen3-coder` rejected for general
  chat. Lower tiers seeded pinned-but-untested: `qwen3:4b` / `qwen3:8b` / `qwen3:14b`.
- **Engine HTTP client: reqwest + tokio** (async, streaming) — engine impl (`OllamaBackend`)
  lives in `common/`, behind an async trait (`async-trait`).
- **Probe via `libc::sysctlbyname`** (no subprocess), not `/usr/sbin/sysctl`.
- **Phase 0 scope confirmed:** probe + cookbook + engine interface + install only; chat stays
  Phase 1.

## Decision ledger (2026-08-24, v0 scoping session — see PLAN.md)

- **v0 = Phase 0 + 1** (user decision): scaffold + probe + cookbook + engine interface +
  install + chat. Agent mode (Phase 2) and packaging/signing (Phase 3) are out of v0. This
  resolves STATE.md's "What's next" item 1 → the phased plan now exists (`PLAN.md`).
- **Engine: Ollama backend first** (user decision; fires doc 02's trigger — the author
  already runs Ollama). Ollama 0.32.15 running with `qwen3.6` (36B) + `qwen3-coder` (30.5B)
  on disk. The llama.cpp sidecar is the Phase 2 swap behind the same `EngineBackend` trait.
- **Toolchain: Deno** (user decision) — JS runtime + task runner (`deno.json` tasks +
  `package.json` deps, `deno install`), in place of Bun. Tauri 2 + Rust stays.
  The Deno + Vue + Vite pattern is documented in `../CLAUDE.md` (its experimental
  `deno desktop` shell is not used).
- **Author dogfoods the top tier** — this machine is an Apple M5 Pro / 64 GB (Flint's top
  tier); the other tiers ship pinned-but-unvalidated with an honest "as of" date (doc 01).
- **Plan artifact:** `PLAN.md` written (goal/builds/shippable/verification/dependencies/
  out-of-scope per phase, cross-phase rules, future Phase 2/3 sketch).

## Decision ledger (2026-07-06, inception session)

- **Separate app to validate through usage** (user decision) — standalone, not folded into
  another roadmap.
- **Codename Flint** ("for now").
- **Scope: cookbook + chat + agentic harness only.** Explicitly NOT odysseus's workspace modules
  (email, calendar, notes, documents editor, deep research, compare mode) — those serve the
  self-hoster enthusiast, a different audience.
- **Harness: opencode.** MIT, single self-contained binary (no runtime deps), `opencode serve`
  headless HTTP API + typed SDK, `tool.execute.before` permission hook — the only non-Claude CLI
  that cleared both seams (permission hook + headless API) in the multi-backend comparison.
  **pi rejected** (Node-only →
  Node becomes a blocking prereq, fights "no system dev tools").
- **Radical opinionation:** hardware probe → one recommended model per RAM tier. No catalog, no
  quantization jargon, no parameter UI. (The market gap: LM Studio/Jan/Ollama are catalog-shaped;
  whichllm/llmfit have the right idea but are terminal tools.)
- **Capability-tiered features:** chat on all supported tiers; **agent mode gated** by hardware
  tier + a first-run capability smoke test (precedent: "format compatibility ≠ capability" —
  agentic tool-calling disappoints far harder than chat on weak models).
- **Stack:** Tauri 2 + Vue 3; the WKWebView constraints apply wholesale (polling over events,
  no modals/portals, hash router, no `window.prompt`).
- **Engine behind a backend interface from day one** (pluggable backends principle) — llama.cpp
  sidecar is the lean, Ollama/oMLX stay swappable (doc 02 here).
- **Honest-fit stance** (established practice): the first screen says what local models can
  and can't do vs ChatGPT/claude.ai; "your machine can't run anything satisfying — a chat website
  is the honest answer" is a reachable conclusion, not prevented.

## Where things live

- **Exploration docs:** `design/docs/00`–`06` — thesis/positioning, cookbook, engine &
  sidecars, opencode harness, safety net & run UX, packaging, environment/toolchain facts.
  Each keeps alternatives open with a stated lean; conversation decisions are in the
  decision ledgers above. `06` is operational (machine facts), not design.
- **Implementation plan:** `design/PLAN.md` — the phased plan (Pre-Phase 0 bootstrap +
  Phase 0 probe/cookbook/engine/install + Phase 1 chat + Phase 2 agent + Phase 3 shell/engine).
- **The app:** this repo root — Tauri 2 + Vue 3 + Deno toolchain, Cargo workspace
  (`common/` + `src-tauri/`).
- **External prior art:** `github.com/pewdiepie-archdaemon/odysseus` (AGPL — UX reference only).

## What's next, in order

1. ~~Pre-Phase 0~~ / ~~Phase 0~~ / ~~Phase 1~~ / ~~Phase 2~~ **all shipped 2026-08-24** — the
   v0 slice plus the agent + safety net are complete and dogfoodable on this machine.
2. **Phase 3b — standalone engine** (see `PLAN.md`): M1 gate **passed** (tool-calling via
   llama.cpp), M2–M4 built + tested, M5 headless pass done, code committed. **Remaining:**
   the author's UI dogfood (chat/agent/install in the window with Ollama off).
   3a (signing/bundling) is paused on the Developer ID cert.
3. **Phase 4 — distribution** (sketched): live updater hosting, wider rollout, `de` locale.

The remaining validation task is living with it: the author using chat + agent for real work
is the test; the three-outcomes judgment (absorbed / stays / retires) is made from that usage.

## Verification when resuming

```bash
# STATE.md + PLAN.md + 7 exploration docs (00–06):
ls design/ design/docs/

# No odysseus code/config references (UX mentions only — hits must be prose, never paths/snippets):
grep -rni "odysseus" design/docs/
```

## How to pick up

Read this file, then `PLAN.md` (the phased implementation plan), then docs `00` → `06` in
order for the design detail behind each plan step (`06` is machine/env facts for fast
onboarding). The binding engineering rules (WKWebView constraints, Deno toolchain, data-dir,
absolute-paths) are in `../CLAUDE.md`. Next deliverable: **Phase 3b UI dogfood** (the author
using chat + agent with Ollama off), per `PLAN.md`.
