# UI Plan History and Current Entry Points

This page retains navigation for earlier UI plans. Their completion conditions and toolkit
assumptions describe their original scope; they do not schedule later work.

## Current Entry Points

- [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md) owns
  Editor workspace, panel, frontend and creation-template boundaries. The selected frontend is
  Dear ImGui; the earlier single-window egui implementation is historical.
- [Architecture map](../architecture/README.md) and
  [implementation status](../architecture/adr/implementation-status.md) locate current contracts,
  evidence and bounded remaining gaps.
- [Native Editor qualification](../verification/native-editor-qualification.md) describes actual
  Windows/DX12 checks and the limits of their platform/input claims.
- [Rust Product Recipes](../guides/rust-product-recipes.md) and
  [persistent components](../guides/persistent-components.md) explain current author workflows.

## Earlier Plans

| Plan | Historical role |
|---|---|
| [UI Foundation Correction](2026-08-08-001-refactor-ui-foundation-correction-plan.md) | Shared UI interaction, compilation and presentation baseline. |
| [Runtime UI Product Slice](2026-08-08-002-feat-runtime-ui-product-slice-plan.md) | First runtime HUD and terminal consumer of that baseline. |
| [Editor Authoring Loop v1](2026-08-08-003-feat-editor-foundation-workflow-plan.md) | First egui editing, persistence and external-product authoring loop. |

These plans are complete. Later Scene/Game views, supervised Play, image authoring, docking,
detached windows and the selected Dear frontend have their own contracts and evidence. Consult
the current owners above before treating an old successor list as unfinished work. New tasks are
chosen from user goals and observed product needs, not automatically from this history.
