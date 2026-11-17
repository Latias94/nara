# Runtime Module Performance Comparison: Source Boundaries

Date: 2026-11-17. Source review only; Nara baseline `3e7f436aef1c1d0621508380cf75ef6c0dad4fe0`.
No benchmark, build or formatter was run for this source note. Measurements and executed binary/library
versions belong in the [local module comparison](../benchmarks/2026-11-17-runtime-module-comparison.md).
Source inspection cannot establish a latency target or engine speed ratio. The earlier
[payload ownership study](2026-11-17-large-tilemap-payload-ownership.md) owns Nara's copy, validation and retention investigation.

## Pinned Primary Sources

| Engine | Local read-only checkout and inspected revision | Version in source |
| --- | --- | --- |
| Godot executed version | `repo-ref/godot/` via `git show ed1daf0bf001b61586d9930840f2f1394092c079:<path>` | [`version.py`](https://github.com/godotengine/godot/blob/ed1daf0bf001b61586d9930840f2f1394092c079/version.py#L3): 4.7.2 stable. |
| Godot development reference | `repo-ref/godot/` HEAD `7a37606a433c0e7735525037cdf774f043146a21` | `version.py`: 4.8.0 beta. [^g-version] |
| Bevy | `repo-ref/bevy/`, `03ed5a27c92715c8fa2188f31f04e454ec39942f` | Root `Cargo.toml`: 0.20.0-dev, Rust 1.97.1. [^b-version] |

Both checkouts were clean, with official GitHub origins; no checkout was switched. The benchmark
owner reports `4.7.2.stable.official.ed1daf0bf`, binary SHA256
`ab1824f85bfd8e0e4128182c000c4003a3e042245b2967848d089b2a04b22424`.
Nara's locked crates.io Bevy dependencies remain distinct from the development source pin. [^n-config]

The executed Godot source preserves the reviewed decode → SceneState → property-assignment
boundaries. Its TileMapLayer lives in `scene/2d/tile_map_layer.cpp`: the same two-byte header,
12-byte records, per-cell setter and deferred dirty/quadrant membership work apply to this fixture.
Packed-byte base64 decoding still copies into its result buffer. [^g-stable-load] [^g-stable-scene] [^g-stable-tile] [^g-stable-parser]
The beta changes recursive local-resource remapping and bubbles inherited-scene deferred NodePaths
outward; these are material for richer scene tests,
but do not change the proposed plain tile control. The tables below cite the beta separately. [^g-instantiate]

## What The Corresponding Modules Actually Do

| Boundary | Godot | Bevy at this pin |
| --- | --- | --- |
| Text/value decode | `VariantParser` builds Arrays/Dictionaries recursively, or typed packed arrays. `ResourceLoaderText` resolves resource IDs; node properties enter `SceneState`. This does not construct the node's native cell map. [^g-parser] [^g-load] | Reflect's `TypedReflectDeserializer` resolves registrations and builds dynamic reflected values unless a custom deserializer/processor takes over. Struct lookup rejects unknown fields; list decoding visits every element. This is a value facility, not the new Scene file loader. [^b-decode] |
| Scene resolution | Text loading records node types, parent/owner paths and stored property values; inherited scenes and resources have their own load/instantiation work. There is no directly equivalent Nara compile phase. [^g-load] [^g-instantiate] | `Scene::resolve` composes templates and related scenes. `ScenePatch::load` starts dependencies; `resolve` consumes the source Scene and stores an `Arc<ResolvedSceneRoot>`. It does not parse scene text. [^b-scene] [^b-patch] |
| Native instantiation | `SceneState::_instantiate` creates nodes, localizes resources, handles container copies/conversions and calls `node->set` per stored property. A TileMapLayer's setter builds its native cells here. [^g-instantiate] [^g-wire] | `ScenePatch::spawn` requires a resolved scene. Applying it builds template outputs, writes components, creates related entities and triggers `Ready`; template/application errors remain possible. [^b-patch] [^b-apply] |
| Tile representation | `tile_map_data` is an inline stored `PackedByteArray`: nonempty data have a two-byte header plus 12 bytes per cell, decoded into a native cell HashMap through `set_cell`. [^g-wire] | Official `TilemapChunk` config and `TilemapChunkTileData(Vec<Option<TileData>>)` are separate components. The insertion hook builds a mesh/material and an Image from eight-byte `PackedTileData` records. This is render preparation, not an on-disk source codec. [^b-chunk] [^b-packed] |

**The new Bevy scene path has no released `.bsn` file format at this pin.** Its documentation says
so explicitly, and `ScenePlugin` registers scene assets/systems, not a production scene-text loader.
Do not substitute the older `DynamicScene` API or call an independently written Reflect/Serde
adapter an official scene loader. Measure that adapter as its own operation. [^b-format]

Generic rows and compact native tiles answer different questions. Godot parses Array/Dictionary
members and PackedInt32Array numeric tokens; PackedByteArray can use base64. Both packed branches
copy their parsed buffer into a result. Vector COW copies share backing until mutation, but parsing,
the complete tile-data getter and per-cell native construction still cost work. [^g-parser] [^g-cow] [^g-wire]

Bevy's generic Vec reflection cloning and `FromReflect` construction visit elements. Its blanket
`Template` for `Clone + Unpin` clones on build and template clone; patching a cached component
template clones that template, not just the edited field. Typed scenes bypass a generic source
tree, but do not make a large Vec copy free. Separate config/data components avoid that particular
coupling. Reflect ECS construction through `from_reflect_with_fallback` is a different route
(`FromReflect`, then Default/apply, then FromWorld/apply). [^b-vec] [^b-template] [^b-reflect]

## Validation And Update Frequency

| Owner | Load/instantiate/change work | Unchanged steady frames |
| --- | --- | --- |
| Godot parser/scene | Syntax, declared resource references, scene structure, property conversion and concrete setters run at their corresponding operations. Array/Dictionary parser routines carry no cumulative Nara-style node/byte/depth allowance. `Object::validate_property` edits property metadata, not a proposed value. [^g-parser] [^g-load] [^g-instantiate] [^g-property] | These load and property-assignment routines are not a recurring full source-value validation pass. |
| Godot TileMapLayer | The byte setter checks format/header/record boundaries. `set_cell` updates one map entry and queues dirty work. Atlas source/tile/alternative membership checks occur during rendering-quadrant updates. Initial entry, TileSet or quadrant-shape changes can revisit everything. [^g-wire] [^g-update] | Dirty updates are deferred and coalesced; unchanged tile data do not require reparsing or rebuilding all quadrants. Culling/rendering remains separate runtime work. [^g-update] |
| Bevy Reflect/Scene | Deserialize checks types/field names at decode. Template building can enforce contextual requirements. Neither route supplies universal numeric-range, aggregate budget or domain-invariant validation. [^b-decode] [^b-template] [^b-reflect] | The typed Scene apply path does not serialize components back into a generic tree for a recurring universal validation pass. [^b-apply] |
| Bevy TilemapChunk | The insertion hook and `Changed<TilemapChunkTileData>` update check length against chunk dimensions. Invalid length warns and skips work. Every valid changed chunk is completely repacked and its Image bytes replaced, even for one edited cell. [^b-chunk] | The system still runs its query/change filter, but unchanged tile vectors skip its per-chunk validation/repacking body. No visibility filter limits changed chunks. [^b-chunk] |

These mechanisms do not establish Nara-equivalent refusal, immutable generation custody or
logical work/retention accounting. Godot's setters/dependency policies and Bevy's warning/error
paths also differ from strict whole-input rejection. Keep those contract differences visible;
they justify identifying additional work, not automatically repeating every payload traversal.
See the earlier study's [validation boundaries](2026-11-17-large-tilemap-payload-ownership.md#5-validation-at-owning-boundaries).

## Minimal Workload Design

Use the same logical **64 / 4,096 / 16,384 occupied cells**: 8×8 / 64×64 / 128×128,
deterministic coordinates and tile IDs, one tileset, white tint, no collision/navigation,
animation or Y sorting. Map Godot's source/atlas identifiers and Bevy's dense Y-up indexing to
that common content explicitly. Godot's serialized coordinates are signed 16-bit; Bevy's tile
index is `u16`; Nara uses `i32` coordinates and `u32` tile IDs. Keep this fixture within their
intersection. Godot's wire record has no per-cell RGBA field, and Bevy's render packing quantizes
color, so this restricted white-tint case does not prove general semantic parity. [^g-wire]
[^b-chunk] [^b-packed] [^n-tile]

Keep two small comparisons, with a distinct purpose for each:

| Comparison | Minimum representation | Interpretation |
| --- | --- | --- |
| Generic payload | One Godot Node/Resource property containing Array/Dictionary rows, one Bevy component/resource containing reflected `Vec<Row>`, one Nara persistent component containing its cell collection. Match logical fields and record actual nesting, scalar tags and encoded sizes. | Decode/materialization cost for large inline generic data. Godot script/custom setter overhead, if used, must be named. These containers still have different wire/type machinery. |
| Native tile control | Godot TileMapLayer's packed inline cells; Bevy's official typed chunk config/data components; Nara's native Tilemap codec/preparation. | Domain representation and preparation cost. Native construction from prebuilt cells excludes source decode; report that boundary explicitly. Godot's 12-byte wire records and Bevy's eight-byte render records are not equivalent source formats. |

Neither comparison needs an entity/node per cell. A per-cell ECS/scene test measures entity
allocation, component insertion, hierarchy/hooks and possibly transforms; keep it separate if
needed. Do not compare 16,384 Bevy entities to one Nara component or one Godot layer as equal work.
Specify chunk/layer/component counts independently of cell count. A fixed chunk size introduces
more chunks as N grows; one growing chunk changes mutation/visibility granularity. [^b-chunk]

For a module control, use one collection at each size. Reusing the existing Nara gameplay fixture
instead requires acknowledging its 16K case: two 8K maps, including a root-bound Prefab, rather
than one 16K collection. Its source intentionally omits configuration defaults. Keep those absent
in a compile/default comparison; adding them would change the operation under investigation.
Record dependency/Prefab work separately from a dependency-free module control. [^n-fixture]

Report separately: (1) file read; (2) bytes to stored value/scene state; (3) resolve/compile;
(4) instantiate/native CPU preparation; (5) first required deferred/update/render preparation;
(6) unchanged frames; (7) one-cell change through its completed update. Not every engine exposes
every phase separately; use honest combined envelopes instead of subtracting unrelated timings.
Godot can defer quadrant work until entering the tree; Bevy's hook/commands and first Changed
update can add work after construction. An instantiated CPU scene is not a first rendered frame.
[^g-update] [^b-chunk] [^b-apply]

Keep fixture generation and correctness checks outside timed envelopes, while retaining each
engine's normal production checks inside them. State optimized/debug build, version, allocator,
sample count and warmup. Distinguish engine asset-cache hits, warm OS file cache and fresh decode:
Godot `CACHE_MODE_REUSE` can return an existing Resource without parsing; `CACHE_MODE_IGNORE`
still reuses external dependencies unless the deep mode is selected. Cached Bevy scene spawning
likewise excludes dependency acquisition and resolution. Do not sum nested envelopes. [^g-cache]
[^b-patch]

## Performance Interpretation

Use the corresponding optimized operation on this machine as empirical context, with its input
and guarantees attached. The three sizes distinguish setup, traversal and chunk-count effects;
cell count alone cannot normalize different encodings. No numerical latency SLA follows from source.

For the overvalidation concern, the upstream precedent supports admitting dynamic input at
decode/construct boundaries and checking changed domain data at its setter/update owner.
Reuse applicable facts for unchanged immutable data; recheck changed schema/resource/context
facts without using another full generic traversal as a proxy. Attribute repacking, copying,
reference resolution and rendering independently from validation. This is a locality criterion
for interpreting the measured results, not a proposal for another general validation framework.

## Using Local Measurements As Engineering Targets

The [measured comparison](../benchmarks/2026-11-17-runtime-module-comparison.md#optimization-targets-and-next-decisions)
provides local optimization magnitudes with its representation and ownership limits attached.
Its Nara production baseline is `9bd3a54`, with the measured consumer sources at `70140a3`.
The later documented baseline, `34d1ebd`, includes joint reference interpretation (`6295ccb`)
and active-container cursor changes (`26c12dc`). The historical table does not measure that baseline;
its later complete-game observations have their own envelopes and cannot replace this
single-collection comparison. A consumer's printed historical revision alone does not identify
the production sources in a later executable.

The proposed **30–40 ms warm source-to-native**, **5–10 ms compile plus prepare/commit**, and
**1–2 ms main-thread occupancy per bounded loading/replacement turn** are initial engineering
budgets for this machine and workload, not current Nara results, engine-equivalence claims or
release gates. The first two concern the 16K collection before external assets and rendering;
the occupancy proposal still needs a real consumer measurement, since fresh-World loops do not
measure responsive owner turns. Measure a complete source-to-native envelope with its read
boundary stated explicitly; adding separately reported phase medians does not measure it.

Godot's shared metadata and packed-array COW do not establish an independently mutable native
materialization budget. Its restricted TileMapLayer does construct native cells, while omitting
first deferred/render work. Bevy's World decode already includes native reflected conversion;
its subsequent write and its separately preconstructed typed Scene cannot be substituted for
Nara's generic-to-native preparation. Use those observations to prioritize attribution, not to
remove receiving checks merely to match a smaller timer.

Keep validation local to the facts that changed: admit dynamic source at its receiving boundary,
check changed domain values and affected aggregate invariants at their setter/update owner, and
reuse still-applicable facts for unchanged admitted values. Recheck affected Schema, resource or
context dependencies when they change. Locality follows the owner's granularity: Godot queues
dirty cells, whereas Bevy repacks an entire changed chunk; neither mechanism promises constant
work for one edited cell. Separate derivation/copying work from validation instead of rebuilding
a generic value tree each frame.
Preserve Nara's refusal and ownership contracts at the affected boundary, using existing focused
checks and measurements when that boundary changes. These proposed targets add no universal
validator, proof registry or requirement to rerun unrelated product qualification.

## Source Permalinks

Paths are relative to the checkout roots above; every URL pins the inspected revision.
[^g-version]: [`version.py`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/version.py#L3).
[^g-stable-load]: Executed version: [`scene/resources/resource_format_text.cpp`](https://github.com/godotengine/godot/blob/ed1daf0bf001b61586d9930840f2f1394092c079/scene/resources/resource_format_text.cpp#L184): `_parse_node_tag`, property storage (305), resource assignment (695), cache mapping (1433).
[^g-stable-scene]: Executed version: [`scene/resources/packed_scene.cpp`](https://github.com/godotengine/godot/blob/ed1daf0bf001b61586d9930840f2f1394092c079/scene/resources/packed_scene.cpp#L155): `SceneState::instantiate`, property handling (434–494), deferred NodePaths (597); resource remapping (83–133).
[^g-stable-tile]: Executed version: [`scene/2d/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/ed1daf0bf001b61586d9930840f2f1394092c079/scene/2d/tile_map_layer.cpp#L3220): setter/getter (3220/3264), rendering/membership (225/527), deferred update (2038), `set_cell` (2782).
[^g-stable-parser]: Executed version: [`core/variant/variant_parser.cpp`](https://github.com/godotengine/godot/blob/ed1daf0bf001b61586d9930840f2f1394092c079/core/variant/variant_parser.cpp#L601): `_parse_byte_array`, packed byte/int32 branches (1411/1429), Array/Dictionary (1644/1684).
[^g-parser]: [`core/variant/variant_parser.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/variant_parser.cpp#L554): `_parse_construct`, `_parse_byte_array` (602), packed branches (1339), `_parse_array`/`_parse_dictionary` (1545/1579), packed writer (2196).
[^g-load]: [`scene/resources/resource_format_text.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/resource_format_text.cpp#L107): resource references, `_parse_node_tag` (186–302), `load` resource assignment (632–670).
[^g-instantiate]: [`scene/resources/packed_scene.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp#L226): `SceneState::_instantiate`, property handling (507–567), deferred node references (677–719).
[^g-wire]: [`modules/tilemap/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.cpp#L3207): `set_tile_map_data_from_array`, `get_tile_map_data_as_array` (3251), stored property registration (2261).
[^g-cow]: [`core/templates/cowdata.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/templates/cowdata.h#L513): `_copy_on_write`, `_ref`; [`core/templates/vector.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/templates/vector.h#L91): `ptrw`.
[^g-update]: [`modules/tilemap/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.cpp#L229): `_rendering_update`, membership (526), deferred/internal update (2028–2100), `set_cell` (2769).
[^g-property]: [`core/object/object.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/object/object.cpp#L623): `Object::validate_property(PropertyInfo&)`.
[^g-cache]: [`core/io/resource_loader.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/io/resource_loader.cpp#L800): cache hit; [`scene/resources/resource_format_text.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/resource_format_text.cpp#L1399): cache mode mapping.
[^b-version]: [`Cargo.toml`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/Cargo.toml#L1).
[^b-decode]: [`crates/bevy_reflect/src/serde/de/deserializer.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/deserializer.rs#L352): custom/typed dispatch; [`struct_utils.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/struct_utils.rs#L91): fields; [`lists.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/lists.rs#L27): elements.
[^b-scene]: [`crates/bevy_scene/src/scene.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/scene.rs#L49): `Scene`, `TemplatePatch` (328), `InsertTemplate` (340).
[^b-patch]: [`crates/bevy_scene/src/scene_patch.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/scene_patch.rs#L20): `ScenePatch::{load,load_with,resolve,spawn,apply}`.
[^b-apply]: [`crates/bevy_scene/src/resolved_scene.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/resolved_scene.rs#L26): resolve/spawn, `apply_with` (224), `Ready` (314), `ErasedTemplate::apply`/`SceneEffect` (722–760).
[^b-format]: [`crates/bevy_scene/src/lib.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/lib.rs#L394): unreleased format; `ScenePlugin::build` (964).
[^b-template]: [`crates/bevy_ecs/src/template.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_ecs/src/template.rs#L390): blanket `Template`, contextual `EntityTemplate` (450); [`crates/bevy_scene/src/resolved_scene.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/resolved_scene.rs#L472): cached template clone/default.
[^b-vec]: [`crates/bevy_reflect/src/impls/macros/list.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/impls/macros/list.rs#L118): `reflect_clone`, `FromReflect` (178).
[^b-reflect]: [`crates/bevy_ecs/src/reflect/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_ecs/src/reflect/mod.rs#L108): `from_reflect_with_fallback`.
[^b-chunk]: [`crates/bevy_sprite_render/src/tilemap_chunk/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/tilemap_chunk/mod.rs#L51): types, insertion hook (137), Changed update (199), Y-down conversion (267).
[^b-packed]: [`crates/bevy_sprite_render/src/tilemap_chunk/tilemap_chunk_material.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/tilemap_chunk/tilemap_chunk_material.rs#L53): `PackedTileData`, color conversion, `make_chunk_tile_data_image` (96).
[^n-config]: Nara baseline above: [`Cargo.toml`](../../Cargo.toml), [`Cargo.lock`](../../Cargo.lock).
[^n-tile]: Nara baseline above: [`crates/nara_tilemap/src/lib.rs`](../../crates/nara_tilemap/src/lib.rs), `TileCoord`, `TileIndex`, `TileCell`, `read_cells` (1000).
[^n-fixture]: Nara baseline above: [`samples/two-level-game/src/bin/runtime_workload/project.rs`](../../samples/two-level-game/src/bin/runtime_workload/project.rs), population (57), two-map/Prefab construction (222–287), `tilemap_record` (390–445).

## Further Module Targets: Work Counts Before Timings

This follow-up reuses the frozen local comparison; no measurements were rerun. The additional
sources use the Bevy `03ed5a27` and Godot development `7a37606a` pins above, not another executed
engine version. **No useful checked-in, hardware-qualified timing series for these workloads was
found in the inspected benchmark/performance paths.** Bevy supplies Criterion workload code and
instructions for generating results; the inspected transform/ECS cases' 500 ms warmup and 4/6 s
measurement windows are configuration, not operation times. Godot's Performance monitors expose
counters, not published timing data; its checked-in Android macrobenchmarks exercise editor
startup, not the runtime modules below. No additional engine latency or speedup follows. [^b-bench-method]
[^g-perf-paths]

| Module | What the pinned official sources establish | What needs a matched workload before assigning milliseconds |
| --- | --- | --- |
| Transforms | Bevy constructs 51,696 hierarchical transform entities from 48 roots with fanouts `[4,4,3,3,2,2]`, plus 12,000 standalone transforms. Cases mutate 1,024/4,096 movers or eight roots; a separate case reparents 4,096 of 16,000 children. Mutation and scheduled work are inside the timers. These are 3D transforms, not Nara's 2D operation. [^b-transform-bench] | Separately use 64/4K/16K **entities**, flat and parented layouts, unchanged/one/all changes, and leaf versus ancestor changes. Match worker policy and finish global-transform consumption: Godot's CanvasItem getter computes invalid globals lazily, so timing only setters misses work. [^g-transform-consume] |
| ECS insertion | Bevy direct spawn adds 1/100/10,000 entities per iteration; batch cases each add 10,000 total, with different batch sizes. Worlds are reused. The 10,000-entity deferred insert cases include command application; later iterations replace already inserted components. These are not fresh Scene loads. [^b-insertion-bench] | Compare Nara's native insertion with its own locked `bevy_ecs` first, keeping bundle contents, allocator/world lifetime, hooks and deferred application explicit. Measure Scene identity/codec work separately. The earlier 16K-row case contains one payload entity, not 16K entities. |
| Tile mutation | Bevy skips unchanged chunk bodies but repacks every slot of a changed chunk. Godot coalesces dirty cells and reconstructs affected rendering quadrants, including sorting and canvas-item work. TileSet/layer changes can invalidate the local case. [^b-chunk] [^g-update] | Fix occupied cells, chunk/layer counts, visibility and resources; compare unchanged, one valid cell replacement and full replacement through required deferred preparation. The measured Godot TileMapLayer had no TileSet/tree entry, so its instantiation time does not cover this update. |
| Render preparation | Bevy's extraction benchmark marks 10–100,000 zero-payload `DummyAsset`s modified and times extraction only; the following `app.update()` is outside that timer. It does not measure sprite queueing, real upload or GPU execution. [^b-extract-bench] | Match views, visible sprites/tiles, material/order rules, resolution, backend and warmed resources. Separate extraction, visibility/queue/batching, upload submission and GPU work before allocating a module budget. |

An actionable target ladder is therefore:

1. **Set locality counts now.** Propose zero repeated full source-admission walks for unchanged
   admitted payloads, and zero tile-data repacks/dirty-quadrant rebuilds in a settled unchanged
   case without data-change signals or resource/layer changes. Query/change detection, visibility
   and drawing may still do work. These are candidate work budgets, not claims of zero frame
   time or measurements of today's Nara implementation.
2. **Bound one-cell derivation by the chosen region.** In Godot's non-Y-sorted control, the
   default rendering quadrant is 16×16: replacing one atlas cell at the same coordinate with
   the same TileSet/configuration dirties one rendering quadrant, containing at most 256
   occupied cells; it does not rebuild just one cell. [^g-quadrant-size] [^g-update]
   A proposed Bevy control with 16K slots in 64 chunks of 16×16 repacks one changed chunk's
   256 entries / 2 KiB, versus 16K entries / 128 KiB in one large changed chunk, because each
   packed entry is eight bytes. This is **64× fewer repacked entries, not 64× faster**. These
   are CPU image payload bytes, not measured GPU transfers. More chunks also change entity,
   material, culling and draw overhead; this arithmetic does not select Nara's production
   chunk size or promise the same byte layout. [^b-chunk] [^b-packed]
3. **Measure a corresponding module before proposing its time or multiplier.** Preserve the
   existing local source/native targets in their original scope. There is no supported new
   sub-millisecond transform, 16K-entity insertion, one-cell update or render-preparation
   target here. Use the small controls above when that owner changes; record the actual
   revision, hardware, thread/build policy and completed boundary alongside timings. A
   same-substrate comparison isolates Nara's added work without treating a development ECS
   release as its locked dependency. No broad benchmark framework is needed.
4. **Allocate frame budgets in the complete consumer.** The arithmetic 16.67 ms at 60 Hz or
   8.33 ms at 120 Hz bounds the whole frame; it does not give each module a millisecond
   allowance. Unchanged tile data do not eliminate ordinary render preparation: Bevy clears
   and rebuilds sprite batches/instances and writes the instance buffer. Godot's
   `frame_setup_time` stops before viewport drawing, and threaded `draw()` can enqueue work,
   so neither is a complete rendering/GPU timer. Use actual consumer CPU and GPU observations
   and frame-tail distributions before subdividing those budgets. [^b-sprite-prepare]
   [^g-render-timing]

Keep checks at their owning boundaries: dynamic input admission, changed domain values and
affected aggregates retain their checks; unchanged facts remain reusable only while their
schema/resource/context basis still applies. A cheap setter or dirty flag is not completed
work, and dirty-region derivation is separate from validation. Godot's lazy transforms and
Bevy's optional static-tree optimization support locality, not identical guarantees or an
O(1) unchanged-frame claim. [^g-transform-consume] [^b-transform-locality] This section proposes
bounded investigation targets only; it creates no validation registry, runtime authority or
additional release gate.

[^b-bench-method]: Pinned Bevy [`benches/README.md`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/benches/README.md#L5): run/save/compare generated Criterion baselines; the source files below configure observation windows, not measured latencies.
[^g-perf-paths]: Pinned Godot [`main/performance.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/main/performance.cpp#L254): live time/object/render monitors; [`EditorBenchmarks.kt`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/platform/android/java/editor/macrobenchmark/src/main/java/org/godotengine/editor/v4/macrobenchmark/EditorBenchmarks.kt#L53): Android editor-startup workload code.
[^b-transform-bench]: Pinned Bevy [`benches/benches/bevy_transform/propagate.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/benches/benches/bevy_transform/propagate.rs#L23): hierarchy construction (135), standalone transforms (74), reparent setup (196), timed cases (276–386). Hierarchical count is derived from the checked-in fanouts, not a timing result.
[^g-transform-consume]: Pinned Godot [`scene/main/canvas_item.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/main/canvas_item.cpp#L201): lazy global-transform getter; dirty notification propagation (1159).
[^b-insertion-bench]: Pinned Bevy [`world/spawn.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/benches/benches/bevy_ecs/world/spawn.rs#L12): direct/batched iteration scope; [`world/commands.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/benches/benches/bevy_ecs/world/commands.rs#L156): repeated insert and command application.
[^b-extract-bench]: Pinned Bevy [`extract_render_asset.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/benches/benches/bevy_render/extract_render_asset.rs#L13): empty asset and prepare implementation; scales (42), event generation and extraction timer (73–98).
[^g-quadrant-size]: Pinned Godot [`modules/tilemap/tile_map_layer.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.h#L399): default quadrant side; the rendering update cited above rebuilds each dirty quadrant's cells.
[^b-sprite-prepare]: Pinned Bevy [`crates/bevy_sprite_render/src/render/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/render/mod.rs#L555): batch/instance clearing (581), phase traversal (591), instance-buffer write (720).
[^g-render-timing]: Pinned Godot [`servers/rendering/rendering_server_default.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/servers/rendering/rendering_server_default.cpp#L76): setup time ends at 100, viewport drawing at 109; threaded draw dispatch (447–456).
[^b-transform-locality]: Pinned Bevy [`crates/bevy_transform/src/systems.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_transform/src/systems.rs#L81): configurable static optimization and dirty marking; parallel propagation subtree skipping (532, 715).
