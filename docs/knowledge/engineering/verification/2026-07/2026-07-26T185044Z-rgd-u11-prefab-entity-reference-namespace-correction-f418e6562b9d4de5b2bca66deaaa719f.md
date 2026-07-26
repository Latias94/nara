---
type: "Verification Evidence"
title: "RGD-U11 prefab entity-reference namespace correction"
description: "Verifies migration-aware, bounded, failure-atomic SceneLocal entity-reference projection for repeated and nested prefab instances."
timestamp: 2026-07-26T18:50:44Z
record_id: "f418e6562b9d4de5b2bca66deaaa719f"
tags: ["rgd-u11", "scene", "prefab", "entity-reference", "correction"]
status: "verified-local"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d88ddab2056b42da225e5d28970062c49a345c97"
verified_by: "codex-root"
---

# Verification

Commit `d88ddab2056b42da225e5d28970062c49a345c97` closes the RGD-U11 prefab
entity-reference correction under Accepted ADR 0038 and ADR 0058:

- `ComponentRegistry` migrates an owned component value before the current
  schema is used to interpret its declared `entity_ref` locations.
- A bounded iterative plan validates the complete declared reference shape,
  reports the final projected stable identity, and rewrites values in place
  without a nested-value scratch copy.
- Prefab expansion rewrites only `EntityReference::SceneLocal` values into the
  concrete instance namespace. `EntityReference::Persistent` values retain
  their durable identity.
- Repeated and nested prefab instances project the same source-local reference
  independently; each final entity identity is namespaced exactly once.
- Exact final-prefix and value-growth budgets are charged before publication.
  A rejected migration, reference shape, path, budget, or override leaves the
  source document and applicable unpublished target state unchanged.
- The crate-private consuming patch path avoids retaining an unused inverse or
  a third override payload while the public patch API keeps its existing
  failure-atomic scratch-and-inverse contract.

# Result

- **Correction status:** verified and committed. This closes one of the five
  U11 pre-publication source gates.
- **Remaining U11 source gates:** optional-owner lineage, unforgeable
  persistence receipts, bounded terminal asset reload, and paused-input
  transition retention.
- **Non-claims:** this record does not complete broader prefab provenance,
  authoring write-back, U11 pressure evidence, final U8/U10 candidates, any
  protected dispatch, or a Publish decision.

# Evidence

- `cargo nextest run -p nara_reflect -p nara_scene --locked
  --test-threads=1`: 155 passed.
- `cargo nextest run -p nara_reflect -p nara_scene --all-features --locked
  --test-threads=1`: 189 passed.
- Strict changed-target Clippy passed for `nara_reflect` and `nara_scene`
  across all targets and features after allowing only the repository's known
  unrelated `result_large_err`, `collapsible_if`, `double_must_use`,
  `too_many_arguments`, and `derivable_impls` baseline lints.
- `cargo check --workspace --locked` passed.
- `cargo fmt --all` and `git diff --check` passed.
- Independent correctness, performance, and simplicity reviews reported no
  remaining P0/P1 finding.

# Review Follow-ups

Non-blocking P2 follow-ups remain evidence-triggered: repeated reference-plan
path lookup, wide-value traversal stack pressure near maximum admitted node
counts, trusted migration allocation before post-migration accounting, shared
low-level patch mutation primitives, and additional consuming-path coverage
for less common patch operations. They do not weaken the verified publication
and identity semantics.

# Follow-up

Resolve the remaining four focused U11 corrections. The optional-owner lineage
correction first requires the bounded OQ-044 decision; the other three already
have sufficient Accepted authority. Only after all correction gates and the
local U11 pressure/author work close may the plan request a new final hosted U8
authorization.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u11-complete-pre-publication-successor-and-candidate-evidence`
- `docs/architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md`
- `docs/architecture/adr/0058-stable-runtime-identity-and-entity-references.md`
- `crates/nara_reflect/src/entity_reference.rs`
- `crates/nara_reflect/src/registry.rs`
- `crates/nara_scene/src/prefab.rs`
- `crates/nara_scene/src/patch.rs`
- Commit `d88ddab2056b42da225e5d28970062c49a345c97`
