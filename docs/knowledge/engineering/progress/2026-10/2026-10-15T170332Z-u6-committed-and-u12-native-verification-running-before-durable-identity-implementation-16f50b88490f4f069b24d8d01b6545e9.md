---
type: "Work Progress"
title: "U6 committed and U12 native verification running before durable identity implementation"
description: "Work Progress for U6 committed and U12 native verification running before durable identity implementation."
timestamp: 2026-10-15T17:03:32Z
record_id: "16f50b88490f4f069b24d8d01b6545e9"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e8ac2ba"
---

# Summary

U6 owned paths are committed at `e8ac2ba` after real project/Reference consumer verification.
The original staged binary patch remains byte-identical to the initial snapshot. U12 native
verification is running; U7 is authorized to add only its first regression test.

# Details

- U6: 198 root/CLI tests pass in `1fb5cc88-7d9e-405e-bf31-69519ccc36d4`, including actual
  custom-schema native type rename/migration, first-build Check, Repair and exact two-product
  Export. The subsequent profile-hint/Closing regressions were observed red before fixing;
  100 focused project-tooling/Editor tests pass in `9fc43229-7be5-408b-9f5d-b383114dde50`.
- Reference all-features/all-targets check passes. The 42-test consumer checkpoint had one stale
  source-lineage assertion; all seven authoring tests pass after correcting it without changing
  production semantics. Default generated headless tooling's active dependency tree contains no
  WGPU, Winit, or egui.
- U12: real generated-starter Play, Stop and Editor exit pass. Six application helpers still
  assumed physical closure within immediate fixed tick counts; bounded incremental polling fixes
  them, and all 13 application tests pass. Focused response reviewer confirms the Closing quiet
  frame bug is closed. Three simplification passes completed; the applied changes preserve the
  generation, cleanup and stable-deadline checks.
- Native Reference supervised Play session `93873` is running, with a real copied project build.
  Collect it before any other Cargo command. Native Linux/macOS evidence is still absent.
- `/root/u7_identity_migration` may add the five-entity opaque-ID collision regression only in
  `crates/nara_scene/src/tests.rs`; no production mapping before root observes red. U8, U10 and
  U11 completed read-only preparation; U9 remains ready for its first ordering regression.
- The committed slices deliberately exclude pre-existing Cargo/Host/SPL index/worktree changes.
  Integrated worktree evidence is not a standalone committed-HEAD build claim. No destructive
  Git operation has been used. Owned-path lists/messages remain in the external prework backup.

# Next Action

Collect native canary `93873`; fix real failures. Run the U7 first red test, then authorize only
that unit's production mapping/consumer migration. Complete affected static checks and the U12
owned-path checkpoint. Continue U7-U11/U13, keeping U8/U11 shared authoring hunks coordinated and
U9/U10 renderer hunks sequential. Full final ce-code-review still remains; the focused reviewer
and simplification reports are not its receipt. The goal remains active until U1-U13 are done.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T170038Z-u6-reference-composition-consumers-and-all-target-build-pass-78587446992944bfbb5116ba39974108.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T165700Z-generated-supervised-play-closes-physically-and-incremental-application-drivers-pass-20e375e5915c4d0b9eb7681308249f22.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T163150Z-u11-frame-admission-hides-an-existing-document-transaction-and-preservation-safe-history-a04b19e987b94838aa9c1ff6e0648f08.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T164359Z-u10-reuses-one-target-transaction-with-bounded-views-and-explicit-image-extent-fb2eb447263742228e16b2243167e835.md
