---
type: "Verification Evidence"
title: "Complete source-owned Prefab conversion and current stable lookup qualification"
description: "Qualifies nominal owned source conversion, same-history replay, actual semantic dependency fencing and fresh Host stable lookup validation; Editor conversion custody and native interaction remain open."
timestamp: 2026-10-27T04:17:18Z
record_id: "cbce190af0c4488b97895e68905573a6"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-27-prefab-conversion-plan.md"
git_branch: "refactor/prefab-convert-to-local"
git_commit: "989793c"
supersedes: "c55e3532dbfd4367990424c40afe75c1"
---

# Verification

This successor qualifies the complete source-owner conversion at `58a3eea` and the current
Authoring catalogue correction at `989793c`. It replaces the predecessor's manually localized
history fixtures as the only conversion evidence. The full foundation Goal remains active.
The Editor conversion command, contextual operation custody and native interaction are still
unimplemented. No remote operation occurred.

# Result

- Scene and nominal Prefab sessions mint owned conversion requests from their actual admitted
  revision, immutable document and immutable classification state. The requests can move to an
  existing background task. Shared static preparation serves both the request and synchronous
  editing; there is no temporary session, synthetic source incarnation, second history or dry run.
- The same staged compiler selects the actual anchor's complete semantic projection. A healthy
  instance can be localized without reading an unrelated broken instance or an independent link
  on an ordinary local child. Source, addition and nested origins with repeated terminal names
  receive distinct fresh persistent IDs. Declared internal references use the typed remap;
  untouched local targets and Persistent namespace/UUID meaning remain intact.
- Effective defaults and current Scene-capable data survive without native bindings, creation
  templates or field Edit capability. Current interpreted unclaimed records gain the actual
  admitted Schema-owner claim on their new local copies. Unavailable selected survivors refuse
  atomically. Raw unavailable anchor/child records and unselected declarations remain preserved.
- A move-only nominal token keeps the exact candidate, review mapping, typed effective asset
  observations and all actual semantic dependency requests. No ordinary prepared edit can be
  extracted. Commit rejects a changed root incarnation/revision/Schema or dependency key,
  authority, body or disappearance, preserving the provider's concrete error. Equal reacquired
  bodies are accepted. Structural dependencies with no surviving entities remain fenced.
- One accepted source history entry owns the exact detach, populated local records and closed
  replay footprint. Undo/Redo uses fixed records and IDs without loading dependencies. Public
  Scene/Prefab cases cover cancellation, background preparation, review, commit, exact replay,
  JSON/RON disk Save/Reopen, stale root/reopened lineage and failure atomicity.
- Existing file facts cannot detect new metadata that makes a stable lookup ambiguous. The
  Authoring catalogue now records the actually used stable requests, source kind and target,
  admits their retention before copying, and re-resolves them in a fresh bounded catalogue
  during source validation. Existing source/Schema receipts remain. New conflicts or changed
  results reject; an unrelated unique metadata file does not reject a healthy lookup. Path-only
  capture keeps its fast path. Unresolved stable results remain provisional and require recapture.

## Review Findings And Disposition

The first localizer review found repeated placement copies of a large ordinary-child ID and
declared-reference planner paths outside the source work allowance. Both are now admitted before
copying. The Reflect adapter reuses the admitted-Schema reference walker and moves leaf paths into
the final plan; it does not introduce a second parser.

The independent source review found two additional P2 gaps. Conversion dependency retention and
loader retention previously used independent minimum ceilings. The wrapper now starts with its
container debit, presents the loader with the actual compiler plus prior captured work, restores
only its own prior debit in that private view, and applies the loader's returned allowance minus
all captured retention. Capturing a new source subtracts only its additional debit. Therefore
`Knew = Lnew - Dnew <= Kold`; repeated aliases do not double-charge and small subsequent loader
growth cannot disappear behind a previous conversion debit. Tight-loader and successive-growth
regressions cover the refusal before retaining handles.

The second P2 was an error-path copy after reference-budget refusal. The localizer now retains
that refusal outside the visitor's path-bearing error wrapper and returns the original report.
The visitor callback does not allocate another selector path after refusing work.

The public Prefab test initially expected the entire old unavailable manifest unchanged.
Insertion of projected roots legitimately changes the preserved local child's predecessor.
The corrected test checks its complete raw record and reviewed child order, compares the exact
manifest for the new topology, and still verifies the complete original manifest after Undo and
the accepted manifest after disk Reopen. No production preservation guard was weakened.

New-ID collision lookup now uses a temporary BTreeSet, consumed before constructing the record
index, instead of scanning all previously generated map values for each new ID. This removes the
quadratic lookup without adding a simultaneous third temporary index or claiming a measured
latency improvement.

Descartes independently rechecked the source fixes and closed the assigned source review with no
remaining concrete P1/P2. Dewey separately reviewed the Host stable-query correction and found no
concrete P1/P2. Both reviews were read-only; execution belongs to the parent. All workers and
reviewers used for this unit are closed.

# Evidence

Cargo ran serially with Rust 1.95.0, `CARGO_BUILD_JOBS=1` and the shared root `target`.
The independent workspace also used that target. Cohorts overlap and must not be summed.

| Scope | Actual result |
| --- | --- |
| Reflect reference-plan admission | `acbd9c5b-3402-494a-8ed0-8a345847a537`: 5 passed, build 9.82s, execution 0.161s |
| Complete Scene/Reflect all-feature cohort | `2c42cba9-1096-40dc-bc5c-c9fb79015e47`: 885 passed, 3 skipped across 55 binaries; build 4m45s, execution 43.419s |
| Final affected source conversion cohort | `474ea1ea-94ea-4021-b640-4817569aa95c`: 33 passed across library/public binaries; build 48.64s, execution 1.078s |
| Host stable lookup/catalogue cohort | `0eb2d5dd-e8bd-47e1-b697-aef5484567ea`: 27 passed, 532 filtered; build 1m54s, execution 5.140s |
| Independent direct Scene and renamed-root Prefab consumer | `38afa23e-ff58-4685-addd-b215fbcfef32`: 2 passed; build 1m00s, execution 0.049s |
| Complete root all-feature library | `132090b9-0bcb-4fdb-ae50-d3f371c05a48`: 556 passed, 3 skipped; build 0.86s, execution 156.641s |
| Workspace all targets/features | `cargo +1.95.0 check --workspace --all-targets --all-features --locked`: passed, 1m29s |
| Minimal Scene library | `cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked`: passed, 1.47s |
| Reflect/Scene correctness Clippy | `cargo +1.95.0 clippy -p nara_scene -p nara_reflect --all-features --all-targets --locked -- -D clippy::correctness`: passed, 43.90s |
| Documents and formatting | Root `cargo fmt --all -- --check`, changed/staged diff checks, 724 local ledger/evidence links and memory validation pass; derived rollups are current |

The combined 885-case run preceded the final collision-index change and one additional
tight-loader regression. The final affected source cohort, independent consumer, root library
and workspace checks cover the final production implementation. Non-correctness Clippy warnings
remain, including large report/error types and style suggestions; this is not a clean-warnings
claim. The existing skipped cases were not executed.
Memory validation retains its existing legacy metadata/absolute-path warnings; historical
records were preserved.

No native Editor conversion trajectory, resource execution, performance benchmark, RSS or
allocation-instrumentation result is claimed. This source/lookup unit did not add a native
conversion command. Earlier native evidence retains its original source and feature scope.

# Follow-up

Continue the full rebaseline through the complete Editor conversion workflow:

1. Extract the concrete actual-demand Prefab acquisition from `capture_scene` for reuse by
   Scene/Prefab conversion. Keep one compiler and explicit nominal root inputs. Do not discover
   demand with raw inventory, activate a native preview or add an erased provider/document store.
2. Retain the actual source request/token in one bounded Host operation. Capture the exact root
   query/source receipt and actual File versus WorkingCopy participants. After review, revalidate
   current file/Schema/catalogue facts and participating lineages; unrelated open-document edits
   remain outside that fence.
3. Route the original nominal token through Workspace source publication. Reuse the existing
   history/report; do not extract a patch and prepare again. If validation runs in the background,
   initial dispatch is pending and final publication has a separately correlated terminal result.
   Keep task custody through physical return on cancellation or Close.
4. Integrate explicit complete-conversion wording, bounded mapping/diff review, cancel and exact
   result in the Editor, informed by the existing Unity/Godot study. Qualify public Host behavior,
   source Save/Reopen/Undo/Redo and selected actual native hierarchy/input/pixel trajectories.

The catalogue correction above serves existing authoring capture. It does not by itself connect
that physical evidence to the new conversion token or prove its Editor custody.

# Citations

- [Conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source requests and tokens](../../../../../crates/nara_scene/src/authoring/prefab_conversion/request.rs)
- [Localizer](../../../../../crates/nara_scene/src/authoring/prefab_conversion/localize.rs)
- [Dependency snapshots](../../../../../crates/nara_scene/src/authoring/prefab_conversion/localize/dependencies.rs)
- [Public source workflow](../../../../../crates/nara_scene/tests/prefab_conversion.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/prefab_conversion.rs)
- [Current stable catalogue](../../../../../src/project_host/authoring/project/catalogue.rs)
- [Predecessor](2026-10-27T022733Z-selected-prefab-compiler-and-scoped-source-replay-private-qualification-c55e3532dbfd4367990424c40afe75c1.md)
