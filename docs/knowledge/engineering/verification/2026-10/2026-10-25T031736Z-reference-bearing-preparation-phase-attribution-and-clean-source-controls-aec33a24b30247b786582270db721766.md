---
type: "Verification Evidence"
title: "Reference-bearing preparation phase attribution and clean-source controls"
description: "Five executions locate populated instrumented preparation wall time in format admission; explicit disturbance limits, corrected analyzer and exact source cleanup retain full R2 as open."
timestamp: 2026-10-25T03:17:36Z
record_id: "aec33a24b30247b786582270db721766"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "5ebe751cffeb34a93dbe0e023e0db863b5e638b7"
---

# Verification

This is the next bounded R2 diagnostic after prepared-panel commit `5ebe751`. It changes no
production implementation or acceptance policy. The
[report](../../../../benchmarks/2026-10-25-reference-preparation-phases.md) and
[retained context/data](../../../../benchmarks/data/2026-10-25-reference-preparation-phases/context.json)
own complete results, hashes, commands/workload and limits; this record preserves continuity.

# Result

- Actual reference-bearing Scene/Prefab property preparation takes local classification while
  the finite format shape certificate declines. All 1,152 observed owner calls, including warmups,
  have those exact branches across two instrumented executions.
- In 257-entity cases, format admission occupies median within-call shares of 97.15-97.53% of
  public preparation wall time. Classification/manifest occupies 1.37-1.77%. JSON/RON parser
  preflight and both encodings remain whole-document work. This supports investigating the
  format owner before replacing classification storage, history, ECS or task execution.
- Top-level phases meet 90% aggregate attribution in all cases (minimum 94.77%); all populated
  cases exceed 99.87%. One single-entity Direct Scene call covers only 79.39%, so not every
  adapter call is fully attributed. No observation is trimmed or silently reassigned.
- Absolute disturbance is not isolated. One instrumented populated Prefab median exceeds its
  original baseline by about 19.15%. Same-binary disabled and byte-restored/recompiled controls
  investigate the trigger but do not prove zero observer effect. No speedup, regression, exact
  uninstrumented phase percentage, CPU time, native/release latency or full R2 completion follows.

# Evidence

## Five Serial Executions

Rust 1.95.0, Windows/i9-13900KF, debug + debuginfo, shared root target,
`CARGO_BUILD_JOBS=1`, one test thread; machine load/isolation is uncontrolled:

| Execution | Nextest run ID | Actual result |
| --- | --- | --- |
| Original-source baseline | `0691f176-bcdd-4d5b-8255-d2c6ef70001a` | Existing diagnostic passes |
| Instrumented 1 | `0dc6e1b9-6ca9-4415-9b84-376f3266bb43` | Diagnostic passes; 576 owner rows / eight cases |
| Instrumented 2 | `40a91949-7aa8-4431-9328-8aba1ae592e9` | Diagnostic passes; 576 owner rows / eight cases |
| Same-binary recording disabled | `0ee35f6b-7783-4e49-8d2b-a3e668ffc61d` | Diagnostic passes; no owner rows |
| Original bytes restored and recompiled | `a474bfff-7faf-4e67-a58c-ebcebe77466e` | Diagnostic passes; no probe/extra sample output |

Each execution is the same ignored public diagnostic: eight cases, eight warmups and 64 changed
values per case. Every entity has a real Link(reference, amount) plus an unknown record. Applied
results, one history entry, value readback, retained non-active navigation, final Undo/Redo and
unchanged source bytes are asserted. The diagnostic is not a CI timing gate or a native widget test.

Temporary spans measure lowering, clone/field preparation, whole format admission including its
envelope destruction, classification plus manifest comparison, and history admission. Child spans
measure canonical-envelope construction, certification, complete JSON/RON and encoding/preflight.
They are nested, not additive to their parents. Rows buffer in thread-local state and print at
thread exit, not during measured calls. Public samples print after the case, before sorting.

An independent apparatus reviewer found one P2 analysis gap: same-size Direct/Panel labels could
be swapped without detection. The corrected analyzer now checks all eight exact case tuples and
sequence/kind/count/nested-time constraints. A deliberately swapped input is rejected before any
result is emitted. Both unchanged raw instrumented logs were reanalyzed; no runtime rerun is
claimed for that parser-only correction. All actual owner calls and all timed samples are retained.

Public-minus-Scene includes probe bookkeeping and normal adapter work; parent-minus-children is
not isolated destruction cost. The source review identified no successful-path admission, owner,
result or production-object destruction-order change. The complete raw logs, patch and corrected
analyzer are archived as deterministic gzip, with compressed and raw SHA-256 values in context.

A fresh independent results reviewer recomputed all retained metrics and both report tables,
verified all branch/coverage/parent-child constraints, independently rejected label-swapped inputs
without result writes, and checked source restoration, patch reconstruction and archive hashes.
No new actionable issue remained. The same-binary claim has unchanged-source/no-recompilation
log evidence, not an archived executable digest or independent binary authentication. Both Codex
reviewers completed and were closed; no cross-provider independence is claimed.

## Exact Apparatus Removal

The working tree started with no tracked changes at `5ebe751`; untracked `vendor/` was excluded.
Seven original source files were backed up with hashes before edits; the only created source was
private `crates/nara_scene/src/preparation_cost_probe.rs`. Before removal, every instrumented
hash and the complete expected tracked-path set was checked. Only this run's edits were replaced
from their original bytes, and its own hash-verified new module was removed. No Git restore,
reset, stash, clean, recursive deletion or unrelated file mutation was used.

Every original file matches its pre-experiment raw bytes. The archived patch forward-checks
against restored source. The restored run log confirms compilation of Scene and the public
consumer; its diagnostic passes. No Cargo handle remains live. Subsequent delivery is evidence
and documentation only, so the preceding production qualification retains its original scope.

# Follow-up

Test a finite reference-shape certificate at the format owner against the unchanged full oracle
before adding retained per-field summaries or encoded fragments. Keep exact bounds, canonical
bytes, unknown preservation and source/history semantics. Passing that comparison would remove
only parser-preflight work; both full encodings would remain. The report records zero preflight
calls for selected admitted samples and a matched populated debug median at most 60% of control
as future experiment criteria, not accomplished results or native/CI latency promises.

If encoding remains material, compare a conservative format-owned accept-only byte proof with
full fallback before introducing stateful cached summaries. The deliberate editing/Save/repair
contract alternative remains open and needs owning ADR reconciliation, not a stealth admission
relaxation. No new proof, optimization, permanent profiling API or policy is implemented here.

Continue the complete dependency-ordered foundation plan. Full R1/R2, remaining R3-R12 and the
Goal remain open; this diagnostic does not finish an entire wave. No push, PR, migration guide or
vendor change belongs to this unit.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Precommitted experiment entry](../../../../benchmarks/2026-10-25-prepared-panel.md#owner-phase-experiment-entry)
- [Editing versus persistent admission](../../../../research/2026-10-25-editing-versus-format-admission.md)
- [Prepared-panel qualification](2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md)
