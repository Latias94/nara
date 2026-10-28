---
type: "Subagent Finding"
title: "Installed existing-owner evolution is a separate transaction after additive adoption"
description: "Records four current refusal boundaries and a creation-metadata-only installed tool successor workflow without claiming value migration or implementation."
timestamp: 2026-10-28T21:45:06Z
record_id: "6b701a39429c41ddbc273c80bab9474c"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a18474-ff8c-78a1-8257-573c24bd9930"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/native-prefab-template-import"
---

# Finding

The installed tool V1/V2 consumer changes execution while retaining the same catalog. It does
not qualify changing an already admitted owner's declaration. Four existing refusal boundaries
correctly prevent that unsupported operation; none should be removed merely to enable an update.
This is a separate evolution transaction under ADR 0114, not a defect in additive owner adoption.

# Evidence

- `source_tools/release.rs:248` decodes a single catalog and admits its owner with no predecessor;
  `release/manifest.rs:45` supplies no predecessor coordinate. A generation-two release cannot
  enter that installed path despite Reflect's existing lineage validation.
- `authoring/bundle/owner.rs:161` returns `ExistingOwnerChanged` when the same owner's complete
  current/predecessor pair differs. Exact equality alone permits `AlreadyAdmitted`.
- `nara_scene/authoring_decode/reclassification.rs:22` requires the old owner, complete Schema
  and Adapter to remain equal. Workspace applies this before whole-set preparation at
  `workspace/schema_reclassification.rs:448`, including creation-only metadata changes.
- `source_tools/selection.rs:719,854,1066` checks project/tool owner equality before probing and
  again before promotion. Desired selection publication cannot grant Schema admission.
- `source_tools.rs:922` already decodes a probe with its selected predecessor. The child protocol
  does not require wholesale replacement to represent a valid successor.
- ADR 0118 already requires a creation-template-only change to advance catalog generation while
  keeping component wire version and existing stored-value semantics unchanged.

The audit was read-only, with no Cargo/native execution or vendor/reference-tree access. File
coordinates describe the native Prefab integration source before its final commit. Existing
ordinary tests are source evidence, not new executions by this reviewer.

# Recommendation

Use an actual installed EnemyStats successor that changes only `authoring_initial_value`, with
the same owner/type/field identities, field shapes and component wire version. Capture its complete
predecessor through the existing release owner. Require that predecessor to equal the project's
current catalog, then reuse bundle publication, the shared nominal basis, whole-open-set/history
preparation and reconciliation. Limit the accepted difference explicitly; do not weaken generic
schema equality or create another history interpreter.

The affected Adapter's implementation identity, version and coverage remain fixed, but its
generation/fingerprint/predecessor-bound evidence must be freshly prepared by its actual owner
from the validated successor catalog and included in the same new basis. Other owners' evidence
remains exactly equal. Existing opaque V1 evidence cannot be relabeled as V2 admission.

Require explicit retirement of the affected V1 route and actual task return before Schema
publication. Enable V2 separately after successful adoption. A published Schema with failed V2
activation remains a usable new Schema with an inactive, retryable tool; it is not an automatic
cross-file rollback. Qualify the intermediate state through Close/Open as well as the final path.

Cover inactive dirty Scene/Prefab documents and both history directions, unchanged old values and
source/checkpoint identities, new creation values, stale bases, wrong predecessor, budget/proof
failure, publication-without-adoption, selection failure, cancellation and physical Close.

# Disposition

Selected as the next bounded design/implementation workflow in the full active foundation Goal.
Its [execution plan](../../../../plans/2026-10-29-installed-owner-creation-metadata-evolution-plan.md)
records the intended boundaries. No production evolution API or runtime qualification is claimed
by this finding. General field/value migration, parallel candidate probing and package authority
over semantic owners need separate concrete consumers and evidence. The operation is independent
of the future Nara-owned custom-drawn frontend.

Independent document reviewer `01a1847a-db09-7300-9120-255a7ef30a48` identified this Adapter
evidence distinction as a P1 flaw in the first plan draft. The corrected step now reconciles
ADR 0090 alongside 0114/0118, preserves additive equality guards and requires genuine successor
evidence rather than unchanged old evidence or relaxed matching. This design correction does
not report an implemented defect in the existing additive operation.
The same reviewer rechecked the corrected steps and closed the P1 with no confirmed residual
in that narrow design scope. No implementation or runtime qualification follows from closure.

# Citations

- [Additive adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0114](../../../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0098](../../../../architecture/adr/0098-schema-owner-lineage-and-active-runtime-composition.md)
