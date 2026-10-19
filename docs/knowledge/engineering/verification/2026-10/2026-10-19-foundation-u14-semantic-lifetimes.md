---
type: Verification Evidence
title: Foundation U14 aggregate input lifetimes and paused restoration
description: One staging owner prevents normalized and synthetic presses from reviving after configuration exits.
timestamp: 2026-10-19T09:28:00Z
record_id: 4dcf580575e84c79a8ff54d764fd915d
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract

`ActionOutcomes` retains physical per-binding edges. `SemanticActions` aggregates active bindings
and endpoints by action/context: the first contribution starts, the final release ends, and an
observed context/mapping exit ends a delivered held action exactly once. Restoring configuration
does not restart an input that remains physically held; release and a fresh press are required.
The Gameplay command bridge consumes semantic edges before fixed-tick admission.

One private `ActionStaging` owner distinguishes rollback-capable configuration snapshots from
non-reverting observed lifetimes. Normalized timeline edges and legacy/synthetic ButtonInput edges
retain the same staging authority across paused frames. Restoring a cloned or moved ActionMap
cannot erase an already observed exit. Only the last configuration observation and current
lifetime are retained, without an unbounded historical identity registry.

Synthetic metadata appends and drains with the bounded button log. Public ButtonTransition remains
Copy; raw observations and independent keyboard/mouse change ticks retain their existing contract.
Private semantic implementation and contract tests are separate modules rather than extending the
large timeline test file.

# Review, Red And Green

The initial App/Input/Gameplay/Tilemap run passed 242 tests, including five normalized semantic
cases. Independent review then found two additional defects: synthetic paused presses acquired
resume-time identities, and configuration restoration could revive expired held state. These were
required corrections, not a change to the user's selected fresh-press behavior.

Red run `b6b349c1-1a19-49f3-8c0b-64c61665b796` failed all three new tests at their behavioral
assertions. The third observed three semantic edges instead of the one fresh press expected.
The MSVC abort code followed those assertion/unwind paths; it is not evidence of a buffer overflow.

After the shared staging correction, complete Input/Gameplay all-features run
`dac538be-868f-48da-8f6d-f0a38d36d0ae` passed all 97 tests, including nine semantic cases. They
cover multiple bindings, two endpoints and one focus loss, context exit/restoration, paused mapping
changes, synthetic keyboard/mouse history, fresh release/press, unaffected contexts, and cloned/
moved context and mapping restoration. Existing zero/multi-tick command delivery, replay, retained
edge budgets, raw outcomes and change-detection tests also passed.

Independent follow-up source review found both defects closed and no new actionable issue. All
Cargo was serial on Windows, Rust 1.95.0, with the lockfile and CARGO_BUILD_JOBS=1.

# Remaining Evidence

The Reference Game's ordinary desktop consumer now binds WASD and Arrow keys to the same actions.
Its command/MovementIntent/velocity/position regression still needs execution. Root public-prelude
integration and the plan-wide workspace gate remain pending; this record does not yet close U14.
