---
type: "Verification Evidence"
title: "RUP-U1 neutral modal-entry focus reproduction"
description: "Records the renderer-neutral product-triggered failure that pauses RUP-U1 and admits one focused UI Foundation correction."
timestamp: 2026-08-10T15:59:45Z
record_id: "cc8d66c49d134e9f9d18d680afe19f7a"
tags: ["ui", "runtime-ui", "focus", "modal", "rup-u1", "uif-correction"]
status: "blocked"
producer_id: "codex-root"
run_id: "ui-modal-entry-focus-correction"
related_plan: "docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3fe3c19"
verified_by: "codex-root"
---

# Verification

The renderer-neutral test
`nara_ui::adapter_tests::initial_modal_presentation_commits_focus_for_the_surface_owner_seat` was
added against UIF-U5 behavior and run with:

`CARGO_BUILD_JOBS=1 cargo nextest run -p nara_ui
initial_modal_presentation_commits_focus_for_the_surface_owner_seat --test-threads=1 --no-tests
fail --no-fail-fast`

Nextest run `650198a8-3023-4b3f-86a7-4c6691b3b719` failed exactly at the focused-state assertion.
The candidate compiled and committed an exact receipt, but the semantic snapshot for the Surface
Slot owner seat reported Retry as unfocused before any input.

# Result

RUP-U1 is paused under its R3 stop condition. This is a shared staged-session defect rather than a
Reference Game presenter omission:

- the reproduction uses `nara_ui` only;
- no WGPU, Winit, AccessKit or product callback participates;
- the Surface Slot already owns the endpoint seat, but candidate reconciliation iterates only seats
  created by earlier input;
- semantic projection correctly refuses to invent focus, so the owning fix must happen before final
  visual compilation and presentation commit.

The admitted correction is limited to passing the endpoint seat into staged candidate
reconciliation and applying the existing modal first-focus rule in committed session state.

# Boundaries

This record does not claim the correction is implemented and does not invalidate immutable UIF-U5
evidence. It creates a successor lineage. Runtime UI product work cannot resume until the focused
correction publishes a new baseline and the RUP plan pins it explicitly.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `crates/nara_ui/src/adapter_tests.rs`
