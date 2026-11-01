---
type: "Work Progress"
title: "Reviewed inventory ownership and failed preparation settlement"
description: "Source preparation consumes and returns the exact reviewed inventory; failed SourceWork storage settles without refunding work. Scene22 and Root74 pass while complete Session/Application orchestration remains open."
timestamp: 2026-11-01T10:15:10Z
record_id: "c55a1b016d364afcaf180e23d75aa4bd"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-20261101"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23"
supersedes: "c141c4a18d7647fa8cbbe862abfd84ce"
---

# Summary

The full foundation Goal remains active. `main` and `refactor/foundation` remain at `ed21b23`;
the authored-upgrade replacement is still uncommitted WIP. No remote publication occurred.
This implements the selected same-inventory preparation seam and failure settlement, not the
complete CP1 Session/Application operation, fresh-process recovery or v2 Check/Play.

# Details

`PreparedAuthoredSourceUpgrade::prepare` now consumes the exact `SourceUpgradeInventory` captured
before the project command. It borrows that owner during source/history preparation and moves it
into the result on success. `SourceUpgradePreparationFailure` returns the same original inventory
on refusal. Only the attempt's new Host storage/handle reservations end; borrowed Schema/recovery
inputs and returned original custody keep their previous charge. Work remains cumulative.

The old internal inventory recapture and independent `SceneFileLimits` parameter are removed.
Limits come from the captured owner. `validate_preparation_inputs` compares captured authority and
the complete bound open-source facts without IO, re-enumeration, allocation or another custody
reservation. Reordering is accepted; omission, repeated-source substitution, unbound inputs,
changed accepted observations, reforged native keys and wrong authority refuse. Comparisons are
charged before execution. The bounded borrowed open-set comparison deliberately creates no
second map or new general tracking service.

Six existing callers now explicitly capture once using their cumulative budget. Repeated refusal
tests keep the same file Arc and disk-byte allocation, unchanged original retained bytes/handles
and recovery ownership, while retaining performed work. A changed-disk-after-capture test proves
preparation uses the reviewed immutable corpus and leaves the external edit intact; actual
confirmation then rejects through fresh enumeration. Candidate data preparation grants no write.

Independent review found one P2: returning inventory made corrected retry possible, but SourceWork
still charged failed candidates and retained incumbents after their owning locals had dropped.
`AuthoredSchemaUpgradeWork::with_preparation_attempt` now records entry retained/scratch and
restores those on `Err`, keeping pre-existing reservations, nominal identity, cumulative work and
historical peak. Success keeps its outputs charged. Its trusted engine-owner callback must return
no charged preparation/token in its error or side effects. The Root closure satisfies that rule:
its error type contains no preparation/token, and all new source/Workspace locals drop before
settlement. The original inventory stays outside the closure. Parent review covered the Scene
implementation; final independent read-only review covered the complete Root wiring and tests.

The actual Scene/Prefab retry tests retain a prior token across refusal and exercise nested
failure/success. The Root regression measures one healthy preparation's logical peak, submits a
stale working proposal, then corrects it using the same returned inventory and SourceWork. Exact
single-success capacity admits the retry, while one byte less requires the specific capacity
diagnostic. Work exceeds the healthy single attempt; no production limit was increased.

Actual serial evidence, Rust1.95.0, jobs1, shared root target:

- Before SourceWork correction: source/inventory cohort73/73,
  `eaf13308-76b6-49dc-9d15-3b30fdd29f87`. This did not cover corrected retry capacity.
- Scene all-feature focused preparation/upgrade cohort22/22, 506 excluded,
  `6758f18e-b3ff-4943-9d45-dfe01b058cde`; build46.39s, run3.796s.
- Final Root source/inventory cohort74/74, 1202 excluded,
  `02d572db-3e6b-4c34-b775-f8a1fdad379d`; build2m07s, run13.869s. This includes real source,
  publication, adoption and inventory consumers plus the tight corrected-retry regression.
- Final Root all-feature library check22.65s, Root minimal library check0.51s and Scene minimal
  library check4.93s passed. Formatting check passed. Existing unused warnings remain.

All agents are closed and no Cargo command remains live at this capture. The preceding428-test
source-observation qualification is preserved at its prior scope; it was not rerun after this
private preparation change. No new native Editor, complete Session/Application, independent
Runtime-v2 or actual separate-process recovery result is claimed.

# Next Action

Connect these actual owners to the complete Session/Application workflow. Accept and persist the
write fence, drain accepted Save, capture complete inputs once, expose bounded borrowed inputs
to the project command, and admit the returned data proposal. Reuse the same inventory and the
operation's cumulative Host/SourceWork allowances. Keep work-pending distinct from writes-fenced.

Then join exact operation/project-generation physical tool/Play retirement, recovery/Pending/
source/bundle publication, complete successor verification, and all Workspace/persistence/lane/
project guards across final Ready. Prepare replacement Play custody before Ready and install only
already-bound values afterward. The source preparation still only borrows its Workspace result;
provide its consuming final-install seam as that concrete owner is connected, preserving original
custody and truthful refusal outcomes. Do not add unused public publishers.

Complete actual before/between/after-write process interruption, conflict/idempotence and public
EnemyStats v2 edit/Undo/Save/Reopen/Check/Play with strict v1 Runtime refusal. Full-unit public/native
qualification and a precise commit/ff-main remain pending. CP2, motion/executor, materials and
editable Nara-drawn Inspector stay in the complete active foundation Goal.

# Citations

- [Current unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Previous checkpoint](2026-11-01T093759Z-source-observation-handoff-and-startup-preview-ownership-integration-checkpoint-c141c4a18d7647fa8cbbe862abfd84ce.md)
- [Consuming preparation](../../../../../src/project_host/authoring/project/source_upgrade.rs)
- [Root retry and custody tests](../../../../../src/project_host/authoring/project/source_upgrade/tests.rs)
- [Inventory input checks](../../../../../src/project_host/authoring/source_upgrade_inventory.rs)
- [SourceWork owner](../../../../../crates/nara_scene/src/authoring/schema_upgrade.rs)
- [Scene/Prefab attempt tests](../../../../../crates/nara_scene/src/authoring/schema_upgrade/tests/preparation_attempt.rs)
