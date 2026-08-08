---
type: "Verification Evidence"
title: "UIF-U1 presentation reconciliation correction"
description: "Closes candidate/session generation splits, remove/reinsert resurrection, release backpressure, and staged reconciliation ownership."
timestamp: 2026-08-08T14:20:32Z
record_id: "055102d378e94c819ef419be51d29159"
status: "complete"
producer_id: "codex-root"
run_id: "uif-u1-correction"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d43de14"
verified_by: "codex-root"
---

# Verification

- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui_widgets -p nara_ui
  --test-threads=1`: 91 passed, 0 failed after the final staged-reconciliation Interface change.
- `cargo nextest run --locked -p nara_winit --all-features --test-threads=1`: 32 passed,
  including the bounded accessibility projection.
- `cargo run --locked -p nara_ui --example interaction_lab -j 1`: passed.
- `cargo clippy --locked --no-deps -p nara_ui_kernel -p nara_ui_widgets -p nara_ui
  -p nara_winit --all-features --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`, scoped `git diff --check`, and staged diff inspection passed.
- An independent adversarial review found no remaining UIF-U1 P0/P1 defect. It assigned backend
  one-shot publication to UIF-U3 and retained-modal barrier authority to UIF-U2 rather than
  widening this unit.

# Result

UIF-U1 is complete at `d43de14`. This evidence supersedes the conformance claim of the immutable
first-pass UIF-U1 evidence without rewriting that historical record.

- Candidate paint, semantics, interaction session and widget reducers are staged as one generation.
  Cleanup is reduced before final candidate projection, and committed input invalidates a compiled
  candidate instead of allowing stale pixels to be presented.
- Unpresented candidate reconciliation is read-only on the presented session and returns an opaque
  staged result through a documentation-hidden first-party seam.
- Equivalent presentation facts rebind an in-flight gesture to the latest epoch. Geometry, modal
  scope, removal, action or enabled-state changes cancel it, and same-key reinsertion cannot revive
  capture, hover, focus-restoration or reducer state.
- Journal backpressure drops a pointer activation with typed evidence while still committing the
  physical release. Keyboard, gamepad and accessibility activation remain rejected unchanged when
  their action cannot be admitted.
- Latest modal authority cannot be weakened by a historical non-modal route. One surface admits one
  active seat and pointer owner; additional identities fail closed before allocation.
- Disabled focus remains a navigation origin, never an activation destination, and retains a
  non-color focus indicator through disable/re-enable.

# Evidence

- `crates/nara_ui_kernel/src/interaction/{session,state,pointer,focus,types}.rs`
- `crates/nara_ui/src/runtime/{candidate,state,presentation}.rs`
- `crates/nara_ui/src/interaction/transaction.rs`
- `crates/nara_ui_kernel/tests/interaction.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_ui_widgets/{src/button.rs,tests/button.rs}`
- `crates/nara_ui/examples/interaction_lab.rs`
- `docs/migrations/2026-07-engine-foundation.md#uif-u1-2-presented-reconciliation-candidate-staging-and-pointer-ownership`

# Deferred Owners

- UIF-U2 owns root-envelope ordering, inert last-good modal barriers and compiler cache staging.
- UIF-U3 owns pre-backend reservation, one-shot presentation commit, route publication infallibility,
  target-frame budgets and backend capability closure.

# Citations
