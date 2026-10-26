---
type: "Work Progress"
title: "Shared authoring Prefab compilation and actual-demand capture integration checkpoint"
description: "Uncommitted shared compiler and Host capture integration; structural-demand and early semantic admission counterexamples remain open."
timestamp: 2026-10-26T15:56:05Z
record_id: "97e0cc8be0f24b7e8856ea8f88e7dbc4"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/declarative-prefab-overlays"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
---

# Summary

Continue the complete foundation Goal. Base main is still `47a2000`; the production overlay
replacement remains a large authorized uncommitted change. No commit, merge, push or native
qualification has occurred. Existing untracked `vendor/` is untouched and unread. User briefly
paused and resumed the Goal; it remains active with its complete original scope.

# Details

- Native Scene library baseline before the shared authority change passed 290/290, run
  `28de297a-6e46-490b-89a9-d53c492036ed`. Focused eight-binary integration
  `a352a207-9cac-43f8-abe3-3cbf73a2fff2` passed 82/88. Six fixture/contract mismatches were
  corrected with independent cost formulas, exact repair inverses, unrelated unavailable-record
  protection, Save/Reopen and native decode only at activation. These corrections are not yet green.
- The original post-change Cargo handle `1580` was missing after resume; no Cargo/rustc process
  remained. Repeated serial command handle `28718` finished with one test-constructor error
  (`PrefabExpansionContext::new` now takes `SchemaAuthority`); fixed afterward. No test result
  from that repeat is a passing qualification. Temp log: `nara-overlay-scene-qualification.log`.
- Earlier Reflect all-features 281/281 and correctness Clippy passed before the new owned property
  helper and admission callback work. Do not reuse that result for the changed visitor.
- `AuthoringSchemaBasis::admit`, candidate `admit_authoring_with_basis`, published `schema_basis`
  now retain one exact admission across sources. Native and Authoring compilation share a private
  `SchemaAuthority`; Authoring uses actual data Adapters, never fake native codecs/migrations.
  `ResolvedPrefabSource::bound_authoring` retains published allocation and checks exact basis.
- The Host startup capture removes the raw Prefab BFS and source inventory-driven asset reads.
  Compiler callbacks read selected sources, cache them, retain canonical source/metadata receipts,
  and enforce dynamic suspended/resumed work caps. Captured source identity/digest and lookup
  metadata are revalidated before publication. Selected missing files preserve a degraded source;
  syntax, identity, access and budget errors stay typed. Indexed Open requests retain the same
  admitted basis rather than rebuilding one per document.
- `AuthoringPrefabExpansionReport` retains effective assets, omissions, final work bytes and budget
  status. Consuming native preview filtering preserves full provenance. Existing Editor native
  preview is not yet wired to this compiler/capture; it still omits nested Prefabs.
- Property Sets consume the isolated candidate using Reflect's owned replacement helper; the
  public replacement API remains failure atomic. New tests prove untouched String allocation reuse
  with `as_ptr`, selector parity and late-failure candidate isolation. These tests are unexecuted.
- Source/transaction ADRs and navigation now distinguish explicit complete-anchor repair from
  unrelated unavailable data protection, source inventory from actual activation, and adopted
  contract from implementation qualification. Five affected architecture documents have no broken
  local link targets. Historical evidence is not rewritten as current qualification.

The independent reviewer found a concrete structural-demand P1: `host -> T`, T's `m -> P`
places `through(a,A).q` below `through(b,B).r`; the Scene suppresses `through(m,P).b`.
P contains `a -> A`, `b -> B`; A's q instances a missing source. The final result removes
q, yet current DFS asks for q's source first. This is recorded in the design and a new production
regression `outer_suppression_after_a_cross_instance_move_does_not_request_the_discarded_instance`
with both present/absent discarded-source controls. It is expected red, not qualified.

The separate P1 is early value interpretation: outer complete component Replace/Suppress can be
blocked by native normalization/default/reference checks, Runtime source canonicalization, or
standalone Authoring classification of the prior raw payload. Fixing only the compiler does not
remove the Host semantic gate. Do not weaken standalone source admission or add routing heuristics.

At capture, Ampere `01a178dd-22d8-7381-b15a-b7f2236a6bb4` owns only Reflect
`asset_reference.rs`, `entity_reference.rs`, `lib.rs`, and `tests/property_asset_reference.rs`.
It is implementing the agreed hidden effective-value collector with typed admission callback before
reference/path/output allocations, reusing the existing visitor. Parent has written Scene
validation/context integration against that signature; wait for source-ready before Cargo/fmt.
Heisenberg `01a178db-5ac1-7250-8cba-6a725e0895e7` is read-only on a staged skeleton,
structural-demand and deferred-value design. Prior implementation workers are closed.

# Next Action

Finish callback integration, freeze writers, inspect diffs and format. Serial Scene/Reflect then
root Host compilation/tests must provide fresh evidence. Root Host tests include six actual-file
fixtures under `authoring/tests/actual_demand.rs`; parent added two direct shared-compiler/basis
tests in `authoring_schema_lookup.rs`. They remain unexecuted. The cross-instance-demand test must
stay red until its owning phase design is replaced; do not skip or weaken it to obtain a checkpoint.

Then replace interleaved DFS/value interpretation with the selected staged source skeleton,
layout/liveness and component-program mechanism. Reusable second-document capture, unsaved source
revision/closure generation fences, nested native preview, structured conflict discovery and
convert-to-local remain required work, followed by independent review/consumer/native evidence.
No ordinary startup cache can establish closure for a newly edited or separately opened document.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Overlay design and open counterexamples](../../../../architecture/prefab-overlay-design.md)
- [Previous WIP capture](2026-10-26T114409Z-production-declarative-prefab-overlay-replacement-integration-in-progress-b0111148a8964507912918f46ac2dace.md)
