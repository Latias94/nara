---
title: UI Dynamic Text Content Projection Correction - Plan
type: refactor
date: 2026-08-12
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
execution_state: inactive
product_contract_source: docs/architecture/reference-game-ui-product-slice.md
origin: docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md
depends_on: docs/knowledge/engineering/verification/2026-08/2026-08-10T155957Z-uifm-u2-successor-foundation-conformance-baseline-19cfa496dc2346368764057367cc2163.md
pauses: docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md
completion_evidence: docs/knowledge/engineering/verification/2026-08/2026-08-12T051123Z-uifd-u2-successor-foundation-conformance-baseline-a628828a8ff941239d79a0b8b3d60316.md
plan_id: ui-dynamic-text-content-correction-2026-08
unit_namespace: UIFD
operator_execution_authority: external-control-plane
---

# UI Dynamic Text Content Projection Correction - Plan

## Goal Capsule

- **Objective:** Let a Runtime UI adapter replace display text content while preserving every
  Scene/Prefab-authored presentation and semantic field on the immutable text value.
- **Trigger:** RUP-U2 requires gameplay facts to update HUD text without making Rust reconstruct or
  overwrite authored typography, brush, alignment, overflow or semantic-text policy. A neutral
  `nara_ui` adapter reproduction fails to compile against the pinned Foundation baseline because the
  public wrapper exposes neither complete read access nor a content-only replacement operation.
- **Scope:** Add one value-preserving content replacement operation to `nara_ui_kernel::UiText` and
  the public `nara_ui::UiText` adapter, with neutral tests. Do not add product facts, callbacks,
  bindings, mutable handles, generic patch APIs or another text model.
- **Resume rule:** Publish one successor Foundation Conformance Baseline, update the implementation
  ledger, then explicitly rebase and reactivate RUP-U2.

## Owning Contract

`UiText` is an immutable authoring value. Product adapters may project dynamic domain facts into its
display content, but the authoring document remains authoritative for presentation and semantic
policy:

```text
authored UiText
    + dynamic display content
            |
            v
content-only replacement
    -> same logical size
    -> same brush
    -> same alignment and overflow
    -> same semantic-text policy
```

Reconstructing `UiText` from a subset of public fields is not an acceptable workaround because it
silently transfers ownership of authored facts into product Rust code.

## Requirements

- R1. `nara_ui_kernel::UiText` exposes one consuming content replacement operation.
- R2. `nara_ui::UiText` exposes the same operation without leaking kernel internals.
- R3. Replacement preserves logical size, brush, alignment, overflow and semantic text exactly.
- R4. The operation performs no validation unrelated to content and introduces no callback,
  interior mutability, runtime identity or product-specific vocabulary.
- R5. Kernel and adapter tests prove the preservation contract without ECS, Scene, WGPU, Winit or
  Reference Game types.
- R6. The neutral adapter test is observed red against the pinned baseline and green after the
  owning correction.
- R7. RUP-U2 may consume the operation only after a successor baseline is published and pinned.

## Units

### UIFD-U1 — Immutable text content replacement

**Goal:** Correct the owning value API and prove exact field preservation.

**Files:** `crates/nara_ui_kernel/src/style.rs`, focused kernel tests,
`crates/nara_ui/src/authoring.rs`, and focused adapter tests.

**Verification:** Serial focused `nara_ui_kernel` and `nara_ui` nextest, strict scoped Clippy,
formatting and diff review.

**Exit:** Both neutral layers can replace content without reconstructing or changing authored
presentation or semantic policy.

### UIFD-U2 — Successor baseline and RUP rebase

**Depends on:** UIFD-U1.

**Goal:** Publish the correction lineage and resume RUP-U2 without rewriting prior Foundation or
product evidence.

**Files:** immutable verification and registration shards, this plan, the Runtime UI Product Slice
frontmatter, implementation ledger and derived engineering-memory rollups.

**Verification:** Re-run the focused neutral tests, validate engineering memory and render derived
rollups deterministically.

**Exit:** A successor Foundation baseline names the proving commit, RUP pins that baseline, this
plan is inactive, and RUP-U2 is again the sole active UI unit.

## Non-Goals

- A generic data-binding or expression language.
- Editable text, localization, rich text, project-font selection or animation.
- New widgets, reactive dependency graphs, mutable text handles or ECS callbacks.
- Any Reference Game identity, action or layout rule in shared crates.

## Verification Contract

- Cargo commands run serially with `CARGO_BUILD_JOBS=1`.
- The renderer-neutral adapter reproduction must be red before the correction and green after it.
- Tests compare structured fields directly; snapshots, opaque hashes and product fixtures cannot
  substitute for the owning contract.
- `cargo fmt --all -- --check`, strict scoped Clippy, `git diff --check`, engineering-memory
  validation and deterministic rendering close UIFD-U2.

## Definition of Done

- R1-R7 have executable evidence.
- The public change is exactly one content-only immutable value operation in each owning layer.
- No compatibility wrapper, product-specific helper or second text representation remains.
- One immutable successor baseline supersedes `19cfa496dc2346368764057367cc2163`
  only for this corrected behavior, and RUP-U2 explicitly rebases before resuming.
