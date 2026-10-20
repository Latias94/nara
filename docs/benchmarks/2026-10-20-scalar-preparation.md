# Scalar Preparation Locality

Status: Implementation `601b4671545f0d6835620e88c7e2e51c7b36895d` passes owner regressions,
three public measurement processes and the separate requested-allocation comparison.
The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T090217Z-scalar-preparation-locality-qualification-7da17674f8c7464ea1612dc119686c45.md)
binds exact source, regression/build results and independent review.

The [selected L1 plan](../plans/2026-10-20-scalar-preparation-locality-plan.md) follows the
[unchanged public interaction baseline](2026-10-20-production-costs.md). Starting product source
is `b9d25dc9688ac6d66edc47f31e3a5882c5e9a07a`. Historical observations remain unchanged.

## Exploratory Attribution

One separate optimized Windows/Rust 1.95.0 process uses a temporary scalar-only entry into the
existing public Editor consumer: 4,096 Transform records, two warm-ups and sixteen edit/Undo
pairs. Real selected rotation, exact history/dirty state, saved bytes and physical session close
remain asserted. No Browser work is included. Temporary private probes bracket candidate clone,
field preparation, full format validation, canonicalization, classification and forward commit.
Format probes separately bracket their nested canonical envelope, JSON encode/parse and RON
encode/parse. Root runs Cargo serially with one build job and the existing shared target.

Probe output occurs inside surrounding owner calls and can affect them. This process is
exploratory attribution, excluded from final public timing comparisons. Its nested inclusive
intervals and independent medians must not be summed or subtracted to reconstruct a complete
call. The forward commit includes history publication and old document/classification disposal;
it is not an independent Undo commit measurement. All probes and the temporary test entry are
removed before final verification; no production timing API, environment switch or logger stays.

| Measured phase | Retained count | Median milliseconds |
| --- | --- | --- |
| Candidate document clone | 32 | 2.902 |
| Field preparation and inverse validation | 32 | 0.008 |
| Complete format validation, inclusive | 32 | 79.587 |
| Candidate canonicalization | 32 | 0.049 |
| Complete classification | 32 | 30.916 |
| Forward history/state publication and disposal | 16 | 3.134 |
| Format canonical envelope, nested | 32 | 6.631 |
| JSON encoding, nested | 32 | 12.983 |
| JSON structural parsing, nested | 32 | 9.220 |
| RON encoding, nested | 32 | 17.336 |
| RON structural parsing, nested | 32 | 31.331 |

Nextest `2e2a25ce-4c9d-41f6-87fd-1a164c3bb7e9` passes. The
[raw phase records](data/2026-10-20-scalar-preparation/initial-phases.jsonl.gz) include warm-ups
and sixteen retained pairs. The [summary](data/2026-10-20-scalar-preparation/initial-phase-summary.json)
describes selection and retains unrounded median/range. This is Scene attribution; Prefab has
additional instantiation/copy work and needs its own correctness and allocation evidence.

## Selected Corrections

- Keep actual JSON and RON serializers and their exact byte guards. For the finite supported
  Scene/Prefab wire grammar, a private conservative accept-only shape certificate can prove both
  parsed shapes fit their limits. Otherwise retain the complete original encode/parse validation,
  including near-limit acceptance and diagnostic order. No unchecked path or generic serializer
  framework is exposed; untrusted file decoding remains unchanged.
- Pure field changes retain entity/component identity, owner/version, topology and namespace.
  Reclassify each affected complete direct Available component using the existing validator,
  including effective defaults, local references and declared asset references. Keep the existing
  exact unavailable manifest and history owner. Structural changes and Reopen use complete
  classification; unprovable field candidates fall back to it.
- Retain owned candidates and rejection-before-publication. Do not mutate the live document and
  roll back later, defer rejection until Save or introduce pending edit-result semantics here.

The certificate supports entity hierarchies, component maps, owner options and primitive/list/map
values through a conservative wire depth of 32. Embedded Prefab instances, standalone patches,
EntityReference values, larger depth or an unproven shape budget use the complete validator.
The bound is an optimization cutoff, not a new input limit. Both real serializers still run;
successful certified validation counts their output bytes without retaining a temporary buffer.
This is not a substitute wire encoder or parser. New wire fields/variants must keep the private
proof and the complete differential oracle consistent.

Field-only classification retains an O(N) state copy, fact scan and namespace construction.
It validates each touched complete record, in the original classification order. Prefab shape
validation and classification move their candidate storage instead of copying it twice more.
No public document representation, interface or history owner changes.

## Final Results

Three unchanged, uninstrumented optimized public Editor processes pass, each retaining 768
numeric rows across the original populations and workflows. The exact collector has no source
changes in this slice. Values below are per-process nearest-rank P50 / P95 milliseconds; with
sixteen samples, P95 is the observed maximum, not a stable tail estimate.

| Population / ordinary operation | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| 1,024 / edit dispatch | 7.973 / 8.664 | 8.380 / 8.969 | 8.693 / 10.817 |
| 1,024 / Undo dispatch | 7.913 / 8.903 | 8.211 / 9.620 | 8.573 / 10.411 |
| 4,096 / edit dispatch | 42.125 / 45.964 | 39.236 / 42.902 | 38.463 / 42.970 |
| 4,096 / Undo dispatch | 42.121 / 44.714 | 37.810 / 41.959 | 37.490 / 39.992 |
| 4,096 / frame drive after edit | 2.390 / 3.459 | 2.284 / 2.759 | 2.164 / 2.617 |
| 4,096 / frame drive after Undo | 2.389 / 3.199 | 2.230 / 3.031 | 2.113 / 2.463 |

The preceding indexed-lookup cohort has edit-dispatch P50 116.843 / 119.726 / 119.096 ms.
The new 38-42 ms level is a substantial reduction while preserving exact field/history/source
oracles. Independent processes are not paired or pooled; ordinary desktop work affects timing.
The remaining 4,096-record dispatch exceeds the local 8 ms investigation trigger. This slice
does not claim instantaneous large-document editing or native input-to-display qualification.

The [summary](data/2026-10-20-scalar-preparation/editor-summary.json) retains all 144 groups,
three run identities, gzip filenames and uncompressed digests. To reproduce a group, sort its
sixteen `elapsed_ns` values: P50 is element 8 and P95 element 16, one-based. Exploratory nested
phase observations remain separate and cannot explain the final gain through percentile sums.

## Requested Allocation Comparison

The [ignored test](../../crates/nara_scene/tests/scalar_edit_allocations.rs) is a separate
executable with a System-delegating allocator. Scene and Prefab each contain 4,096 records with
one ordinary U64 field, two warm-ups and four retained edit/Undo/clone/drop cycles. This simpler
cohort differs from the Transform public timing fixture. No allocation-instrumented timing is
mixed into the public latency results. Inputs, baseline snapshots and schema admission exist
outside the windows; semantic/history assertions and JSON output occur after each window.

Each allocation window measures successful requests, not allocator usable size or process RSS.
Reallocation records the complete new requested size and releases the old size. The live counter
runs from process startup; no App or task worker is created. Unrelated activity is not filtered.
Edit/Undo windows include the returned report; settled checkpoints follow report Drop.

| Owner / edit or Undo window | Before requested bytes | After requested bytes | Before peak above start | After peak above start |
| --- | ---: | ---: | ---: | ---: |
| Scene | 65,772,568 | 22,161,541 | 20,769,901 | 17,098,066 |
| Prefab | 83,917,851 | 22,685,832 | 29,056,110 | 17,098,066 |

Values are identical across the four retained samples for each owner/operation. Scene allocation
calls fall from 455,853 to 96,424; Prefab from 513,204 to 96,425 (reallocations are separately
retained in the raw rows). Settled live-request delta is zero for each edit and Undo. A separately
retained document clone costs 8,286,208 requested bytes before and after; dropping it releases
exactly that amount. These finite observations do not establish complete owner heap, GPU memory,
general leak absence or zero-cost document snapshots.

The baseline is an exact Git archive of `b9d25dc` plus the same measurement test. Both executions
use the shared target, one build job, release profile and Rust 1.95.0. After copying the final
formatted probe into the archive, its SHA-256 matches the current probe. Shared-target reuse
initially selected the baseline artifact for the current checkout; those purported after rows
are excluded. Refreshing the owned archive/current source mtimes without changing bytes forces
current compilation. The retained after log confirms compilation from the current checkout and
the changed allocation population. The [context and complete groups](data/2026-10-20-scalar-preparation/allocation-context.json)
bind archive/probe hashes, implementation commits, two run IDs and both raw gzip digests.

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --release -p nara_scene --features serde --test scalar_edit_allocations --locked --offline --run-ignored only --test-threads=1 --success-output immediate
```

## Remaining Work

Both complete canonical encoders, a candidate document clone and an O(N) classification-state
copy remain. The finite shape proof deliberately falls back for embedded Prefabs, references
and unproven budgets, so the measured improvement is not uniform across all documents.
Future work should attribute the remaining current path before choosing format-owned encoding
summary reuse, touched-record storage or an explicit pending edit/commit interaction. Each
choice must preserve exact dual-encoding limits and synchronous success semantics unless the
owning action contract is deliberately revised. Do not add a generic async trait to hide work.

Browser transport, large Tile visibility and Rust/Play attribution remain separate units in the
[successor plan](../plans/2026-10-20-measured-work-locality-plan.md). No new job kernel, public
document storage layer or Tile authoring tool is introduced.
