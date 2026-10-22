# Stable-ID delivery catalogue measurement

## Scope

This bounded measurement qualifies the catalogue work in U9 of the
[workflow boundary plan](../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md).
It runs the real `ProjectContentLoader::load_delivery` and public `check_project` paths. The
attributed correction retains one private catalogue for a delivery operation. It introduces no
persistent asset index, cross-operation cache, or public measurement API.

The focused harness is
[`measurement_tests.rs`](../../src/project_content/catalogue/measurement_tests.rs).
Counters exist only under `cfg(all(test, feature = "project-workflow"))` and observe the
synchronous work on the current test thread:

- A catalogue build starts when an operation first needs complete stable-ID discovery.
- A directory enumeration counts a successfully opened enumeration, including an empty directory.
- Directory entries count returned entries and rejected-entry facts, across all batches.
- Identity metadata reads count successful `index_metadata` reads before identity decoding.
- Metadata revalidations count comparisons with previously catalogued metadata during admission.

These counts exclude manifest/schema reads, Scene and Prefab source reads, relative path traversal
opens, and filesystem implementation internals. The loader's existing budget snapshot separately
reports peak reserved directory entries, files, and work bytes. Peaks are not cumulative work or
allocator/RSS measurements. The public Check creates its own loader; this harness reports no
budget snapshot for that separate loader.

## Workloads and procedure

Both fixtures declare every Scene for delivery, give every Scene one shared stable-ID Prefab and
one distinct stable-ID Prefab, and store additional unreferenced valid Prefab identities in groups
of 32. Stable-ID lookup scans the configured Assets and Prefabs directories. Unreferenced catalogue
members must not become delivery members. The Assets directory is empty; this measures catalogue
scaling without image decoding, GPU work, custom domains, or nested Prefab expansion.

| Fixture | Declared Scenes including startup | Metadata identities | Prefab subdirectories | Delivery members |
|---|---:|---:|---:|---:|
| Tiny | 2 | 4 | 1 | 10 |
| Larger | 12 | 256 | 8 | 40 |

Fixture generation and compilation are outside timing. Loader timing starts with an already
ingested manifest and resolved recipe and includes runtime schema agreement plus the complete
multi-Scene delivery. Public Check timing additionally includes its own manifest ingestion and
recipe/schema resolution. Each workflow uses three serial samples; each sample creates a fresh
loader and drops its delivery receipt before the next sample. The same filesystem fixture is reused,
so OS filesystem caches are uncontrolled. The report uses observed minimum, median, and maximum;
three samples do not establish percentiles or a stable performance distribution.

Run from the repository root, with no other Cargo command in this checkout:

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo nextest run -p nara --features tooling,project-workflow,desktop-winit,render-wgpu --lib --locked --run-ignored all -E 'test(catalogue::measurement_tests)' --success-output immediate --test-threads=1
```

The ignored timing test emits one `CATALOGUE_MEASUREMENT` JSON line per fixture/workflow.
The ordinary regression uses the same loader after a successful operation, then:

1. Changes a selected Prefab and requires a different delivery/source digest.
2. Captures an index, changes valid metadata bytes, and requires admission to reject the mismatch.
3. Performs a fresh load and requires the changed metadata digest to be observed.
4. Adds an unreferenced duplicate stable identity and requires both the warmed loader and public
   Check to reject the complete operation, with no retained loader reservations.

## Results and limits

Serial verification passed on Windows 11 using the existing checkout target. The test emitted the
following minimum, median, and maximum elapsed times from three samples (seconds):

| Fixture | Workflow | Minimum | Median | Maximum |
|---|---|---:|---:|---:|
| Tiny | `load_delivery` | 0.054 | 0.059 | 0.061 |
| Tiny | `check_project` | 0.064 | 0.066 | 0.081 |
| Larger | `load_delivery` | 27.866 | 56.727 | 99.897 |
| Larger | `check_project` | 6.884 | 46.002 | 54.418 |

The large-workload spread is substantial and the sample count is too small to establish a stable
latency distribution. This is local evidence, not a performance guarantee or release claim. In the
larger `load_delivery` run, the loader's peak budget reservations were 2,082 directory entries,
261 files, and 393,316,628 work bytes. These are budget-accounting high-water marks, not measured
process memory or allocator usage. Public Check does not expose its internal loader budget snapshot.

Before the correction, the source-derived counts were:

| Fixture, each workflow | Catalogue builds | Directory enumerations | Directory entries | Identity metadata reads | Metadata revalidations |
|---|---:|---:|---:|---:|---:|
| Tiny | 2 | 6 | 18 | 8 | 4 |
| Larger | 12 | 120 | 6,240 | 3,072 | 24 |

The former source created a fresh catalogue for every root, making complete discovery proportional
to declared Scene roots times catalogue entries. These deterministic counts justified sharing the
captured catalogue within one operation.

## Operation-scoped correction

`load_delivery` and Check now carry the first complete catalogue through the remaining roots.
Discovery remains lazy: startup without stable references does not force a scan. A standalone
`load` or `load_scene`, and every fresh delivery or Check, still discovers its own current sources.
Decoded snapshots retire between roots. Only the operation retains the complete index, with its
own budget ticket; each Scene's index/database copy is charged before allocation. Neither the
loader nor the returned delivery receipt retains that index.

The captured catalogue rejects malformed metadata and duplicate identities at discovery. Each
referenced metadata source is reread and compared with its captured digest during admission, and
shared delivery members must retain matching digests across roots. An unrelated duplicate added
after discovery does not revise that operation's catalogue; the next operation discovers and
rejects it. This is an operation snapshot contract, not an atomic filesystem snapshot or a final
whole-directory rescan.

The deterministic regression now asserts these counts for both workflows:

| Fixture, each workflow | Catalogue builds | Directory enumerations | Directory entries | Identity metadata reads | Metadata revalidations |
|---|---:|---:|---:|---:|---:|
| Tiny | 1 | 3 | 9 | 4 | 4 |
| Larger | 1 | 10 | 520 | 256 | 24 |

The five ordinary regressions passed in nextest run `c6cdd26a-54ed-4d81-b26b-04215ec3a3c0`.
They cover equal Check/delivery members and digests, later-root lazy discovery, changed referenced
metadata between roots, fresh-operation duplicate detection, and capture/copy budget rejection
with zero remaining reservations. The 30 public content boundary/delivery tests also passed in
run `ef7573f5-5e38-4235-b785-21ef3807a4c1`.

A separate ignored timing run used the same harness with `--features project-workflow` and
`--run-ignored only` (run `5f273b82-650e-4859-9253-473bd21921e0`):

| Fixture | Workflow | Minimum | Median | Maximum |
|---|---|---:|---:|---:|
| Tiny | `load_delivery` | 0.132 | 0.141 | 0.167 |
| Tiny | `check_project` | 0.089 | 0.152 | 0.157 |
| Larger | `load_delivery` | 1.538 | 1.675 | 2.217 |
| Larger | `check_project` | 0.885 | 1.677 | 1.793 |

Elapsed values are seconds. The follow-up larger delivery reserved peaks of 2,082 directory
entries, 261 files, and 393,548,257 work bytes. The retained operation index increases the work
reservation relative to the former 393,316,628-byte peak. These remain accounting values, not RSS.
The runs used different feature unions and uncontrolled machine/filesystem conditions; tiny
timings increased and earlier larger timings had wide variance. The defensible improvement is
the eliminated repeated discovery work, not a fixed latency ratio or tail-performance claim.
