---
type: "Work Progress"
title: "U12 Windows closure verified while U7 source identities and U9 material implementation continue"
description: "Work Progress for U12 Windows closure verified while U7 source identities and U9 material implementation continue."
timestamp: 2026-10-15T17:37:50Z
record_id: "0603cb7fa62e4498b1b0b930090cdd9a"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
status: "active"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e8ac2ba"
---

# Summary

U6 owned paths are committed at `e8ac2ba`. U12 now has integrated Windows native and static
evidence. Initial U7 opaque-locator and U9 shared-order slices pass; their full consumer closure,
U8/U10/U11/U13, and final review remain active work. The goal is not complete.

# Details

- U12 native canary `93873` exited 0: saved generation 1, injected build failure without false
  Running, Run Last Good generation 1, saved generation 2, physical cleanup complete.
- Workspace check `45255` and affected Clippy `16545` passed before the new U7/U9 edits. Evidence
  `4a4c7b0afbcb4897b6aef21ad01ed38c` records exact scope; they are not final checks of later code.
- U7 scene library run `b909ad5d-da21-4fe1-b245-390fc96c06a6` passes 116 tests. The private complete
  mapping replaces concatenated projected IDs; all source IDs remain opaque. The same U7 agent is
  migrating Reference role/instance/parent selectors, with no Cargo authority.
- Root U7 stable-reference catalogue is being implemented in `src/project_content/catalogue.rs`,
  `project_content.rs`, canonical scene publication, and startup materialization. It uses native
  capability enumeration and existing budgets, rereads referenced metadata, retains only closure
  records, and binds identity residency to its lease. Loader rename run
  `2664a2e1-6b54-47b8-a312-7c8dc357de02` passes both image and prefab rename tests. An initial test
  fixture used a UUID different from its metadata; the fixture now takes its actual metadata ID.
  Actual Runtime and both preview paths still need verification/migration.
- U9 order run `9611204c-6006-406b-9f96-0e4c59e1a624` passes 47 tests. The same U9 agent is now
  implementing the concrete fixed fragment input, palette example, second prepared image, private
  GPU execution and readback test. The main thread owns the root prelude hunk. No Cargo is running
  while the material dependency files are being edited.
- U11's first test-only change is frozen in `tests/workspace_play_runtime.rs`:
  `document_transaction_and_undo_use_live_revisions_without_another_frame`. It awaits a real red
  run before production changes. The existing action/history implementation remains unchanged.
- The original user index and pre-existing SPL/Host/filesystem/Cargo work are preserved. Owned
  checkpoint commits do not claim an independently buildable clean HEAD.

# Next Action

Collect the U9 material freeze and U7 Reference freeze. Main-thread-only Cargo then runs the U11
red, full affected content/Runtime tests, Reference selectors, and material/native readback tests.
Continue U7 explicit conversion/two-source/copy/save-binding fixture and generic authoring asset
context. U8 uses a real same-entity native-effect domain owner without weakening lifecycle-free
guards; U10 follows material closure. Keep all remaining units and final review in the goal.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T173408Z-supervised-play-static-checks-and-initial-opaque-identity-and-ordered-stream-regressions-pass-4a4c7b0afbcb4897b6aef21ad01ed38c.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T164359Z-u10-reuses-one-target-transaction-with-bounded-views-and-explicit-image-extent-fb2eb447263742228e16b2243167e835.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T163150Z-u11-frame-admission-hides-an-existing-document-transaction-and-preservation-safe-history-a04b19e987b94838aa9c1ff6e0648f08.md
