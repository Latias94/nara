---
type: "Verification Evidence"
title: "SRT-U1 focused startup and Retry Trial activation"
description: "Verifies the RGS stop condition, the narrow ADR 0089 Trial boundary, reviewed successor execution order, and truthful active authority."
timestamp: 2026-08-02T12:00:06Z
record_id: "17ad79c7bdfd417d90ee3d524909b120"
tags: ["srt-u1", "adr-0089", "startup-activation", "atomic-retry", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d4e166f"
verified_by: "independent seam audits plus coherence, feasibility, scope, product, and adversarial plan review"
---

# Verification

RGS-U4 reached its recorded ADR 0089 stop condition at baseline `d4e166f`. Project Host owns the
exact startup document and spawn receipt during materialization but does not publish that pair to
product Startup. The private hierarchy-aware scene replacement path cannot compose product-owned
candidate values and exact runtime-entity retirement. Continuing through the reference game's
partial live-World template would therefore preserve duplicate authority and false atomicity.

# Result

Passed for SRT activation at baseline `d4e166f`.

- The RGS plan is reciprocally superseded after verified RGS-U1 through RGS-U3; their immutable
  evidence remains authoritative.
- The successor plan is the sole implementation-ready active plan and keeps ADR 0089 Proposed. It
  authorizes only one retained startup source/receipt, ordered product Startup consumption, one
  scoped replacement overlay, exact additional retirement, and the reference game's existing
  fixed-step boundary.
- Root Project Host retains the content lease; `nara_scene` remains independent of Project Content
  budgeting. One sealed materialize operation prevents Direct App from pairing a receipt from one
  document with another document.
- Replacement limits, component pre-registration, identity-axis rejection, and an infallible
  post-mutation commit token are explicit implementation gates. General scene sessions, raw World
  callbacks, provider registries, schedule-abort machinery, and broad travel semantics remain out of
  scope.
- The reference-game generation-4 schema disposition now classifies every existing aggregate and
  runtime field before fixture changes. Retry rejection uses the existing game-owned bounded status
  rather than a second authority.
- Independent current-HEAD audits confirmed stale direct image replacement and the unconsumed
  prepare-invalidation log. SRT-U2 closes those defects before desktop evidence; bounded fault detail
  joins SRT-U3, and hierarchy query complexity joins SRT-U4. The importer catalog/execution cleanup
  remains a later artifact-cache slice rather than expanding this Trial.
- Coherence, feasibility, scope, product, and adversarial review findings were applied. Scope review
  found no general Scene Manager/provider expansion. No independent different-model promotion was
  claimed.

# Evidence

- Active-plan pointer, predecessor supersession, ADR status, and implementation-ledger inspection:
  passed.
- Engineering-memory validation/render and direct link/whitespace checks: recorded after this shard
  is published.
- Cargo and `tests/architecture_docs.rs`: intentionally not run for this documentation-only unit.

# Follow-up

SRT-U2 is the sole active implementation unit. It adds `AssetSlotRevision` to image prepare identity,
centralizes fallible `ImageAsset` validation, and removes the unconsumed invalidation event surface
without adding another version system or consumer queue.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md`
- `docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md`
- `docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md`
- `docs/architecture/adr/implementation-status.md`
- `src/project_host/runtime.rs`
- `crates/nara_scene/src/spawn.rs`
- `crates/nara_image/src/prepare.rs`
- `crates/nara_render/src/prepare.rs`
