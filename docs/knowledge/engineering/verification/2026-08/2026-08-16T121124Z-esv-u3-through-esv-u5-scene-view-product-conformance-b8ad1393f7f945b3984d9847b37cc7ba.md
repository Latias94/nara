---
type: "Verification Evidence"
title: "ESV-U3 through ESV-U5 Scene View product conformance"
description: "Verifies edit-world preview publication, presented-generation Scene View picking, real Editor controls, source round-trip, downstream check/run/export, and bounded GPU owner recovery."
timestamp: 2026-08-16T12:11:24Z
record_id: "b8ad1393f7f945b3984d9847b37cc7ba"
resource: "editor-scene-composition-view-v1"
tags: ["editor", "scene-view", "scene-composition", "esv-u3", "esv-u4", "esv-u5"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f99de44"
verified_by: "focused-nextest-check-native-canary"
---

# Verification

This record closes ESV-U3 through ESV-U5 against implementation commit `f99de44`. It composes the
earlier ESV-U1 execution receipt and ESV-U2 revision-bound composition evidence with the landed
edit-preview, picking, native control, persistence, downstream-consumer, and recovery facts.

# Result

- **ESV-U3 complete**: the private Editor Host materializes only the accepted edit document into a
  dedicated preview World. Each candidate is bound to the exact document and source revision and
  carries ordered sprite and hit facts for one image target. Play World state is not consulted.
- **ESV-U4 complete**: the native Editor executes Scene image rendering, egui composition, and
  window presentation through one WGPU owner. Pointer selection is interpreted only against an
  `EditorScenePickStamp` containing the document revision and exact
  `WgpuEditorCompositeReceipt` target, backend instance, device epoch, and presentation generation.
  Stale receipt, geometry, document, and revision tuples reject without changing selection.
- **ESV-U5 complete**: real egui controls create `editor_entity_1`, edit
  `Transform2d.translation.x` to `128`, drive undo/redo/save/reopen, and preserve the same authored
  source through project check, the generated headless product, and source-carrying export.
- **Recovery complete for this fixed native owner**: the first terminal GPU fault retires and
  reconstructs the entire private execution owner once. A second consecutive fault leaves the
  document and window alive in an explicit unavailable state; the Host-local `F5` retry command is
  then required. No GPU handle or retired target lease is reused.
- **Native presentation canary passed**: on Windows at 150% display scale, the real Editor window
  presented a nonblank Scene View, moved the blue quad to the right after the `128` edit, reported a
  successful save, and selected `editor_entity_1` after the user first selected `main-camera` and
  then clicked the presented quad. The saved Scene source contains `translation.x = 128.0`.

# Evidence

Focused serial verification:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo nextest run -p nara-editor --run-ignored ignored-only starter_edit_crosses_editor_check_generated_product_and_export
PASS 1/1 (110.689s)

$env:CARGO_BUILD_JOBS='1'; cargo nextest run -p nara-editor gpu_recovery_requires_explicit_retry_after_one_automatic_replacement
PASS 1/1

$env:CARGO_BUILD_JOBS='1'; cargo check -p nara-editor --tests
PASS

$env:CARGO_BUILD_JOBS='1'; cargo build -p nara-editor
PASS

.\target\debug\nara-editor.exe <guarded-project-root>
PASS: presented, edited, saved, and picked the moved entity
```

The ignored product trajectory uses `egui_kittest` to activate the real `+ Root`, Inspector text,
Undo, Redo, Save, and reopened Scene Tree controls. It verifies the candidate clip coordinate
`[0.2, 0.0]`, stale-revision failure atomicity, saved-source reopening, and the existing
check/generated-headless/export consumers. The separate native canary uses the production Winit,
egui, WGPU, persistence, and presented-generation picking path; its final capture is
`nara-editor-esv-final-presented-pick.png`.

The following earlier focused gates remained green during the ESV-U4 landing:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo nextest run -p nara_tooling_egui
PASS 10/10

$env:CARGO_BUILD_JOBS='1'; cargo nextest run -p nara-editor
PASS 4/4; 1 ignored

$env:CARGO_BUILD_JOBS='1'; cargo nextest run -p nara accepted_scene_revision_publishes_one_matching_preview_candidate
PASS 1/1
```

# Follow-up

No ESV successor is active. Game View/Play presentation, docking or detached windows, a Nara UI
Inspector parity trial, and broader render scheduling remain separate trigger-gated workflows. This
record proves the fixed Scene View product path; it does not authorize a general RenderGraph,
multi-viewport framework, cross-toolkit widget abstraction, or Editor-wide product validation.

# Citations

- [Editor Scene Composition and Scene View v1](../../../../plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md)
- [ADR 0111](../../../../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md)
- [ESV-U1 execution contract](2026-08-16T084916Z-esv-u1-editor-scene-view-execution-contract-1de9903b1b7b4623a0633b2a6565e0b0.md)
- [ESV-U2 revision-bound composition](2026-08-16T091730Z-esv-u2-revision-bound-scene-composition-2ea0e3e881ff4c6a9d98e52e0937445a.md)
- Commits `ef5dda5`, `70de73e`, and `f99de44`
