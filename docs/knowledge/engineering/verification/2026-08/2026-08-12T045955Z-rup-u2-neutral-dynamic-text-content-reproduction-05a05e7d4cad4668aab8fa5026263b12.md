---
type: "Verification Evidence"
title: "RUP-U2 neutral dynamic text content reproduction"
description: "Records the renderer-neutral public-value failure that pauses RUP-U2 and admits one focused UI Foundation correction."
timestamp: 2026-08-12T04:59:55Z
record_id: "05a05e7d4cad4668aab8fa5026263b12"
tags: ["ui", "runtime-ui", "text", "authoring", "rup-u2", "uif-correction"]
status: "blocked"
producer_id: "codex-root"
run_id: "ui-dynamic-text-content-correction"
related_plan: "docs/plans/2026-08-12-001-refactor-ui-dynamic-text-content-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "abfbe84"
verified_by: "codex-root"
---

# Verification

The renderer-neutral adapter test
`nara_ui::adapter_tests::dynamic_text_content_preserves_authored_adapter_presentation` was added
against the pinned Foundation baseline. With the proposed `with_content` operations temporarily
removed from both owning layers, the focused command was run serially:

`CARGO_BUILD_JOBS=1 cargo nextest run -p nara_ui_kernel -p nara_ui
dynamic_text_content_preserves_authored --test-threads=1 --no-tests fail --no-fail-fast`

Compilation failed at `crates/nara_ui/src/adapter_tests.rs` because public `nara_ui::UiText` had no
content-only replacement operation. The wrapper also exposes no complete set of authored-field
getters from which an external adapter could reconstruct an equivalent value.

# Result

RUP-U2 is paused under R3. This is a shared immutable-value API defect rather than a Reference Game
rule:

- the reproduction uses only `nara_ui` public authoring values;
- no Scene loader, ECS runtime, WGPU, Winit, package tool or product identity participates;
- reconstructing `UiText::new(dynamic_content, size)` would silently replace authored brush,
  alignment, overflow or semantic-text policy with product-owned defaults;
- adding product-specific getters or copying all authored fields into the game would preserve the
  ownership defect instead of fixing it.

The admitted correction is limited to consuming content replacement on the existing immutable
`UiText` value in the kernel and Runtime adapter layers.

# Evidence

- Red result: the focused test target failed to compile specifically on the missing public
  `with_content` operation when both proposed methods were absent.
- The same focused command passed two tests in nextest run
  `d5a2ed83-d0a4-4401-8b87-7ccc00fc556f` after restoring the owning operations.
- Strict no-dependency Clippy passed for `nara_ui_kernel` and `nara_ui` after the correction.

# Follow-up

Implement UIFD-U1, publish immutable implementation evidence and a successor Foundation
Conformance Baseline, then explicitly rebase RUP-U2. This record does not invalidate or rewrite
earlier UIF/UISM evidence.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/plans/2026-08-12-001-refactor-ui-dynamic-text-content-correction-plan.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `crates/nara_ui_kernel/src/style.rs`
- `crates/nara_ui/src/authoring.rs`
- `crates/nara_ui/src/adapter_tests.rs`
