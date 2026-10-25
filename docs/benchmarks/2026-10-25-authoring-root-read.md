---
title: Selected Authoring Root Read Baseline
date: 2026-10-25
---

# Selected Authoring Root Read Baseline

## Scope And Source

This diagnostic characterizes one public `EditorWorkspace::read_document_root` operation, not
Application frame publication, renderer latency or full R2 qualification. The uncommitted source
is based on `7f2cafa`; its 16-Rust-file LF manifest is
`d25ce513868bc698bd0239cebd680383cc9522447a5f071dbc9009a83288012a`.
The query/source implementation and the exact workload are in
[the integration test](../../crates/nara_tooling/tests/authoring_queries.rs).

Environment: Windows 11, Intel Core i9-13900KF, explicitly selected Rust 1.95.0, debug test profile,
root `target`, `CARGO_BUILD_JOBS=1`. No exclusive-core, scheduler, power-state or cache isolation
is claimed. Nextest run `39ebfcfa-f0b2-49cf-90be-cf917fa31c81` executes the ignored diagnostic.

## Workload And Timing Boundary

Each Scene/Prefab has three fixed entities and 256, 4,096 or 16,384 additional empty entities.
The requested `amount` root contains U64(7), alongside a 10,000-byte String root, a small List and
other optional roots. An unavailable component and a separate admitted zero-root component are
present. The zero-root component uses a matching trusted-provider authoring receipt: the finite
data Adapter deliberately does not admit empty-field Schemas. This is not native materialization
or executable replacement evidence.

The exact basis/request is captured before timing; constructing a complete workspace model is
not in the timed loop. Each case runs eight warmups and 64 individual samples. The timer includes
cloning the fixed request, exact basis/source lookup, selected-root interpretation, bounded cost
admission and the returned root copy. Assertions, result destruction, fixture creation, source
decode/classification, Schema admission, persistence and normal Application drive are excluded.
The returned payload costs one value node / eight logical bytes; returned UTF-8 target metadata
costs 19 bytes. The request permits 32 nodes, eight logical payload bytes and 1,024 metadata bytes.

Samples are sorted; P50 is element 31 and P95 element 60 (zero-based). The observed 100 ns steps
and single-process fixed case ordering limit interpretation of such small samples. This is a
baseline, not a stable tail estimate, release-performance comparison or allocator/RSS bound.

## Observation

| Source | Entities | Read P50 (ns) | Read P95 (ns) |
| --- | ---: | ---: | ---: |
| Scene | 259 | 900 | 1,000 |
| Scene | 4,099 | 900 | 1,000 |
| Scene | 16,387 | 1,000 | 1,100 |
| Prefab | 259 | 900 | 1,000 |
| Prefab | 4,099 | 1,000 | 1,100 |
| Prefab | 16,387 | 1,000 | 1,100 |

The observation shows no obvious linear full-document-scan slope for this selected scalar read.
The owning implementation uses canonical sorted entity/root sequences and direct classification
fact lookup. It does not materialize the component's sibling values. Source inspection and budget
tests establish that mechanism; timings alone do not prove a memory bound or complexity theorem.
Path lookup still constructs diagnostic path prefixes, so this is not allocation-free.

For a subsequent change to this same workload on this host, twice each recorded P95 is a private
investigation trigger (2,000 or 2,200 ns), not a failing test, native SLA or permission to weaken
admission. Repeat noisy observations before attribution. Deep paths, large selected roots, index
maintenance, output disposal and Application/frame occupancy need their own attribution.

## Ergonomic Comparison

The complete independent Application consumer now discovers actual components and field IDs,
reads an editable integer, computes its replacement and submits an existing prepared property
edit before Undo/Redo and Save/Reopen on non-active Scene/Prefab sources. It copies no creation
catalog owner/template metadata and requires no engine-private getter or active Inspector switch.
Its query actions use the same owned action/outcome routing as registered panels.

The previous Application consumer supplied already-known replacement values after entity-ID
enumeration. It was not a complete discover/read/computed-edit control. Consequently this report
does not manufacture a before/after speedup or claim fewer calls than an equivalent old workflow.
The new interface deliberately separates metadata discovery from selected value copying, adding
an explicit read rather than coupling discovery progress to an oversized sibling.

See the [design and alternatives](../architecture/authoring-read-projection-design.md) for the
remaining query, panel and broader foundation qualification boundaries.
