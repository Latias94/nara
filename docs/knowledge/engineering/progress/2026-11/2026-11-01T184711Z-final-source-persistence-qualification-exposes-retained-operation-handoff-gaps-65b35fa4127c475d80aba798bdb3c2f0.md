---
type: "Work Progress"
title: "Final source persistence qualification exposes retained-operation handoff gaps"
description: "Source persistence cleanup passes Root388 and Application34; independent review reproduces a stale saved predecessor and selects two original-owner retry corrections."
timestamp: 2026-11-01T18:47:11Z
record_id: "65b35fa4127c475d80aba798bdb3c2f0"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-final-integration-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "6efcd54caabb41828d0d23606c44fc6b"
---

# Summary

The full foundation Goal remains active. The complete authored-upgrade unit is uncommitted on
`refactor/foundation` at `ed21b230`. Root private Native persistence/readmission fallbacks and
their obsolete native-callback tests are removed. Final review found three retained-operation
integration gaps; do not commit or claim complete qualification until their corrections pass.

# Details

Final Scene library run `e1e7fd9c-f4ae-4fa1-8f9d-92f58f941c22` passes546/546 and Tooling
`4ef71f5d-bfec-4e38-8bc1-8c9d290d656d` passes127/127. Public Scene35, documentation12 including
three compile-fail cases, correctness Clippy and Scene minimal compilation also pass in the
previous owner scope. These checks precede the Root-only persistence cleanup.

After cleanup and final formatting, Root run `90bb24f2-24d1-4aa0-ab3c-3f00d83ec57d` passes388/388,
1015 excluded, across library and product_data_components/workspace_play_runtime/editor_persistence.
Final ordinary Editor features initially exposed a test-only accidental project-tooling import.
The source-lock fixture now uses actual `crate::app::open_project_root`, without widening features.
Application/retirement run `f721ff10-f5a1-4793-b174-d53922c7e339` passes34/34,285 excluded.
Neither cohort covers the subsequent corrections below.

Carson `01a19863-7715-77f0-b80b-20d5d98b7242` found P1: Save advances the persistence Host's
accepted observation, while the lane's base source fact is historical. Capture correctly freezes
the projected accepted receipt; final upgrade binding incorrectly used the historical lane tuple.
New complete Scene+Prefab Save/draining-Save regression run
`7b5e61ef-8f01-467d-9178-4bcc5f5cee57` reproduces `Persistence(TargetChanged)` after Pending,
before the fix. Its failed assertion is followed by fail-closed fixture teardown refusal and a
Windows abort; this is not evidence of a separately diagnosed native stack overflow.

Parent adds a borrowed path-indexed `PreparedAuthoredSourceUpgrade::predecessor_source` over the
already validated complete open set and uses that exact receipt in the final binder. Host tuple,
current leaf/parent/digest checks remain strict; no fresh observation or lane mutation replaces the
operation's original evidence. Regression success is pending.

Peirce `01a19863-a0f2-7540-9df7-4ea01baa226d` found two P2s: Application Close/Exit fails to
recheck cancellation if actual pre-publisher refusal restores it after the request; preparation
Task admission rejection drops the closure-owned baseline instead of returning it. Both findings
are accepted. Peirce now owns only editor/src/app.rs and authored-upgrade Application tests for the
first fix. Carson now owns only the private finite owner container, preparation/publication work,
Root authored_upgrade.rs, proposal.rs and proposal tests for the second. They may not run Cargo,
fmt or Git mutations. Parent owns final binder/source getter and publication tests. Do not format
or compile the affected Root/Editor source until both writers report source-ready.

Recovery reviewer Hypatia `01a19868-3712-7cf0-a37c-2e926860a716` found no remaining assigned
P1/P2 in complete fresh inventory, deterministic output/no-rewrite, retained safe retry, Ready and
CLI-only closure. It was a static review, not product evidence, and that reviewer is closed.

Unaffected owner run `5f9a9b61-d83c-4a39-9abc-8e8fd851f02f` passes99/99,155 excluded, across
nara_project full library and Reflect property::value_admission_tests. Cargo session52283 needs
one final poll to acknowledge its already logged terminal success. No other Cargo is running.

# Next Action

Freeze both writers, inspect changes and format. Verify the saved-predecessor regression, Task
rejection/physical-return controls and complete Root/Application related cohorts. Rebuild the
independent CLI recovery executable into root target and run normal authored_upgrade_command plus
all six ignored subprocess recovery controls. Complete affected feature/lint/independent/native
qualification, then honest evidence/ledger, precise local Conventional Commit and ff main.
No push/PR; original vendor remains untouched. The supplied research ZIP is already integrated;
do not reimport it or reset the roadmap. CP2/motion/render/custom editable Editor remain open.

# Citations

- [Complete authored-upgrade plan](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [Previous checkpoint](2026-11-01T182221Z-source-authority-scoped-writeback-and-immutable-preview-report-controls-pass-6efcd54caabb41828d0d23606c44fc6b.md)
- TEMP logs: nara-authored-upgrade-source-final-tests.log,
  nara-authored-upgrade-final-application-tests.log,
  nara-authored-upgrade-saved-predecessor-regression.log,
  nara-authored-upgrade-project-and-value-owners-tests.log.
