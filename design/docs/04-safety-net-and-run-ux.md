# 04 — Safety net & run UX

> The bulk of the real work. The harness wiring (doc 03) is maybe a tenth of the effort; what
> makes an agent shippable to a layperson is the surround: permission banners, snapshot/undo,
> a legible transcript, and a review step. Pattern sources: established agent-run UX conventions
> (design), prior runner UX (running code), the consent-gate pattern (pattern, not widget).

## Scope

The permission UI, the pre-run snapshot + undo, the run transcript, chat UX, and the staging
policy for the validation phase. Out of scope: the hook that feeds it (doc 03), which model runs
(docs 01/02).

## Current leaning

- **Permission prompts are inline banners inside the transcript, never modals** (the WKWebView
  rule, CLAUDE.md; the right consent pattern — exact command shown,
  Allow/Deny, auto-deny timeout — was implemented as a modal elsewhere; port the pattern, not the
  widget). The banner shows
  the real tool name, args/command, and paths (glass-box stance): familiarity
  accrues by exposure, and it keeps Flint honest.
- **Pre-run snapshot + one-click undo before any file-writing agent run** — the detached-git
  design is the pattern source (`git2`, snapshot the workspace before the run, diff
  + Keep/Undo after). This is **non-negotiable before any non-author user** touches Flint; the
  validation-phase staging below is the only sanctioned softening.
- **Workspace scoping:** agent runs happen inside one user-chosen folder ("workspace"); the
  permission plugin path-checks against it (doc 03). No home-directory-wide agent in any phase.
- **Run transcript = typed tool cards, not a markdown stream** (prior runner UX is
  the closest running code: streamed responses, per-tool prompts, cancel, persisted transcripts —
  driven here through polling per doc 02). Cancel is always available; a cancelled run still gets
  the review step.
- **Review step after every agent run:** what changed (diff), Keep / Undo. Chat runs have no
  review step — nothing was touched, and absence is honest.
- **Chat UX: a plain pane.** No parameters, no system-prompt editing, no temperature. Model
  identity + "runs on this device" shown; that's it.
- **Validation-phase staging (the lean):** while the author is the only user, ship agent mode with
  (1) workspace scoping, (2) the permission net, and (3) pre-run snapshot via `git2` — but defer
  the *polished* review/diff UI (a "restore snapshot" button suffices). Rationale: the snapshot
  mechanism must exist from the first file-write (retrofitting it is how it never happens), but
  diff-viewer polish doesn't gate validation. The full review UX is the price of the first
  non-author user (doc 00 trigger).

## Alternatives considered

- **Reuse `claude`'s `--permission-prompt-tool` net** — inapplicable: no `claude` in the chain
  (doc 00 thesis); the equivalent seam is doc 03's plugin hook.
- **OS-level sandboxing for enforcement** — rejected for the same reasons it's rejected in
  comparable agent-run designs: OS-specific, high cost; the permission net + path checks +
  snapshot give layered protection at validation scale. Revisit only on evidence.
- **No snapshot during validation ("I'm careful")** — rejected: the author's own files are the
  test bed, and the mechanism's absence is exactly what validation is supposed to catch. The
  staging compromise above keeps the mechanism and defers only polish.
- **Trash-based undo (move originals to Trash)** — rejected: doesn't cover in-place edits or
  multi-file consistency; `git2` detached snapshots are already proven prior-art territory.

## Resolved (2026-07-06)

- *Is a safety net required for an agentic layperson app?* → **Yes — non-negotiable before any
  non-author user.** The staging policy above is the only softening, and it keeps snapshot +
  permission net + scoping from day one.
- *Modals?* → **Never** (the WKWebView rule wholesale).
- *Does chat need the net?* → No tools fire in chat (doc 03) → no banners, no snapshot, no review.

## Open questions

- **Snapshot granularity & retention** — per-run snapshots of a whole workspace folder: size
  limits, exclusion rules (`.git`, `node_modules`-scale dirs unlikely for this audience but
  cheap to exclude), retention/pruning policy. Lean: keep last N runs (~10), prune oldest,
  show disk usage next to the doc 02 model storage row.
- **Undo semantics after post-run manual edits** — restoring a snapshot over files the user
  touched afterwards. Lean: warn when workspace mtime > run end; full three-way handling is out
  of scope for validation.
- **Cancel semantics** — what "cancel" guarantees mid-tool (kill the tool process, then snapshot-
  diff whatever happened). Needs the doc 03 session-lifecycle answer first.
- **Transcript persistence** — where run history lives. Lean: JSONL per run under
  `~/.flint/runs/<workspace-hash>/` for validation; a SQLite store only
  if history UX ever matters here.

## Decision triggers

- If a run ever writes outside the workspace despite scoping → stop feature work; that's a
  path-check hole (doc 03's confinement question), and the path-traversal checklist gets
  re-audited before the next run.
- If the author reaches for undo even once during validation → the polished review/diff UI moves
  up the queue; that's the signal it earns its cost.
- If permission banners fire so often they train click-through → add per-run "allow this tool for
  this run" grouping (never a global allow-list default — that's the permission-weakening trap).
- If a non-author user is imminent → the staging allowance expires: full review/diff UX, retention
  policy, and the doc 00 locale gate all become blocking.
