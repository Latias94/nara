# Nara UI Program Roadmap

This page is a navigation index, not an execution authority or an implementation ledger. The
active plan is the only plan allowed to order work. UI Foundation, the Runtime UI Product Slice,
and Editor Authoring Loop v1 are complete. No successor is active.

```text
UI Foundation Correction (complete)
        +-----------------------------+
        |                             |
        v                             v
Runtime UI Product Slice      Editor Authoring Loop v1
         (complete)                   (complete)
        |                             |
        +-------------+---------------+
                      v
             Nara UI Inspector Parity
                    (inactive)
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

## 3. Editor Authoring Loop v1

Plan: [Editor Authoring Loop v1](../plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md)

Builds the first single-window egui Editor flow: open, canonical Scene Tree, Inspector, exact
revision-bound edit, stale rejection, Undo/Redo, Save, Close/Reopen/Close, diagnostics, and handoff
of the same source change to check, generated-product run, and source-carrying export. Viewport,
in-Editor Play placement, docking, detached windows, IME/accessibility completion, and convergence
on Nara UI remain evidence-gated successors.

Runtime UI and Editor are independent technical successors. Only one becomes active at a time; the
choice is product priority, not a fabricated dependency. Both first product slices are now
complete. The later Nara UI Inspector parity gate, not the egui Inspector, is the heterogeneous
consumer for shared widget/convergence decisions and requires a new reviewed plan before work.

## Admission rule

Open-GPUI, Dear ImGui, Godot, Bevy and Unity are comparative sources. A shared Nara module needs
an owner, a named invariant, a neutral assertion and either independent comparative pressure or a
second heterogeneous consumer. A product-specific need alone stays in its adapter or data model.
