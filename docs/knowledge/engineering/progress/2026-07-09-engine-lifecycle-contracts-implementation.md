---
type: Work Progress
title: Engine lifecycle contracts implementation
timestamp: 2026-07-09T22:03:26+08:00
status: implemented
related_plan: docs/plans/2026-07-09-006-refactor-engine-lifecycle-contracts-plan.md
git_branch: main
tags: [nara, ce-work, lifecycle, plugins, tooling, render, ui]
---

# Summary

Implemented the engine lifecycle contract hardening plan on `main` with fearless pre-1.0 breaks.

# Delivered

- Plugin metadata and explicit plugin groups are implemented; `WgpuRenderPlugin` no longer installs sprite/UI submitters implicitly.
- Root facade exports are split into gameplay, authoring, tooling, and backend preludes.
- Component schema uses capability metadata instead of a single serializable flag.
- Runtime frame lifecycle now separates real, virtual, fixed, and render time, with explicit frame outcome and cancellable close requests.
- Asset watch and reload translation failures are surfaced through diagnostics/events instead of silent drops.
- wgpu sprite/UI rendering now has texture cache stats, grace-frame eviction, alpha-mode pipeline keys, and UI-owned render types.
- `EditorWorkspace` is the UI-agnostic owner for open scene documents, active document, selection, dirty/saved revision, external reload state, and per-document undo/redo.
- egui tooling panels now return `EditorWorkspaceCommand` values instead of owning editor mutation concepts.
- Runtime UI interaction is target/view-aware and preserves pointer capture until release or invalidation.
- Architecture docs were aligned with the implemented contracts and remaining risks.

# Commits

- `0c26177` `docs(plan): define engine lifecycle hardening work`
- `ac0bc67` `refactor(engine)!: harden plugin facade and schema contracts`
- `6f0c28a` `refactor(runtime)!: make frame lifecycle explicit`
- `0f8cb8c` `refactor(render)!: define gpu resource lifetimes`
- `5504685` `refactor(tooling)!: introduce editor workspace authority`
- `62a3747` `refactor(ui)!: route pointer interaction by target`

# Remaining High-Risk Follow-Ups

- Runtime diagnostics and observability bus.
- Persistent document migrations and golden fixtures.
- Task backpressure, cancellation, and long-running diagnostics.
- GPU upload budgets and full resource-class lifetime policy.
- Runtime UI text, keyboard/gamepad focus, action routing, and eventual editor dogfooding.
- File-backed `nara_project` manifest and effective runtime settings lowering.

# Citations

- [Plan](../../../plans/2026-07-09-006-refactor-engine-lifecycle-contracts-plan.md)
- [Foundation](../../../architecture/nara-foundation.md)
- [Plugin metadata ADR](../../../architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md)
