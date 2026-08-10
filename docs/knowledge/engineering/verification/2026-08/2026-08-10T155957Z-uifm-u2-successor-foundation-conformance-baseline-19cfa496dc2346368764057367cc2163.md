---
type: "Verification Evidence"
title: "UIFM-U2 Successor Foundation Conformance Baseline"
description: "Publishes the modal-entry-focus correction as the successor shared UI baseline and permits an explicit RUP-U1 rebase."
timestamp: 2026-08-10T15:59:57Z
record_id: "19cfa496dc2346368764057367cc2163"
tags: ["ui", "foundation", "conformance", "baseline", "uifm-u2"]
status: "complete"
producer_id: "codex-root"
run_id: "uifm-u2"
related_plan: "docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "81f0394"
verified_by: "codex-root"
supersedes: "70b35505e8da4cb388c36d3ede602259"
---

# Verification

- UIFM-U1 implementation commit `40d3f87` and immutable verification record
  `04e51278170c4a839dbeb0c9658741d8` prove the focused owning-contract correction.
- The focused `nara_ui_kernel`, `nara_ui` and Reference Game `ui_product` nextest gates passed 31,
  63 and nine tests respectively on that implementation.
- Strict scoped no-dependency Clippy, `cargo fmt --all -- --check` and `git diff --check` passed.
- Engineering-memory validation passed and deterministic rollups rendered cleanly; only registered
  legacy missing-identifier warnings remain.

# Successor Baseline

This record supersedes UIF-U5 record `70b35505e8da4cb388c36d3ede602259` for subsequent product
consumers. It preserves the UIF-U5 public inventory and conformance claims, with one focused
correction:

- the Surface Slot endpoint seat participates in staged candidate reconciliation;
- initial modal focus is committed before final paint and semantic publication;
- initial non-modal presentation remains unfocused;
- endpoint replacement transfers modal-entry focus authority only to the successor seat.

The successor baseline does not claim Runtime UI product validation. RUP-U1 remains responsible for
the real gameplay trajectory, backend-presented desktop proof, first-party platform accessibility,
headless parity and source/package roundtrip.

# Lineage

```text
UIF-U5 70b35505e8da4cb388c36d3ede602259
    -> neutral RUP-U1 defect reproduction cc8d66c49d134e9f9d18d680afe19f7a
    -> UIFM-U1 implementation 40d3f87
    -> UIFM-U1 verification 04e51278170c4a839dbeb0c9658741d8
    -> UIFM-U2 successor baseline 19cfa496dc2346368764057367cc2163
    -> explicit RUP-U1 rebase
```

# Citations

- `docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155955Z-uifm-u1-modal-entry-focus-correction-04e51278170c4a839dbeb0c9658741d8.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
