# Immutable Spatial Cut Cost

This compares the replaced Global Component implementation with the U4 phase snapshot design.
It is one process per source state, not a native frame-rate, hardware-isolated speed claim or
supported population ceiling. Correctness and explicit temporal reads are the reason for the change.

## Protocol

- Windows 11, Rust 1.99.0, x86_64-pc-windows-msvc, Cargo release profile, one build job and one
  nextest thread. Root target reused; other desktop/background work was not controlled.
- The existing `nara_transform` ignored `scale_measurement` and `scale_allocations` fixtures run
  unchanged populations/workloads. Consumption changes from Global queries to snapshot reads;
  input admission, managed drive, fixture topology, warmup and correctness oracles remain.
- 64, 4,096 and 65,536 members; four ordered roots and deterministic four-way topology. Each owns
  Transform. 120 warmups and 1,000 samples per timing row, subject to the existing measured-time cap.
  Every recorded row reached 1,000 samples. Nearest-rank percentiles stay separate by process.
- Static uses zero fixed steps. Sparse changes one leaf; root-dirty changes one root. Interpolation
  maintains approximately half-step alpha. Four-step changes one pending source only in the first
  tick, with three additional unchanged ticks. This is not dense motion or a render workload.
- Before timing `59826873-729a-4f83-877d-fbfd268bc3ae`; allocation
  `08aa05ec-0cf8-499c-86fb-5efb46e4c311`. Initial snapshot timing
  `2c2c3671-59c8-4b70-9634-82bf2adf1e7f`; allocation
  `e6656810-7f62-4373-86bb-88b04754ecd3`. Final timing
  `707d55c9-f477-4338-b9e8-a5a35411540f`; allocation
  `effc4979-9caa-4cf9-a1dd-ea12498e8383`.
- Final-source confirmation after shared checked publication identity: timing
  `0bfd8da8-f8d3-4ba9-b0bf-307bfdd30d3b`, allocation
  `f55a5b01-2d10-458f-bdbe-7760f6b1c1f5`. Its `qualified-*` data stays separate from the earlier
  optimized `final-*` process. Source is the final U4 working tree on `fb6af18`.
- Before source is the original U4 owner path before its working-tree replacement, recorded by the
  active implementation lane. Final source is the coordinated U4 working tree on parent `cfde813`,
  including exact hierarchy publication, contiguous source observation and ordered candidate build.
  No stronger committed-before-source identification is inferred from the run ID.

## 4,096-Member Timing

All values are microseconds. Initial snapshot P50 identifies the intermediate implementation that
triggered investigation; it is not pooled with the final process.

| Work | Before P50 / P95 | Initial snapshot P50 | Final P50 / P95 / P99 |
| --- | ---: | ---: | ---: |
| Static, zero steps | 18.5 / 20.1 | 66.5 | 39.7 / 44.5 / 56.8 |
| One leaf, one step | 159.3 / 187.9 | 250.2 | 177.4 / 196.3 / 242.4 |
| Root dirty, one step | 178.7 / 199.9 | 249.4 | 178.0 / 198.5 / 278.4 |
| One leaf with interpolation | 419.6 / 595.5 | 419.9 | 344.0 / 420.8 / 538.9 |
| Four steps, one pending edit | 181.8 / 198.2 | 348.2 | 230.0 / 282.4 / 368.5 |

The initial source comparison performed per-entity World parent lookup and hash lookup, and its
candidate rebuilt a second adjacency graph and visited set. The final implementation queries Parent
with Transform, compares retained contiguous `(Entity, source)` rows, and reuses Hierarchy's exact
validated parent-before-child order. Archetype/query order changes conservatively rebuild; equality
does not rely on change ticks. Candidate data still publishes only after complete finite validation.

The remaining static and multi-step increase is real: every publication scans source values and
membership to observe edits sharing an ECS change tick. At 4,096, static P50 adds 21.2 us and the
four-step row adds 48.2 us. Eliminating this scan would require a different mutation contract or a
separately proven tracking mechanism. No writer-only gameplay interface or cache authority is added
to hide the cost. Sparse P50 adds 18.1 us, root-dirty is approximately unchanged, and interpolation
decreases in this process. These observations do not establish a general throughput improvement.

After the final publication-identity correction, a separate source-confirmation process reports
the following. Every row again contains 1,000 samples; do not pool it with the table above.

| 4,096-member work | Final-source P50 / P95 / P99 (us) |
| --- | ---: |
| Static, zero steps | 38.3 / 59.9 / 72.9 |
| One leaf, one step | 174.3 / 191.0 / 270.3 |
| Root dirty, one step | 173.1 / 193.0 / 258.6 |
| One leaf with interpolation | 343.5 / 429.2 / 510.8 |
| Four steps, one pending edit | 225.5 / 268.4 / 347.7 |

The correction allocates simulation/presentation identities from one checked atomic sequence
independent from movable slot/history storage. This run does not show a material added CPU cost;
its higher static P95 remains a real observed tail under the uncontrolled desktop environment.

## Allocations And Retention

Allocation instrumentation delegates to System and measures successful Rust GlobalAlloc requests
in a separate executable. Four blocks each contain 64 measured operations. These are requested
bytes, not allocator usable size, RSS, GPU memory or a leak proof. Setup/oracles/output are excluded.

| 4,096-member block | Before calls / requested bytes | Final calls / requested bytes |
| --- | ---: | ---: |
| Static zero steps | 0 / 0 | 0 / 0 |
| One leaf, one fixed step | 0 / 0 | 0 / 0 |
| Batch eight reparents | 960 / 4,355,328 | 960 / 4,355,328 |
| Eight independent reparents | 4,800 / 4,664,576 | 4,800 / 4,664,576 |

The block-start live requested bytes change from 3,930,538 to 3,466,732 for static and from
3,933,260 to 4,408,860 after sparse mutation warms both candidate buffers. Buffer state differs
between those scenarios. The final sparse retained storage is 475,600 bytes above before; this is
not a per-frame allocation. Initial snapshot sparse retention was 4,789,276 bytes. Removing the
second adjacency graph reduces retained storage while preserving complete immutable publication.
The final-source allocation confirmation retains zero static/leaf allocations and the same
hierarchy counts/bytes. Static and sparse block-start live requested bytes are 3,465,708 and
4,407,836, respectively: sparse is 474,576 bytes above the original source. These final-source
figures differ slightly from the earlier optimized executable's process-retained storage.

## Other Populations And Limits

At 64, static/leaf/root P50 changes from 5.1/8.5/9.2 us to 6.1/9.6/9.5 us. At 65,536 it changes
from 224.9/2,537.3/2,822.7 us to 542.8/2,934.5/2,981.9 us; final P95 is
614.6/3,313.7/3,384.5 us. The larger static scan cost is visible. No real consumer establishes
65,536 spatial members as a product requirement. Future incremental work requires measured
consumer pressure and must preserve exact same-tick mutation/publication semantics.
The final-source confirmation at 64 has static/leaf/root P50 6.2/9.4/9.3 us; at 65,536 it has
522.8/2,836.1/2,869.7 us and P95 615.8/3,250.5/3,197.1 us. It preserves the same cost conclusion
without turning a second process into a pooled throughput claim.

All population summaries and the complete 4,096 allocation blocks are retained in
[measurement data](data/2026-10-23-low-level-spatial). Raw sample files remain in the ignored root
`target/low-level-refactor/`; summaries retain separate source names rather than merging producers.
The [existing baseline protocol](2026-10-18-foundation-scale.md) documents fixture detail. Native
Editor qualification and actual GPU presentation remain separate U11 evidence.
