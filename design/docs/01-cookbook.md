# 01 — The cookbook (hardware probe → one model)

> Flint's core organ and its market differentiator: the machine is probed, one model is
> recommended, the user never sees a catalog. Named after odysseus's "Model Cookbook" concept
> (UX reference only — see doc 00's AGPL bright line).

## Scope

The hardware probe, the tier table that maps hardware to a single recommended model, the
capability gate that decides whether agent mode unlocks, the first-run smoke test, and the honesty
screen. Out of scope: how the model is served (doc 02), what the agent does with it (docs 03/04).

## Current leaning

- **Probe:** chip identity + generation and physical RAM via `sysctl` (`machdep.cpu.brand_string`,
  `hw.memsize`) — no third-party detection library, no network. Apple Silicon is the target;
  unified memory makes RAM the single dominant sizing factor, which is what makes a simple tier
  table honest.
- **Tier table (~4 RAM tiers → exactly one pick each).** Shape, not content (content needs the
  model spike below):
  | Tier | Chat | Agent mode |
  |---|---|---|
  | 8 GB | small model, expectations set low | locked |
  | 16 GB | mid model | locked by default — smoke test may unlock (open) |
  | 32 GB | capable model | unlocked after smoke test |
  | 64 GB+ | strong model | unlocked after smoke test |
  One GGUF (or MLX — doc 02) artifact per tier, pinned by exact file/revision, quantization chosen
  by us and never surfaced. The user sees "your Mac: M2, 16 GB → we recommend *X*" and a single
  download button.
- **Agent-capability gate = tier floor + first-run smoke test.** The established warning transfers
  and intensifies: "format compatibility ≠ capability", and a model that mangles tool calls
  disappoints far harder in agent mode than in chat. The smoke test is a short scripted agentic
  run in a temp directory (read a file, edit it, report) scored pass/fail; on fail, agent mode
  stays locked with a plain-language reason. Chat is available on every supported tier regardless.
- **Maintenance model: build-time-pinned table with a dated "as of", refreshed via app updates.**
  The table is the product and it rots monthly — the volatility that motivates Flint also churns
  its core asset. Pinning inherits the no-silent-remote-fetch posture; the UI shows
  "recommendations as of 2026-07" so staleness is honest rather than hidden.
- **Honesty screen (first run, before any download):** what local models are good for (private
  drafting, summarizing, working files offline, resilience), where they fall short of
  ChatGPT/claude.ai (reasoning depth, speed on big contexts), and — per tier — what *this machine*
  can expect. On the 8 GB tier this includes saying outright that a chat website will be more
  satisfying for most work (doc 00's honest-fit stance).

## Alternatives considered

- **Ranked list / fit scores (whichllm, llmfit style)** — rejected: scores and lists re-import the
  choice burden Flint exists to remove. One pick; the tier table *is* the ranking, done offline by
  us.
- **Runtime benchmark on the user's machine to pick the model** — rejected for v1: minutes of fan
  noise before first value, and it measures throughput, not capability. The smoke test gates a
  *feature*, it doesn't select the model.
- **Remote-fetched table (always fresh)** — rejected for now: silent remote dependency contradicts
  the inherited privacy posture and reintroduces a vendor-shaped SPOF into the resilience app.
  Revisit as an explicit "check for new recommendations" button if update cadence becomes a
  problem (trigger below).
- **Supporting Intel Macs via an external-engine fallback** — leaning against even
  chat-tier support: no unified memory, no Metal MLX path, and the capability floor drops below
  the honesty bar. Open question below.

## Resolved (2026-07-06)

- *Catalog or recommendation?* → **One recommended model per hardware tier; no catalog, no
  quantization jargon, no parameter UI** (radical opinionation — the differentiator, doc 00).
- *Same features on all hardware?* → **No — capability-tiered.** Chat everywhere (supported
  tiers); agent mode gated by tier floor + smoke test.
- *Does Flint hide the capability gap?* → **No** — the honesty screen states it per tier, before
  download.

## Open questions

- **Which models seed the table?** Needs an empirical spike: current open-weights models with
  solid tool-calling at each RAM tier, tested *through opencode* (doc 03), not just in chat.
  Candidate families to evaluate as of 2026-07: Qwen3, gpt-oss, Llama, Gemma, Mistral — but the
  spike decides, not the doc. Lean: prefer models with native tool-calling templates over
  prompt-hacked ones.
- **Where exactly does the agent bar sit?** Lean: 32 GB+ unlocks after smoke test; whether a
  16 GB machine can pass is an empirical question the spike answers — design the gate so the smoke
  test is the authority and the tier floor is just its precondition.
- **Intel Macs: chat-only tier or unsupported?** Lean: **unsupported** in v1 with an honest
  message ("Flint needs Apple Silicon"), revisited only if a real Intel user shows up. Simpler
  than carrying a second engine path (doc 02) for a shrinking hardware base.
- **Smoke test content** — what tasks, how scored, how long. Lean: ≤3 scripted steps in a temp
  dir, hard pass/fail on tool-call validity (not output quality), under a minute.
- **Re-probe cadence** — RAM never changes but the table does; re-run gate checks when the app
  updates the table version. Lean: probe on launch (cheap), re-run smoke test only on model or
  table change.

## Decision triggers

- If the pinned table is embarrassingly stale between releases → add the explicit
  "check for new recommendations" button (user-initiated fetch, shown transparently) before
  considering any silent refresh.
- If smoke-test pass rates are flaky for a tier's model → demote the tier to chat-only rather
  than shipping an unreliable agent (demote-on-failure precedent).
- If users (eventually) ask for model choice → offer at most a "capable vs fast" toggle within
  the tier, never a catalog.
- If the 8 GB tier's honest answer is always "use a chat website" → drop the tier's model download
  entirely and let the honesty screen be the whole 8 GB experience.
