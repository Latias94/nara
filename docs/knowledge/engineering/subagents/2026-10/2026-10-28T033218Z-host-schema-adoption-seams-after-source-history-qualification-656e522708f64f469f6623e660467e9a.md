---
type: "Subagent Finding"
title: "Host schema adoption seams after source history qualification"
description: "Maps actual bundle, capture, Workspace and Session ownership and records proposed streaming historical proof without a second interpreter."
timestamp: 2026-10-28T03:32:18Z
record_id: "656e522708f64f469f6623e660467e9a"
producer_id: "codex-root"
run_id: "source-schema-reclassification-2026-10-28"
subagent_id: "01a18063-2029-7da2-a243-537f65cd038a"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "main"
git_commit: "3b64210"
---

# Finding

At local `main = 3b64210` the source/history prerequisite is qualified, but actual installed-owner
adoption remains unimplemented. The next owner is the existing Project Session, composed with
Reflect bundle admission, Scene source/history and the current file publication Host. A new
Package Schema executor, standalone registry or generic document framework is unnecessary.

The required sequence is immutable installation inspection, bundle composition and one successor
basis, complete current open-source preparation, actual current/historical dependency capture,
bundle publication with live-leaf handoff, then one exclusive project/Workspace adoption turn.
All IO, callbacks, new admission and evidence validation precede the first classification swap.

# Evidence

- `src/project_host/authoring/bundle.rs::AdmittedAuthoringSchemaBundle` retains the actual file
  capability, bytes, digest and admitted bundle. `AuthoringSchemaReceipt` owns it. The receipt
  lacks the narrow live-leaf borrow/accepted-publication handoff needed by a successor.
- `src/project_host/file_publication.rs::FilePublicationHost::bind_existing` already rebinds
  while the observed leaf is alive. `AcceptedFilePublication::into_parts` returns actual evidence
  and `FilePublicationSource`, whose capability is private. Hand off this live object rather
  than discarding it and reacquiring from an identity/digest pair.
- `src/project_host/authoring/adapter.rs::project_authoring_adapters` composes current engine
  Adapters. `PreparedAuthoringProject::admit_startup_closure` admits a new basis internally and
  cannot be reused unchanged when source candidates already hold the unique successor basis.
- `src/project_host/authoring/project/capture.rs::capture_scene` already takes an explicit basis,
  uses the selected compiler and captures assets/database, source facts, stable lookups,
  unresolved dependencies and validation. Its receipt currently requires an already admitted
  bundle leaf, so publication-before-preparation cannot be avoided by fabricating a receipt
  with predecessor leaf and successor bytes. Split prepared capture inputs from leaf attachment
  inside this owner.
- `AuthoringPrefabSessionSnapshot::capture` retains dirty working Arc, basis, revision and file
  receipt. Current Session `authoring_source_selection` obtains the latest persistence Host
  observation. A successor view needs the prepared classification basis, rather than retagging
  an incumbent snapshot. Closed dependencies retain raw actual-demand admission; calling
  `publish_document` on every demanded Prefab would wrongly require standalone validity.
- `crates/nara_scene/src/authoring/schema_reclassification.rs::prove_history` is the sole branch,
  scope, inverse and cost kernel. Source tokens retain state and original revision/basis but no
  second document. A validated borrow from session plus token can expose the original source
  Arc and successor classification to the actual Host consumer.
- `EditorWorkspace` owns the complete map of Scene, Prefab and Extension slots. Existing Open
  and Reopen are unsuitable because they replace sessions/history. Whole-set bind/swap is absent;
  unsupported open extension owners require explicit refusal or proved independent authority.
- Session owns existing finite tasks, persistence/import/conversion/tool/selection lanes and
  HostCustody. Tool `jobs()` omits selection IO, and UI phases alone do not prove physical return.
  Use concrete owner blockers and restart fences, not a requirement that shared Tasks/GPU be idle.
- `EditorSceneCapture::invalidate` and `EditorAuthoringRuntime::invalidate_preview_content`
  revoke old preview authority. They must run in the adoption turn because source revision stays
  unchanged and the content stamp does not itself carry Schema basis. `into_session_snapshot`
  can fail on aliased ownership and must complete before any infallible source swap.

# Recommendation

First amend ADRs 0114/0047/0118 for their specific successor boundaries, then implement real
bundle/publication handoff and current prepared-source inputs. Do not add unused general-purpose
APIs ahead of their concrete Host consumer. Preserve complete prior owner lineage and use Reflect
composition/collision checks. Rebuild or invalidate the bundle-bound source index.

For historical effective validation, extend the existing history kernel with an internal borrowed
visitor, leaving the source-only API unchanged. It should inspect current once and stream each
forward state after source forward/reverse/cost proof, without collecting source snapshots or
calling public Undo/Redo. Reverse restores a previously proved source under the fixed dependency
observations. This is proposed integration, not an implemented callback API.

A second pass through the same kernel is acceptable initially: prepare all current successors
and dirty Prefab inputs first, then prove their historical compositions. Charge the extra replay
cumulatively. Host capture must share one finite operation allowance, reserve actual compiler
working capacity before entering it, and release each historical capture afterward. Retain only
bounded historical file/lookup evidence separately from current active assets/database. Inputs
requested only by history still require publication-time validation.

Do not equate capture `Ok` or `Degraded` with success/failure. Current capture can return Degraded
with composition diagnostics; native preview omissions and preserved unknown records are separate
policy outcomes. Reject invalid selected surviving values/references through the actual compiler.
Keep the paired lawful outer-discard case. A historical Prefab's effective interpretation needs
its actual containing-use context; an unconditional independent-root check must not silently
narrow the accepted outer-repair contract.

The complete compiler should borrow Scene or Prefab roots/entities through its existing nominal
topology machinery. Avoid cloning every Prefab through `instantiate`, or substituting the
conversion compiler's selected-anchor scope for complete composition. If a proof-local historical
Prefab must become a resolver input, move the already generated owned state into an admitted Arc;
do not label it as the current Session revision or publish it into the final receipt.

Finite Tasks require an owned request. Capture one bounded journal input under the Scene owner
with original source/classification Arcs, revision/content state and exact patch/scope/cost
entries, then execute the same kernel. Do not reconstruct conversion permissions in the Host
or move live sessions out of Workspace merely to satisfy `'static`.

# Disposition

Accepted as the next implementation map and as concrete constraints on the existing owner-adoption
plan. Proposed view/visitor/request signatures remain unselected until their real Host consumer
is implemented. No production code or tests were changed by this read-only research. Source
qualification at `3b64210` does not depend on finishing this integration, and the full foundation
Goal remains active. Installed dependency/content/removal and later waves remain open.

# Citations

- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Source/history qualification](../../verification/2026-10/2026-10-28T032419Z-source-owned-schema-reclassification-with-exact-retained-history-and-bounded-interpretation-becb85b221e241aebeb4845c692a7379.md)
- [ADR 0114](../../../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
