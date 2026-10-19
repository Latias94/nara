---
type: Verification Evidence
title: Foundation U13 stateful native panel composition
description: Paired source-built panel factories own independent drafts through native startup and GPU reconstruction.
timestamp: 2026-10-19T09:38:02Z
record_id: a87566900fa64c97bc0fac68f43378c9
producer_id: codex-root-foundation-u13-stateful-native-panels
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Structure

`EditorNativeComposition` admits a panel descriptor and its per-instance frontend factory
together. Empty identities, duplicates and replacement of built-in kinds are rejected. Batch
installation validates against a candidate registry/layout before publishing either, so a later
conflict cannot leave partially installed contributions. The ordinary and trusted native runners
use the same composition installation and `NativeEditorHost` construction path.

`ImguiEditorPanel` owns one instance's frontend draft. Its context exposes read-only projections
and the existing admitted action vocabulary; it grants no mutable document, World or GPU access.
The old public stateless painter registration was removed. A private function-pointer adapter
keeps built-in painters on the same per-instance widget owner and retirement path.

Closing or hiding a panel retires only its local draft state. Retargeting, changing its resolved
document, replacing the project, and resetting/restoring the layout also retire affected state.
Docking and relocation preserve it. Native startup distinguishes a fresh frontend from a
recovered frontend with a private enum; GPU reconstruction moves the retained frontend,
factories and surviving instances instead of reinstalling composition. First-load preferences
and retained frame/view counters keep their existing owners.

# Independent Review And Corrections

Review found that the public embedded path could reset or restore a workspace without notifying
the frontend. Numeric panel IDs could then identify another kind, or an identical restored
instance could inherit a retired draft. The layout now owns a private, nonserialized `Arc<()>`
lifecycle token. Reset and restore establish a fresh token, including an identical restore;
ordinary layout clones retain it. The frontend compares token identity before painting and
retires old widgets when it changes. Holding the previous token avoids identity reuse while it
is observed. Widgets additionally compare their registered kind during reconciliation and
dispatch. Preference serialization and layout value equality remain unchanged.

The public regression covers an earlier-sorting `a.custom` contribution taking a former built-in
ID after workspace reset, identical-content restore, and direct public layout reset. Two custom
contributions assert their own factory kinds and verify fresh creation/Drop for each lifecycle.

# Verification

Root-owned runs used Windows, Rust 1.95.0, the root lockfile and serialized Cargo with
`CARGO_BUILD_JOBS=1`. Source was the active U13 working-tree implementation and its described
follow-up corrections; these runs do not establish a final committed-source release claim.

- `c987b415-1f7c-44b8-9d48-e5a1744cae82`: the ordinary Editor library, panel and generic-component
  cohort passed 147 tests; 16 native tests were ignored. This protects composition admission,
  panel independence and the surrounding Inspector/application workflows.
- `8a069d68-50e9-4b1e-af38-0b3bafd34ed9`: the public regression
  `public_layout_reset_and_restore_retire_objects_before_reusing_instance_ids` passed. The new
  native panel trajectory failed its reopen freshness assertion: it inspected the last pre-close
  painter observation after only one reopened frame. Its preceding Drop assertion had already
  confirmed retirement of the old object. This was a stale test observation, not evidence that
  the old object survived close.
- The native test was corrected to remove the old observation and wait at most eight actual
  native paint attempts for a new painter publication. It still requires a different object
  serial and an empty draft. Failure diagnostics retain frame, factory, Drop, sibling and layout
  observations; the production retirement contract was not weakened.
- `19de03a4-4005-449f-9652-c1038e6e07a1`:
  `native_composed_panels_keep_independent_drafts_through_gpu_reconstruction` passed in 2.746 s.
  With `WGPU_BACKEND=dx12` and an adapter required, it created a hidden Windows native Editor
  through the production composition/Host constructor, operated the real panel buttons using
  owned Context input, and submitted/presented through WGPU. Two independent drafts held `x`
  and `xx`; real GPU owner reconstruction preserved both objects, their drafts and retained
  counters while replacing the GPU. Closing the first dropped only its state; reopening created
  a fresh empty object. The test checked three factory calls, three eventual drops, unchanged
  document facts and normal graphics/session cleanup.

No system pointer movement, global keyboard injection, monitor scanout or other-platform
qualification is claimed. This focused native result does not replace the complete native
qualification procedure or the final workspace gate.

# Remaining Evidence

The separately compiled `tests/fixtures/module-consumer/editor-panel-consumer` source now uses
the paired composition and includes an ordinary `composition.run(options)` entry point. Its
external workspace build/tests and packaging verification were still pending at this capture;
source migration alone is not a passing external-consumer result. Final integrated review,
workspace checks and committed-source evidence remain with the root integration owner.

See [the panel guide](../../../../guides/editor-panels.md),
[ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
and [native qualification](../../../../verification/native-editor-qualification.md) for the
owning interface and procedure.
