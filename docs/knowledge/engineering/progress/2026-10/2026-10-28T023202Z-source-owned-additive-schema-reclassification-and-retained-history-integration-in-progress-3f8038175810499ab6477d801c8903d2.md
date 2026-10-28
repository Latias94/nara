---
type: "Work Progress"
title: "Source-owned additive schema reclassification and retained history integration in progress"
description: "Records the in-progress source/history replacement and concrete budget, nominal identity and shared preparation interfaces before qualification."
timestamp: 2026-10-28T02:32:02Z
record_id: "3f8038175810499ab6477d801c8903d2"
producer_id: "codex-root"
run_id: "source-schema-reclassification-2026-10-28"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "feat/source-schema-reclassification"
supersedes: "8afcd1ede3dc4451ac719a1a236e9c85"
---

# Summary

Continue the full active foundation Goal from local `main = e7cb464`, ahead of origin by 169,
without remote publication. Current branch is `feat/source-schema-reclassification`; all changes
in this unit are uncommitted. The original untracked `vendor/` remains untouched. No Cargo has
run in this unit yet; classification writer and test writer are active and their transitional
files must freeze before formatting or qualification. Reuse root target and one build job.

# Details

The source-owned module is `crates/nara_scene/src/authoring/schema_reclassification.rs`.
It prepares additive classification without replacing source Arc, revision, content state,
upgrade flag or history. A shared exact declaration/owner/Adapter extension check is being added
under `authoring_decode/reclassification.rs`. The Host must separately prove complete catalog
lineage and actual source/dependency/publication facts; this does not implement Session adoption.

Public source API currently written:

- `SchemaReclassificationLimits::{new,default}` bounds cumulative modeled work and peak logical
  retention separately, with defaults 512/256 MiB. Shared catalogs and allocator capacity/RSS are
  outside this model.
- `SchemaReclassificationWork` retains observed incumbents/prepared outputs, charges all admitted
  copy/classification work cumulatively and releases temporary edge scratch between edges. Its
  `usage()` exposes work/peak observations. Candidates dropped early stay conservatively charged.
- Scene/Prefab `prepare_schema_reclassification(successor,&mut work)` produce move-only nominal
  candidates. `commit_schema_reclassification` validates original revision/basis before swapping
  classification. Hidden `bind_schema_reclassification` returns an exclusive guard whose `commit`
  is infallible, allowing a future Workspace to bind its entire set before any swap.
- Prepared candidates expose `schema_basis()` and `base_revision()`. Old edit/query bases reject
  the newly adopted nominal basis. Same-content independently admitted bases remain distinct.

Normal Undo/Redo now delegate to private source-specific `prepare_scene_step` / `prepare_prefab_step`
which match original ordinary/conversion scope. `admit_history_replay_report` is extracted from the
actual history owner; normal replay and proof share inverse/cost validation. The generic private
proof traverses Undo and Redo independently from current source, prepares each edge with actual
history permissions, freezes the inverse by borrowing it, prepares the reverse under the same
scope, verifies exact source restoration/regenerated original patch and both original cost caps.
It allocates no new history identities and clones no history stacks. Only current branch state
and one forward/reverse edge coexist, not every historical snapshot.

The classification writer is adding admission callbacks to existing classification, scalar field,
structural and scoped-conversion preparation, with borrowed cost/retention helpers and shared
Reflect visitors. Normal no-admission paths must not eagerly run budget-only cost scans. Prefab's
temporary nominal projection, where needed by the existing classifier, must be admitted before
copy. Post-allocation-only measurement is not an adequate budget proof.

The owning ADRs 0090/0026 now specify this narrow source contract. Their decision text is not
implementation evidence; the production source is still unqualified and may change during review.
Only the previous prerequisite chain at `e7cb464` has the 997 Reflect/Scene, 15 focused, 11 public,
minimal and Clippy results from the predecessor evidence. Do not repeat those as this unit's checks.

# Next Action

Three active agents and disjoint writes:

- Jason `01a1804a-fb31-7743-87b4-bb272ceb28ac`: `authoring_decode.rs`, its new `reclassification.rs`
  and tests, plus `structure.rs`, `field_candidate.rs`, `retention.rs` and necessary initializer
  adjustments for shared admission hooks. Parent owns `authoring.rs`, new source-owner module,
  `lib.rs` and ADR/docs. Coordinate before editing the worker's files.
- Confucius `01a18054-41c3-7a83-8b35-1d525b0b3172`: new owner module `tests.rs` and optionally
  public `tests/schema_reclassification.rs`; actual source/history counterexamples, no Cargo/fmt.
- Huygens `01a18055-64c9-7002-af61-b690c28322fb`: read-only adversarial review of parent kernel,
  candidate guards, exact history/source fences and modeled lifetime accounting.

All agents are forbidden Cargo/fmt/Git writes; parent is integration owner. Wait for source-ready,
inspect final changes, format and run focused serial Scene checks/tests. Fix real errors, then
appropriate full source/consumer/minimal/Clippy qualification and independent review. Test/qualify
normal Undo/Redo because their preparation/cost kernel is extracted, not just the new API.

Explicit audit threads before claiming this source slice complete:

- Writer callback calls must cover actual copies/default/reference/index/manifest/diagnostic
  allocations before construction and preserve exact bounded failures. Source output retention
  must fit its admitted scratch model. Lifetime/trait-object callback plumbing has not compiled.
- Exact reverse-patch equality may expose lawful normalization; explicitly bound/refuse rather
  than adding a patch-equivalence interpreter or clearing retained history.
- Source Work cannot know uncalled sources in a future Workspace. Whole-open-set peak requires
  admitting every incumbent before its first candidate. Reviewer is assessing a precise private
  registration seam versus documenting the current source scope until Workspace integration.
- The new source tests file is not yet guaranteed present. Do not treat missing symbols/modules
  in a transitional writer state as reasons to revert the design.

After qualified source ownership, continue the plan's bundle live-leaf successor, actual dependency
closure, whole Workspace guard binding and Session publication/reconciliation. Full foundation,
metadata adoption and installed lifecycle remain active and incomplete.

# Citations

- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0026](../../../../architecture/adr/0026-editor-command-patch-and-undo-model.md)
- [Previous qualified source](../../verification/2026-10/2026-10-28T014807Z-recipe-schema-refresh-preserves-independently-admitted-owner-lineage-8afcd1ede3dc4451ac719a1a236e9c85.md)
