---
title: UI Modal Entry Focus Correction - Plan
type: refactor
date: 2026-08-10
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
execution_state: inactive
product_contract_source: docs/architecture/reference-game-ui-product-slice.md
origin: docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md
depends_on: docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md
pauses: docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md
completion_evidence: docs/knowledge/engineering/verification/2026-08/2026-08-10T155957Z-uifm-u2-successor-foundation-conformance-baseline-19cfa496dc2346368764057367cc2163.md
plan_id: ui-modal-entry-focus-correction-2026-08
unit_namespace: UIFM
operator_execution_authority: external-control-plane
---

# UI Modal Entry Focus Correction - Plan

## Goal Capsule

- **Objective:** Make the Surface Slot endpoint seat part of staged candidate interaction authority
  so a newly presented modal commits its first eligible focus target before input, final visual
  compilation and semantic publication.
- **Trigger:** RUP-U1 requires Completed and Defeated modal entry to focus Retry. The renderer-neutral
  `nara_ui` adapter reproduction fails on the pinned UIF-U5 baseline without WGPU, Winit or product
  presenter involvement.
- **Scope:** Correct the existing staged presentation/session contract only. Do not add autofocus
  components, a generic focus-policy DSL, new widgets, callbacks, product-specific Retry logic or a
  public programmatic-focus service.
- **Resume rule:** Publish one successor Foundation Conformance Baseline, update ADR 0108 and the
  implementation ledger, then explicitly rebase and reactivate the Runtime UI Product Slice.

## Owning Contract

The Surface Slot already owns one endpoint `(window, seat, pointer)`. Candidate staging currently
reconciles only seats created by prior input. Therefore modal reconciliation has no seat to focus on
an initial modal or on a base-to-modal transition before any UI input. A later Enter, Tab or semantic
action first mutates session state instead of activating the already visible control.

The correction makes the endpoint seat an input to candidate reconciliation:

```text
Surface Slot endpoint seat
        + candidate frame
        + prior presented session
                |
                v
staged interaction reconciliation
        -> modal first eligible focus
        -> controlled-widget reducer state
        -> final immutable frame
        -> backend acceptance and exact receipt
```

Semantic projection remains read-only. It reports focus already committed by staged session
reconciliation; it does not synthesize a default focus while being read.

## Requirements

- R1. `UiInteractionSession::stage_candidate_presentation` receives the authoritative endpoint seat
  and admits it before structural/modal reconciliation.
- R2. An initial modal and a base-to-modal transition focus the first enabled, visible focus target
  in stable structural order without prior input.
- R3. Focus reconciliation is reduced before final candidate compilation, so paint, reducer state,
  focus facts and semantics describe the same candidate generation.
- R4. An initial non-modal frame does not automatically focus an ordinary control. Semantic reads
  cannot invent focus.
- R5. Disabled, hidden, retired or out-of-modal controls are not selected; reconciliation emits no
  action intent.
- R6. Surface endpoint replacement clears old endpoint session state and the next candidate seeds
  only the new owning seat.
- R7. The correction changes no product callback, Scene/Prefab vocabulary, widget catalogue,
  presentation receipt or backend boundary.
- R8. The Reference Game product trajectory removes its manual Tab prerequisite and observes the
  same focused Retry identity through pending and rejection.

## Units

### UIFM-U1 — Surface-owner modal focus reconciliation

**Goal:** Correct candidate staging and prove same-generation focus facts.

**Files:** `crates/nara_ui_kernel/src/interaction/**`, `crates/nara_ui/src/runtime/state.rs`, focused
kernel/adapter tests, ADR 0108 and the implementation ledger.

**Test scenarios:** Initial modal without input; base-to-modal transition; initial non-modal frame;
disabled first candidate; presentation commit semantics; endpoint-seat replacement.

**Verification:** Serial focused `nara_ui_kernel` and `nara_ui` nextest, strict scoped clippy,
formatting and diff review.

**Exit:** The renderer-neutral reproduction passes and every focus fact is staged before final frame
publication.

### UIFM-U2 — Successor baseline and RUP rebase

**Depends on:** UIFM-U1.

**Goal:** Publish the correction lineage and resume product work without rewriting UIF-U5 evidence.

**Files:** immutable verification/registration shards, this plan, the Runtime UI Product Slice
frontmatter, ADR ledger and derived engineering-memory rollups.

**Verification:** Re-run the focused Reference Game trajectory without manual focus injection;
validate and deterministically render engineering memory.

**Exit:** A successor UIF baseline names the proving commit, RUP pins that baseline and RUP-U1 again
becomes the sole active product unit.

## Non-Goals

- Explicit authored autofocus keys or arbitrary focus policies.
- Text editing, IME, scroll, menus, docking, multi-window policy or Editor UI convergence.
- A public direct input executor or a generic Host callback registry.
- Product-specific logic in `nara_ui_kernel` or `nara_ui`.

## Verification Contract

- Cargo commands run serially with `CARGO_BUILD_JOBS=1`.
- The original renderer-neutral adapter reproduction must be red before the correction and green
  after it.
- Kernel tests prove seat admission and structural modal selection; adapter tests prove reducer,
  immutable frame and semantic agreement.
- Reference Game evidence remains supplementary until the shared correction baseline is published.
- `cargo fmt --all -- --check`, scoped strict clippy, `git diff --check`, engineering-memory
  validation and deterministic rollup rendering close UIFM-U2.

## Definition of Done

- R1-R8 have executable evidence.
- ADR 0108 distinguishes committed modal-entry focus from prohibited semantic focus invention.
- The public inventory changes only where the endpoint seat must cross the staged reconciliation
  boundary; obsolete signature shapes are removed without compatibility wrappers.
- A successor immutable baseline supersedes UIF-U5 for this behavior and RUP-U1 explicitly rebases
  before resuming.
