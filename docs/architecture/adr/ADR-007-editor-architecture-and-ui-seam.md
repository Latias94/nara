# ADR-007 Editor architecture and UI boundaries

**Accepted on 2026-11-24**

Use Open-GPUI Mode A for the editor. It owns native windows and events, IME, AccessKit, clipboard,
drag and drop, and floating `dockspace` panes. Use `gpui_wgpu` across Windows, macOS, and Linux.
Retain DirectWrite on Windows and CoreText on macOS while replacing bespoke D3D11 and Metal
renderers. Use peer top-level windows and style the shell with Design Kit v0.4.

## Ownership boundaries

1. `nara_editor_core` owns a synchronous typed `EditorSession`, multi-selection, shared and mixed
   Inspector values, precise `value_draft` parsing, `EditorRegistry`, and gesture-merged Undo. It
   works headlessly. Only I/O, imports, builds, and Play need asynchronous jobs.
2. Keep `dockspace`, `jellyflow-core`, Design Kit tokens, and widget behavior independent of the UI
   shell. Use `gpui_base` for unstyled controls and a thin docking adapter. Route Dialogue Graph
   edits through the same source document and Undo path as other edits.
3. Render world-space grid, colliders, selection outlines, and Gizmos in `EditorOverlayPass`.
   Nara owns CPU picking and Gizmo state. The shell owns screen-space controls, focus, keyboard
   contexts, and OS pointer capture.
4. Grow `nara_ui` on game workflows. Reassess editor use when it meets the criteria in
   [ADR-008](ADR-008-game-ui-and-future-convergence.md).

Scope viewport shortcuts to viewport focus and give text editing priority within its own context.
Commit one Undo action per completed drag and cancel it on Escape or focus loss. Use asset and
entity IDs as cross-panel drag payloads and preview viewport placement before committing it.

Keep the standalone game runner on Winit. Retain [`vendor/winit`](../../../vendor/winit/NARA-PATCH.md)
until an upstream release includes and passes native qualification for both Windows 11 mixed-DPI
dragging and macOS ISO physical key handling.
