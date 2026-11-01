---
type: "Work Progress"
title: "Exact captured predecessors and retained Task inputs pass final Root and Application handoffs"
description: "The three independent handoff findings are corrected and reviewed; final Root392 and Application35 pass before fresh-process and native qualification."
timestamp: 2026-11-01T19:03:32Z
record_id: "95571bffbb114d509693a5c6cab03243"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-final-integration-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "65b35fa4127c475d80aba798bdb3c2f0"
---

# Summary

The complete foundation Goal remains active. The authored-upgrade unit is still uncommitted on
`refactor/foundation` at `ed21b230`; no push/PR is authorized. The three retained-operation findings
from the predecessor checkpoint are corrected, independently reviewed and covered by final
Root/Application cohorts. Actual process, affected consumer/feature/lint/native qualification and
the precise complete-unit commit remain.

# Details

The final persistence binder borrows `PreparedAuthoredSourceUpgrade::predecessor_source` from the
operation's already validated complete open set. It does not use the lane's historical admission
tuple or replace the original evidence with a new Host sample. The accepted Host comparison and
new leaf/parent/digest validation remain strict. The complete saved Scene+Prefab control covers
both completed Save and Save drained by Begin; its original TargetChanged failure is now resolved.

Private `OwnedJob<I,O>` in authoring_work.rs retains the original input outside disposable Task
callbacks and is shared by actual preparation and publication consumers. It adds no executor or
public compatibility API. Preparation callbacks hold only admitted proposal data. Shared storage
is charged before allocation, remains charged while an actual worker/result exists, and settles
only after physical return and slot/callback disposal. QueueFull and PoolClosed return the same
baseline/basis/inputs and preserve cumulative work. Real IO saturation verifies two refusals and
public retry to Review. Explicit Task cancellation also retains its actual Review until return.

Application waiting retirement rechecks the Session's cancellation authority before dirty
arbitration. Its new App control seeds only an already-accepted retirement intent while using a
real captured Session; actual pre-publisher refusal/physical return and the post-Pending source-lock
fence have separate Root/App controls. Do not combine these scopes into one full end-to-end claim.

Initial focused run `017619d3-5a7d-484d-80ea-ac824f07fc9e` compiled and passed30/31. The only failure
was the new PoolClosed fixture calling shutdown on Runtime-owned Tasks, correctly refused with
CloseOwnerTransferred. That private control now uses its own closed inline pool and actual
PreparationWork, then restores the same returned baseline; the preceding QueueFull remains a
public Session test. Production close authority was not weakened.

Final Root run `de78b1d3-1a73-428a-93ca-df00b29d387b` passes392/392,1015 excluded, across four
binaries. It includes the fixed handoffs, authored recovery/publication, Schema adoption, actual
source demand and all selected product_data_components/workspace_play_runtime/editor_persistence.
Final Editor run `f9ddcc94-31d9-4685-93f5-9a62e245d13b` passes35/35,285 excluded, including ordinary
Application persistence, complete confirmation, actual registered Play retirement/replacement,
source-lock retry, waiting Close/Exit recheck and quiet fenced Browser refresh.

Independent static reviewer Poincare `01a19877-579a-72f3-b8b1-945b23bf872c` found no remaining
assigned P1/P2 in the three corrections, including physical disposal/accounting. That lane and the
two fix workers are closed. Recovery static review remains separately scoped as recorded in the
predecessor. None of those static reviews replaces actual test evidence.

Only Bacon `01a1987a-1564-7552-b7ba-f8a226700861` remains active, read-only. It studies the separate
CP2 nested extraction/remap successor against pinned Bevy/Godot and identifies which finite
publication pieces are neutral versus CP1 Schema-specific. No writes, Cargo or CP1 scope expansion
are assigned. Receive its brief later; do not make CP2 a prerequisite for this unit's delivery.

The independent recovery CLI normal dependency tree contains neither nara_tooling nor nara-editor.
Root/independent formatting must wait for no Cargo and use the final frozen source. Memory validation
passed with existing legacy warnings. Root target is reused, jobs1, about612GiB free; no artifact
deletion/compression helpers run. An unrelated Baozi Cargo may exist and must not be stopped.

# Next Action

Poll current serial Cargo session6332, building tests/fixtures/authored-upgrade-recovery/Cargo.toml
with --locked and root target. Log: TEMP/nara-authored-upgrade-recovery-cli-final-build.log.
Set NARA_AUTHORED_UPGRADE_RECOVERY_EXE to root target/debug/nara-authored-upgrade-recovery.exe,
then run the independent behavior-languages authored_upgrade_command normal tests and six ignored
subprocess controls. Select --run-ignored all while excluding only process_recovery::child_entry;
the actual children execute that entry explicitly. All tests must run, not skip for missing binary.

Continue affected Root integration/independent module/Reference consumers, final Root+Editor
all-feature correctness Clippy and minimal compilation, and selected hidden Windows/DX12 native
trajectories under docs/verification/native-editor-qualification.md. Update final evidence/ledger
and selected plan status, review/stage exact intended source including new files, commit locally
and ff main. Preserve vendor and every concurrent change; no remote publication. CP2, joint
motion/execution, durable materials and editable Nara-drawn Editor remain outside CP1 completion.

# Citations

- [Complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Predecessor findings](2026-11-01T184711Z-final-source-persistence-qualification-exposes-retained-operation-handoff-gaps-65b35fa4127c475d80aba798bdb3c2f0.md)
- TEMP logs: nara-authored-upgrade-final-handoffs-tests.log,
  nara-authored-upgrade-final-root-qualification.log,
  nara-authored-upgrade-final-application-qualification.log,
  nara-authored-upgrade-recovery-cli-tree.log.
