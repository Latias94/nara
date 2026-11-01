---
type: "Work Progress"
title: "Process recovery and v2 product execution evidence with final integration continuing"
description: "Real independent recovery, v2 Check/Runtime/Editor Play and ordinary history workflows pass; scripts membership and compiled source admission need final integration checks."
timestamp: 2026-11-01T17:09:36Z
record_id: "fb79542928e445f9a19d66671ccb4f43"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-recovery-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
git_branch: "refactor/foundation"
git_commit: "ed21b23051342587574be75da017da888270b597"
---

# Summary

The full foundation Goal remains active. The authored-upgrade unit is still uncommitted on
`refactor/foundation`, with main/branch base `ed21b230`. This continuation implements and tests
fresh recovery, safe retained live retry and real fixed v2 product consumers. It does not close
CP1/S2 or the broader plan before the remaining integration, review and native checks.

# Details

The public headless owner, neutral limits/history policy and shared bounded wire decoder are in
the preceding [checkpoint](2026-11-01T162742Z-fresh-authored-upgrade-recovery-owner-and-mixed-generation-qualification-48a5b7438aa44d56880dd7100a4afe70.md).
An independent review's unmetered-cleanup P2 is corrected through private
`source_upgrade_policy::with_cleanup_admission`: actual pending cleanup admits positive 1024 work
and bounded result/metadata scratch before calling; failure keeps work/custody and returns scratch.
No cleanup incurs no cleanup charge; no new handles, path parse or content IO are charged.

Session/Application `retry_authored_schema_upgrade` now continues the exact returned live owner
on existing publication Tasks. `can_retry` comes from that owner. Unknown returns, uncertain
publishers and RequiresReconcile witnesses cannot blind-retry or release the fence. Close/Exit
waiting retains that same operation's retry route while its original facilities stay open. Native
cleanup, including pre-Pending cancellation cleanup, runs through the finite worker rather than
on the Session owner thread. No preparation/basis/history/allowance is reconstructed.

Actual Root cohort `73fadc7b-ba55-4cd4-b8ef-74614b02d423`: 258/260 passed, 1089 excluded.
It includes all12 fresh-owner/cleanup tests, original inventory/publication/Schema-owner and
external-Play controls, plus two successful new live retry tests. The other two new tests took
the persistence host guard before confirmation and correctly received Stale. Parent moved that
contention after successful confirmation so the tests reach final binding. Their rerun remains
pending; no production guard was weakened.

The independent CLI companion builds with only `project-workflow-cli`; its normal 376-row graph
contains no `nara_tooling`. Its own lockfile is generated and tracked with the new workspace.
`process_recovery.rs` launches a real public Session producer, stops driving after the Pending
Task starts, observes actual Pending and exits without running any source Task. Separate CLI
processes resume deterministic stored outputs. Already-new leaves stay continuously held by the
test consumer and are not rewritten. The five original all-old/mixed/all-new/conflict/Ready-before-
ack scenarios passed in run `4277ef16-eecd-40b5-b9bb-6f3f1dc96bac`, together with the surviving-
unavailable v2 refusal control (6/8 passed at that point).

Initial process probes used `fs::read` while the actual Pending publisher still held its exclusive
range lock and panicked with Windows Error33. The test observation now retries only Windows32/33
within its finite deadline; no publisher behavior or assertion was relaxed. The positive product
test then incorrectly demanded Save after Undo had returned to its saved checkpoint. Its real
follow-up is now edit113000, Undo109000, Redo113000, Save, edit117000, Reopen113000, then ordinary
edit/Save109000 for subsequent literal product oracles.

Focused final product2 passed: `cc01005e-4333-4181-b8f3-e74bb1481606`, 21 excluded. This covers
both live publication and a seven-source actual fresh-process recovery, fixed v2 Check and real
Headless fixed-tick consumption, exact old-v1 selected-owner-divergent refusal, actual trusted
compiled Editor Play/Stop, fresh history and ordinary v2 edit/Undo/Redo/Save/Reopen. Successful
fixtures include complete native builtin owners and remove only surviving foreign unknown rows
during initial setup, before any operation. Dormant opaque declarations remain intact. A separate
unavailable-survivor control refuses v2 execution. No published output is repaired by rewriting
bundle or fixture documents. Static product definitions come from embedded v1/project source,
not from reading the tested v2 bundle as executable authority.

Real Play exposed a structural gap: the trusted compiled Editor still opened raw sources through
a Native-only persistence validator despite already admitting a mixed Native/Data Runtime plan.
Parent added `compiled_authoring_adapters` using actual selected native contribution receipts
and data-owner definitions, one real `AuthoringSchemaBasis`, and
`ScenePersistenceHost::open_authoring` through the existing lossless source classifier and
preservation lane. The old Native-only Scene opener is test-only. Initial persistence digest must
match the admitted snapshot's actual Scene member. No native codec/registry or old receipt is
fabricated. ADR0047 is amended. Minimal Root project-workflow+tooling lib check passed in 9.74s;
the successful product2 exercises this correction without rendering features. Broader compiled
Editor/persistence/preview/native qualification is still required.

Independent reviewer Feynman found one further real P2: successor inventory only restored explicit
recovery member claims in Recovered mode, omitting originally open `scripts/nested/open.scene.json`
outside normal enumeration roots when the fresh owner's final successor capture had no open set.
The fix is in the inventory owner: both PendingUpgrade and Recovered modes add authenticated
member claims, preserving complete enumeration and exact key/metadata/successor checks. Feynman
(`01a197fc-fa3c-7413-8a87-c7faedbc5a19`) is the only active writer, limited to inventory.rs and
fresh recovery tests; source-ready/final tests are not received yet. Do not add Scripts as a Scene
enumeration root or construct old open-source facts to hide this defect.

Parent added a real Application Close/Exit retry test in
`editor/src/app/persistence_tests/authored_upgrade/confirmation.rs`. It holds an actual source lock
after Pending becomes visible, waits for refusal, requests Close/Exit, releases the lock and retries
the same operation to actual Ready/retirement. It is not yet compiled/run. Its manifest probe also
handles only finite Windows range-lock contention.

# Next Action

Poll already-terminal focused product session99160 if its tool handle is still present (log has
2/2 success); no Nara Cargo is otherwise known running. Check actual process commands before a
new build: another checkout, `F:/SourceCodes/Rust/baozi`, was compiling with jobs1; never kill it.
All other worker/reviewer lanes are closed. Wait for Feynman's scope freeze before root fmt.

Run script-member/public-owner regression, corrected live retry tests and broaden affected
Root compiled Editor/persistence coverage. Run Editor App retry and current-unit controls,
independent normal command tests plus process scenarios on final source, minimal/all-feature/
correctness Clippy and selected native qualification as justified by the source-admission change.
Review the compiled Native/Data correction independently. Update ledger/guides and capture actual
scope before precise commit and local ff main. Do not push, touch vendor or narrow the Goal.

# Citations

- [Complete unit plan](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- TEMP logs: `nara-recovery-live-owner-final-tests.log`,
  `nara-upgrade-recovery-process-product-tests.log`, `nara-upgrade-recovery-product-followup-tests.log`,
  `nara-recovery-companion-final-build.log`, `nara-compiled-data-authoring-check.log`.
