---
type: "Session Handoff"
title: "Authoring retirement merged and initial construction custody design in progress"
description: "Pins the verified retained authoring unit to main and preserves the next raw App versus planned candidate design question without claiming initial Open custody or full R4b complete."
timestamp: 2026-10-25T18:15:35Z
record_id: "e1574e61206a4dc885356b44896703b0"
producer_id: "codex-root"
run_id: "foundation-authoring-open-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/authoring-open-custody"
git_commit: "a1ff454"
---

# Summary

Full foundation Goal remains active. The retained predecessor/post-start rejected candidate unit
is committed as `a1ff4549b25f27336e5a327cbb8f7c142f612c97` and fast-forwarded to local `main`.
No push/PR. Current branch `refactor/authoring-open-custody` starts from that commit. Initial
construction/Open failure custody is being implemented; current source is uncommitted and is not
qualified or merged merely because the previous unit passed.

# Verified State

- Previous committed unit: final root420, ordinary Editor198, Tasks/Tooling147, native11, workspace
  check, affected Clippy, formatting, links and memory validation pass. Full independent review
  `20261026-014014-27c03c96` completed with one P2 final-custody-refusal proof, now addressed and
  independently rechecked. Its evidence shard is linked below.
- Next unit lower-App proof-first attempt intentionally fails with missing `begin_retirement` and
  `prepare` methods: `target/r4b-configured-app-retirement-red.log` (E0599, not a runtime red test).
  Implementation then passes focused8 at nextest `9878a5c1-003b-4d68-a2d2-3c57e23e0154`.
- Current all-feature App library/integration suite:152 passed,1 skipped, raw log
  `target/r4b-initial-authoring-custody-app-regression.log`. This includes configured App abort,
  failed preparation/Startup without close polling before return, same World/generation, finish
  and Startup counts, plus Host/plugin ledger collision and invalid sealed schedule custody.
- Two ordinary Shell tests pass at `c567e6be-4ff3-473e-8f0f-c99ccef33137`: real prepared-source
  drift rejects initial and replacement Open; cleanup survives Retry refusal/manager navigation/
  Exit and retains the exact incumbent. A later Retry completes Save/Close.
- New actual public held-Open test initially fails at
  `75504364-427d-4631-acd0-d7062207a260`, because it assumes the first Stop drive already polls file
  custody. The existing control contract yields once; the test now explicitly advances that first
  Stop before expecting participant effects. The subsequent Windows abort is not independently
  diagnosed as stack/memory corruption. Corrected root initial-Open/content-refresh cohort7 passes
  at `62aaf9e7-d36d-4099-91c9-3fdf04e36dbe`.
- Root lib and Editor all-target check passed earlier current-source revisions; later Retry UI,
  lower-App proof and source-test changes still require final checks. All Cargo is serial, jobs1,
  Rust1.95.0, existing F: checkout `target`. No new C:/D: build tree.
- Subsequent full root421 passes at `e36052b8-3b5c-4552-be1a-93ae1b48ad5e`; ordinary Editor200
  passes at `61590fe4-633a-4109-a983-fb87d0af815a`. Those precede the consumer cleanup-error fix
  below and the new Configuration adapter proof.
- After the cleanup-result fix, focused Shell/result3 passes at
  `c42bfe04-09fc-425f-affc-b62850b13687`; the new result test obtains actual terminal evidence
  from a failed Plugin shutdown and verifies primary/native/project cleanup preservation.
- Actual Windows/DX12 native10 passes at `57b5d20f-1641-48dc-9a88-a1d329dc06b8`: critical3,
  Canvas, Manager widgets recovery/replacement/dirty close, and all five graphics retirement cases.
  This does not prove held authoring cleanup in native quiet cycles or injected initial-Open
  terminal cleanup through the full event loop.

# Open Threads

- New narrow `App::begin_retirement` exposes existing `RuntimePreparationRetirement` with read-only
  World observations; no finish/Startup or cleanup polling during ownership transfer.
  `RuntimeInstance::prepare` splits/reuses the exact ordinary-start admission prefix. Ordinary
  start keeps its finite failure-close convenience. Failed Startup retains `RuntimeCandidateFailure`.
- Authoring now configures an externally owned App under one error boundary; configuration,
  admission and Startup errors retain concrete enum variants. `EditorAuthoringRetirements` can
  own these as well as complete replaced Runtimes, using existing close protocol and report sums.
- Ordinary/trusted Session Open wrap post-start image/UI/source/Workspace publication so failures
  retain the complete unpublished Runtime. Error/failure expose retirement state/evidence/poll;
  diagnostic-only `EditorProjectOpenError::into_diagnostics` is removed.
- Application owns a bounded failed-Open slot independently of Recovery or manager state. It gates
  Retry/new construction, schedules independent maintenance, waits on Exit and keeps first terminal
  close-failure evidence. Recovery projects borrowed diagnostics and disables Retry while pending.
  Trusted synchronous application error also forwards retirement observation/poll.
- The new public Open held-work proof uses only a private root `cfg(test)` hook. Worker
  `01a17455-135e-74c1-b3f0-da3cdf37a940` wrote only `persistence/tests/initial_open.rs`, was inspected,
  then closed. It proves body and late-result destruction separately, exact original failure,
  repeated close evidence, no quarantine increase and restored Open/edit/Save/Reopen/Close.
- Design sidecar `01a17443-c224-7093-a42f-c24fa238e713` selected raw App bridge over another
  PluginPlan input/output mechanism; completed and closed. Lower-App/authoring sidecar
  `01a1745f-e312-72a1-a1dd-f259c5a45e8f` found no confirmed code defect and requested real
  Configuration-variant TaskShutdownReport coverage. It completed and closed. Worker
  `01a17467-37a5-7371-a1c7-520438829f44` owns only the new
  `authoring_runtime/construction_tests.rs` and is still writing that focused test; Main has added
  its cfg(test) module declaration. Collect/inspect it before root tests or full formatting.
- Application/native sidecar `01a1745f-e51c-7fd2-ab88-7f7eb03a3548` found a real P2: sticky
  failed-Open cleanup evidence had no consumer and the runner could return Ok. Main connected it
  to native final result and visible UI status, generalizing `EditorRunError::NativeCleanup` to
  `Cleanup` without an alias, preserving prior/native errors. The reviewer independently closed
  that scoped finding, completed and closed. Manager Open/Recovery Retry now also expose actual
  admission availability to disabled UI controls. This is not a full CE completion receipt.
- Current initial-custody source still needs complete independent review/simplification, ordinary
  root/Editor regressions, affected native trajectories and final workspace/lint/document checks.
  No new native qualification or full R4b completion is claimed. Product Tasks allocation remains
  eager. Trusted Play complete Session close and R4c/remaining waves remain open.

# Next Action

1. Recheck running Cargo handles and preserve all changes/vendor. Collect active sidecar review.
2. Finish initial Open custody qualification, including actual configuration/Startup/report/failure
   evidence and Application Exit/Retry/gating. Reconcile any real issues before final review.
3. Run appropriate serial broad App/root/Tasks/Editor regressions and selected native qualification;
   then all-target/all-feature workspace check, affected Clippy, fmt, links and memory validation.
4. Commit a coherent qualified stage on its branch and ff local main. No push/PR. Continue full
   master plan: private-lazy product deployment, trusted Play final Session-close coverage and
   R4c/remaining dependency-ordered waves, without narrowing the active Goal.

# Citations

- [Previous unit qualification](../../verification/2026-10/2026-10-25T172648Z-retained-authoring-replacement-and-physical-retirement-qualification-f93efd28e9324ff4accf45fc3d38311c.md)
- [Activity and initial-failure design](../../../../architecture/task-activity-scope-design.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
