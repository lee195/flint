# 00 — Thesis & positioning

> Foundational doc: why Flint exists, who it serves, its validation framing, and the prior-art
> boundaries. Acts as the tie-breaker for the other docs.

## Scope

The product thesis (vendor resilience for laypersons), the audience and validation framing, the
outcomes that count as success, and the prior-art inventory with its licensing bright line. Out of
scope: every mechanism (docs 01–05).

## Current leaning

- **Thesis: a single-point-of-failure hedge must be vendor-independent end-to-end.** The AI market
  is volatile. A "private mode" of a claude-based tool addresses *privacy* (files stay local) but
  its engine is still the vendor's — if the vendor wobbles, so does that tool. Flint's chain has no
  vendor in it: local engine (bundled), open-weights model (user-downloaded), open-source harness
  (opencode), GUI (ours). Resilience, not privacy, is the spine — privacy comes along for free.
- **Audience: terminal-never, chat-familiar.** The existing tools fail this
  audience in two ways: polished GUIs (LM Studio, Jan, Ollama's app) are *catalog-shaped* (hundreds
  of models, quantization jargon), and the hardware-aware recommenders (whichllm, llmfit) are
  *terminal tools*. Flint's differentiator is **radical opinionation**: probe the hardware,
  recommend one model, go. Making the choice *for* the user is the craftsman move none of the
  enthusiast-serving incumbents will make.
- **Validation-phase framing: the first user is the author.** Flint exists to validate the
  harness + cookbook + chat idea through real usage before any wider ambition. Design for the
  layperson, but ship to yourself first; layperson-blocking gaps (full safety net, de locale) may
  be staged (docs 01/04) so long as they're closed before any non-author user.
- **Three valid outcomes, all success:**
  - *Absorbed* — the idea validates; another home inherits the organs (cookbook, opencode
    permission bridge, safety-net port) and Flint retires or shrinks.
  - *Stays* — Flint remains the standalone resilience app.
  - *Retires* — usage shows local models under-deliver for real work on real hardware; the
    honest conclusion is recorded and the cookbook table still stands on its own. An informed
    "not worth it yet" is a successful validation.
- **Honest-fit, established practice:** the first screen says plainly what local models can
  and can't do against the user's reference point (ChatGPT/claude.ai), and "your machine can't run
  anything satisfying" is a reachable conclusion the app states outright rather than papering over.
  This bites *harder* here than in a claude-based tool because agentic mode multiplies the
  capability gap (doc 01).

## Prior art

- **odysseus** (`github.com/pewdiepie-archdaemon/odysseus`) — validates the concept: a self-hosted
  AI workspace whose **Model Cookbook** ("hardware-aware model recommendations and downloads") is
  exactly Flint's core organ, gated behind a dev onboarding (git clone, `.env`, Docker Compose,
  `localhost:7000`). **AGPL-3.0-or-later — bright line: UX/concept reference only, never code,
  never config, never data files.** Nothing may be copied. Its
  workspace modules (email, calendar, notes, documents, deep research, compare) are explicitly out
  of Flint's scope — they serve the Docker-comfortable self-hoster, a different audience.
- **Market:** LM Studio / Jan / Ollama app (catalog-shaped GUIs); whichllm / llmfit (hardware-aware
  but terminal-gated). Watch item: LM Studio already shows "too large for your machine" hints —
  the opinionated-GUI gap could narrow (trigger below).

## Alternatives considered

- **Keep only cookbook + chat standalone, fold the agent ambition into an existing harness
  roadmap** — considered; the user chose the separate validation app instead to de-risk vendor
  independence through usage now. The convergence expectation survives as the "absorbed" outcome
  above.
- **Wrap odysseus in a desktop shell** — rejected: Docker Desktop can't be bundled (license +
  size), shipping Python+Node inside a shell is the exact unbundled-runtime trap, and AGPL
  forecloses derivation.
- **Host odysseus on a server as a URL** — dodges packaging entirely but is a
  different product: server-side, so "runs on my machine", the hardware-aware angle, and offline
  resilience all evaporate. Parked; fine as a demand experiment, not a substitute.

## Resolved (2026-07-06)

- *One app or separate?* → **Separate validation app** (user decision). The resilience
  job structurally can't depend on `claude` (STATE.md).
- *Full odysseus-style workspace or a focused slice?* → **Cookbook + chat + agentic harness only.**
- *Codename?* → **Flint** ("for now") — the thing that makes fire with no external supply.

## Open questions

- **Who is the eventual audience?** Personal validation first is decided — but does Flint stay a
  personal/enthusiast tool, or grow toward a broader rollout once validation produces evidence?
  Lean: defer until validation produces evidence; nothing in docs 01–05 depends on the answer.
- **Bilingual de/en from day one?** Lean: **en-first, i18n-ready** (all chrome strings through Vue
  i18n from the first commit so de is a catalog pass, not a refactor) — a deliberate, documented
  deviation from a bilingual-day-1 rule, revisited the moment a non-author user is in sight.

## Decision triggers

- If LM Studio or Ollama ship an opinionated "one model for your machine" flow → re-evaluate
  Flint's differentiator; the probe + cookbook organs still stand on their own.
- If validation shows agent mode is the whole value and chat is noise (or vice versa) → rebalance
  scope before implementation planning, don't carry both by default.
- If a non-author user is about to touch Flint → the staging allowances (en-only, reduced safety
  net) expire; docs 01/04 gates become blocking.
- If Anthropic/vendor landscape stabilizes to the point the resilience motive feels theatrical →
  revisit the thesis honestly; the cookbook may still justify the app on its own.
