# ADR-008 Game UI and future convergence

**Accepted on 2026-11-24**

Consolidate `nara_ui_kernel`, `nara_ui`, `nara_ui_widgets`, `nara_ui_render`, and `nara_text` into a
single game UI crate. Use `taffy` for layout, `parley` and `swash` for text, and the Nara renderer
for a 2D signed-distance-field overlay. Draw HUDs with `LoadOp::Load`. Use game consumers to drive
reactive updates, layout caching, and accessibility work.

Consider running the editor on `nara_ui` only after it demonstrates all five capabilities:

1. Multilingual bidirectional and CJK text editing with IME preedit.
2. 10,000-row virtual lists at 120 FPS with stable scrolling and keyboard focus.
3. Viewport-aware popups and menus with nested focus management.
4. Incremental AccessKit tree synchronization.
5. Native floating windows and mixed-DPI behavior at least as capable as the Open-GPUI shell.
