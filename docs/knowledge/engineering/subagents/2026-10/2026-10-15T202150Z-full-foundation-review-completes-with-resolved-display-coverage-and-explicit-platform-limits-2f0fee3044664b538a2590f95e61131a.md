---
type: "Subagent Finding"
title: "Full foundation review completes with resolved display coverage and explicit platform limits"
description: "Subagent Finding for Full foundation review completes with resolved display coverage and explicit platform limits."
timestamp: 2026-10-15T20:21:50Z
record_id: "2f0fee3044664b538a2590f95e61131a"
status: "reviewed"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/foundation_review_resume"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7718a86"
---

# Review

Actual `ce-code-review mode:agent` run `20261015-194103-d42bfd69` completed all six stages:
scope, select, dispatch, merge, validate and report. The terminal receipt has `status: complete`,
verdict `Ready to merge`, and zero actionable findings. The reviewed source checkpoint is
`7718a86`, against base `757fb6a`, in the integrated tracked worktree.

Ten Codex review lenses returned actual artifacts: correctness, project standards, testing,
maintainability, action/context parity, security, performance, API contract, reliability and
adversarial review. Initially malformed acknowledgments were retried through bounded fresh
reviewers; no acknowledgment was counted as a clean review. The final metadata records 375
tracked changed files, 55,989 diff lines and 38,916 executable non-test lines. The user required
Codex-only review, so no independent external-model review is claimed.

# Candidate Dispositions

- Testing's missing late-display-completion scenario is resolved in `7718a86`. The actual CPU
  toolkit consumer completes the successor, attempts the old finish, receives ImageMismatch,
  and retains the successor token, pixels and picking. Focused implementation run
  `8bac4b1e-486f-482e-b986-a8c8c76f83e8` passes. Reviewers inspected current source and did not
  rerun this test or claim independent dynamic corroboration.
- Reliability's mandatory CLI timeout claim was rejected at admission. The trusted synchronous
  Cargo/project-tool adapter bounds stdout bytes and retains the direct child through wait;
  stderr is inherited. The candidate established no CLI time-limit promise, concrete producer
  of the alleged hang or new lost cleanup authority. Editor Save/Play responsiveness and a
  finite product-observation deadline do not establish an arbitrary Cargo build timeout.
  Future cancellable automation needs an explicit deadline/cancellation contract with owned
  process/reader retirement; no arbitrary timeout or universal process framework was added.
- The primary set was empty after merge. Validation therefore recorded `empty-batch`, selected
  zero and launched no validator. This is not an independent validation execution or a
  same-model agreement shortcut.

# Coverage and Limits

The review includes committed, staged and unstaged tracked content against the base. The 39
untracked scope exclusions include pre-existing/overlapping supervised-Play implementation,
research and evidence records. The external receipt lists each exclusion. A passing tracked
review does not establish an independently buildable committed HEAD or review every untracked
implementation file. The root's finite integrated runtime tests provide distinct evidence.

The final pause/Step/Resume correction and late-finish delta were inspected separately in
current source. All R1-R11 and U1-U13 received source/consumer assessment; no new unrequested
user-facing behavior rule was identified. The action/context reviewer sampled nine high-priority
Host/CLI/Editor capability groups and found corresponding programmatic actions and context in
all nine; this is not a whole-engine capability score.

U4 has actual 59-test Windows and 57-test native Linux filesystem evidence. Complete native
Linux/macOS process/window/render release trajectories remain missing; macOS filesystem native
evidence is also missing. Windows success cannot establish those releases. Other Clippy
style/deprecation warnings remain outside the passing correctness/suspicious deny gate.

A nonblocking coverage improvement remains: reparent or detach at a half tick after interpolation
history exists, remove the old parent, then inspect camera/sprite/tile matrices. Current code
resets history on parent changes, but the existing parent fixture builds its topology before
startup. No production defect was established by that gap.

# Evidence Retention

The completed external run retains `review.json`, `metadata.json`, all reviewer artifacts,
`post-dispatch-delta.diff`, candidate dispositions and all six terminal stage events under the
run ID above. Repository memory stores this concise disposition rather than raw tool logs or
machine-specific absolute paths. The reviewer made no repository edits, Cargo runs, commits,
pushes or publications. Final architecture/evidence closure remains the implementation owner's
responsibility.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- tests/support/fake_preview_toolkit.rs
- src/project_host/runtime/editor/presentation.rs
- src/project_tooling/bootstrap.rs
- crates/nara_transform/src/presentation.rs
- crates/nara_sprite_render/tests/presentation_interpolation.rs
- docs/knowledge/engineering/verification/2026-10/2026-10-15T102927Z-foundation-u4-windows-and-native-linux-publication-cleanup-4f79a46cf5c4455a8700d9090c13e386.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T200246Z-paused-presentation-no-longer-rewinds-and-late-display-completion-cannot-replace-its-successor-114c2cfd30854192a821c22b8afd8487.md
