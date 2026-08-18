---
type: "Verification Evidence"
title: "EUSC-U4 egui structural authoring workflow"
description: "Verifies the bounded egui Add, Move/Reparent, and Delete workflow, exact frame-lowered action parity, stale-state containment, keyboard accessibility, and Host-owned outcome focus."
timestamp: 2026-08-18T06:06:03Z
record_id: "af32c829b365491e84fe0af17c46d6eb"
tags: ["engineering-memory", "verification", "editor", "ui", "prefab"]
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9540deb"
verified_by: "codex-root"
---

# Verification

EUSC-U4 was verified at commit `9540deb` from the repository root. Cargo commands ran serially and
used the shared target directory.

# Result

The fixed egui Editor shell now exposes Add, Move/Reparent, and confirmed subtree Delete for an
active UI Prefab. The Adapter renders only the closed catalog, exact project image choices, and
source-local placements supplied by the current authoring frame. It delegates structure lowering
to the Host and appends only the returned document- and revision-bound mutation to the ordered
action batch; egui owns no entity allocation, schema paths, placement repair, selection, history,
or persistence.

Transient forms close when document revision or image-choice generation changes. Host outcomes
return focus to the created or moved row, or to the surviving hierarchy location after deletion.
Keyboard-only Enter, Escape, Tab, invalid-field focus, accessible labels, and typed rejection text
are covered by the same representative flows rather than a control matrix.

# Evidence

- `$env:CARGO_BUILD_JOBS='1'; cargo nextest run --locked -p nara_tooling_egui
  --test-threads=1` - 19 passed.
- The focused create test captures the exact `UiStructureIntent`, proves the Adapter appends the
  Host callback result unchanged, and checks document, revision, kind-specific values, and
  placement. The existing root Host test independently exercises
  `EditorUiAuthoringFrame::lower_structure_intent` for the same frame seam.
- The focused delete/stale/keyboard test proves explicit confirmation, stale closure, Escape
  isolation, Enter activation, Tab traversal, and focus restoration without adding a GUI matrix.
- Independent correctness review findings for required image/action defaults, exact action-result
  correlation, invalid-field focus, Escape consumption, and Host-lowering parity were closed before
  the proving commit.
- `cargo fmt --all -- --check`, `git diff --check`, and staged-diff ownership checks passed before
  commit.

# Follow-up

EUSC-U5 owns one real source trajectory through structural mutation, Save/Reopen, project-content
refresh, and explicit Runtime restart, plus one bounded native UI Canvas successor-presentation
canary. It must reuse the existing product and platform owners without multiplying catalog,
backend, DPI, or platform cases.

# Citations

- [Editor UI Prefab Structural Composition v1 plan](../../../../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
