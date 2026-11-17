# Large Tilemap Payload Ownership

Date: 2026-11-17. Status: source research with a qualified bounded persistent-record comparison;
frozen effective reads are implemented at `6ceb534` and qualified within the linked Windows scope.
Editor/UI/Dialogue remain paused.

The [owning follow-up](../plans/2026-11-13-rust-state-reconstruction-plan.md#closed-effective-values-and-large-payload-follow-up)
records the complete consumer comparison. Its separate
[isolated-migration correction](../plans/2026-11-13-rust-state-reconstruction-plan.md#preserve-isolated-migration-storage-during-defaults)
addresses a later copy of already owned data; it does not solve the current-version shared
payload case studied here. The source findings below remain pinned to the revisions in section 2.

## 1. Summary

At the pinned baseline, the immediate problem is **copy granularity**: inserting two small missing defaults
copies the entire generic Tilemap value, including `cells`. Component-local sharing has moved
that copy from capture to default materialization in the qualified workload. Neither Godot nor
Bevy establishes that moving authored cells into an external asset is necessary. Both supplied
snapshots separate large inline cell data from small configuration, although their runtime and
serialization mechanisms differ substantially. [^N-measure] [^N-defaults] [^G-storage] [^B-chunk]

Compare the persistent configuration/data boundary before selecting a new common read interface.
Preserving an unchanged `cells` subtree while materializing small effective fields is one candidate;
finer private storage requires complete borrowed consumption and explicit owned-materialization
boundaries under today's `&ComponentValue` interfaces. Separating configuration and inline cell
records is another candidate that can reuse current whole-component sharing. A domain-owned typed
encoding separately addresses source decoding. External
asset identity is a separate product decision, justified by independently reusable cell content,
not by the presence of a large array alone. These are Nara recommendations, not upstream guarantees.

## 2. Versions, Provenance, and Scope

| Source | Verified revision and local primary source | Version and interpretation |
| --- | --- | --- |
| Nara | `2dd008301e8a8ebd05e7a6a4d8e79412d3f5ca64`, research baseline | Component-local sharing landed at `bcc481fa3fcbbd991b1841315fd62ef2effcddc4`. The subsequent isolated-migration correction is `e2c2a38`; it does not replace this study's frozen comparison or establish a new measurement. |
| Godot | `7a37606a433c0e7735525037cdf774f043146a21`, `repo-ref/godot/` | `version.py`: 4.8.0 beta. Tilemap code is under `modules/tilemap/`, not an older `scene/2d/` path. [^G-version] |
| Bevy | `03ed5a27c92715c8fa2188f31f04e454ec39942f`, `repo-ref/bevy/` | Root package: `0.20.0-dev`, Rust 1.97.1. This pin **does contain official `TilemapChunk` support** in `bevy_sprite_render`; it is not evidence from `bevy_ecs_tilemap` or another extension. [^B-version] [^B-chunk] |

Both reference commits were verified with `git show -s --format=fuller <revision>`. Each reference
checkout's HEAD equals its pin, its working tree was clean, and its origin is respectively
`github.com/godotengine/godot` and `github.com/bevyengine/bevy`. References below identify files and
symbols in those exact supplied upstream snapshots. No fork or unmerged proposal supplies an
implementation claim here. All design alternatives in section 4 are proposals for Nara.

Nara's root lockfile resolves `bevy_ecs` and `bevy_reflect` to crates.io `0.20.0`; its workspace
requires Rust 1.95. The development snapshot is architectural evidence, not the source of those
locked packages or a recommendation to adopt its APIs/toolchain. Nara's persistent value/schema
and codec implementations, rather than Bevy Reflect alone, govern the affected path. [^N-version]
[^N-value] [^N-codec]

The initial source study uses local primary source, not moving documentation, search summaries,
or comparative benchmarks. It ran no Cargo, formatter, runtime, reference-checkout mutation, or
Git mutation. The later Nara-only probe below has its own execution scope.

## 3. Key Concepts and Source Findings

### Nara: the remaining copy is real default materialization

`ComponentPayload` privately wraps `Arc<ComponentValue>`. `SceneComponentRecord::clone` calls
`deep_clone`; only `clone_for_compilation` shares the main allocation. Mutation and owned extraction
isolate shared storage. Public value access exposes a borrow, not a detachable Arc. [^N-payload]
[^N-record]

The compiler calls `effective_component_value_with_default_overrides`. Its
`effective_component_value_cow_impl` preflights the complete effective value and invokes
`Cow::to_mut` when a default plan exists. Because `ComponentValue::Map` and `List` own recursive
`BTreeMap`/`Vec` children, converting this borrowed value to owned storage clones the entire tree.
The compiler then installs that owned result. Merely changing `Arc::make_mut` would not repair
this separate default-materializer boundary. Tilemap declares defaults for `sort_key` and
`y_sort_anchor`, and the measured authored data omits them. [^N-value] [^N-defaults] [^N-tile]

Recorded medians below are cumulative named wall envelopes per complete 16K lifetime, six
lifetimes per version, from the frozen sharing-only comparison. They are not per-frame numbers,
independent additive costs, or measurements made for this document. [^N-measure]

| Envelope | Before sharing | After sharing |
| --- | ---: | ---: |
| Record capture | 178.443 ms | 0.026 ms |
| Default materialization | 44.249 ms | 212.121 ms |
| Complete component materialization | 335.002 ms | 323.956 ms |
| Expansion, including nested Prefab acquisition | 938.288 ms | 910.035 ms |
| Source payload decode | 524.793 ms | 526.745 ms |

The capture hook now measures a different ownership operation. The report establishes no stable
whole-process speedup or physical-memory saving. Decode remains a separate measured envelope;
removing a later default copy would not itself remove generic source decoding. Its 526.745 ms
does not attribute all decode time to cell storage or establish the savings available from a
packed codec, typed payload, or external asset. [^N-measure]

### Godot: compact inline persistence, mutable runtime cells, independent properties

- **Storage:** `TileMapLayer` is a `Node2D` with `HashMap<Vector2i, CellData>` cells and a separate
  `Ref<TileSet>`. The `TileSet` is a Resource; the layer's placed cells are not thereby a separate
  external cell Resource. `TileMapCell` packs four 16-bit identifiers into a 64-bit union, while
  `CellData` also carries runtime bookkeeping. The union's size does not describe total runtime
  cell memory. [^G-storage] [^G-cell]
- **Serialization:** `_bind_methods` registers `tile_map_data` as a stored `PACKED_BYTE_ARRAY`.
  `get_tile_map_data_as_array` allocates a complete byte vector: for nonempty data, a two-byte
  format header and 12 bytes per cell. `set_tile_map_data_from_array` clears existing cells,
  decodes records and calls `set_cell` for each. It is not zero-copy instantiation or a partial
  serialized patch. Coordinates are serialized through signed 16-bit casts; copying this format
  would narrow Nara's `i32` coordinates and its tile/color semantics. The normal Variant text
  writer can encode packed bytes as base64; this remains inline data. [^G-wire] [^G-text] [^N-tile]
- **Sharing and cloning:** `Vector<T>` uses `CowData<T>`; `_ref` shares a buffer and `ptrw` requests
  separation when it has multiple owners. `Variant::reference` shares packed-array references.
  This is not a universal persistent Variant tree: Array/Dictionary ordinary copies share their
  container, and `Variant::recursive_duplicate` explicitly duplicates packed vectors. Godot's
  copy and Resource-locality policies are not Nara's public deep-Clone contract. [^G-cow]
  [^G-variant] [^G-dictionary]
- **Small defaults:** layer defaults such as `y_sort_origin = 0` and `rendering_quadrant_size = 16`
  are native members. `SceneState::instantiate` assigns stored properties individually; packing
  obtains a property's value and compares it to its default before deciding to omit it.
  There is no Nara-style whole component effective-map insertion in this path. Packing can still
  invoke the full tile-data getter; default elision does not prove cost-free serialization.
  Resource localization and Array/Dictionary duplication also occur during instantiation.
  [^G-storage] [^G-scene]
- **Updates and rendering:** `set_cell` changes one map entry, marks that cell dirty and queues an
  update. `_rendering_update` rebuilds dirty rendering quadrants, but initial/TileSet/shape changes
  can visit all cells or recreate all quadrants. Non-Y-sorted grouping uses the configured size;
  Y sorting groups by the computed vertical origin instead. Quadrants can contain multiple canvas
  items by material/Z. Canvas-item rectangle culling occurs downstream. This is update/render
  locality, not evidence that offscreen source cells are skipped during decode. [^G-update]
  [^G-render] [^G-cull]

### Bevy: official typed chunks, with explicit whole-chunk work

- **Storage and assets:** `TilemapChunk` holds size/display/tileset/alpha configuration;
  `TilemapChunkTileData` separately owns `Vec<Option<TileData>>`. Both are components; the latter
  derives ordinary `Clone`. The tileset is `Handle<Image>`. The insertion hook validates the vector
  length, packs all entries, creates a tile-data Image and material asset, and attaches a cached
  rectangular mesh. Asset-backed render data does not make authored CPU cells an external asset.
  [^B-chunk] [^B-material]
- **Packed output is not a source codec:** `PackedTileData` contains a `u16` tileset index, four
  color bytes and `u16` flags. `make_chunk_tile_data_image` copies those bytes into an `Rgba16Uint`
  Image retained for both main and render worlds. The chunk module does not establish a compact
  on-disk cell format or a complete Tilemap authoring pipeline. Generic Reflect list serialization
  visits each element; Vec's Reflect cloning and `FromReflect` construction also build elements
  individually. `Reflect` alone is not proof of shared payload decoding. [^B-material] [^B-list]
- **Updates:** `update_tilemap_chunk_indices` selects `Changed<TilemapChunkTileData>`, repacks its
  entire vector, clears the corresponding Image bytes and extends them with the complete packed
  result. There is no dirty-cell range upload in that function, and its query has no visibility
  filter. Chunk size defines this update scope; a single edited cell can cause whole-chunk work.
  [^B-chunk]
- **Scene/default granularity:** this pin's scene system uses `Scene`, `TemplatePatch`,
  `ResolvedScene` and `ScenePatch`. `get_or_insert_erased_template` clones an existing cached
  template when accessed for patching, or constructs its default. The blanket `Template` for
  `Clone + Unpin` calls `clone` both for building and cloning a template. Thus cached COW is at
  template granularity, not an automatic field-level persistent tree. Separate config and data
  components avoid coupling their templates. Separately, ECS Reflect fallback tries
  `FromReflect`, then default-plus-apply, then `FromWorld`-plus-apply; that is not proof of a
  cheap missing-field operation on a large component. [^B-scene] [^B-template] [^B-reflect]
- **Lifetime and visibility:** `ScenePatch::resolved` retains an `Arc<ResolvedSceneRoot>`;
  `Handle::Strong` shares a strong-handle allocation while `Assets<A>` stores mutable data.
  This does not supply immutable generation snapshots or Nara's retained-budget accounting.
  Chunk rendering uses `Mesh2d`: the ordinary 2D bounds system computes mesh bounds, visibility
  tests entity bounds per view, and material queuing consumes visible Mesh2d entities. The shader
  selects tiles within that rectangle and discards empty/hidden entries. This supports
  chunk/entity visibility, not a per-cell CPU culling or Nara-compatible per-cell sort contract.
  [^B-scene] [^B-handle] [^B-cull] [^B-shader]

## 4. Implementation Guide: Compare the Ownership Choices

The table evaluates proposals against the observed Nara path, not against unmeasured upstream
speed. The governing mechanisms are the recursive value and codec boundaries, typed Tilemap
decoder, and existing source/compiled ownership. [^N-value] [^N-codec] [^N-tile] [^N-record]

| Choice | What it can address | What it leaves or costs | Fit for the next unit |
| --- | --- | --- | --- |
| Keep whole-component private Arc | Read-only capture and unchanged compiled/prepared sharing already work. | Any real owned effective-map update still copies `cells`; source decoding and typed runtime construction remain. | Qualified baseline/control, not a solution to the missing-default copy. |
| Finer private ComponentValue container/subtree storage | Detach a small root map or changed path while retaining unchanged `cells`; preserve generic inline data and public deep Clone. | Current enum containers, borrowed `value()` and codec signatures demand a complete ordinary tree. A private tree plus automatic flattening would relocate the copy. Nested edits may still copy an entire cell list unless a finer unit is separately justified. | Conditional candidate requiring complete borrowed consumption and justified codec/derive migration. No per-scalar Arc or general persistent-tree library is selected. |
| Separate persistent configuration and cell records, retaining the current cell encoding | Prevent configuration defaults from taking ownership of cell data while reusing existing whole-component sharing. | Native-only struct separation occurs too late. Persistent Schema, Prefab/override paths, source upgrade, component ergonomics and receiving/native joins must be reconciled without rebuilding the original monolithic generic tree. | First bounded vertical comparison for the named Tilemap consumer. It tests ownership granularity independently of a packed encoding and is not assumed cheap. |
| Domain-owned typed cells, inline or in a cell resource | Separate configuration/defaults from typed payload; permit a domain codec/packed format and independently chosen cell/chunk mutation units. | Decoding into typed cells after full generic normalization cannot remove that earlier copy. Earlier typed admission requires complete validation, encoding, override, migration and retained ownership contracts. A resource adds an explicit reference/lifetime relation. | Strong alternative for Tile-specific representation pressure; keep inline ownership viable. No measured decode improvement follows from typing alone. |
| External cell asset plus small component reference | Express actual cross-Scene reuse and independent content lifetime/publication. | Adds identity, dependency, source/processed delivery, edit isolation and generation custody. Moving the same generic tree into an asset can retain the same decode/copy work elsewhere. | Choose for a named independent reuse/lifecycle consumer, not as the default response to size. |

**Recommended next unit:** decide the Tilemap payload boundary using its complete existing
consumer before selecting engine-wide interfaces. Trace source decode, effective defaults,
reference/receiving validation, compiled storage, native preparation and cook. First compare
separate persistent configuration/cells with unchanged List/Map cell encoding. This tests whether
existing component-local sharing already solves the coupling at a better boundary. A runtime-only
split cannot fix a compiler copy, and a later monolithic reassembly would reintroduce it.
Preserve native/Prefab/override semantics and explicitly account for Schema/source-upgrade costs.
[^N-record] [^N-value] [^N-codec] [^N-defaults]

Frozen overlays remain appropriate if actual mixed-value/override/reference requirements justify
retaining one generic component. Today's borrowed codec interface cannot read separately retained
children as an ordinary tree; that candidate requires a complete admitted view and codec/derive
migration. Lazy flattening remains a full copy and retained allocation. A generic List plus missing
defaults is useful mechanism coverage but does not itself justify imposing a new read interface
on every domain. Compare a compact inline format only against attributed remaining decode work;
do not simultaneously change encoding and ownership and then assign all savings to either one.

Keep public record/document Clone as recursive independent allocation. An explicit public owned
materialization may deep-copy; ordinary internal compilation must not accidentally call it.
Avoid maintaining two permanent full representations. Select one complete production direction
after the comparison; do not require the full generic subsystem before evaluating the domain
boundary. A separate cell asset remains optional. Godot's inline packed
property and Bevy's separate data component demonstrate alternatives to compulsory extraction;
they do not prove which Nara implementation wins. [^N-record] [^G-wire] [^B-chunk]

Evaluate the original authored gameplay/geometry, including missing defaults. The view candidate
keeps source bytes. A record/format change must identify semantically equivalent new input and
report old-format upgrade separately from ordinary new-format loading. Retain actual component
and phase counts when representation changes. Do not populate the omitted defaults in the fixture
to avoid the operation being investigated. [^N-measure] [^N-receive]

### Existing Nara Seams That The Record Split Must Reconcile

A configuration/cells split has concrete integration costs. These are current source findings,
not a selected replacement or evidence that the split is cheaper than a view:

| Current seam | Consequence for the comparison |
| --- | --- |
| `Tilemap` owns configuration, `cells` and dirty chunk revisions; its registered codec creates exactly that native component. | A native-only wrapper split cannot affect the earlier generic copy. A persistent split must define native ownership, cell editing/dirty updates and ordinary game ergonomics explicitly. [^N-tile] |
| `ProjectContentLoader::validate_tilemap_membership` reads one compiled Tilemap record, then checks that record against actual admitted TileSets. | Separate records need a domain-owned same-entity join for cells and configuration. Preserve complete coordinate/index checks and atlas authority without building another monolithic generic value. [^N-membership] |
| `PreparedComponentCandidate<T>` produces exactly `T`; persistent admission rejects implicit required components and lifecycle hooks. | Do not insert a peer component from a codec or use a required-component hook to hide the split. Each explicit native value needs its original admission. Missing configuration/data and empty-map behavior require an explicit contract. [^N-native-candidate] |
| A registered component migration consumes and returns one `ComponentValue`. | That callback cannot turn one stored component into two entity records. Identify a complete source-upgrade route for direct records, Prefab additions/sets/properties and version/owner metadata; do not silently ignore legacy `cells` or introduce a universal migration executor solely for this comparison. [^N-migration] |
| Stock extraction queries one `Tilemap` and iterates its cells; runtime cook owns independent full values. | Preserve original spatial/order/material authorization and frozen render inputs when joining native data. Keep cooked output independently owned. Neither a second render owner nor old-source retention follows from splitting the data. [^N-render] [^N-receive] |

This trace makes migration and native/public composition part of the decision. If those required
semantics cannot be preserved reasonably, they are concrete evidence in favor of the closed-view
candidate. Conversely, retaining current generic cell encoding during this comparison prevents
an encoding change from being mistaken for an ownership benefit.

### Bounded Persistent-Record Comparison

The [test-only consumer](../../samples/two-level-game/tests/tile_payload_trial.rs) at `7088b9e`
reuses the ordinary game's actual 16,384-cell source generator. It keeps Meadow's direct `checkpoint`
record and the `portal` root-bound Prefab, original source declarations/keys and cell List/Map
encoding. It omits the other four Scene members and all non-Tilemap components, including
Transforms; it is not a complete game, world-geometry, atlas-admission, rendering or cost acceptance.
The stock control contains two Tilemap records; the trial contains two explicit Configuration
and two explicit Cells records on the same two entities. The probe starts from source revision
`531d3e7`; the frozen runtime comparison
program remains `2dd0083`, and the source-only reference pins above are unchanged.

Configuration retains stock fields and defaults except `cells`. Cells has one required List
without a default. Both use test-local native codecs. The populated fixture's conversion is
explicitly limited to those records, with no owner/contextual metadata or instance overrides;
it preserves the omitted `sort_key` and `y_sort_anchor`. It is not a general legacy source
converter. Compilation, native preparation, cancellation, cook and readmission use the public
Scene owners. A temporary comparison Tilemap checks complete native data and dirty revisions;
borrowing the stock TileSet handle for that comparison does not establish actual trial asset
resolution or renderer support.

The discriminators are original raw/compiled allocation identity through default insertion,
complete cell/native values, canonical provenance, independent cooked storage, aggregate exact
and one-short node/byte/depth receiving limits, source preservation and original retirement.
A Cells-only negative control intentionally demonstrates that ordinary independent component
registration does not supply or require Configuration. No hidden peer insertion is introduced.
The first complete cook attempt exposes the separate default processed-wire node ceiling:
the flattened 16K trial reaches 1,000,001 nodes against a 1,000,000 limit. The source generator's
two 8K files do not prove that the combined processed wire fits one file's default bound. The
corrected comparison requires that default refusal for both stock and split controls, then
requests the same explicit finite 2,000,000 wire nodes for both complete positive cooks and trial
readmission. All other original wire/value/work/provenance ceilings remain unchanged. This
does not increase product defaults or qualify the full 16K game under default cook limits.
The corrected Windows/Rust1.95 release consumer passes 5/5 without skips, including both new
ownership/receiving cases and the original three source-generator controls. The full direct and
root-bound values survive source edits, native preparation and independent cook/readmission.
Exact aggregate node/byte allowances and depth 6 succeed; one-short nodes/bytes and depth 5
refuse with their actual error kinds and no target publication. Independent review closes the
initial depth-boundary coverage gap and finds no remaining P1/P2, including the explicit wire
budget correction. This establishes the proposed granularity's mechanical feasibility, not
cheaper complete production integration or a selected representation. Structural pairing,
source upgrade and native/render joining remain unresolved costs.
Execution and review results belong in the
[owning plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#bounded-persistent-tilemap-record-probe).

The parallel source-upgrade trace identifies a separate contract: today's per-component migration
returns one value under one component ID. Finite source tools write selected fields, live export
flattens source, and Prefab conversion localizes inheritance; none preserves this general
declaration transformation.
The existing reviewed schema-upgrade publication/recovery shell preserves field-level record
counts and currently admits a data-owner successor. Extending it for native record splitting is
additional product work, not an automatic consequence of registering two native codecs.

If a persistent split is selected, direct entities and additions need both explicit records;
Set Add/Replace and Suppress must preserve their target and mode on both sides. Properties must
partition by stable root field ID while retaining Absent/Set/default-equal pins and collection
selectors, without replacing inherited data with a flattened Set. Nested additions, claims and
dormant overlays require the same treatment. Keys, IDs, placements, `through` and `expected_base`
remain unchanged; omission versus explicit empty cells needs a rule. Accepted upgrade history
uses a reviewed new checkpoint, not arbitrary replay of an old Undo stack after a format change.
These are integration costs to compare with a closed borrowed-view design, not a selected new
migration framework. Relevant owners are the
[component migration API](../../crates/nara_reflect/src/registry.rs),
[reviewed source-upgrade proposal](../../src/project_host/authoring/project/source_upgrade/proposal.rs),
[schema-upgrade lowering](../../crates/nara_scene/src/authoring_decode/schema_upgrade.rs) and
[Prefab overlays](../../crates/nara_scene/src/prefab/overlay.rs).

### Production Direction After The Probe

The selected next implementation keeps one persistent record and the complete native Tilemap,
with frozen effective reads. This selects the implementation to qualify, not a measured cost
winner. The [owning successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#frozen-effective-reads-through-the-existing-component)
requires complete codec, receiving, native and cook integration plus the unchanged full game
and balanced frozen-program comparison. That scoped implementation and qualification are complete;
validation locality remains the next optimization.

The production source comparison provides concrete reasons for retaining the current domain
composition. Native Tilemap mutations already change cells without copying configuration or
cells. Dirty revisions currently do not limit stock extraction, which reads configuration and
all cells each time. A native split would permit independent borrowing/change tracking, but no
current consumer requires it. It also creates a presence contract: suppressing only configuration
can leave Cells behind, so content validation must inspect either record, not just configuration.
The split would change ordinary `set_cell` usage, native registration, project and processed
membership joins, extraction and real consumers. The original captured render inputs already own
their data and must not begin querying live configuration/cells later.

The source-upgrade proposal can reuse the existing publication/recovery owner, but needs a new
exact one-to-two declaration proof across Scene, Tooling and ProjectHost. Required Cells without
a default cannot preserve a legal old `Absent(cells)` simply by writing `Set([])`. Retaining the
old type ID at a higher version requires a registered migration chain that cannot return both
records; retiring it instead requires explicit predecessor/tombstone metadata and two successors.
Neither policy follows from the green test-only probe. The known four Scene/Prefab files contain
11,148,694 bytes in the existing 16K report. Three recovery copies alone total 33,446,082 bytes,
leaving 108,350 bytes below the current 32 MiB recovery aggregate before both schema bundles and
other overhead. Thus a complete upgrade may also need explicitly selected operation/recovery
limits. This is source evidence and arithmetic, not an executed upgrade.

A compact inline alternative addresses a separate decode cost. A lossless ordinary row would
retain i32 coordinates, a u32 index and four authored f64 colors: 44 bytes per row before headers
and text encoding. Native f32 conversion occurs later. Current opaque cell Maps also admit extra
members, so copying only those seven numbers would discard retained source data. A complete
format needs bounded extensions, one lossless alternative representation or an explicit refusal
policy. Base64 inside an ordinary String is not sufficient: receiving node/depth checks run before
the domain codec. A new packed primitive must supply independently validated complete logical
cost, traversal, reference and scratch policies in Reflect/Scene. Even that format still copies
the entire block under the current default materializer. This is not chosen as a local codec fix.

Frozen reads instead keep source wire, omissions, component/field IDs and native ergonomics.
Reflect freezes its existing planner's selected insertions; complete effective data remain closed
after compilation. The borrowed Interface reads raw unchanged subtrees and those frozen branches
without a receiving-Schema lookup. Actual writes and independent ownership explicitly materialize
after admission. This has a substantial cross-consumer migration cost; the implementation must
not disguise it as a small private Arc change or flatten before actual codecs. Full logical work,
receiving checks and independent source/compiled/prepared payload/backing fees remain required.

The implemented successor now qualifies actual borrowed codec reads, original direct/inherited
cell allocation identity and complete game/GPU lifetimes within the
[owning plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#frozen-effective-reads-through-the-existing-component).
The [balanced original-program comparison](../benchmarks/2026-11-14-development-runtime-costs.md#frozen-effective-reads)
reduces the default wall envelope but increases complete validation/traversal cost. Complete
materialization and expansion show no stable end-to-end gain. This supports the storage boundary,
not a physical-memory or speedup claim; the next optimization must attribute repeated admission
and raw-subtree read work without relaxing receiving or retained-owner contracts.

## 5. Validation At Owning Boundaries

The following source investigation was added after the frozen-read qualification at Nara
`c1edb21`. Both reference checkouts remain clean at the section 2 pins. It investigates runtime
value validation, rather than the amount of testing needed for unchanged upstream code. No
upstream build or benchmark was run, and this investigation changes no production validator.

### Godot: Parsing, Resource Resolution And Property Assignment

`VariantParser::_parse_array` and `_parse_dictionary` recursively parse members and check syntax,
delimiters and premature EOF. These routines grow their containers while parsing; they do not
carry a cumulative value-node, byte or depth allowance. The `MAX_RECURSION` checks elsewhere in
the same file belong to the writer and must not be credited to this parser. Resource parsing
checks declared external/internal IDs and dependency loading, with configurable missing-resource
behavior. A cache-reuse branch deliberately keeps an existing resource without assigning its
properties again. [^G-validation-parser] [^G-validation-loader]

Scene text loading stores node properties in `SceneState`. Instantiation checks local scene
structure, creates nodes and assigns stored properties with `Object::set`. It does not apply a
general whole-value budget pass before every assignment. Some assignment failures and missing
dependencies do not abort the complete load/instantiation. Consequently, successful loading is
not proof that every stored property satisfied a strict native constraint. [^G-validation-loader]
[^G-validation-instantiation]

`Object::validate_property` accepts `PropertyInfo`, not a proposed value. It is a metadata hook,
not the equivalent of Nara's value validator. Concrete setters own concrete constraints:
`Timer::set_wait_time` rejects values at or below zero, independently of its Inspector range hint.
Godot's metadata, conversion, setter and error-propagation policies should not be conflated into
one strict validation guarantee. [^G-validation-property]

### Bevy: Typed Construction, Resolved Scenes And Changed Data

The supplied Bevy pin has the `Scene`/`Template`/`ResolvedScene` system. It is not the older
`DynamicScene` file-loader implementation. Separately, its Reflect deserializer resolves a type
registration and deserializes members using that type. `visit_struct` rejects unknown fields;
`ListVisitor` deserializes each member with the registered element type. Custom deserializers and
processors can own different policies. This code does not establish universal required-field,
numeric-range or aggregate work/retention-budget enforcement. [^B-validation-deserialize]

`ScenePatch::resolve` retains an `Arc<ResolvedSceneRoot>`; `spawn` requires that resolved result
and applies its templates. Template construction can return errors for contextual requirements.
The blanket `Template` implementation for `Clone + Unpin` simply clones the typed value.
`ErasedTemplate::apply` builds the output and applies its scene effect; this route does not
reserialize the value into a generic tree for another universal shape/cost pass. Reflect-based
ECS insertion is a separate route: `from_reflect_with_fallback` tries `FromReflect`, then
default-plus-apply and `FromWorld`-plus-apply, and can panic on invalid registrations or failed
construction. Neither route promises validation of every domain invariant. [^B-scene]
[^B-template] [^B-reflect] [^B-validation-apply]

The stock Tilemap chunk checks data length against chunk dimensions in its insertion hook and
in the update system filtered by `Changed<TilemapChunkTileData>`. It reports an invalid length
and skips construction/update; valid changed data are completely repacked. This is a concrete
check at use/change, not a generic scan of all unchanged chunks every frame. It is also not a
general incremental-validation or memory-budget mechanism. [^B-chunk]

### Nara: Separate Necessary Checks From Repeated Traversal

The current compiler admits a source value, materializes defaults and then invokes
`validate_component_value_with_limits`. The default freezer has already obtained the complete
effective cost. For a frozen read, that validator nevertheless walks the complete value again
through `validate_value_materialization_budget` before checking field coverage and shape.
The semantic checks remain necessary because freezing defaults alone does not validate the
whole component; the repeated cost traversal is a distinct operation to remove. Its raw
no-insertion branch already trusts the caller's raw admission, so the two storage choices now
have unnecessarily different cost behavior. [^N-validation-current]

Preparation also has genuinely different contexts. A source-compiled input currently performs
complete receiving validation. A processed Runtime candidate already reuses pure admission
after checking its exact immutable definitions witness, while still checking receiving World
and asset state. This existing Runtime path demonstrates a local reuse mechanism; it does not
authorize trusting an arbitrary borrowed value or a candidate admitted against another schema.
Reference planning also scans for undeclared reference leaves, including opaque payloads. Such
a scan has a semantic purpose until the unchanged value's original owner retains that fact;
it cannot be removed merely because no schema-declared reference exists. [^N-validation-current]

The measured semantic-validation wall envelope increased from 1.377 to 76.514 ms in the
[balanced frozen-program comparison](../benchmarks/2026-11-14-development-runtime-costs.md#frozen-effective-reads).
Defaults fell from 242.509 to 94.931 ms, but complete materialization remained
371.721/367.683 ms before/after. These are cumulative named wall envelopes, not isolated
benchmarks of one validator. They justify investigating redundant scans and view traversal;
they do not assign the entire regression to one call or establish a future speedup.

The next implementation should use the following division of responsibility:

| Situation | Required checks and reuse |
| --- | --- |
| File, public dynamic value or arbitrary migration/extension output | Check format, structural limits, type/field semantics and references before the receiving owner relies on the value. Known project input still needs diagnostics for ordinary mistakes. |
| Unchanged effective value inside its original compiler operation | Reuse the default freezer's complete cost and the already admitted schema. Validate component semantics once at the point they become complete. Do not repeat a full cost walk to establish the same fact. |
| Property edit, migration, default insertion or reference rewrite | Re-establish facts invalidated by that transformation. A callback returning arbitrary data cannot inherit the original value's admission. Do not assume a changed result is valid solely because the input was valid. |
| Closed candidate with exact definitions at preparation | Reuse applicable pure value facts through its existing private owner. Check current World, namespace, product membership and reservations where they are actually consumed. A different definition or stricter receiving policy still requires its own admission. |
| Commit after asynchronous preparation | Recheck mutable owner/generation/resource facts needed to publish. Do not use a complete immutable-payload traversal as a substitute for those current-state checks. |

The scoped compiler correction at `9fc9322` separates explicit-field semantics from the repeated budget
walk after defaults; its [qualification and paired comparison](../benchmarks/2026-11-14-development-runtime-costs.md#compiler-validation-locality)
record the actual original game and unchanged 16K workflow. Standalone and receiving admission
keep their own paths. Broader preparation reuse remains a follow-up. Prefer Rust ownership and
the existing compiler/candidate state to a public `unchecked` API, global
address/hash cache or general validation-certificate framework. Logical size and retained
allowances can be checked from applicable admitted facts; work counters should account for the
operations actually performed, rather than preserve removed scans. Tests should protect the
receiving limits and failure behavior, without making an incidental traversal count a permanent
product requirement. Detailed accounting machinery itself needs a concrete bounded-operation
consumer; neither reference engine makes it a prerequisite for normal game execution.

### Current Ownership And Escape Contracts

These are the existing Nara contracts to reconcile during implementation, rather than a reason
to keep every current validation pass. Neither inspected upstream path supplies the same
accounting contract. [^N-contract] [^N-retain] [^N-custody] [^N-receive]

| Boundary | Required behavior |
| --- | --- |
| Public copies and escapes | Deep-copy public record/document values, including contextual defaults. A public clone must remain readable after original reservations refund without retaining that original allocation. A private share must not escape as an uncharged owning handle. |
| Receiving/materialized value | Retain complete per-occurrence node/byte/depth limits, schema owner/version/shape and reference checks. A shared or typed payload cannot become a one-node exemption. Receiving a compiled value must not silently add newly declared defaults or run a new migration. |
| Cumulative work | Admit actual traversal, reference interpretation, scratch and copying. Reuse valid facts when the original owner and its unchanged data justify it; do not require another traversal merely to charge the same logical size. Changes to accounting/refusal behavior need an explicit implementation record. |
| Retained ownership | Source, compiled and independently prepared owners retain full logical payload allowances and backing overhead independently. Do not deduplicate charges based on Arc identity. Refunding an original reservation must never leave an independently retained share without its admitted owner. |
| Retirement and reachability | Payloads own only their data, not their raw Prefab, resolver, snapshot or candidate. Preserve raw-Prefab release while escaped SceneSource/prepared data remain valid, existing generation capacity and final refund at the last actual owner. |

Existing compiler source charges metadata and complete value work before sharing, and reserves
default-program temporary storage before materialization. Source and compiled retention separately
include both the full payload and backing fee. A locality refactor must identify which facts remain
valid and which checks/charges it removes. Exact/one-short receiving controls remain meaningful;
tests tied to eliminated internal work require reviewed new expectations. This investigation
changes no limits, counters or refusal ordering. Existing tests are source evidence here, not rerun results.
[^N-work] [^N-retain] [^N-custody] [^N-sharing-tests]

## 6. Common Issues and What the Sources Do Not Prove

- **A faster capture is not a removed lifetime copy.** The existing report already shows the copy
  moving to defaults. Neither these sources nor this document predicts an end-to-end gain, a
  physical-memory reduction, frame-tail improvement, or a numeric production budget. [^N-measure]
- **Rendering locality does not repair source ownership.** Nara's `Tilemap` already tracks dirty
  chunk revisions, but stock extraction walks all cells; queue clipping occurs after extraction.
  Godot quadrants and Bevy chunks answer different update/render questions. A payload-only change
  must preserve current extraction, material/resource authorization and ordering; it does not
  establish a retained render cache or offscreen decode skipping. [^N-render] [^G-render] [^B-chunk]
- **Packed formats encode domain choices.** Godot's 16-bit coordinate encoding and Bevy's
  `u16` tile indices/byte colors are not Nara's `i32` coordinates, `u32` indices and `Color` contract.
  Similar storage shapes do not establish wire, precision, Y-sort, collision or navigation parity.
  No upstream benchmark or large-cell memory experiment was executed. [^G-wire] [^B-material] [^N-tile]
- **A handle is not an immutable receipt.** Bevy documents shared mutable assets; Godot explicitly
  localizes/duplicates some Resources when instantiating. Neither is evidence that Nara can reuse
  a mutable cell resource across generations while preserving older prepared/retained inputs.
  A cell resource would need an explicit immutable payload/edit boundary. [^B-handle] [^G-scene]
  [^N-contract]
- **Acceptance remains the original complete consumer.** A later implementation must retain the
  receiving/work/retention refusal controls and original Tile travel/source-removal/retained-input
  GPU replay/cleanup behavior recorded by the qualified unit. A default-only microprobe or source
  review cannot replace those lifetimes. This research adds no new acceptance run and reopens no
  UI/Dialogue work. [^N-measure] [^N-custody] [^N-receive]

## 7. Primary References

All `N-*` references bind to Nara revision `2dd008301e8a8ebd05e7a6a4d8e79412d3f5ca64` except
`N-validation-current`, which uses `c1edb21`, and linked measurements naming their frozen program.
Local Nara paths are primary; remote
Godot/Bevy links below pin the exact local source inspected. Links are not claims that a later
release preserves these mechanisms.

### Nara

[^N-version]: [Cargo.toml](../../Cargo.toml), workspace Rust and Bevy dependency declarations;
    [Cargo.lock](../../Cargo.lock), `bevy_ecs` / `bevy_reflect` package records.
[^N-measure]: [Development Runtime Costs](../benchmarks/2026-11-14-development-runtime-costs.md#component-local-payload-sharing),
    section `Component-Local Payload Sharing`, named wall-envelope table and frozen comparison scope.
[^N-payload]: [component_payload.rs](../../crates/nara_scene/src/component_payload.rs),
    `ComponentPayload::{deep_clone, make_mut, into_owned, retained_bytes}`.
[^N-record]: [document.rs](../../crates/nara_scene/src/document.rs),
    `SceneComponentRecord::{clone, value, value_mut, clone_for_compilation, into_value}`.
[^N-value]: [value.rs](../../crates/nara_reflect/src/value.rs), `ComponentValue`, derived `Clone`,
    `cost_up_to_with_admission_measured`.
[^N-defaults]: [effective.rs](../../crates/nara_reflect/src/effective.rs),
    `effective_component_value_with_default_overrides`, `effective_component_value_cow_impl`;
    [staged/values.rs](../../crates/nara_scene/src/prefab/overlay/compile/staged/values.rs),
    `Compiler::materialize_defaults`.
[^N-tile]: [nara_tilemap/lib.rs](../../crates/nara_tilemap/src/lib.rs), `TileCoord`, `TileIndex`,
    `TileCell`, `Tilemap`, `tilemap_fields`, `register_tilemap_components`, `read_cells`,
    `build_prepared_tilemap`.
[^N-codec]: [registry.rs](../../crates/nara_reflect/src/registry.rs),
    `ComponentRegistryBuilder::register_persistent_component_codec_with_context`, preflight signature.
[^N-contract]: [ADR 0117](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md),
    paragraphs beginning `Source component values may use component-local immutable storage` and
    `Source, compiled and independently prepared owners`.
[^N-work]: [staged/values.rs](../../crates/nara_scene/src/prefab/overlay/compile/staged/values.rs),
    `Compiler::{materialize_program, reserve_program_temporary}`, `value_work_bytes`.
[^N-retain]: [scene_retention.rs](../../src/scene_retention.rs),
    `compiled_scene_retained_bytes`, `component_record_retained_bytes`.
[^N-custody]: [tilemap_custody.rs](../../src/project_content/tests/tilemap_custody.rs),
    `large_root_bound_tilemaps_release_original_documents_and_budget_at_their_last_owners`.
[^N-receive]: [compiled_receiver_tests.rs](../../crates/nara_scene/src/spawn/preparation/compiled_receiver_tests.rs),
    `added_receiving_default_requires_source_recompilation`,
    `receiving_version_requires_source_recompilation_without_activation_migration`,
    `receiving_local_references_include_empty_entities_after_source_sorting_and_limit_rejection`.
[^N-sharing-tests]: [payload_sharing.rs](../../crates/nara_scene/src/prefab/overlay/compile/component_program_tests/payload_sharing.rs),
    `migration_and_default_insertion_preserve_sources_and_previous_compilations`,
    `data_preparation_keeps_old_values_after_candidate_drop_and_enforces_full_receiving_limits`;
    [component_payload/tests](../../crates/nara_scene/src/component_payload/tests/mod.rs),
    `public_record_and_document_clones_do_not_retain_original_storage`.
[^N-render]: [nara_tilemap/lib.rs](../../crates/nara_tilemap/src/lib.rs), `Tilemap::{mark_dirty, dirty_chunks}`;
    [extract.rs](../../crates/nara_sprite_render/src/extract.rs), `extract_sprites`;
    [queue.rs](../../crates/nara_sprite_render/src/queue.rs), `queue_sprites`, `clip_quad_may_intersect`.
[^N-membership]: [project_content.rs](../../src/project_content.rs),
    `ProjectContentLoader::validate_tilemap_membership`;
    [admission.rs](../../crates/nara_tilemap/src/admission.rs),
    `prepare_persisted_tilemap_membership`, `tilemap_membership_work_bytes`.
[^N-native-candidate]: [persistent_apply.rs](../../crates/nara_reflect/src/persistent_apply.rs),
    `PreparedComponentCandidate`, `PersistentApplyRejection::RequiredComponents`;
    [ECS metadata admission](../../crates/nara_ecs/src/__private.rs),
    `validate_persistent_component_registration`, `validate_registered_persistent_component_apply`.
[^N-migration]: [registry.rs](../../crates/nara_reflect/src/registry.rs),
    `register_component_migration`, `migrate_component_value_owned`;
    [schema_authority.rs](../../crates/nara_scene/src/schema_authority.rs), `SchemaAuthority::migrate_owned`.

### Godot at `7a37606a433c0e7735525037cdf774f043146a21`

[^G-version]: [`version.py`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/version.py),
    `major`, `minor`, `patch`, `status`.
[^G-storage]: [`modules/tilemap/tile_map_layer.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.h#L105),
    `CellData`, `TileMapLayer::{tile_map_layer_data, tile_set, y_sort_origin, rendering_quadrant_size}`.
[^G-cell]: [`modules/tilemap/tile_set.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_set.h#L62),
    `TileMapCell`, `TileSet : Resource`.
[^G-wire]: [`modules/tilemap/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.cpp#L3207),
    `TileMapLayer::{set_tile_map_data_from_array, get_tile_map_data_as_array, _bind_methods}`.
[^G-text]: [`core/variant/variant_parser.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/variant_parser.cpp#L2196),
    `VariantWriter::write`, `PACKED_BYTE_ARRAY` branch.
[^G-cow]: [`core/templates/vector.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/templates/vector.h#L71),
    `Vector::_cowdata`, `ptrw`, copy construction;
    [`core/templates/cowdata.h`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/templates/cowdata.h#L513),
    `CowData::{_copy_on_write, _ref}`.
[^G-variant]: [`core/variant/variant.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/variant.cpp#L1269),
    `Variant::reference`, packed-array branch;
    [`core/variant/variant_setget.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/variant_setget.cpp#L1982),
    `Variant::recursive_duplicate`.
[^G-dictionary]: [`core/variant/dictionary.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/dictionary.cpp#L302),
    `Dictionary::_ref`, `clear`;
    [`core/variant/array.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/array.cpp#L56),
    `Array::_ref`.
[^G-scene]: [`scene/resources/packed_scene.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp#L507),
    `SceneState::instantiate` property assignment/localization;
    same file at [property packing](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp#L964),
    `SceneState::_parse_node`, default comparison at line 1063.
[^G-update]: [`modules/tilemap/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.cpp#L2769),
    `TileMapLayer::set_cell`.
[^G-render]: [`modules/tilemap/tile_map_layer.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.cpp#L229),
    `TileMapLayer::_rendering_update`, `_rendering_quadrants_update_cell` at line 526.
[^G-cull]: [`servers/rendering/renderer_canvas_cull.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/servers/rendering/renderer_canvas_cull.cpp#L260),
    `RendererCanvasCull::_attach_canvas_item_for_draw`, `_cull_canvas_item`.

### Bevy at `03ed5a27c92715c8fa2188f31f04e454ec39942f`

[^B-version]: [`Cargo.toml`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/Cargo.toml#L1),
    root package version and `rust-version`.
[^B-chunk]: [`crates/bevy_sprite_render/src/tilemap_chunk/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/tilemap_chunk/mod.rs#L51),
    `TilemapChunk`, `TileData`, `TilemapChunkTileData`, `on_insert_tilemap_chunk`,
    `update_tilemap_chunk_indices`.
[^B-material]: [`tilemap_chunk/tilemap_chunk_material.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/tilemap_chunk/tilemap_chunk_material.rs#L28),
    `TilemapChunkMaterial`, `PackedTileData`, `make_chunk_tile_data_image`.
[^B-list]: [`bevy_reflect/src/impls/alloc/vec.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/impls/alloc/vec.rs#L9),
    `impl_reflect_for_veclike!`;
    [`impls/macros/list.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/impls/macros/list.rs#L1),
    generated `reflect_clone` and `FromReflect::from_reflect`;
    [`serde/ser/lists.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/ser/lists.rs#L13),
    `ListSerializer::serialize`.
[^B-scene]: [`bevy_scene/src/scene.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/scene.rs#L49),
    `Scene`, `TemplatePatch::resolve`;
    [`resolved_scene.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/resolved_scene.rs#L472),
    `ResolvedScene::get_or_insert_erased_template`;
    [`scene_patch.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/scene_patch.rs#L20),
    `ScenePatch::{resolved, resolve, spawn}`.
[^B-template]: [`bevy_ecs/src/template.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_ecs/src/template.rs#L390),
    blanket `Template::{build_template, clone_template}` and `FromTemplate` implementations.
[^B-reflect]: [`bevy_ecs/src/reflect/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_ecs/src/reflect/mod.rs#L108),
    `from_reflect_with_fallback`.
[^B-handle]: [`bevy_asset/src/handle.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_asset/src/handle.rs#L97),
    `Handle`, `StrongHandle::drop`, documented shared mutation semantics;
    [`bevy_asset/src/assets.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_asset/src/assets.rs#L288),
    `Assets::{add, get_mut}`.
[^B-cull]: [`bevy_sprite/src/lib.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite/src/lib.rs#L107),
    `calculate_bounds_2d`;
    [`bevy_camera/src/visibility/mod.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_camera/src/visibility/mod.rs#L748),
    `check_visibility_cpu_culling`;
    [`bevy_sprite_render/src/mesh2d/material.rs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/mesh2d/material.rs#L1007),
    `queue_material2d_meshes`.
[^B-shader]: [`tilemap_chunk/tilemap_chunk_material.wesl`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/tilemap_chunk/tilemap_chunk_material.wesl#L27),
    `get_tile_data`, `fragment`.

### Validation Follow-Up References

[^G-validation-parser]: [`VariantParser::_parse_array`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/variant/variant_parser.cpp#L1545)
    and `_parse_dictionary` at line 1579; `VariantWriter::write` recursion checks at line 2048.
[^G-validation-loader]: [`ResourceLoaderText::_parse_ext_resource`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/resource_format_text.cpp#L128),
    `_parse_sub_resource` at line 107, node-property storage at lines 285-302,
    cached-resource reuse at lines 567-571 and resource assignment at line 670.
[^G-validation-instantiation]: [`SceneState::_instantiate`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp#L226),
    native property assignment at line 567 and deferred node-reference resolution at lines 677-719.
[^G-validation-property]: [`Object::validate_property`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/object/object.cpp#L623)
    and [`Timer::set_wait_time`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/main/timer.cpp#L93).
[^B-validation-deserialize]: [`TypedReflectDeserializer::deserialize`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/deserializer.rs#L352),
    [`visit_struct`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/struct_utils.rs#L91)
    and [`ListVisitor::visit_seq`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_reflect/src/serde/de/lists.rs#L27).
[^B-validation-apply]: [`ResolvedSceneRoot::{resolve, spawn, apply}`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/resolved_scene.rs#L26),
    `ErasedTemplate::apply` at line 722 and `SceneEffect::apply` at line 752;
    [`EntityTemplate::build_template`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_ecs/src/template.rs#L450).
[^N-validation-current]: Nara `c1edb21`: [compiler normalization](../../crates/nara_scene/src/prefab/overlay/compile/staged/values.rs),
    `materialize_program` source admission/defaults/validation and `materialize_defaults`;
    [frozen defaults](../../crates/nara_reflect/src/effective/frozen.rs), `FrozenComponentDefaults::cost`;
    [Reflect validation](../../crates/nara_reflect/src/registry.rs), `validate_component_value_with_limits`;
    [receiving validation](../../crates/nara_scene/src/validation.rs),
    `validate_effective_component_values` and `preflight_compiled_scene_with_context`;
    [materialization selection](../../crates/nara_scene/src/spawn.rs), `exact_definitions_admitted`;
    [reference planning](../../crates/nara_reflect/src/entity_reference.rs),
    `collect_present_reference_leaves_with_admission` and `find_undeclared_reference_path`.
