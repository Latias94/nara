---
type: "Verification Evidence"
title: "NUI-U1 ordered UI kernel authority activation"
description: "Records the accepted correction decisions, successor authority, architecture-governance simplification, and the transition to NUI-U2."
timestamp: 2026-08-04T09:26:06Z
record_id: "e3d3a8bda597403f8d8d4a6336bb2fc2"
tags: ["nui-u1", "ordered-hierarchy", "ui-kernel", "authority", "verification"]
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d13d53e827262df08b68073101008c975b65e599"
verified_by: "direct authority inspection, engineering-memory validation, git diff check, and focused CI policy nextest"
---

# Verification

The authority reset was inspected against the current worktree at baseline
`d13d53e827262df08b68073101008c975b65e599`.

- ADRs 0101 through 0106 are Accepted and each begins with a `not-started` implementation row.
  Their predecessor relations are reciprocal, while the focused slices extracted from Proposed
  ADRs 0085 and 0087 leave the remaining proposals explicitly non-authoritative.
- The NUI plan is the only plan with `execution_state: active`. RUI reciprocally supersedes SRT and
  is reciprocally superseded by NUI; the stale SRT `successor` spelling is gone.
- The architecture map and implementation ledger each link the NUI plan exactly once. The stale
  physics exploration registration is closed through an immutable successor, and the RUI lane is
  superseded by the NUI registration.
- The Rust `architecture_docs` parser was removed with its dedicated CI step and suite exclusions.
  Its final attempted run passed eight governance/parser tests and failed only because a historical
  assertion required ADR 0084's latest evidence label to remain `RGD-U7` after later `SRT-U3`
  verification. This demonstrated that the test froze prose history rather than product behavior.

# Result

Passed for NUI-U1 authority activation.

- `cargo nextest run --locked --test ci_policy --test-threads=1`: 2 passed, 0 failed.
- `git diff --check`: passed; Git reported only checkout line-ending conversion warnings.
- Engineering-memory `validate`: passed. Its existing legacy-record warnings remain historical and
  are not NUI-owned failures.
- Engineering-memory `render --check`: passed after owner-rendered `current-state.md` and `log.md`.
- Direct plan and pointer inspection found one active NUI plan, no legacy `successor` metadata, and
  exactly one active work registration (`engine-foundation-contract-completion-codex-root`).

# Evidence

- Accepted decisions: ADR 0101 through ADR 0106 and their reciprocal predecessor metadata.
- Active execution authority: the NUI plan, architecture map, and implementation ledger.
- CI simplification: `.github/workflows/ci.yml`, `tests/ci_policy.rs`, and removal of
  `tests/architecture_docs.rs`.
- Active registration:
  `docs/knowledge/engineering/registry/2026-08/2026-08-04T100554Z-engine-foundation-contract-completion-codex-root-30c38d87d4314e2994c2958a415a7f05.md`.
- Closed stale research registration:
  `docs/knowledge/engineering/registry/2026-08/2026-08-04T092714Z-nara-deep-module-architecture-exploration-codex-root-79d2b2d4433640e0ae79c53a5062de03.md`.

# Follow-up

NUI-U2 is the sole next unit. Remove `ToolingPlugin` and the runtime ECS `EditorWorkspace` copy,
then prove that Host-owned open/edit/save/reopen, Play, Retry, and command-returning adapter behavior
remain intact. Do not start another behavior unit until its owning ADR ledger row is moved to a
truthful unit-scoped `partial` state.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u1-activate-decisions-and-successor-authority`
- `docs/architecture/README.md`
- `docs/architecture/adr/implementation-status.md`
- `.github/workflows/ci.yml`
- `tests/ci_policy.rs`
