---
type: "Verification Evidence"
title: "UIF-U4C surface suspension and platform authority closure"
description: "Verifies unique target ownership, explicit Surface Slot availability, balanced endpoint migration, stale attempt retirement, and exact accessibility publisher generations."
timestamp: 2026-08-10T05:51:48Z
record_id: "9c48d2de48184e7e973c9871b7e32b6c"
tags: ["ui", "foundation", "surface", "platform", "accessibility", "uif-u4c"]
status: "complete"
producer_id: "codex-root"
run_id: "uif-u4c"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d28efcf"
verified_by: "codex-root"
---

# Verification

- `cargo nextest run --locked -p nara_input --lib --test-threads=1` passed all 43 tests with
  run ID `321a3ded-9912-4b6c-932f-f85394ca64ce`.
- `cargo nextest run --locked -p nara_ui_kernel --test interaction --test-threads=1` passed all
  27 interaction tests with run ID `9fadb100-36fc-4c39-92e7-47029ca20cdf`.
- `cargo nextest run --locked -p nara_ui_widgets --test button --test-threads=1` passed all six
  controlled-Button tests with run ID `8ed45c3e-bb72-4e66-a830-c0a7ea6586dc`.
- `cargo nextest run --locked -p nara_ui --lib --test-threads=1` passed all 61 Runtime UI adapter
  tests with run ID `d602ee6e-c17e-41b9-b32d-2a4d5ae537e8`.
- `cargo nextest run --locked -p nara_winit --features accessibility --test accessibility
  --test-threads=1` passed the exact publication/action/lifecycle test with run ID
  `125ce596-1cf3-4e58-b467-0749f4a21e15`.
- `cargo nextest run --locked -p nara --lib --features presentation-ui --test-threads=1
  --no-fail-fast` passed all 27 root Host/presentation tests with run ID
  `1994b43a-fb0b-419d-8d56-c0c5f1903b69`.
- `cargo fmt --all -- --check`, scoped `git diff --check`, and strict `--no-deps` clippy for
  `nara_ui` and accessibility-enabled `nara_winit` passed.
- Strict root `nara` clippy stopped only on four pre-existing unrelated diagnostics: three
  `clippy::result_large_err` findings in `src/project_host/ingest.rs` and one
  `clippy::needless_lifetimes` finding in `src/startup_scene.rs`. The same root library/tests gate
  passed with only those existing targeted allowances plus the existing
  `clippy::cloned_ref_to_slice_refs` allowance; none of those files belongs to UIF-U4C.
- Independent API-contract review found three P1 issues: caller-forgeable publisher identity,
  incomplete consecutive suspension/retirement generation handling, and missing migration
  guidance. Independent test review found four P1 coverage gaps for target-conflict atomicity,
  outstanding-attempt suspension, direct suspended retirement, and suspension ordering. All seven
  applicable findings were fixed before commit `d28efcf`; the dedicated correctness reviewer was
  unavailable because its upstream route returned 502/503, so that independent lens is recorded as
  degraded rather than silently counted as coverage.

# Result

UIF-U4C is complete at commit `d28efcf`.

- `UiSurfaceTargetIndex` owns the one-window-target-to-one-Surface-Slot baseline. Install, active
  update and suspended restore reject occupied targets before state changes; exact retirement and
  endpoint handoff release only the prior reservation.
- Every installed surface exposes an explicit `Active` or typed `Suspended` availability state.
  Zero extent, missing endpoint and invalid scale advance the slot/route/geometry/presentation
  lineage, remove candidate, packet, receipt, route and pointer/capture authority, and require a
  fresh successor generation before presentation resumes. Retirement is permanent.
- Logical unbind preserves only balanced physical held-edge ownership. Endpoint A-to-B migration
  cannot transfer an existing press to B, while true retirement performs the bounded release and
  cleanup transaction exactly once.
- The root Host aborts every outstanding prepared/submitted attempt before suspension. A late
  backend acceptance is discarded as stale, cannot restore an old receipt, and a restored surface
  can publish only a newly compiled generation.
- `WinitAccessibilityPublisher` allocates an opaque process-unique publisher identity; callers
  cannot choose or reuse it. Publications and incoming actions bind the exact publisher, surface,
  route, publication generation and presented receipt. A suspended generation itself cannot
  republish semantics; only a strict successor can restore the mapping. Consecutive suspension and
  restore-before-publication retirement remain ordered and fail closed for stale generations.
- The Host derives one exact authored-to-physical content transform for desktop targets. A 4:3
  window keeps the 960x540 authored design space with letterboxing, and black-bar input is never
  clamped into a widget.
- The accessibility receipt proves synchronous Host-owned AccessKit tree/mapping record commit,
  not operating-system or player observation. A future real `accesskit_winit` adapter remains a
  bounded internal trial and may not weaken this receipt meaning.

# Evidence

- `crates/nara_ui/src/runtime.rs`
- `crates/nara_ui/src/runtime/{state,surface,targets}.rs`
- `crates/nara_ui/src/interaction/transaction.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_winit/src/accessibility.rs`
- `crates/nara_winit/tests/accessibility.rs`
- `src/ui_presentation.rs`
- `src/ui_presentation/{attempts,surfaces,tests}.rs`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/migrations/2026-07-engine-foundation.md`

# Follow-up

- UIF-U5 is the sole next active unit. It publishes a Foundation Conformance Baseline from the
  frozen public inventory, neutral examples and independent consumer; it does not claim Runtime UI
  product validation.
- Reference Game/NUI-U15B product acceptance remains paused until UIF-U5 pins that baseline and a
  successor registration explicitly resumes the product unit.
- UIF-U5 must retain the single-consumer backend-outcome ownership statement, run the three neutral
  examples and independent `ui-module-consumer` gate, audit public re-exports, and avoid adding
  widgets, Editor APIs, target aggregation, reactive invalidation or a real platform accessibility
  registry.

# Citations
