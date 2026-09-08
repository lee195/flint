# 02 — Inference engine & sidecars

> The layer that makes Flint truly standalone: a bundled local inference engine behind a backend
> interface, plus consent-gated model downloads. "Standalone" is the hard requirement — no Docker,
> no Python, no Node, no separate installs, double-click and go.

## Scope

Which engine serves the model, how it's bundled and driven, the backend interface that keeps the
choice swappable, and model download/storage. Out of scope: which model (doc 01), who consumes the
endpoint (doc 03 — opencode is the client, not the GUI).

## Current leaning

- **Engine: `llama.cpp` server bundled as a Tauri sidecar.** MIT-licensed, a single static binary,
  Metal acceleration on Apple Silicon, and an OpenAI-compatible HTTP API including the tools/
  function-calling endpoints opencode speaks. It is the engine LM Studio, Jan, and Ollama all build
  on — Flint just skips their catalogs. Spawned/managed by the Rust side (start on demand, health
  check, stop on quit), bound to `127.0.0.1` on an ephemeral port, never `0.0.0.0`.
- **Backend interface from day one** (pluggable-backends principle). The Rust trait is the seam:
  roughly `start(model) / stop() / health() / base_url()` + a capability descriptor (wire format,
  tool-calling support). `llama.cpp` is the only implementation that ships in v1; Ollama and oMLX
  remain paper implementations documented here so the seam stays honest.
- **Engine updates ride app updates.** The sidecar binary is pinned per Flint release and updated
  via the Tauri updater — no self-updating engine, no version drift between GUI and engine to
  detect-and-warn about (a deliberate simplification, affordable because Flint bundles rather
  than guides-and-detects).
- **Model download:** runtime fetch of the tier's pinned artifact (doc 01) from Hugging Face on an
  explicit consent gate showing size, source URL, and the model's license; stored under
  `~/.flint/models/`; sha256-verified against the pinned digest; resumable (multi-GB files on
  office Wi-Fi *will* be interrupted). No model ships in the `.dmg` (doc 05's size budget).
- **Disk hygiene:** one model per tier means at most a handful of files ever; show "on disk: X GB"
  with a delete affordance in settings. On tier-table updates that change the pick, the old model
  is offered for deletion, never silently removed.

## Alternatives considered

- **Guided-install Ollama `.dmg`** (external-engine fallback) — kept as a paper backend,
  rejected as default: a second install breaks "double-click and go", Ollama self-updates out of
  step with Flint, and its model store duplicates `~/.flint/models/`. Becomes interesting only if
  a user already runs Ollama (trigger below).
- **oMLX** — serves the **Anthropic** Messages API, which is
  the wrong wire format here: opencode speaks OpenAI-compat natively. oMLX matters to Flint only
  in the "absorbed" outcome; documented so
  the convergence path is visible, not planned for v1.
- **MLX-based serving (`mlx_lm.server`)** — attractive perf on Apple Silicon but Python-runtime
  shaped; violates the no-bundled-runtimes rule (the Bun-bridge lesson). Revisit only
  if a true single-binary MLX server emerges.
- **Writing our own inference wrapper** — rejected flatly; Flint's value is opinionation and
  packaging, not inference engineering.

## Resolved (2026-07-06)

- *Must Flint be standalone?* → **Yes — hard requirement.** No Docker/Python/Node, no second
  install; this is the gate odysseus fails for the audience (doc 00).
- *Engine behind an interface or hard-wired?* → **Interface from day one**; `llama.cpp` sidecar is
  the sole v1 implementation.
- *Do models ship in the installer?* → **No** — runtime consent-gated downloads only.

## Open questions

- **Sidecar binary provenance.** Build `llama.cpp` from source in CI vs vendoring an official
  release binary. Lean: build from a pinned tag in CI — supply-chain posture consistent with the
  thesis (a resilience app shouldn't depend on trusting a third-party binary blob), and needed for
  signing anyway (doc 05).
- **One process or per-model reload?** llama.cpp loads one model at start; tier changes need a
  restart. Lean: restart-on-change — trivial given one model per tier; revisit only if a
  "capable vs fast" toggle (doc 01 trigger) makes switching frequent.
- **Context-length and memory-pressure policy** — how much RAM headroom to leave (the user's Mac
  still needs to *work* while a model is resident). Lean: conservative defaults per tier, chosen
  during the doc 01 model spike; never user-tunable in v1.
- **Download resume mechanics** — HTTP range requests vs the HF CLI protocol. Needs a small
  build-time spike; whatever wins must be pure-Rust (no external downloader binary).

## Decision triggers

- If a user already runs Ollama and resents a second engine → implement the Ollama paper backend
  behind the existing interface (detection + endpoint reuse), don't special-case the UI.
- If llama.cpp's OpenAI-compat tool-calling proves unreliable for the doc 01 smoke test → evaluate
  swapping the sidecar (the interface exists for exactly this) before blaming the model.
- If the "absorbed" outcome fires → the oMLX paper backend becomes the bridge: implement
  it behind the same interface to prove the seam before the organ transplant.
- If model files exceed ~2 per user in practice → revisit disk hygiene with a real storage manager;
  until then the settings row suffices.
