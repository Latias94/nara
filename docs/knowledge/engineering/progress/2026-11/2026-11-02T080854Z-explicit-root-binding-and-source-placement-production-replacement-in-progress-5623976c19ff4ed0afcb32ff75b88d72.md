---
type: "Work Progress"
title: "Explicit root binding and source placement production replacement in progress"
description: "Work Progress for Explicit root binding and source placement production replacement in progress."
timestamp: 2026-11-02T08:08:54Z
record_id: "5623976c19ff4ed0afcb32ff75b88d72"
producer_id: "codex-foundation-main"
run_id: "root-binding-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "dcbbb93"
---

# Summary

The complete foundation Goal remains active. The previous compiled/source prerequisite is
committed and qualified at `b7eac29`, with local main and the long-lived refactor branch at
`dcbbb93`. This successor begins the selected root-binding/source-placement production
replacement. All new changes are uncommitted; no complete CP2 extraction claim is made.

# Details

- Source Scene/Prefab hierarchy now uses ordered `SceneEntityPlacement` declarations pairing a
  typed occurrence target with a shared `ScenePlacement` parent/slot. Source roots and record
  children are removed. Overlay placements use the same grammar and layout implementation.
- `PrefabInstanceBinding` explicitly selects a forest mount or a canonical-base-checked named
  root. A root contributes to its containing anchor without a wrapper. Compiler scope-local
  alias maps retain the distinct declarations, lexical input environments and instance slots.
- The compiler uses private flat `SceneDraft` storage. Readonly `ExpandedSceneEntity` observations
  have no Prefab slot and cannot be copied back through an authored-document facade. Compiled
  storage and complete Native batch admission retain their previous qualified boundary.
- Exact source transactions replace the complete ordered placement vector. Source entity
  additions/removals are explicit batch records, without inferring visual subtree ownership.
  Independent source/history/tooling and raw-format/classification workers are adapting callers.
- Source-local topology observations call the same layout kernel with a direct-target map and
  explicit finite storage allowance. Routed targets refuse rather than approximate missing source
  context. This supports the existing direct UI authoring product without a second evaluator.
- Selected conversion needs the compiler's real typed-target correspondence, including root
  aliases. Witnessed containing-source input-reference queries are now being integrated through
  the same occurrence/input interpreter; external projected endpoints must remain typed source
  routes, never generated materialization IDs. This integration is not yet qualified.

The new public `root_binding` no-serde binary passes **16/16**, nextest run
`ce03496c-ca43-4802-b535-b5cc48a1fc95`. The first run reached 6/16 because its native test helper
counted ECS component-registration entities as Scene entities. Counting actual
`SceneEntitySource` entities corrected the oracle; compiled entity count, instance membership,
real hierarchy, component values and reference resolution remain independently asserted.
The original first-run failure is not a product-layout failure.

An independent read-only adversarial review found no confirmed P1/P2 in the current core.
Its two concrete gaps produced seven additional public tests for distinct merged-root instance
slots, finite recursion with source placements and routed local-query refusal. The all-feature
two-binary command is currently being integrated; these seven tests are not yet passing evidence.
No broad owner/public/independent/native regression qualification has run for this new unit.

# Next Action

Finish selected conversion query demand and exact endpoint rewriting, then run the new public
root-binding and facet tests under all features. Continue raw format/classification, exact
history/conversion, Tooling/UI/Root/Editor and independent/Reference Game source migration.
Freeze writers before final formatting. Qualify the complete prerequisite, independently review,
commit precisely and fast-forward local main only after actual acceptance. Then implement full
CP2 occurrence correspondence and project publication/recovery. No push, PR, vendor access or
reference-checkout mutation is authorized by this record; retain existing user constraints.

# Citations

- [Current CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Qualified compiled prerequisite](../../verification/2026-11/2026-11-02T071856Z-closed-compiled-scene-production-and-independent-delivery-qualification-91cc04f45ba249f881cec4f9f78bd3d1.md)
- [Research disposition](../../../../research/2026-11-02-infrastructure-comparison-reconciliation.md)
