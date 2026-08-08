---
type: "Verification Evidence"
title: "UIF-U1 atomic presented-session interaction"
description: "Verifies closed dispatch outcomes, atomic reconciliation, invalidation-safe pointer routing, committed focus semantics, and disabled-focus identity."
timestamp: 2026-08-08T11:34:50Z
record_id: "d13d3f5c4c68440fa5ddacb88ee64eab"
status: "complete"
producer_id: "codex-root"
run_id: "uif-u1"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c2806a5"
verified_by: "codex-root"
---

# Verification

- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui_widgets -p nara_ui
  --test-threads=1`: 72 passed, 0 failed.
- `cargo nextest run --locked -p nara_winit --all-features --test-threads=1`: 32 passed,
  including the bounded AccessKit focus/disabled-action projection.
- `cargo run --locked -p nara_ui --example interaction_lab -j 1`: passed.
- `cargo clippy --locked --no-deps -p nara_ui_kernel -p nara_ui_widgets -p nara_ui
  -p nara_winit --all-features --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- The same strict clippy command without `--no-deps` reached existing `nara_app` warnings for large
  error variants and collapsible conditionals before the changed packages. Those unrelated warnings
  were not modified or claimed as UIF-U1 failures.

# Result

UIF-U1 is complete at `c2806a5`.

- `UiDispatchOutcome` makes consumed batches and rejection reasons mutually exclusive.
- The runtime transaction can commit only a consumed session/reducer/journal candidate; rejection
  owns no commit path.
- Capture, hover and focus cleanup is a consumed reconciliation. A later invalid route rejection
  leaves the reconciled session unchanged.
- Geometry invalidation clears pointer coordinates, and synthetic invalidation releases enter the
  cancellation path instead of Button release activation.
- Semantic snapshots report committed focus only. Disabled stable controls retain focus identity,
  remain navigation origins, leave destination/activation eligibility, and recover the focused
  visual when re-enabled.
- No Reference Game product matrix or hash/smoke expansion was used as Foundation evidence.

# Evidence

- `crates/nara_ui_kernel/src/interaction/{types,session,state,pointer,focus}.rs`
- `crates/nara_ui/src/interaction/{transaction,timeline}.rs`
- `crates/nara_ui_kernel/tests/interaction.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_ui_widgets/src/button.rs`
- `crates/nara_ui_widgets/tests/button.rs`
- `crates/nara_winit/tests/accessibility.rs`
- `crates/nara_ui/examples/interaction_lab.rs`
- `docs/migrations/2026-07-engine-foundation.md#uif-u1-1-atomic-presented-session-interaction-and-invalidation`

# Follow-up

Activate UIF-U2. Split global snapshot admission from ordered root-local envelopes, cache only
root-local visual facts, rebind last-good content to the current layer/ordinal, and preserve current
modal barrier safety without exposing normalized IR or a new provider seam.

# Citations
