# Preview Projection Lifetime

Date: 2026-10-24. Scope: R5c disposable Scene projection, not the entire foundation rebaseline.
The [design](../architecture/editor-preview-projection-lifetime-design.md) fixes alternatives,
ownership proof, entry workload and stop conditions before candidate measurements.

## Frozen Incumbent

Three serial optimized processes ran the actual `EditorAuthoringRuntime` with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, the shared root target, and features `presentation-2d,tooling,serde` without
default features. The fixed App still owns real tasks, images, Scene, Render and SpriteRender;
these Transform-only measurements do not exercise a native window or pending image work.

Hardware: Windows 11, Intel Core i9-13900KF (24 cores / 32 logical processors), 68,400,455,680
bytes reported physical memory. Ambient local contention was not isolated. Each population has
two warmups and sixteen retained alternating scalar-edit publications. P50/P95 use nearest rank;
sixteen samples do not establish P99. Source edit, owner setup, publication and retirement have
separate logged intervals. Publication includes the actual zero-duration Runtime drive and
candidate capture. The full-owner control rotates every eight publications, not every edit;
successor publication is checked while the predecessor remains alive.

Raw observations and metadata are immutable incumbent artifacts in
[`data/2026-10-24-preview-projection-lifetime`](data/2026-10-24-preview-projection-lifetime).
All 255 JSON observations report exact acceptance; both ignored experiments passed in each
process. The source HEAD and dirty-tree scope are recorded in `baseline-metadata.json`.

| Population | Process | Incumbent publication P50 / P95 (ms) | Full-owner control P50 / P95 (ms) |
|---|---|---|---|
| 64 | 1 | 0.778 / 1.128 | 0.753 / 0.902 |
| 64 | 2 | 0.759 / 0.941 | 0.772 / 1.100 |
| 64 | 3 | 0.740 / 0.987 | 0.740 / 0.948 |
| 1,024 | 1 | 13.986 / 14.908 | 13.804 / 14.880 |
| 1,024 | 2 | 14.146 / 14.734 | 13.867 / 14.407 |
| 1,024 | 3 | 13.940 / 15.207 | 13.500 / 14.419 |

The incumbent reaches 1,235 and 19,475 claims after nineteen publications at 64 and 1,024
entities. The control caps history at eight publications: 520 and 8,200 claims. Its setup and
retirement are additional owner-reconstruction costs, not subtracted from or hidden inside the
publication table. It is not selected for production because this workload does not prove
continuity of the Runtime's pending filesystem/task/image responsibilities.

The four-entity / fifteen-claim witness admits three incumbent publications and rejects the
fourth, preserving last-good lookups and the exact old ledger. A fresh full owner then progresses.
This is an observed capacity failure, not a throughput extrapolation or data corruption claim.

## Candidate And Qualification Boundary

The selected candidate for integration qualification keeps the same World and authoring Runtime,
but uses a Scene-owned fresh identity epoch after proving complete predecessor ownership. It does
not increase the claim limit or clear a continuing domain's history. Candidate measurements must
use this exact feature/profile/workload and compare against the frozen incumbent, not relabel the
new default as the old implementation. The predeclared gross stop condition is publication P95
above twice the incumbent in all three matched process comparisons for a population.

Candidate measurement, full public editing/persistence and native presentation/picking evidence
are not implied by these baseline results. Allocation bytes, pending image/Save continuity and
complete frame-tail budgets require their own tests or collectors.
