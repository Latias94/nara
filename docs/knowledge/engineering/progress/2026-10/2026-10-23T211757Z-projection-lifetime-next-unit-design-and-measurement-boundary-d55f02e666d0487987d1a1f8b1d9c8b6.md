---
type: "Work Progress"
title: "Projection lifetime next-unit design and measurement boundary"
description: "Ground the remaining R5c preview-history pressure and compare explicit epoch handoff with owner-specific incremental projection; no production selection yet."
timestamp: 2026-10-23T21:17:57Z
record_id: "d55f02e666d0487987d1a1f8b1d9c8b6"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-projection-entry-20261024"
subagent_id: "01a16a9b-a61f-7302-9d3a-29d1f9025790"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "8c15e32ee745540b5ff91bbf6c27a9a6cc7813f7"
---

# Summary

Next R5c work after implementation `ebc0d48` and qualification `8c15e32`: sustained editing of
the retained Scene preview without exhausting one long-lived Runtime's identity claims. This is
a comparison entry, not an Accepted selection, ADR amendment, new API or completion of projection/
history work. The full root goal remains active.

# Details

Read-only design lane `01a16a9b-a61f-7302-9d3a-29d1f9025790` inspected current owners and pinned
Bevy/Godot source. The integration owner rechecked the concrete anchors and pins. Neither ran a
new benchmark or churn test in this lane; the earlier F5 witness remains the executed evidence.

## Confirmed Pressure

- `runtime/editor/preview.rs` replaces its materializer document when the source revision changes.
  Native `SceneAuthoringSession::sync_world` then replaces the existing live Scene instance.
- `WorldIdentityDomain::commit_scene_instance` retains one instance claim plus one per entity.
  Retirement removes active mappings, not lifetime claims. `scene_entry_capacity` refuses at the
  configured lifetime total, not the current live count.
- The default limit is 1,048,576. An otherwise unused domain admits at most 104 complete
  10,000-entity publications, at 10,001 claims each, including initial publication. This is a
  capacity derivation, not measured latency, memory or a promise of 104 subsequent edits. The
  earlier six-claim/one-entity witness passed three publications and refused the fourth with
  last-good intact.
- This obeys ADR 0058. The debt is coupling disposable preview replacement to permanent same-domain
  history, not corruption or a regression. Do not generalize this exact path to UI Prefab Canvas,
  which has a separate document projection.

## Candidates And Admission

**A: Host-owned explicit preview epoch handoff.** Prepare a complete source/revision/Schema/content-
bound candidate, admit a fresh authoring Runtime epoch, publish the selected view and retain actual
predecessor retirement. A rejected candidate leaves last-good visible but not valid for the new
revision's picking/input. Same-domain exact claims remain intact. The authoring Runtime also owns
task/image state: image publication generations, pending import/revoke work and cleanup must
transfer or remain accounted for. Swapping only `WorldIdentityDomain` is not this ownership proof.
Do not add a permanent App, pool or image store per document.

**B: Scene-owned bounded incremental projection with explicit epoch fallback.** Prepare a typed
delta from admitted old/new documents and Reflect-owned writes; unchanged objects retain their
binding/incarnation. Ordinary fields need no new Scene claims. Unsupported structural, relationship,
lifecycle or Schema changes select the complete candidate path before any mutation. Scalar-only
updates do not solve unbounded structural churn; this design still needs an honest epoch/history
policy. Arbitrary reflected mutation is not a transactional candidate.

Use A as the comparison baseline, not a foregone winner. Compare B on the same workflow only where
its delta is complete. Disposable experiments may preserve current public contracts; reconcile
Accepted ADRs before integrating any changed publication/classification/admission contract.
The lane's suggestion to amend ADR 0058 immediately does not justify a new global Expired API.

Current persistent `Deleted` is terminal in its domain; `Unloaded` may reactivate with a fresh
incarnation. If a later design evicts deletion knowledge, Expired/HistoryUnavailable means unknown,
not Missing or creation permission. Same-timeline readmission requires retained proof or an admitted
durable timeline, otherwise it refuses. A new preview epoch may materialize a document, but is not
a checkpoint restore or permission to resurrect a deleted persistent identity. Old locators reject
the new domain; domain-free runtime targets still need explicit session/timeline admission.
Candidate A need not introduce global eviction or Expired semantics.

## Pinned Reference Limits

Bevy pin `85503941e488152711a4695bbe248e9ed9f453fc` exposes entity-local reflected application
in `crates/bevy_ecs/src/reflect/component.rs`; its EntityGeneration documentation explicitly permits
wrap/alias. Godot pin `81ab961f91066a63bcbf3ec340962bb1a1ada558` records property Undo on existing
objects in `editor/inspector/editor_inspector.cpp` and recycles ObjectDB slots in
`core/object/object.cpp`. Local sources are under read-only `repo-ref/`. These support studying
local updates and recyclable execution identity; neither supplies Nara's atomic candidate semantics
or permanent arbitrary-history Deleted knowledge.

# Next Action

1. Define a complete public workflow: bound Scene -> repeated field edits -> Undo/Redo -> structural
   add/remove -> Save/Reopen -> presented preview/picking -> Close. Include repeated Prefab instances
   and unknown records; reuse the independent prepared-authoring consumer, not a new public test Host.
2. Choose workload sizes, hardware, numerical targets and stop conditions before measuring. None are
   selected here. Compare claims/edit, retained bytes, peak coexistence, owner hold time,
   edit-to-present P50/P95/P99, startup/retirement time and worker deltas separately for field,
   structural and Prefab fan-out workloads.
3. Add small-budget owner oracles for multiple epochs, stale preparation and old locator rejection.
   Candidate failure cannot roll back committed document/history or publish a new picking basis.
   Cover pending/failing image/Host cleanup, independent unload and any eventual expired-Deleted
   readmission rule. Do not enlarge limits, clear claims or weaken tests.
4. Keep write scopes responsibility-local: Identity lookup/admission; Scene/Reflect prepared
   materialization/deltas; Host preview/authoring Runtime/presentation handoff. Leave the newly
   qualified Play-source, persistence lanes and refresh obligations intact.
5. Reconcile ADRs 0058, 0026/0047 and 0111 for the selected durable contract. Delete the displaced
   Editor same-domain per-revision rebuild path if selected; retain Scene replacement APIs with
   deliberate external consumers. Do not introduce a universal lifetime framework.

# Citations

- [Foundation R5](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r5-durable-addressing-prefab-overlays-and-projection-lifetime)
- [Integrated D04](../../../../architecture/foundation-architecture-reassessment.md#d04-document-truth-projection-and-identity-knowledge)
- [F5 and prior witness](../../../../audits/2026-10-23-chat-informed-engine-architecture-review.md#f5-preview-replacement-spends-lifetime-identity-capacity-p1-design-debt)
- [Play-source qualification](../../verification/2026-10/2026-10-23T210921Z-trusted-play-source-continuity-and-source-local-close-qualification-61897d1b17744e2aaff6fc6efb1594a7.md)
- [Preview source replacement](../../../../../src/project_host/runtime/editor/preview.rs)
- [Scene materialization](../../../../../crates/nara_scene/src/authoring.rs)
- [Lifetime claims/admission](../../../../../crates/nara_identity/src/domain.rs)
- [Shared authoring Runtime](../../../../../src/project_host/runtime/editor/authoring_runtime.rs)
- [Independent public workflow](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
