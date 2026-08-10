---
type: "Verification Evidence"
title: "UIFM-U1 Modal Entry Focus Correction"
description: "Proves that the Surface Slot owner seat participates in candidate reconciliation and that initial modal focus is committed before final paint and semantic publication."
timestamp: 2026-08-10T15:59:55Z
record_id: "04e51278170c4a839dbeb0c9658741d8"
tags: ["ui", "foundation", "interaction", "focus", "uifm-u1"]
status: "complete"
producer_id: "codex-root"
run_id: "uifm-u1"
related_plan: "docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "40d3f87"
verified_by: "codex-root"
supersedes: "cc8d66c49d134e9f9d18d680afe19f7a"
---

# Verification

- `cargo nextest run -p nara_ui_kernel --test interaction --test-threads=1 --no-tests fail
  --no-fail-fast` passed all 31 tests with run ID
  `62c15b17-773c-4e06-80dd-c22d18e810ae`.
- `cargo nextest run -p nara_ui --test-threads=1 --no-tests fail --no-fail-fast` passed all 63 tests
  with run ID `9d977ed8-3d43-4e1b-8cad-2878c4d4d5a4`.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test ui_product
  --test-threads=1 --no-tests fail --no-fail-fast` passed all nine tests with run ID
  `a9dd0a81-8235-4a83-81f1-747653b43960`.
- Strict no-dependency Clippy passed for `nara_ui_kernel --test interaction`, `nara_ui --lib
  --tests`, and the Reference Game `ui_product` test. The root package emitted only pre-existing
  dead-code warnings outside this correction. A broader dependency Clippy attempt remained blocked
  by pre-existing `nara_app` `result_large_err` and `collapsible_if` findings.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Three independent simplification passes found no reuse issue; their actionable documentation,
  allocation and no-op reprojection findings were applied before the final verification runs.

# Result

UIFM-U1 is complete at commit `40d3f87`.

- Candidate reconciliation receives the authoritative Surface Slot endpoint seat and admits it
  before modal reconciliation.
- Initial modal and base-to-modal transitions focus the first enabled visible target in stable
  structural order without prior input; an initial non-modal frame remains unfocused.
- Reconciliation updates controlled-widget reducer facts before final compilation, so focus paint,
  reducer state and semantic focus describe one candidate generation and emit no activation intent.
- Replacing the Surface endpoint retires predecessor-seat authority and seeds only the successor
  endpoint seat.
- The Reference Game no longer injects a synthetic Tab step before keyboard, gamepad,
  accessibility, pause or trajectory Retry activation.
- When reconciliation changes no reducer-projected fact, the adapter adopts the staged candidate
  directly instead of performing a redundant projection and compile pass.

This record closes UIFM-U1 only. UIFM-U2 must publish a successor Foundation Conformance Baseline
that supersedes UIF-U5 for this behavior and explicitly rebase RUP-U1 before product work resumes.

# Citations

- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `crates/nara_ui_kernel/src/interaction/session.rs`
- `crates/nara_ui_kernel/src/interaction/state.rs`
- `crates/nara_ui/src/runtime/state.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_ui_kernel/tests/interaction.rs`
- `reference-game/tests/ui_product.rs`
