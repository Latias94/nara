---
type: "Work Progress"
title: "Owned complete Workspace upgrade inputs and real worker preparation"
description: "Scene27, Tooling20 and final Root75 qualify frozen source inputs, shared direct-worker kernels, original Workspace binding and tight retry; Session/Application publication and process recovery remain open."
timestamp: 2026-11-01T10:35:45Z
record_id: "d0cbdbaf09964670b866a9ab64359372"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-20261101"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23"
supersedes: "c55a1b016d364afcaf180e23d75aa4bd"
---

# Summary

The complete foundation Goal remains active. `main` and `refactor/foundation` remain at `ed21b23`;
the complete authored-upgrade WIP is uncommitted. No commit/merge/push/PR occurred. The new owned
source preparation is a concrete integration prerequisite, not a complete Session/Application
upgrade or fresh-process recovery claim.

# Details

The actual Scene/Prefab source owners now supply `SceneAuthoredSchemaUpgradeInput` and
`PrefabAuthoredSchemaUpgradeInput` through `nara_scene::__private`. Capture consumes the existing
Work-bound retained incumbent token, admits inline snapshot storage before cloning either Arc,
and shares the original source/classification. It records actual revision, content state and
history limits; no replacement Session/history or synthetic revision is created. The original
live Session keeps both old history branches. The complete operation's allowance continues to
account their coexistence. Borrowed input preparation uses the same `UpgradeSource` kernel as
direct preparation, retains no second incumbent and keeps the token for failed-proposal retry.
Final live binding still checks exact original source/classification identity and history facts.

Tooling's `EditorWorkspaceAuthoredSchemaUpgradeInput` captures the complete bound nominal set,
original Workspace persistence identity, previous basis and navigation observation. All source
incumbents are retained before the first input is captured or any unopened classification is
prepared. Capture itself uses `with_preparation_attempt` so partial refusal settles new storage
after locals drop, preserving prior reservations/work/peak. It exposes bounded borrowed source,
binding and revision accessors. Its borrowed preparation validates complete proposals and uses
the Scene inputs. Direct Workspace preparation delegates through the same core.

The obsolete public raw-retention type, `retain_authored_schema_upgrade_sources` and
`prepare_authored_schema_upgrade_with_retained_sources` are removed from APIs and exports.
Production retains private helpers where needed; old tests exercise those actual helpers or the
frozen input. No compatibility aliases or duplicate conversion interpreter remain. An `rg` scan
of crates/src/tests and current architecture/guides/plans found no old API references.

Root preparation now consumes the frozen Workspace input beside the original physical inventory.
Both stay outside the candidate-attempt closure, are retained in the candidate on success, and
are returned unchanged on failure. The worker performs no live Workspace calls. Final validation
still checks the real Workspace and current complete physical set. The combined source/history
test moves the real inputs, metadata and both budgets to `std::thread`, mutates the live Workspace
with Undo before releasing the worker, and proves candidate preparation succeeds while binding
refuses. Its unchanged control validates, and both leave disk source files intact.

The tight Root retry regression now uses an invalid successor that changes unrelated `armor`.
This reaches actual source preparation after frozen incumbent capture, then retries corrected
outputs using the same returned input/inventory/Work at one-success capacity. Its one-byte-short
control still requires the explicit capacity diagnostic. Early stale working-copy validation does
not necessarily consume zero source work: sorted closed sources can already have been admitted.
The repeated-refusal assertion preserves cumulative work while requiring unchanged original
input/storage/handle ownership; it does not refund performed classification work.

Independent review found no new assigned P1/P2 in the snapshot/direct-kernel/live-binding chain,
complete-set retention order, actual identities or failure settlement. All writers/reviewers have
returned and are closed. No native Editor qualification or complete Application behavior follows
from that static review.

Actual serial evidence, Rust1.95.0, jobs1, root target:

- Scene preparation/upgrade27/27, 506 excluded,
  `7e1c5e9d-5b4b-4f03-a935-7c2974cda884`; build53.42s, run1.798s. Five added tests exercise both
  Scene and Prefab, direct/owned equivalence, original Arc identity, thread/drop/stale bind,
  exact/one-short capture, foreign Work and same-input bounded retry.
- Tooling authored-upgrade20/20, 89 excluded,
  `207b44ae-298a-4755-931e-75362f8f5720`; build30.85s, run1.271s. A new must-use test warning was
  fixed by explicitly dropping the prepared value; that test's final rerun passes1/1,
  `896bd2bf-7cb7-41ac-8721-19b584e09d6a`, zero warnings from Tooling.
- First Root integration74/75, `0e67f520-c100-48ba-a3b7-fa3e27d9fe6e`. Only the overly strict
  source-work assertion failed; real worker and tight retry cases passed. After correcting that
  actual performed-work expectation, the final Root source/inventory cohort passes **75/75**,
  1202 excluded, `15e6484f-c02c-4042-b9eb-4e18d74c2ffc`; build59.53s, run13.209s.
- Final Root all-feature library check18.72s, Root minimal library1.38s and Scene minimal
  library1.80s pass; final `cargo fmt --all -- --check` passes. Existing unrelated unused warnings
  remain. Earlier broader428 and public19 results keep their earlier scope, not a claimed rerun.

No Cargo command remains live. F free space was about8.47GiB at capture. Continue reusing root
target, jobs1 and serial Cargo. Do not recursively delete computed artifacts; earlier deletion
approval was rejected. If storage pressure returns, inspect actual live commands and use the
previously selected nondestructive NTFS compression approach only while this checkout's Cargo is
stopped. Other projects may compile concurrently; do not kill or modify their processes/files.

# Next Action

Implement the real CP1 Session/Application operation rather than another audit:

1. Fence new mutations/Save/Reopen/CloseSave and competing operations, drain an accepted Save
   to its actual installed result, then capture one complete bounded Workspace input and physical
   all-profile inventory. Use existing Host observation handoff for working-source custody.
2. Expose borrowed actual inputs to the project command, admit the complete data proposal and
   prepare it on existing Tasks using these same inputs and cumulative allowances. Keep refusal
   repairable before Pending; work-pending and persistent writes-fenced remain separate facts.
3. Join exact operation/project-generation source-tool and Application Play physical retirement,
   recovery/Pending/source/bundle publication and complete successor verification. Prepare empty
   replacement Play custody and every fallible installation allocation before final Ready.
4. Hold all Workspace/persistence/lane/project guards across Ready, consume already-bound commits
   and move the prepared project/Play owner before admission resumes. The Root prepared object
   still needs its consuming final-install decomposition; add it with that actual consumer.
5. Qualify actual fresh-process interruption/conflict/idempotence and independent EnemyStats v2
   edit/Undo/Save/Reopen/Check/Play with strict v1 Runtime refusal. Only then complete unit review,
   relevant public/native evidence and precise commit/ff-main.

Do not treat this background-input prerequisite as finished CP1, R5 or the whole Goal. CP2 source
extraction, joint motion/executor, material declarations and editable Nara-drawn Inspector remain.

# Citations

- [Current unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Previous checkpoint](2026-11-01T101510Z-reviewed-inventory-ownership-and-failed-preparation-settlement-c55a1b016d364afcaf180e23d75aa4bd.md)
- [Scene input owner](../../../../../crates/nara_scene/src/authoring/schema_upgrade/input.rs)
- [Shared preparation kernel](../../../../../crates/nara_scene/src/authoring/schema_upgrade.rs)
- [Workspace input owner](../../../../../crates/nara_tooling/src/workspace/authored_schema_upgrade.rs)
- [Combined Root preparation](../../../../../src/project_host/authoring/project/source_upgrade.rs)
- [Actual worker/retry tests](../../../../../src/project_host/authoring/project/source_upgrade/tests.rs)
