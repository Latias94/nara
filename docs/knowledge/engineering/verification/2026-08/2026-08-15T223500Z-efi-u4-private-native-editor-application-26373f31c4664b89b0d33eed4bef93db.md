---
type: "Verification Evidence"
title: "EFI-U4 private native Editor application"
description: "Verifies the private native nara-editor package, exact frame/action integration, bounded failure feedback, dirty-close arbitration, and one real Windows WGPU window canary."
timestamp: 2026-08-15T22:55:10Z
record_id: "26373f31c4664b89b0d33eed4bef93db"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u4", "native"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8977dc5"
verified_by: "focused-nextest-build-native-canary-code-review"
---

# Verification

EFI-U4 is implemented by commit `8977dc5`. The private `nara-editor` workspace package now opens
one project capability and its configured startup Scene, owns one Winit window plus the egui Winit
and WGPU adapters, renders the fixed toolbar/Scene Tree/Inspector/Diagnostics shell, and returns
all durable work through the existing immutable `EditorFrameModel` and ordered `EditorActionBatch`
boundary.

The application does not own document contents, selection, history, persistence, or Play state.
Field parse failures enter the Host as typed validation actions; schema and stale-frame failures
publish exact inline field feedback while their structured diagnostics remain in the Host report.
Dirty native close uses the existing Save, Discard, or Cancel workspace-intent state machine.

# Result

- `editor/` is `publish = false` and depends on the root facade with only `project-workflow` and
  `tooling-egui`; no runtime or engine crate depends on the application package.
- Native window, event-loop, egui Context, clipboard/IME adapter, WGPU painter, resize, suspend, and
  repaint lifetimes remain private to `editor/src/platform.rs`.
- The first shell is intentionally fixed and single-window. It contains no Viewport, docking,
  multi-window, plugin-contribution, or public Editor application framework.
- Missing project roots, missing startup Scenes, and unavailable component schemas fail before an
  application is constructed.
- Empty repaint does not change document revision, dirty state, or selection; dirty close Cancel
  retains the session and Discard reaches a clean process exit.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling_egui`
  - passed: 9 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara-editor --test application_contract`
  - passed: 4 tests.
- `CARGO_BUILD_JOBS=1 cargo build -p nara-editor --bin nara-editor`
  - passed with only unrelated existing warnings.
- `cargo fmt --all -- --check`
  - passed.
- `git diff --check`
  - passed apart from repository line-ending notices.
- Windows native canary:
  - created one ordinary `desktop-2d/v1` starter with the existing `nara new` product path;
  - launched `target/debug/nara-editor.exe <project-root>`;
  - observed one titled native window containing the fixed toolbar, Scene Tree, Inspector, and
    bottom Diagnostics panel through a DPI-aware capture;
  - sent the titled window a normal close request and observed process exit code `0`;
  - no second product window or second GPU owner was created.

# Follow-up

Activate EFI-U5 only. Prove one bounded `desktop-2d/v1` authoring trajectory through exact edit,
stale rejection, Undo/Redo, Save, close/reopen, existing project check, generated-product run, and
source-carrying export. Keep Viewport, docking, multi-window, unknown-schema authoring, and the Nara
UI parity experiment inactive.

# Citations

- `editor/Cargo.toml`
- `editor/src/{main,app,platform}.rs`
- `editor/tests/application_contract.rs`
- `crates/nara_tooling/src/frame.rs`
- `crates/nara_tooling_egui/src/{editor,inspector,tests}.rs`
- `src/project_host/runtime/editor/{actions,model}.rs`
- Commit `8977dc5`
