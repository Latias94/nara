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
sixteen samples do not establish P99. Source editing is outside the measured intervals; owner
setup, publication and retirement have separate logged intervals. Publication includes the actual zero-duration Runtime drive and
candidate capture. The full-owner control rotates every eight publications, not every edit;
successor publication is checked while the predecessor remains alive.

Raw observations and metadata are incumbent artifacts in
[`data/2026-10-24-preview-projection-lifetime`](data/2026-10-24-preview-projection-lifetime).
All 255 JSON observations report exact acceptance; both ignored experiments passed in each
process. The source HEAD and dirty-tree scope are recorded in `baseline-metadata.json`; the exact
incumbent collector is retained at commit `6f558f3`. JSONL hashes cover canonical LF bytes.

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

## Preliminary Candidate: Rejected Producer Coupling

The selected candidate for integration qualification keeps the same World and authoring Runtime,
but uses a Scene-owned fresh identity epoch after proving complete predecessor ownership. It does
not increase the claim limit or clear a continuing domain's history. Candidate measurements must
use this exact feature/profile/workload and compare against the frozen incumbent, not relabel the
new default as the old implementation. The predeclared gross stop condition is publication P95
above twice the incumbent in all three matched process comparisons for a population.

The preliminary candidate ran three further serial optimized processes with that same feature/profile and
Transform workload. Each process passed its bounded comparison; all 240 logged observations
report exact acceptance. The primary variant uses an N+1 claim limit. Its sixteen sampled
publications each close the old Scene identity epoch while retaining the exact authoring owner;
the new default is explicitly labeled `scene-projection-epoch` rather than `retained-default`.

| Population | Process | Candidate P50 / P95 (ms) | Candidate / incumbent P95 | Claims after every publication |
|---|---|---|---|---|
| 64 | 1 | 0.791 / 0.844 | 0.748 | 65 |
| 64 | 2 | 0.802 / 0.861 | 0.915 | 65 |
| 64 | 3 | 0.798 / 1.000 | 1.013 | 65 |
| 1,024 | 1 | 15.141 / 15.802 | 1.060 | 1,025 |
| 1,024 | 2 | 15.069 / 15.512 | 1.053 | 1,025 |
| 1,024 | 3 | 14.981 / 15.551 | 1.023 | 1,025 |

Neither population triggers the declared CPU rejection condition. This is not production
qualification: the 1,024-entity preliminary candidate costs approximately
2-6% more at observed P95, and full Scene preparation still scales with population for scalar
edits. Incremental projection remains a separately justified follow-up, not implemented by epochs.
The post-change full-owner control also uses Scene epochs; its separately logged reconstruction
cost is not mislabeled as the incumbent or necessary for history capacity.

Native target-reuse regression `b869f895-bfa7-4b44-b7fc-3f57ede024df` then failed with
`a disposable identity epoch rebuilt the unchanged native image target or texture registration`.
The first draft incorrectly rotated `EditorRenderGeneration` with every Scene identity epoch.
`ImguiTarget::ensure` correctly treats a new producer as a new target/texture namespace, so that
draft introduced native allocation churn absent from this CPU-only collector. These preliminary
observations remain under `candidate-before-producer-correction-*`; they are not relabeled final.

The corrected selection preserves the renderer producer and continuous frame stream for the
preview owner's lifetime, while every Scene publication still receives a fresh identity domain.
Source revision, frame/submission and presentation receipts retain pixel/picking authority;
domain locators retain ECS lookup authority. No cross-producer GPU resource abstraction is added.
An additional cache correction requires live presentation authority before reusing identical
content: exact last-good recovery must submit/composite a new token, never revive an invalid one.

## Corrected Candidate

Three final optimized processes repeat the original Transform workload with a stable preview
producer and fresh Scene identity domains. All 240 observations pass exact acceptance. Within
each primary sample group the producer remains one value, all sixteen domains differ, and every
publication retains exactly N+1 claims. Raw final data is `candidate-{1,2,3}.jsonl` with run IDs,
canonical source hashes and artifact hashes in `candidate-metadata.json`.

| Population | Process | Final P50 / P95 (ms) | Final / incumbent P95 | Claims after every publication |
|---|---|---|---|---|
| 64 | 1 | 0.820 / 0.921 | 0.816 | 65 |
| 64 | 2 | 0.816 / 0.873 | 0.929 | 65 |
| 64 | 3 | 0.803 / 1.097 | 1.112 | 65 |
| 1,024 | 1 | 14.952 / 15.764 | 1.057 | 1,025 |
| 1,024 | 2 | 14.853 / 15.843 | 1.075 | 1,025 |
| 1,024 | 3 | 15.075 / 16.554 | 1.089 | 1,025 |

The corrected candidate does not trigger the predeclared gross rejection threshold. The measured
1,024-entity P95 is approximately 6-9% higher than the incumbent, not a speedup. These temporally
separate small runs do not attribute the difference between preliminary and final candidates to
producer allocation alone. Full Scene work still scales with population; incremental projection
and native edit-to-present latency remain measured follow-ups, not completed by bounded history.
The enhanced actual Windows/DX12 retained-Scene trajectory separately passes unchanged target
identity/texture registration through four creation/Undo pairs and recovery after token
invalidation. It does not turn this CPU collector into a GPU allocation-byte or latency measure.

Full public editing/persistence and native presentation/picking evidence belong in the associated
qualification record. Allocation bytes and complete frame-tail budgets remain unmeasured;
pending image/Save continuity needs explicit held-work tests rather than timing inference.
