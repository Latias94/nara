---
type: "Verification Evidence"
title: "UIF-U3C prepared submission authority and infallible commit"
description: "Verifies pre-submit authority staging, exact backend permits, infallible Host commit, attempt supersession, and target/device fault-domain isolation."
timestamp: 2026-08-09T16:07:57Z
record_id: "2592110e3d0d4ae8a051a157f49db1ba"
tags: ["ui", "foundation", "presentation", "authority", "uif-u3c"]
status: "complete"
producer_id: "codex-root"
run_id: "uif-u3c"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0f5154f"
verified_by: "codex-root"
---

# Verification

- The authoritative V3 command, `cargo nextest run --locked -p nara_ui_kernel -p nara_ui
  -p nara_ui_render -p nara_render_wgpu --all-features --test-threads=1`, passed all 220 tests
  with run ID `5cece393-1e0d-4d98-afcf-2ec2919d4c0d`.
- `cargo nextest run --locked -p nara --all-features -E 'test(host_)' --test-threads=1` passed
  all 23 Host bridge tests with run ID `ae8278cd-141d-482a-983c-2ff08190cc8f`.
- `cargo nextest run --locked -p nara_input --test-threads=1` passed all 43 input tests with run
  ID `54356b28-138d-427a-a608-c1b58587549d`; the independent `ui-module-consumer` suite passed
  all three tests with run ID `26b644fd-9935-4c2f-a470-7a3091e99032`.
- `cargo run --locked --example ui_presentation_gallery --features
  presentation-ui,desktop-winit,render-wgpu -j 1` completed the neutral
  candidate-to-packet-to-receipt-to-accessibility-action canary and proved that a retired owned
  packet remains readable.
- `cargo check --locked -p nara_input -p nara_ui_kernel -p nara_ui -p nara_ui_render
  -p nara_render_wgpu -p nara --all-features -j 1`, `cargo fmt --all -- --check`, and scoped
  `git diff --check` passed.
- Strict `--no-deps` clippy passed for the input/kernel/runtime/planner crates, for
  `nara_render_wgpu` library and tests with the crate's existing targeted lint allowances, and for
  the root `nara` library with its existing targeted lint allowances. A broader root `--tests`
  clippy run stopped only on the pre-existing unrelated `tests/image_import_limits.rs:318`
  `clippy::needless_return`; that concurrent file was not changed or staged by this unit.
- The fault-order regression was first observed red with run ID
  `2f60f2b7-6c0c-4e8b-84da-cc3bc7a71301`: a device fault processed before a deferred outcome left
  the same GPU generation retryable. Post-batch generation retirement fixed the owner contract;
  the fault-first regression then passed with run ID `a97e9069-e965-4cb7-963a-a0da40addfa0`.
- Independent API-contract, reliability, testing, maintainability and performance reviews reported
  no remaining P0/P1 after fixes. Review run `20260809T105619Z-uif-u3c-3bfc4c8d` retained only
  follow-up observations for UIF-U4C/UIF-U5: document the single-consumer outcome drain, keep every
  backend permit exactly-once and fail-closed, split WGPU/root route internals only if they continue
  growing, and measure allocation/fingerprint pressure before introducing reactive infrastructure.

# Result

UIF-U3C is complete at commit `0f5154f`.

- `UiPreparedPresentationAuthority` stages the exact candidate, presented history, interaction
  session, reducers, journal and input route before backend submission. Capacity, conflict and
  generation failures therefore occur before pixels can be submitted.
- `UiBackendOutcomePermit` is move-only and bound to the exact packet, target, Surface Slot,
  environment, capability and GPU generations. Backend completion consumes that permit, and the
  final Host authority swap is infallible.
- The Host attempt ledger records prepared, submitted and deferred identities, rejects stale or
  superseded outcomes, and prevents an older deferred attempt from reviving after a successor
  commits.
- Target-local rejection retires only its target/slot lineage. Device failure retires every deferred
  attempt for the exact GPU generation regardless of outcome ordering, without disturbing another
  GPU generation.
- Route-generation exhaustion distinguishes pre-submit rejection from fail-closed invalidation.
  Unconsumed attempts block conflicting replacement and are explicitly aborted before a successor
  compile can proceed.
- Public frame-plus-epoch receipt minting and raw backend outcome publication are removed. The
  kernel journal uses an instance-bound authority rather than allowing ordinary consumers to submit
  receipts independently.
- Production/test module splits keep `nara_ui_render::integration` and the root Host presenter from
  becoming mixed thousand-line implementation/test files. The gallery now installs one presenter
  authority instead of duplicating manual and product-plugin ownership.

# Evidence

- `crates/nara_ui/src/runtime.rs`
- `crates/nara_ui/src/runtime/{candidate,presentation,state}.rs`
- `crates/nara_ui_kernel/src/journal.rs`
- `crates/nara_input/src/timeline.rs`
- `crates/nara_ui_render/src/integration.rs`
- `crates/nara_ui_render/src/packet/outcome.rs`
- `crates/nara_ui_render/src/planner/mod.rs`
- `crates/nara_render_wgpu/src/{backend,error,lib}.rs`
- `src/ui_presentation.rs`
- `src/ui_presentation/attempts.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_ui_render/src/{integration_tests,packet_tests}.rs`
- `src/ui_presentation/tests.rs`
- `examples/ui_presentation_gallery.rs`
- `ui-module-consumer/tests/text.rs`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/architecture/adr/0109-root-local-ui-compilation-and-presentation-closure.md`

# Follow-up

- UIF-U4C is the sole next active unit. It owns the unique target-to-slot index, explicit
  active/suspended/retired lifecycle, zero-extent and missing-endpoint invalidation, endpoint
  migration with balanced held-edge ownership, and exact accessibility publication/action
  generations.
- UIF-U5 remains inactive until UIF-U4C publishes immutable evidence. Runtime UI product acceptance
  and Reference Game U15B proof remain paused until the Foundation Conformance Baseline is pinned.
- UIF-U5 must state that the backend outcome drain is a single-consumer owner seam and that every
  first-party backend consumes each permit exactly once or closes it fail-closed.
- No product widget, Editor, docking, generic binding, reactive invalidation or target aggregation
  capability is claimed by UIF-U3C.

# Citations
