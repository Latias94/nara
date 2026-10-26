---
type: "Session Handoff"
title: "Shared Prefab compiler owner and Host verification before staged structural-demand replacement"
description: "Shared Schema/Host integration has bounded passing evidence; one production structural-demand regression is intentionally unresolved, with the staged replacement selected."
timestamp: 2026-10-26T16:33:04Z
record_id: "45d427ddb20d40b0acde05d288566775"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/declarative-prefab-overlays"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
---

# Summary

Full foundation Goal remains active. Base main `47a2000`; branch
`refactor/declarative-prefab-overlays` contains the large authorized uncommitted replacement.
No commit, merge, push, PR or native qualification in this replacement. Existing untracked
`vendor/` is unread and untouched. Local date is 2026-10-27, Asia/Shanghai; this record uses UTC.
All owned Cargo handles are terminal and all newly used agents are closed at this checkpoint.

# Verified State

- Scene run `6958b95c-54d9-4162-8cd5-0f73e46d8308`: 390 executed, 389 passed, one failed,
  zero skipped, across ten binaries. The sole failure is the new production cross-instance
  placement/suppression source-demand regression described below. Format cost, exact source
  repair/history, basis identity, data-only compilation and reference-work boundary cases pass.
- Reflect run `1c85cb78-c50e-40ed-9cc7-c5d6cae4ee71`: 291/291 passed, zero skipped,
  across 24 binaries. This includes the owned property replacement and new admission callback.
- Actual Host run `57dee744-a4d9-4151-965d-e16234bac618`: 47/47 selected tests pass,
  434 filtered out. Filter is `test(project_host::authoring::) |
  test(project_content::tests::runtime_content_)`. This is not full root/workspace/native evidence.
- After a small feature-gating cleanup, Scene no-default library check passes (0.94 s, two
  pre-existing dead-code warnings across Reflect/Scene), and focused shared-authority/native
  loader/reference work tests pass 5/5, 297 filtered, run
  `5767ba6f-e9a4-4b49-8598-539d0f40b3db`.
- `cargo fmt --all` ran after writer freeze and again after final fixes. No runtime source contains
  `R5a/R5b/R5c` or matching R-number phase labels in `src`, `crates`, `editor`, `examples`.
  No broad current Clippy or independent external consumer/native rerun exists yet.

The agreed Reflect hidden API now exists:

```rust
collect_declared_effective_asset_references_with_admission(
    schema, version, effective_value, traversal_limits,
    |additional_nodes, additional_logical_bytes, additional_allocation_bytes| -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>>
```

It reuses the reference walker, borrows normalized values, does not fill defaults, admits
scratch/path/field-ID/parsed-string/output allocations before copying, preserves exact `E`, and
charges failed/empty scans. Output Vec slots are included; Scene separately admits global Vec
capacity growth. Scene has independent document-wide `reference_work_nodes/bytes` limits,
not aliases for materialized-value size. Authoring retains assets only after whole-graph success.
Native-preview omission conversion pre-admits full typed ancestry copies.

Actual Authoring startup capture and indexed Open retain one `AuthoringSchemaBasis`. The compiler
requests selected sources rather than raw inventory. Published Prefab backing remains retained
without a second raw document clone. Host dynamic caps account for suspended work and source
decode/cache growth. Source identity, parent identity, content digest and catalogue lookup
metadata are rechecked; manifest and Schema bundle now also cross the same final bounded fence.
The actual-file test changes each basis input after preparation and observes typed rejection.
Source-change diagnostics keep the relative path; compilation budget failures keep their original
structured report in Editor Open.

Two Host fixture assumptions were corrected rather than weakening production checks: identities
must be compared using `snapshot.project_root()` in the original capability session, and Windows
exclusive sharing failure is OS code32/Uncategorized here, not guaranteed PermissionDenied.
The test now compares the exact kind/raw code to an independent direct native-open control.
Removed the obsolete unused Runtime raw-catalogue completion helper.

# Open Threads

1. **P1, executed:** `compile_tests::
   outer_suppression_after_a_cross_instance_move_does_not_request_the_discarded_instance`
   fails with actual requests `t, p, a, discarded, b`. Scene `host -> T` suppresses
   `through(m,P).b`; T's `m -> P` overlay moves `through(a,A).q` below `through(b,B).r`;
   P contains a/A and b/B; A's q instances discarded; B contains r. Correct structural closure
   is T/P/A/B. q/r/b die after layered layout, so discarded must never be requested. The test
   retains both existing/missing discarded-source controls; do not skip or weaken it.
2. **P1, traced:** native normalization/default/reference remap happens before outer complete
   component Replace/Suppress can discard the old payload. Earlier Host canonicalization and
   standalone Authoring classifier gates can reject raw direct payloads even before the compiler.
   Compiler-only repair cannot close this. Properties do not promise arbitrary invalid-base repair.
3. **P2, independent static review:** new effective asset collector is metered, but final
   `validate_component_value_with_limits` and legacy `validate_effective_local_references` still
   revalidate descriptors outside it. Repeated empty `List<Record<1024 scalar members>>` values
   cause descriptor scans/scratch despite zero references. Reuse admitted descriptor/index facts
   or route remaining work through owner admission; do not claim full pipeline budgeting from
   collector tests. Review by Plato was read-only, no executed counterexample yet.
4. Authoring actual native nested preview still uses the older direct-record omission path.
   `into_native_preview` preserves provenance and works in direct compiler tests, but Host preview
   capture/generation/revision fencing is not integrated. Startup closure cache is insufficient
   for a new or edited second Scene. Unsaved source lineage/revision, structured conflict
   observation/discovery and source-owned convert-to-local remain required.

The selected replacement is documented in `prefab-overlay-design.md` under Selected Staged
Replacement. Heisenberg independently supported it read-only; no staged engine implementation
was added yet. Owning ADRs 0043/0090 now distinguish raw composition admission from standalone
editable publication; their implementation remains incomplete.

# Next Action

Implement the actual staged compiler locally (critical path), reusing existing layout/value helpers:

1. Context-owned occurrences, entity shells, per-layer visibility and deferred instance slots.
   Root/current document is borrowed; resolved sources retain immutable handles. Record/instance
   sites should use source/record indexes plus addition/claim path segments, not borrows into a
   growing Vec and not copies of component payloads.
2. Register an entered source's complete local skeleton; choose outer instance claims before its
   load. Close all active explicit structural/slot target paths before normal nested recursion.
   Terminal resolution does not enter that terminal's instance. Process newly entered overlays'
   own demands, not dormant raw embedded overlays.
3. Run layered layout inner-to-outer, preserving declaring Roots / NestedRoots(anchor) /
   Children(anchor). Reuse layout's algorithm rather than a second interpreter. Remove
   `SourceSelection`/`destination_suppressed` authority. Preserve occurrence facts when a deleted
   anchor has a rescued descendant, while deleted terminals remain unavailable in layer outputs.
4. Enter only remaining live deferred slots. Any ancestor structural intent reaching inside must
   already have demanded its path. A bounded local fragment rebuild is acceptable before proving
   late grafting; do not create a generic incremental graph framework.
5. Compose component presence separately from demand: raw Source/Set plus ordered Properties
   suffix, with each batch's actual declaring namespace. Add/Replace/Suppress existence and
   duplicates are always checked; later whole replacement/suppression drops earlier value
   interpretation. Only retained programs' bindings can demand sources. Do not leave raw binding
   traversal or layout binding checks as an eager second demand owner.
6. Use internal node locators while evaluating intermediate values. A reference valid in an inner
   layer can be overwritten later even if its target is eventually deleted; avoid forcing a final
   surviving ID for every intermediate reference. Finally remap declared references to the complete
   surviving output-ID map and reject actual dangling final references. Preserve typed ancestry
   independently of physical reparenting.
7. Replace composition's early standalone publication with bounded raw source capture retaining
   actual key/basis/IO receipts; strict independent Open stays separate. Do not label raw sources
   as Published/Available or add fake bindings. This needs real Runtime/Authoring file regressions.

Then finish second-document capture and nested preview/fences, conflicts and conversion; run
independent review, no-default/all-feature owner suites, external consumers/reference game and
appropriate native tests. Commit only complete coherent verified units; local ff main is authorized,
remote publication is not. The full foundation Goal must not be reduced to this Prefab slice.

Temporary logs under `$env:TEMP`: `nara-overlay-scene-qualification.log`,
`nara-overlay-reflect-qualification.log`, `nara-overlay-host-qualification.log`,
`nara-overlay-shared-authority-final.log`. No log file is a substitute for the scope above.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Selected phased compiler and open counterexamples](../../../../architecture/prefab-overlay-design.md)
- [Prior integration checkpoint](../../progress/2026-10/2026-10-26T155605Z-shared-authoring-prefab-compilation-and-actual-demand-capture-integration-checkpoint-97e0cc8be0f24b7e8856ea8f88e7dbc4.md)
