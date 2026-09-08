# 05 — Packaging & distribution

> Packaging is not a final phase here — it is the product thesis. Flint's entire differentiation
> over odysseus is "double-click a `.dmg` and it works", so the packaging warnings that other
> desktop-app projects collect as late-phase lessons apply to Flint from day 1.

## Scope

Signing/notarization, bundling the two sidecars, updates, the PATH problem, and size budget. Out
of scope: what the sidecars do (docs 02/03).

## Current leaning

- **Signed + notarized `.dmg` from the first build that leaves the dev machine.** Shipping
  unsigned is the cautionary tale; for a layperson audience Gatekeeper
  friction is fatal, and retrofitting signing across sidecars is worse than starting with it.
- **Two bundled sidecars** — llama.cpp server (doc 02) and opencode (doc 03) — as Tauri sidecar
  binaries, each pinned per release. Both must be part of the signing/notarization scope
  (hardened runtime, correct entitlements for JIT if the engine needs it — verify item below).
- **Login-shell PATH is never trusted.** A documented, unfixed Finder-launch bug
  (a packaged `.app` gets launchd's minimal PATH, silently breaking every spawn) bites Flint on
  day 1 because everything is spawned. Policy: **absolute paths only** — sidecars resolved inside
  the app bundle, user-facing paths through Tauri APIs; no `which`, no PATH lookups, anywhere.
- **Updates via the Tauri updater**, carrying app + both sidecars + the cookbook table (doc 01) as
  one versioned unit. One update mechanism, no self-updating components (affordable because Flint
  bundles).
- **Size budget:** app + sidecars well under ~100 MB; models are never bundled (doc 02) — the
  `.dmg` stays mail-able and the multi-GB cost is a consented, resumable download.
- **Distribution for validation phase:** direct `.dmg` hand-off (it's one user). Post-validation
  distribution (website, share) is deferred with the doc 00 rollout question.

## Alternatives considered

- **Ship unsigned during validation ("it's just me")** — rejected: signing is exactly the kind of
  infrastructure that never lands if deferred, and the validation build *is* the packaging
  test — an unsigned Flint validates nothing about the thesis.
- **Homebrew/CLI distribution** — rejected: the audience definitionally can't; Homebrew is not a
  path for this audience.
- **Mac App Store** — rejected for validation: sandbox entitlements vs spawning sidecars and
  writing user-chosen folders is a fight with no payoff at this scale.
- **Bundling a default model in the installer** — rejected (doc 02): multi-GB installer, license
  redistribution questions, and the cookbook's whole point is picking per-machine.

## Resolved (2026-07-06)

- *Is packaging a later phase?* → **No — day-1 concern**; the standalone requirement *is* the
  thesis (doc 00), and the late-phase warnings (PATH capture, sign from the start,
  no unbundled runtimes) are adopted as day-1 rules here.
- *How many update mechanisms?* → **One** (Tauri updater); sidecars and cookbook table ride it.

## Open questions

- **Notarization of nested sidecar binaries** — hardened-runtime flags and entitlements for a
  Bun-compiled binary (opencode) and a Metal-using engine (llama.cpp: does MTLCompiler need
  JIT-adjacent entitlements?). Needs a build-time spike; this is the highest-risk unknown in the
  doc.
- **Signing identity** — personal Developer ID. Lean: whichever unblocks
  validation (personal), revisited with the doc 00 rollout question.
- **opencode binary provenance** — vendor the official release binary vs build from source in CI.
  Lean: official release pinned by digest (unlike doc 02's build-from-source lean for llama.cpp —
  opencode's Bun build chain is heavier to own); revisit if the digest/signing story is weak.
- **Crash/failure surface for sidecars** — what the user sees when a sidecar dies (engine OOM is
  the realistic one). Lean: status pill pattern (green/amber/grey) + plain-language
  restart affordance; no logs in the UI.

## Decision triggers

- If the notarization spike fails on either sidecar → that sidecar's provenance decision reopens
  (build from source with our own signing vs alternative binary) before any other packaging work.
- If validation hand-off expands beyond the author → the distribution question (where the `.dmg`
  lives, repo home) must be resolved first; no ad-hoc sharing.
- If update payloads annoy (full sidecar re-download for a table tweak) → split the cookbook table
  into a separately-versioned resource *within* the updater flow — still one mechanism, smaller
  payloads.
