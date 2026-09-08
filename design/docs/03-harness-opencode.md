# 03 — The agent harness: opencode

> What turns Flint from "chat with a local model" into "do real work with a local model": an
> opencode sidecar drives the agentic loop (files, commands, multi-step work) against the doc 02
> engine, and Flint's GUI drives opencode. opencode was the verdict of the multi-backend harness
> comparison (2026-06-16); Flint is where that verdict gets exercised for real.

## Scope

How Flint spawns, configures, drives, and intercepts opencode. Out of scope: the model behind it
(docs 01/02), what the permission/safety UI looks like (doc 04 — this doc supplies its hook).

## Current leaning

- **opencode as a bundled sidecar in headless server mode.** `opencode serve` exposes an HTTP API
  (OpenAPI, typed JS/TS client) — verified 2026-07-06. Flint spawns it bound to `127.0.0.1`,
  scoped to the selected workspace folder, and talks HTTP from the Tauri/Rust side. This is
  arguably cleaner for a GUI than driving a CLI and parsing its stdout: a real API
  instead of a parsed stdout stream.
- **Why opencode (spike verdict transfers):** MIT license; **single self-contained binary** (Bun-
  compiled, no runtime dependency — bundleable, unlike pi); **`tool.execute.before` plugin hook**
  = external per-tool permission interception, the seam a layperson safety net requires; provider-
  agnostic (OpenAI-compat endpoints, so the doc 02 sidecar plugs in directly).
- **Permission interception — two layers (revised by the 2026-07-06 verify spike, docs-level):**
  1. **Primary: opencode's native permission system over the server API.** Flint's owned config
     sets `"permission": {"*": "ask", ...}`; the server then emits **`permission.asked`** on its
     SSE event stream (`GET /event`) and **holds the tool call until a client replies** via the
     permission-response endpoint (`POST /session/:id/permissions/:permissionID` with
     `{response, remember?}`). Flint's Rust side subscribes, doc 04 renders the banner, the answer
      goes back over HTTP. This is the permission-bridge shape *built into* opencode — better than
      the originally planned custom hook: enforced by opencode core, so it doesn't depend on a
      plugin loading. An unanswered "ask" blocks rather than proceeds (fail-closed by blocking);
      Flint adds its own auto-deny timer (the 5-min consent-gate precedent — port
      the pattern, not the modal).
  2. **Defense-in-depth: a Flint-owned `tool.execute.before` plugin** for workspace path
     confinement only (throw on file-tool args outside the workspace root). Verified semantics:
     the hook is async, receives `input.tool` + `output.args`, and **throwing aborts the tool
     call** — so the plugin's failure mode on bad input is closed. If the plugin fails to *load*,
     layer 1 still gates everything; the layers fail independently.
  Bash pattern rules (`"bash": {"*": "ask", ...}`, last-match-wins) and per-tool deny defaults
  (e.g. `.env` reads denied out of the box) come free with layer 1.
- **Config isolation: Flint owns opencode's config entirely** (own config dir/env, own plugin
  registration, own provider config pointing at the doc 02 endpoint). Note the deliberate tension
  with the "reuse the user's own CLI config" alternative (that choice exists where users own a
  `claude` CLI and must graduate to it). Flint's audience has no
  pre-existing opencode, there is no graduation story toward it, and an unmanaged user config
  could silently weaken the permission net. Isolation wins here.
- **Streaming → polling:** whatever event/SSE surface the server offers gets adapted to the
  WKWebView-safe polling model on the Rust side (the WKWebView rule, CLAUDE.md; the buffered
  150 ms poll pattern is the copy source). The frontend never holds a long-lived
  stream.
- **Tool scope for v1: deliberately narrow.** File read/write/edit within the workspace folder,
  plus shell execution *only* behind the permission gate. Everything else opencode offers (MCP
  servers, sub-agents, web tools) stays off until validation says otherwise.

## Alternatives considered

- **pi** — rejected (multi-backend comparison): Node-only, so Node becomes a blocking prerequisite;
  fights the standalone requirement the same way an experimental Bun bridge did. Its minimalism
  is otherwise the closest philosophical match; re-evaluate only if it ships a self-contained
  binary.
- **`claude` CLI as the harness** — rejected by the thesis itself: the vendor-resilience chain
  must not contain the vendor. (In the "absorbed" outcome the roles invert: the absorbing home
  keeps `claude` and gains opencode as backend #2.)
- **Driving the opencode TUI via PTY** (a runner pattern) — rejected: scraping a
  TUI when a real HTTP API exists is strictly worse; the PTY runner remains relevant only if some
  install step ever needs it.
- **Building our own agent loop over the raw completion API** — rejected: re-implements tool
  orchestration, context management, and session state that opencode already does well; Flint's
  value is the packaging and the net, not the loop.

## Resolved (2026-07-06)

- *Which harness?* → **opencode** (single binary, MIT, serve+SDK, `tool.execute.before`); **pi
  rejected** (Node-only). The multi-backend comparison verdict adopted.
- *Does Flint reuse the user's opencode config if present?* → Lean recorded as **no — full
  isolation** (see above); confirm during the verify spike before hardening.
- *Harness for chat too, or only agent mode?* → Lean: **opencode sessions for both**, with chat as
  a no-tools session — one code path, and the permission net is simply never triggered in chat.
  Confirm in the verify spike (a raw completion call to doc 02's endpoint is the fallback for the
  chat pane if sessions prove heavy).
- *Can the permission gate block on an external decision, and does it fail closed?* →
  **Yes — verified at docs level 2026-07-06.** Native permission system: `"*": "ask"` config +
  `permission.asked` over SSE + reply endpoint; the tool call is held until a reply arrives
  (unanswered = blocked, not proceed). `tool.execute.before` verified async/blocking with full
  `input.tool`/`output.args`, throw aborts the tool. Architecture revised to native-primary +
  plugin-as-path-confinement (Current leaning). Session lifecycle confirmed API-complete:
  `POST /session`, `/session/:id/message`, `/session/:id/prompt_async`, `/session/:id/abort` —
  the one-session-per-run lean and doc 04's cancel semantics have their endpoints. **Empirical
  confirmation on the pinned version remains** (first implementation task, not a gating unknown —
  see Open questions).

## Open questions

- **Empirical confirmation on the pinned version** (residual of the resolved verify spike; first
  implementation task): (a) `permission.asked` **payload completeness** — an upstream issue
  reports the tool field not always rendering in clients, so confirm the event carries full
  tool name + args for every gated tool on our pin; (b) the **reply endpoint shape** — both
  `POST /session/:id/permissions/:permissionID` and `POST /permission/:requestID/reply` appear
  across versions (API churn is real; the pin + Rust-trait wrap below is the mitigation);
  (c) **config assertion at spawn** — layer 1 fails open only if the permission config is wrong,
  so Flint must read back the effective config on sidecar start and refuse agent mode if
  `"*": "ask"` isn't in force.
- **SSE reconnect resync.** Events are not persisted server-side — a dropped SSE connection can
  miss a `permission.asked`, leaving a run blocked with no banner. The Rust side must resync
  pending permissions via REST after reconnect (heartbeats every 30 s help detect drops). Lean:
  treat "SSE reconnected" as "refresh session + pending-permission state", always.
- **Session lifecycle mapping** — how opencode sessions map to Flint runs (one session per run vs
  persistent per workspace), cancel semantics, and transcript retrieval for doc 04's history.
  Lean: one session per run for v1; simplest to reason about and to snapshot around.
- **Version pinning** — opencode moves fast. Lean: pin the sidecar per Flint release (same policy
  as doc 02's engine; both ride the Tauri updater), record the tested version in the build.
- **Workspace confinement depth** — is cwd-scoping enough, or does the permission plugin also
  path-check file-tool args against the workspace root? Lean: plugin path-checks too;
  prior path-traversal debt is the ready-made checklist; close these from day one.

## Decision triggers

- If the empirical check finds `permission.asked` payloads missing tool/args on the pinned
  version → hold the pin back to a version where they're complete, or enrich the banner from the
  session state REST API; never render an "Allow?" banner that can't say what it's allowing.
- If the spawn-time config assertion ever finds the permission config not in force → refuse agent
  mode outright (chat stays available); this is the layer-1 fail-open guard.
- If opencode's API churns painfully across pins → wrap the SDK surface Flint uses behind a thin
  Rust trait (same seam pattern as doc 02) so upgrades are localized.
- If chat-as-session proves heavy or flaky → drop the chat pane to a raw completion call against
  doc 02's endpoint and keep sessions for agent mode only.
- If validation wants MCP or web tools → each new tool class enters through the same permission
  net and gets its own honesty note; nothing bypasses the gate.
