# Nara UI Program Roadmap

This page is a navigation index, not an execution authority or an implementation ledger. The
active plan is the only plan allowed to order work; successor plans remain inactive until the
Foundation publishes a pinned conformance record and exactly one is selected by product priority.

```text
UI Foundation Correction (active)
        +-----------------------------+
        |                             |
        v                             v
Runtime UI Product Slice      Editor Foundation Workflow
        |                             |
        +-------------+---------------+
                      v
             Nara UI Inspector Parity
```

## 1. UI Foundation Correction

Plan: [2026-08-08 UI Foundation Correction](../plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md)

Closes the shared contracts before product expansion:

- atomic presented-session interaction;
- root-local validation and last-good order rebinding;
- frame-global resource budgets and backend capability closure;
- Host-owned surface/session identity and bounded accessibility ingress;
- neutral conformance examples and a versioned Foundation Conformance Baseline.

## 2. Runtime UI Product Slice

Plan: [Runtime UI Product Slice](../plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md)

Consumes a pinned Foundation Conformance Baseline to prove the data-authored HUD, terminal, Retry
and headless / desktop package workflow. It may expose a missing shared capability, but cannot
change a shared Interface without a focused successor baseline and explicit rebase.

## 3. Editor Foundation Workflow

Plan: [Editor Foundation Workflow](../plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md)

Builds the first single-window egui Editor flow: open, Scene Tree, Inspector, typed patch/undo,
save/reopen, Viewport, Play/Stop and diagnostics. Docking, detached windows and convergence on
Nara UI remain evidence-gated successors.

Runtime UI and Editor are independent technical successors. Only one becomes active at a time; the
choice is product priority, not a fabricated dependency. The later Nara UI Inspector parity gate,
not the egui Inspector, is the heterogeneous consumer for shared widget/convergence decisions.

## Admission rule

Open-GPUI, Dear ImGui, Godot, Bevy and Unity are comparative sources. A shared Nara module needs
an owner, a named invariant, a neutral assertion and either independent comparative pressure or a
second heterogeneous consumer. A product-specific need alone stays in its adapter or data model.
