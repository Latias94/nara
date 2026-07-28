---
type: "Verification Evidence"
title: "RGD-U11 paused input retention"
description: "Verifies bounded exact-once frame observation and deferred action resolution across managed pause and resume."
timestamp: 2026-07-28T14:08:33Z
record_id: "3ab323c78be74bb59d4ea3b62fb36a49"
tags: ["rgd-u11", "input", "pause", "verification"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5c9a622cb615b6327d4a2fed8ae72e1b2f520d6b"
verified_by: "codex-root"
---

# Verification

Commit `5c9a622cb615b6327d4a2fed8ae72e1b2f520d6b` closes the final RGD-U11
source-correction gate. A red managed-runtime regression first reproduced the defect: a paused
frame ran `Last` without `PreUpdate`, cleared a same-interval press/release pair, and left no action
outcome for resume.

# Result

Verified. Each keyboard and mouse `ButtonInput` now owns one bounded, monotonically sequenced edge
log with independent frame-observation and action-resolution cursors. Paused frames expose new
physical edges to frame consumers exactly once while retaining unresolved action work. The first
resumed `PreUpdate` resolves the retained keyboard and mouse timelines exactly once, and `Last`
compacts only the prefix consumed by both lifetimes. Each input domain mutates its own change tick;
one domain cannot mark the other as changed.

The retention ceiling is intentionally fail-closed. A paused interval may retain at most 256 edges
per input domain; the first excess edge is a typed atomic rejection. Completing deferred action
resolution releases the common prefix and restores capacity.

# Evidence

- Red regression before the implementation: the paused keyboard transition assertion observed
  `[]` instead of `[Pressed, Released]`.
- `cargo nextest run --locked -p nara_input --lib --no-fail-fast --test-threads=1`: 21 passed.
- `cargo nextest run --locked --test runtime_instance --no-fail-fast --test-threads=1`: 67 passed.
- `cargo nextest run --locked -p nara_ui -p nara_gameplay -p nara_winit --lib --no-fail-fast --test-threads=1`: 68 passed.
- `cargo check --workspace --locked`: passed.
- Strict affected-crate and root `cargo clippy` gates passed with only explicit allowances for
  pre-existing repository lints outside this correction.
- `cargo fmt --all -- --check` and `git diff --check HEAD`: passed.
- Independent correctness review found no remaining P0/P1/P2 after the dual-cursor correction.
  Testing review gaps for mouse parity, per-domain change ticks, and retention-capacity recovery are
  closed by committed regressions.

# Public Contract Correction

- `MAX_BUTTON_TRANSITIONS_PER_FRAME` is renamed to
  `MAX_RETAINED_BUTTON_TRANSITIONS` because one bounded retention interval may span paused frames.
- `ButtonInput::clear_transitions` is no longer public. InputPlugin owns frame/action cursor
  advancement and callers cannot erase deferred action work.

# Plan And Dependency Routing

The active plan already records the complete dependency correction lane and does not need a mutable
progress edit: after delivery evidence closes, trace `nara_ui -> nara_scene::Parent` through a
focused `nara_hierarchy` plus 2D transform-propagation slice; then deletion-test
`nara_reflect -> nara_asset`; only then consider a workspace normal-dependency allowlist. This
verification does not claim that the current dependency leak is fixed and does not authorize that
deferred lane early.

# Follow-up

All five RGD-U11 source corrections are now closed. U2/U7 authority review and U8-U10
hosted/candidate evidence must be refreshed in dependency order at the advancing revision before
U11 evidence ingest. No protected dispatch, approval, publication, or release stage is authorized
by this local verification.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- `docs/architecture/adr/0036-event-message-and-resource-queue-lifetime.md`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `crates/nara_input/src/lib.rs`
- `tests/runtime_instance.rs`
