---
type: "Verification Evidence"
title: "EFI-U5 Editor authoring-loop conformance"
description: "Verifies one published starter edit through the private Editor, project check, generated headless product, and accepted source-carrying export."
timestamp: 2026-08-15T23:30:00Z
record_id: "b1cd751875db4d91958a0fb9e609a04d"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u5", "starter", "export", "conformance"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6a4a9e2"
verified_by: "single-trajectory-nextest-editor-suite-all-features-review"
---

# Verification

EFI-U5 is implemented by commit `6a4a9e2`. One ignored, explicitly invoked acceptance trajectory
publishes an ordinary local-checkout `desktop-2d/v1` project through the first-party creation
transaction and then drives the private `EditorApplication` through:

1. select `main-camera` and change schema-backed `Camera2d.viewport_height` from `720` to `768`;
2. submit the obsolete base revision and observe structured stale-action feedback without mutation;
3. Undo to the saved content-state marker and Redo to restore the edit;
4. Save through a verified persistence receipt;
5. Close, reopen the persisted source under a fresh document identity, verify the edited value, and
   close again;
6. check the same project root and bind the exact saved Scene member digest;
7. run the generated presentation-free headless product from an unrelated working directory;
8. publish and accept a source-carrying export whose headless and desktop observations bind the same
   manifest and startup-content facts, with the exported Scene bytes equal to the saved source.

The harness consumes Editor models, project-check receipts, generated-product observations, and the
existing export observation protocol. It does not parse Scene source, add a script, or introduce an
Editor-private project format.

# Result

- Result: passed.
- The exact edit advances one document revision and one history state.
- The stale mutation leaves revision and value unchanged and publishes
  `editor.action.stale-document` plus inline `StaleAction` feedback.
- Undo returns the document to clean state; Redo restores the edited value and dirty state.
- Only the verified Save receipt marks the edited source clean.
- Close/Reopen publishes a new document authority backed by the saved bytes; final Close leaves no
  open Scene.
- Project check, the generated headless product, and both accepted export products report the same
  startup-content identity.
- The exported `project/scenes/startup.scene.json` is byte-identical to the Editor-saved source.

# Evidence

- Single real authoring/product/export trajectory:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  cargo nextest run -p nara-editor --test authoring_loop --run-ignored ignored-only
  ```

  Result: Nextest run `89eb8b91-f717-47ab-87f0-c3e0a5f4155e`; `1 passed` in `733.692 s`.
  The export owner built release headless and desktop binaries and completed real Winit/WGPU first-
  presentation acceptance. After this pass, behavior-neutral test cleanup removed one duplicate
  local value wrapper, a non-contractual document-ID inequality assertion, and an unused command
  return value; the final test compiled in the focused suite below without repeating the expensive
  product run.
- Final private Editor suite:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  cargo nextest run -p nara-editor
  ```

  Result: Nextest run `e045e326-0e25-4953-a2ed-9089ae4b0d63`; `4 passed`, the explicit heavy
  trajectory skipped by default.
- Final complete feature ceiling:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  cargo check --all-targets --all-features
  ```

  Result: passed in `2m 18s` with only pre-existing dead-code warnings.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- The native window observation remains owned by the EFI-U4 evidence record. EFI-U5 did not repeat
  that canary because its one real export acceptance already exercised the generated desktop WGPU
  product and the user requested proportional verification.

# Follow-up

- EFI-U1 through EFI-U5 and the Editor Authoring Loop v1 plan are complete. No successor is active.
- This record is an Editor authoring-slice conformance baseline, not the first complete Editor
  Product-Validated Baseline under ADR 0041.
- Viewport, in-Editor Play placement, IME/accessibility completion, docking, multi-window,
  unavailable-schema authoring, structured-data assets, and Nara UI Inspector parity remain
  separate trigger-gated successors.

# Citations

- `editor/tests/authoring_loop.rs`
- `editor/src/{app,platform}.rs`
- `crates/nara_tooling/src/{frame,workspace}.rs`
- `src/project_host/runtime/editor/{actions,model,persistence}.rs`
- `src/project_workflow/{creation,check,export}.rs`
- `starters/desktop-2d/v1/README.md`
- Commit `6a4a9e2`
