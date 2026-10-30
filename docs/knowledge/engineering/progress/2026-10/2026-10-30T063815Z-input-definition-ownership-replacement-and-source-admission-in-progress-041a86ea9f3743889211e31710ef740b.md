---
type: "Work Progress"
title: "Input definition ownership replacement and source admission in progress"
description: "Move-only compiled definitions and actual bindings owners replace live mutable serde; source Host and independent workflow qualification remain in progress."
timestamp: 2026-10-30T06:38:15Z
record_id: "041a86ea9f3743889211e31710ef740b"
status: "in-progress"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-definition-and-source-admission-plan.md"
git_branch: "refactor/foundation"
git_commit: "6ef3cb0"
---

# Summary

The foundation Goal remains active. Source Scene qualification at `6ef3cb0` is preserved; the
next Input unit replaces its owning model before enabling manifest-selected source input. All
Input changes are uncommitted. No remote publication or new worktree was used.

# Details

- Reconciled Accepted ADRs 0035/0041/0051 for immutable definitions, actual owner-backed bindings,
  move-only World state, intrinsic primary Product JSON and explicit source selection obligations.
- `ActionMapDefinition` is plain move-only compiled data. `ActionMapBindings` projects through
  `Arc<dyn AsRef<ActionMapDefinition> + Send + Sync>`; it cannot share borrowed compiled data with
  an unrelated keepalive. Removed live `ActionMap` serde, `Clone` and incremental binding APIs.
- Compiled context slots replace string-keyed revision maps. Restoring bindings always creates
  a new mapping lifetime. Context/revision tickets follow queued snapshots and held lifetime
  identities after resource replacement, without retaining a definition or entire Scene snapshot.
- Public `finalize_input_selection` accepts only `RuntimeCandidateScope`. The private Input
  installation marker checks pristine resource identity and explicit configuration, including
  explicit empty maps. No new App lifecycle query or Startup system is needed.
- Added strict JSON source format/golden, finite builder/compile costs, context and fanout caps,
  exact state allowance and migrated Reference Game, sample and existing physical consumers.
- Source Host capture now owns actual definition/Product/path/digest and an independent lease;
  joins intrinsic identity to the canonical resource catalogue, checks optional sidecar agreement,
  records `InputActionMap` closure members, prepares independent World state and reuses original
  global ownership during travel. This integration is still under qualification.

Actual verification before this capture:

- `cargo +1.95.0 nextest run -p nara_input --all-features --locked ...`: **108 passed**, run
  `b528a85d-8641-46c7-8011-92650a7fa5f0`; compilation 68 seconds, tests 2.916 seconds.
- `cargo +1.95.0 nextest run -p nara_project --all-features --locked ...`: **79 passed**, run
  `b8628feb-f7b8-43b5-8838-287e15133ea8`; includes explicit projection/exactness controls.
- Root no-default `project-workflow-cli,tooling,presentation-2d` lib check passed with existing
  feature warnings; a newly unused old source Product helper has since been removed and its test
  callers use the actual travel preparation path.
- Root Source/Host/projection nextest is still compiling in serial session `89577`; no result
  is implied. No current unit native, external-consumer, Check or Export qualification exists yet.

Independent review challenged arbitrary borrowed-definition retention, early state refunds and
installation lifetime. Those boundaries were rewritten. Remaining review includes actual source
capture costs/identity and Editor authoring receipts: external Play currently derives its digest
from Authoring source facts, which need to include the selected input source. A separate existing
endpoint/contribution growth gap is recorded in the plan; the fixed context/revision allowance
does not claim to bound that physical/semantic state or transient output.

# Next Action

Finish Root integration checks; collect bounded independent review and actual-file tests; repair
the Authoring source-fact boundary; run independent normalized physical input, context cancellation
and A-to-B-to-A travel. Then qualify Check/relocated source Export, affected public/minimal/native
consumers, format after writers freeze and commit only the complete unit. Processed startup Input,
target selection, render graph and the full custom-drawn Editor remain ordered foundation work.

# Citations

- [Input owner plan](../../../../plans/2026-10-30-input-definition-and-source-admission-plan.md)
- [Input baseline review](../../../../research/2026-10-30-input-definition-and-runtime-ownership-review.md)
- [Delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Source Scene qualification](../../verification/2026-10/2026-10-30T060408Z-finite-source-scene-data-closures-and-retained-editor-observations-qualification-deea8e4f86ac44f3a6faf27aefa53a4e.md)
