# UI Key Storage Lifecycle Comparison

## Question And Scope

Does exact prefix sharing justify segmented in-memory UI addresses instead of complete typed
addresses with explicitly shared context? This is a decision-local allocation experiment, not
a measurement of the incumbent engine, complete UI compilation, GPU work or process RSS.

The [standalone fixture](fixtures/ui-key-storage-trial.rs) uses the System allocator and counts
requested allocation sizes from process startup. Three independent Windows x86_64 processes run
Rust 1.95.0 with optimization level 3. [Summary and timing samples](data/2026-10-25-ui-key-storage/summary.json)
bind the exact fixture hash, toolchain and parent implementation commit.

```powershell
rustc +1.95.0 --edition 2024 -C opt-level=3 -C debuginfo=0 docs/benchmarks/fixtures/ui-key-storage-trial.rs -o target/ui-key-storage-trial.exe
./target/ui-key-storage-trial.exe
```

All variants retain complete logical context and leaf bytes. The same framing and canonical
comparison are used, with content-defined prefix encoding independent of pointer sharing.
The complete address has a kind tag, leading context length, context, placement, then trailing
placement length. The enclosing key supplies the complete address length. Putting the final
field's length before its content would cut off shared placement prefixes whenever leaf lengths
differ; the kernel's variable-placement and real three-level Prefab controls cover that case.
The probe checks independent context construction, extraction/recomposition without a recursive
prefix chain, normalized equality/hash, actual 255/256/257 encoding boundaries and full allocation
return after disposal. These are probe checks, not the successor public kernel conformance suite.

Half the children first create Button-like scoped descriptions and clone their locals into a
snapshot. The other half begin as unscoped locals. Validation creates temporary scoped keys;
compilation retains frame keys and a cloned cache. Observe both with the authoring snapshot alive
and after it is dropped. The probe models this clone/lifetime shape, not real Buttons, maps,
layout or retained presented sessions. All key handles are 16 bytes in these candidate layouts.

## Observed Allocation Requests

All three processes produce identical allocation counts and retained-byte values. The table is
for 4096 total addresses (one root, 4095 children); the summary also records the 32-address case.
The values include model containers and Arc allocations, not just string payload lengths.

| Input | Storage | Snapshot alive | Snapshot dropped | Peak above baseline | Validation allocations |
| --- | --- | ---: | ---: | ---: | ---: |
| 120-byte shared context | Complete | 905176 | 872416 | 905176 | 0 |
| 120-byte shared context | Typed context | 426080 | 393320 | 426080 | 0 |
| 120-byte shared context | Segmented | 867997 | 409461 | 1073008 | 4094 |
| 40-byte context, 80-byte shared leaf path | Complete | 905176 | 872416 | 905176 | 0 |
| 40-byte context, 80-byte shared leaf path | Typed context | 753680 | 720920 | 753680 | 0 |
| 40-byte context, 80-byte shared leaf path | Segmented | 867997 | 409461 | 1073008 | 4094 |
| Distinct context per child | Complete | 905176 | 872416 | 905176 | 0 |
| Distinct context per child | Typed context | 983000 | 950240 | 983000 | 0 |
| Distinct context per child | Segmented | 1048536 | 1015776 | 1048536 | 0 |

Compilation allocates two key vectors in complete/typed storage. Segmented storage allocates
4096 times in the shared cases, including those vectors and raw-local suffixes. Already segmented
Button locals reuse their suffix on reconstruction; omitting that reuse would exaggerate the cost.

Timing samples include allocator instrumentation, fixture construction and a common canonical-byte
comparison loop. They are retained for transparency, not an engine frame-latency claim or a basis
for choosing a new global performance budget. Actual allocator usable size and RSS are unmeasured.

## Decision

Select complete typed addresses with explicitly shared opaque context and allocation-free scoping
as the initial production replacement. Root scope must not supply missing local-source identity.
Canonical prefix encoding may reduce the exact encoded budget without dictating segmented storage.
Keep the hard ceiling at 256 bytes and use one cost function for admission and reduced limits.

Segmented storage is not universally worse: it retains fewer bytes after snapshot disposal in the
long shared-leaf case. It also raises peak/validation allocation work, and does not beat typed
context for the common-context case. Conversely typed context has measurable overhead when every
child has a unique context. No variant wins every shape. A measured production workload dominated
by duplicated long placement tails is a concrete trigger to revisit further sharing, not grounds
to add it before complete source/occurrence correctness is implemented.

Only the chosen representation enters production. The standalone fixture preserves comparison
controls as research evidence, not supported alternate backends. Runtime/Canvas adoption, full
public/neutral conformance, actual compile/interaction costs and native qualification remain separate.
