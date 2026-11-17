# Runtime Module Magnitudes Against Godot And Bevy

Date: 2026-11-17. Local Windows CPU comparison, not a complete-engine ranking.
The [source-boundary study](../research/2026-11-17-runtime-module-performance-comparison.md)
explains the official loading, instantiation and changed-data validation mechanisms.
The [owning runtime plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#mature-engine-module-timing-comparison)
uses these observations to choose the next optimization, rather than adding a generic validator
or replacing the complete game's existing acceptance with a microbenchmark.

## Workload And Boundaries

Each consumer owns one collection of 64, 4,096 or 16,384 records containing four integers:
`x=i%side`, `y=i/side`, `tile=i%64`, `flags=i%4`, with `side=sqrt(rows)`.
The logical 16K input contains 256KiB at four 32-bit values per row, independently of actual
generic/native storage overhead. Counts and every field are checked after every timed iteration;
the sum checksum is additional evidence. One collection is not 16K entities or nodes.

| Consumer | Implemented operation and material differences |
| --- | --- |
| Nara JSON/RON | The unchanged public writer stores tagged `ComponentValue` rows in one persistent component. Decode includes structural budgets/duplicates, strict envelope, generic payload and source finishing. Compile and fresh-World prepare/commit are separately timed. Native output is an independently owned Vec. |
| Nara compact JSON control | The same public JSON wire is externally minified before timing, then received by the same public decoder/compiler. This is not the production writer or a new typed source format. |
| Godot Dictionary | Actual `PackedScene` text, one script-free Node, persistent Array/Dictionary metadata. `CACHE_MODE_IGNORE` load includes another file read. Instantiation may share Variant containers; it is not independent native row materialization. |
| Godot packed integers | Actual PackedInt32Array metadata in a PackedScene; no per-field generic dictionary. Native COW sharing applies at instantiation. |
| Godot native TileMapLayer | Actual inline 12-byte cells plus a two-byte header. Instantiation builds the mutable cell map through the original setter. No TileSet, tree entry, deferred quadrants, physics, navigation or rendering is included. Coordinate/identifier ranges and semantics are narrower. |
| Bevy typed Scene | Actual public `ScenePatch::resolve` and fresh-World `spawn`, starting from an already constructed typed patch. It does not parse a file. The pinned new Scene API has no native `.bsn` loader. |
| Bevy serialized World | Separate official `DynamicWorld` `.scn.ron` deserialize/write APIs, not a Scene-to-file adapter. Decode includes `ReflectFromReflect` native conversion; there is no separate Scene compile phase. |

These differences preclude a single honest engine-speed ratio. The typed Bevy case informs the
cost of already typed, resolved data; it cannot replace the file-to-runtime rows in the table.
Likewise, Godot metadata sharing is not a target for independently mutable native construction.

## Versions And Execution

Nara production implementation is `9bd3a54f016992c10cb42d105ea338f4e35e64b0`, with the
documentation-only baseline `3e7f436aef1c1d0621508380cf75ef6c0dad4fe0`.
Its independent fixture lock retains root dependency versions/sources and uses Rust 1.95.0.
Bevy uses the read-only source at `03ed5a27c92715c8fa2188f31f04e454ec39942f`, version
0.20.0-dev, built with Rust 1.100.0 and its own retained lock; upstream supplies no checked-in
root lock at that pin. This includes its actual World serializer's dependency closure without
enabling a renderer or modifying reference source.

Godot is the official Windows 4.7.2-stable binary, reporting source
`ed1daf0bf001b61586d9930840f2f1394092c079`. The downloaded archive's SHA256 is
`731980f9608d61333e5baf54a2ef17210acc7a538446c0cb9969f002aca1e953`, matching GitHub's
official release-asset digest. Actual binary and source/fixture hashes are retained with the
observations. The separately inspected 4.8-beta source pin is not claimed as the executed binary.

Hardware: Intel Core i9-13900KF, 24 cores/32 logical processors, approximately 64GiB RAM,
Windows 11. No affinity, fixed clock, exclusive-machine or cold-cache controls are claimed.
All owning Cargo commands use one build job and the original root target. Preliminary execution
during compilation is only a correctness smoke and is excluded from final timings.

## Reading The Results

The runner uses three serial process rounds in orders Nara/Bevy/Godot, Godot/Bevy/Nara and
Bevy/Nara/Godot. Within each process, each case has three warmups followed by nine retained
samples. Reported medians therefore contain 27 warm samples per case, with three process medians
available to show variation. These are descriptive observations, not confidence intervals or
frame-tail percentiles. Process wall includes generation, checks, repeats and cleanup and is not
a scene-load latency.

Warm reads include file open/read/allocation/close. Rust decode consumes resident bytes;
Godot's load performs its own additional read. Do not subtract the independently timed read.
Source generation, setup, validation, destruction and startup are outside module timers.
Godot's clock resolution is one microsecond even though its arrays use nanoseconds.
The full original runtime, Prefab, asset, retained input, gameplay and GPU workflows remain in
the [development runtime report](2026-11-14-development-runtime-costs.md); its cumulative
18 mixed file decodes/nine expansions per lifetime are not single-file measurements here.

Nara 16K pretty RON exceeds the unchanged 16MiB encoded-file limit. The consumer first checks
that exact refusal, then explicitly selects the public 32MiB encoded allowance for that case
only; all other limits remain unchanged. This does not increase production defaults.

## Reproduction And Limits

The [fixture guide](fixtures/runtime_scene_comparison/README.md) contains the serial build/run
commands. Its small Python runner consumes prebuilt programs and retains generated scenes,
individual stdout/stderr, executable and fixture/source hashes, process walls and raw arrays.
It does not build engines, manage dependencies, analyze source code or establish release safety.

No cold storage, total CPU, process memory, GPU, scanout, steady-frame, P99, foreign-platform or
full Foundation/R9/R10/R12 qualification follows from these CPU-only cases.

## Qualified Local Observations

The final nine serial processes finish successfully on November 17 from 16:45:52 to 16:46:20
UTC+08. They produce 72 case records, 648 retained case iterations and 216 warmups, with complete
result checks outside timers. No owning Cargo runs during that series. The
[raw observations](data/2026-11-17-runtime-module-comparison/observations.json) retain all arrays,
phase scopes, selected file limits, source/executable hashes, fixture hashes and process metadata.
Their SHA256 is `e1d83456c4eb96dd090ced601bd4350b589931772a15cb1893f59c8bf33e5fbc`.

All values below are median milliseconds across 27 retained warm samples. File bytes show the
first process; Godot-generated UIDs cause at most a few bytes of variation across the rounds.
An em dash means the phase is unavailable, not zero cost. The named phases retain the different
work and ownership boundaries above and must not be treated as equivalent engine pipelines.

| 16,384-row consumer | Actual file bytes | Warm read | Decode/load | Resolve/compile | Instantiate |
| --- | ---: | ---: | ---: | ---: | ---: |
| Nara default JSON | 11,340,699 | 3.463 | 37.013 | 17.919 | 8.418 |
| Nara compact JSON control | 2,476,457 | 1.052 | 26.331 | 17.975 | 8.358 |
| Nara default RON, explicit 32MiB allowance | 19,402,007 | 5.717 | 175.807 | 17.784 | 8.049 |
| Bevy official World RON | 1,755,290 | 0.672 | 25.897 | — | 0.390 |
| Bevy already typed Scene | — | — | — | 0.0003 | 0.052 |
| Godot Dictionary metadata | 755,824 | 0.773 | 71.194 | — | 0.049 |
| Godot PackedInt32Array metadata | 247,917 | 0.655 | 15.741 | — | 0.013 |
| Godot restricted native TileMapLayer | 262,267 | 0.637 | 18.683 | — | 1.398 |

The sub-microsecond Bevy resolve value is a constant, asset-free typed patch applied to a template;
row assembly is excluded. It is neither reflected source admission nor file loading. Bevy's World
decode already performs native reflected conversion, so its cheap subsequent write is not the
same work as Nara's complete generic-to-native prepare/commit. Nara includes original instance
identity and resources: its resulting World has seven entities/17 components including internal
bookkeeping; both Bevy cases have two/three including the baseline resource entity. Each source
still declares exactly one payload entity/component. Both Rust native row structs are 16 bytes.

The scale series establishes the local order rather than just the 16K endpoint:

| Consumer | 64 rows: decode / instantiate | 4,096 rows: decode / instantiate | 16,384 rows: decode / instantiate |
| --- | ---: | ---: | ---: |
| Nara default JSON | 0.165 / 0.117 | 9.192 / 1.791 | 37.013 / 8.418 |
| Nara compact JSON | 0.119 / 0.113 | 6.732 / 1.799 | 26.331 / 8.358 |
| Nara default RON | 0.721 / 0.117 | 44.313 / 1.821 | 175.807 / 8.049 |
| Bevy World RON | 0.129 / 0.011 | 6.399 / 0.115 | 25.897 / 0.390 |
| Godot Dictionary | 1.266 / 0.006 | 18.237 / 0.023 | 71.194 / 0.049 |
| Godot packed integers | 1.196 / 0.005 | 5.312 / 0.011 | 15.741 / 0.013 |
| Godot restricted TileMapLayer | 1.668 / 0.016 | 5.760 / 0.351 | 18.683 / 1.398 |

For the 16K cases, three process decode medians range from 36.763–37.799 ms for Nara JSON,
26.170–26.830 ms for compact JSON, 174.996–176.377 ms for Nara RON, and 25.626–26.171 ms for
Bevy World RON. Godot Dictionary ranges 67.935–76.171 ms, packed integers 15.204–15.909 ms,
and native TileMapLayer 18.302–19.869 ms. These warm observations are affected by ordinary host,
allocator, file protection/cache and clock behavior; the range is not an uncertainty bound.

Both standalone optimized locked/offline builds pass, as do package-only formatting checks and
the Python/manifest/document/receipt checks. Independent source review finds no P1/P2. Final
report review corrects one typed-resolve table transcription to the pooled 300ns median rather
than the median of three process medians; the underlying samples and conclusions are unchanged. No full
official engine test suites or unrelated Nara product matrices are rerun: this unit changes
measurement consumers/documents, not engine production behavior. The actual public workflows
and complete resulting data are the relevant execution acceptance here.

Preparation exposed three important controls. Standalone Nara initially resolved newer transitive
packages; seeding its own lock from the root removes every version/source deviation before final
measurement. Pretty RON's actual file-size refusal is preserved before the explicit larger case.
The first Nara prototype used 64-bit native rows; both Rust consumers are corrected to matching
32-bit fields before the final series above. Earlier prototype timings are excluded. A broad
formatter invocation hits Windows path-length limits; package-only commands pass, and both
read-only references plus all 15 paused UI/Dialogue files remain unchanged.

## Optimization Targets And Next Decisions

These observations provide a useful local target range: **tens of milliseconds for 16K generic
text records, and sub-millisecond to a few milliseconds for already native/prepared data**.
Sharing-only instantiation can be tens of microseconds; it is a different ownership contract.
Native rendering, first changed-data preparation and asset work remain additional costs.

| Candidate engineering target on this machine | Reason and qualification required |
| --- | --- |
| 16K comparable source-to-native data: roughly 30–40 ms warm, before external assets/rendering | A first optimization target, not a passing release gate. Nara compact decode alone is about 26 ms; its remaining compile and prepare/commit currently add about 26 ms of separately measured phase medians. These sums are orientation, not a measured end-to-end latency. Preserve actual source format/schema and refusal semantics in a complete game comparison. |
| Combined Nara compile plus prepare/commit: initially 5–10 ms for this collection, then measure pure native commit separately | Focuses on the current roughly 26 ms post-decode envelope. Bevy's 0.390 ms native World write and Godot's 1.398 ms native cell construction show that data construction itself need not take tens of milliseconds, but their prior work and guarantees differ. Attribute repeated tree walks, native conversion, allocation and original identity separately before changing owners. |
| Main-thread loading/replacement occupancy: provisional 1–2 ms per bounded turn | An engineering proposal for later real-game measurements, not a result from these fresh-World loops. Whole-frame budgets are 16.67 ms at 60Hz and 8.33 ms at 120Hz; allow room for gameplay, rendering, input and variance. Background total latency and owner occupancy are different targets. |

Prioritize post-decode work and the actual payload representation. Compacting JSON reduces bytes
by about 78% here but decode by only about 29%; whitespace alone does not remove tagged-value
construction, budget/duplicate passes or compiler/native walks. RON remains a materially larger
source-load path and is not evidence for removing all validation. Compare a domain-owned typed
or processed Tile representation through the existing complete game before selecting a new
persistent format or universal cache. Do not use a small native-copy number to justify a second
Scene lifetime or input authority.

Header fusion remains deferred: the previous roughly 41 ms header figure covers 18 mixed file
decodes in one lifetime, not one frame or this one-file control. A special deferred-error envelope
scanner is not yet justified ahead of larger post-decode/representation work. Continue checking
dynamic source at its owning admission and changed native state at its mutation boundary; keep
already admitted, unchanged facts with their owner instead of reparsing them in normal gameplay.
This does not relax existing dynamic-input rejection or adopt another engine's unstated guarantees.

Transform propagation, visibility/queueing, ECS insertion at high entity counts, physics,
steady-frame/P99 and single-cell updates need their own equivalent workloads before numerical
targets can be claimed. This report does not extrapolate scene timings into those modules.
