# Next Foundation Refactor Assessment

Reviewed baseline: `0ebc6603ccafc41e89acd0d478658610f7cf19ee`.

The [TileSet dependency and Editor loading follow-up](../research/2026-10-18-tileset-dependency-and-editor-loading-research.md)
deepens N02-N04 after the completed component/plugin contract refactor. Four new Image
event/recovery/publication gaps were independently confirmed by source tracing. The subsequent
[R0 repair evidence](../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md)
records their behavioral regressions and authorized implementation. These repairs precede coherent
TileSet/Image publication and generic Editor loading. They do not change earlier test results or
claim the unsupported TileSet reload/hydration workflows are current defects.

This follow-up answers what should change after the [foundation contract audit](2026-10-18-foundation-contract-audit.md)
and F11 hierarchy refactor. Three independent review lanes covered runtime/content,
render/spatial/Tilemap, and Editor production workflows, using local Bevy and Godot source.
The coordinator reconciled their findings against Accepted ADRs and product strategy.

No additional P1 was established in the reviewed paths. One concrete Tilemap data-contract
problem was identified by source tracing; the remaining recommendations concern extension
boundaries or measured scale. Since the assessment, the first Tilemap foundation slice has been
implemented and verified with focused serial `nextest` runs; the evidence and remaining gates are
recorded below.

## Recommended Order

| Order | Work | Classification | Start condition |
| --- | --- | --- | --- |
| 1 | N01: reject ambiguous Tilemap source data | completed, concrete codec repair | Regression coverage now rejects duplicate coordinates and non-positive dimensions |
| 2 | N02: file-backed TileSet and dependency publication | cold-start slice completed; reload remains gated | First saved map with a real TileSet -> Image dependency |
| 3 | N03: semantic collection editing and bounded Undo | P2, authoring extension risk | First real collection editing workflow; settle before its widgets |
| 3 | N04: domain-owned Editor semantic admission | P2, central dispatcher coupling | Introduce the second special authoring domain, alongside N02/N03 |
| 4 | N05: external changes and document reconciliation | P2, bounded production workflow gap | External-tool editing becomes a supported content workflow |
| 4 | N06: shared fixed-resolution presentation owner | Existing F13, consumer-gated extension | A game selects one logical render extent for Runtime and Editor |
| 5 | N07: runtime asset acquisition and residency | P2, streaming prerequisite | Real two-region load/unload consumer, before long-lived streaming |
| Measured | N08: Tilemap and Transform active-work selection | Existing F12 plus map scale assumption | Representative map exceeds an explicit CPU/allocation/frame budget |

N02 is the highest-value next architectural slice. N01 can precede it independently.
N03/N04 should use that same map or another real collection consumer. N05/N06 are independent
lanes; N07 is unnecessary for a bounded startup map.

## Component and Plugin Contract Follow-Up

The [component/plugin implementation plan](../plans/2026-10-18-2345-refactor-component-plugin-contracts-plan.md)
records a later source-only reassessment. It identifies independent/forked AssetServer ID aliasing,
principal Rust type binding, direct-App declared-provider closure, and file-backed atlas/Image extent
agreement as the next bounded repairs. No reproducer was executed during that planning pass.

Implementation now uses one checked process allocator, per-catalogue Rust type claims, and private
raw constructors. Source routing secures the registered domain type before lifecycle mutation;
Image admission and every terminal publication revalidate the current catalogue claim. Direct App
sealing checks the complete declared provider union against executable receipts. File-backed
TileSets validate decoded Image extents before admission, including fractional roundoff that an
ordinary `f32` or `f64` sum can hide. Render targets allocate fresh anonymous typed identities once
per owning view/session and retain them across resize.

Independent implementation reviews exposed the divergent-catalog publication case and numeric
roundoff gap; focused failing regressions preceded their repairs. Their follow-up reviews found
no remaining actionable defect in these paths. This does not add TileSet hot reload, generic
Editor TileSet hydration, residency or a general plugin/provider framework.

The reassessment retracts blanket AssetPlugin dependencies and schema owner tokens: staged asset
resolution and delegated provider registration are intentional supported workflows. Existing Image
store-revision guards remain effective. Tile layers already use separate Tilemap entities; a new
TileSource hierarchy, global resource ledger and chunk cache are not prerequisites.

## N01: Tilemap Source Admission Can Silently Discard Data

`read_cells` accepts serialized rows without coordinate uniqueness checks
([codec](../../crates/nara_tilemap/src/lib.rs#L878)). Materialization uses `BTreeMap::insert`
([writer](../../crates/nara_tilemap/src/lib.rs#L415)), so later rows silently replace earlier rows.
Export emits only the surviving entries ([export](../../crates/nara_tilemap/src/lib.rs#L922)).
Two different authored rows at one coordinate therefore become one after materialization/export.
Generic schema validation establishes shape, not Tilemap coordinate uniqueness.

The same domain admission accepts zero or negative `tile_size`; finite positive values can also
narrow to zero when converted to `f32`. `Tilemap::new` has no positive-size guard
([constructor](../../crates/nara_tilemap/src/lib.rs#L376)), whereas `TileAtlasLayout` checks positive
finite dimensions ([atlas](../../crates/nara_tilemap/src/lib.rs#L209)). Non-finite wire numbers
are already rejected by `ComponentFloat`; they are not a new finding.

**Direction:** reject duplicate coordinates with an indexed field diagnostic and validate the
converted positive dimensions in the Tilemap codec. Preserve deliberate in-memory replacement
through `set_cell`. Keep domain-specific rules out of the generic Reflect visitor.

**Evidence:** `nara_tilemap` regression tests now reject duplicate rows and zero/negative or
narrowed-to-zero dimensions before publication; the focused codec suite passes 19/19 tests. This
is a small correctness repair, not a reason to redesign the map domain.

## N02: A Second File Asset Domain Crosses Image-Specific Infrastructure

Tilemap resolves TileSet references as `Other("tileset")`
([source kind](../../crates/nara_tilemap/src/lib.rs#L786)). Before this slice, project startup
collected all component asset references as Image paths and ran only `load_images`
([startup](../../src/project_content.rs#L997)); reload still grants consumer authority only to
Image ([routing](../../crates/nara_asset/src/reload/resolution.rs#L230)). Manual
`Assets<TileSet>` and codec tests alone did not establish file-backed project startup.

The [dependency graph](../../crates/nara_asset/src/state.rs#L510) contains manually registered
source/dependent edges and notifications. It does not establish a dependency snapshot or recursive
readiness at publication. Adding another match branch alone would leave a compound asset able
to finish against an obsolete dependency during concurrent reload.

**Current evidence:** a bounded RON TileSet envelope now owns the Image `AssetRef`; project content
admission records TileSet metadata/source and its Image dependency in one immutable startup
closure; Runtime/Editor publish `Assets<ImageAsset>` before `Assets<TileSet>`. The project-content
suite passes 4/4 tests and the runtime boot suite passes 11/11. The TileSet consumer additionally
passes with `presentation-2d,serde,tooling`: two saved maps resolve path/stable dependencies,
produce textured render batches containing the expected immutable Image pixels, and open/retire
a trusted Editor session. Source changes produce new snapshot revisions; failed dependency loads
preserve earlier snapshots and release rejected work. The Tilemap/source suite passes 19/19.

**Direction:** implement one concrete TileSet source/import owner with Image dependency capture.
Then extract the finite source-kind consumer admission and claim boundary demonstrated by those
two domains. Keep decoding, task execution, budgets and typed publication in domain owners.
Make startup, Check, Editor preview and Export consume the same admitted closure.

**Remaining acceptance:** native GPU readback, generic Editor referenced-source hydration, then
Image and TileSet hot reload in either completion order, stale dependency completion rejection,
last-good retention, and removal revocation. Cold-start admission, typed publication, static render
batches and trusted Editor startup now have passing evidence. The current snapshot-refresh tests
do not establish concurrent typed hot reload or generic Editor TileSet authoring.

Bevy separates [asset registration](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs#L573),
ordinary and loader dependencies ([LoadContext](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L141)),
and [recursive readiness](../../repo-ref/bevy/crates/bevy_asset/src/server/info.rs#L475).
Borrow responsibility separation. Do not adopt its whole executor or implement Proposed ADR 0087
wholesale; Accepted ADR 0103 keeps execution domain-owned.

## N03: Recursive Data Needs Semantic Editing, Not Whole-Field Replacement

The Inspector edits a stable root field and clones its replacement value into a patch
([editing](../../crates/nara_tooling/src/inspector.rs#L357)). Field projection copies default and
effective values ([projection](../../crates/nara_tooling/src/inspector.rs#L1067)). The frontend
dispatches shallow value kinds and displays collections read-only
([frontend](../../editor/src/frontend/imgui/authoring.rs#L554)). Recursive schema support does
not yet provide an ergonomic or efficient collection editing workflow.

If future inventory, dialogue or map editing uses whole-root replacement for each element, its
validation/history cost follows collection size. No current Tilemap brush is claimed to use that
path. ADR 0026 intentionally deferred list-splice operations; this is a new consumer decision.

**Direction:** use a real collection to define stable root field identity plus bounded relative
paths and semantic insert/remove/move/update operations. Specify revision/old-value preconditions,
inverse data, transaction grouping and stale-draft rejection before adding widgets. Map strokes
may need coordinate-based operations rather than list indices.

**Acceptance:** one gesture creates one atomic Undo entry; small edits retain bounded inverse
data; stale edits fail without partial state; save/reopen and Undo/Redo agree. Measure complete
transaction cost before choosing copy-on-write or persistent storage.

Godot separates [large-collection presentation](../../repo-ref/godot/editor/inspector/editor_properties_array_dict.cpp#L434).
Bevy's [relative paths](../../repo-ref/bevy/crates/bevy_reflect/src/path/mod.rs#L283) are runtime
access precedent, not a replacement for Nara's persistent field identities.

## N04: Specialized Editor Semantics Accumulate in Central Dispatch

The Host action dispatcher contains UI-specific Prefab/composition handling
([actions](../../src/project_host/runtime/editor/actions.rs#L334)), frame/gesture admission
([policy](../../src/project_host/runtime/editor/actions.rs#L754)), and eight fixed UI component
names ([component matching](../../src/project_host/runtime/editor/actions.rs#L892)).
[Panel registration](../../editor/src/workspace/panels.rs#L178) handles presentation, not these
semantic decisions.

**Direction:** when map authoring becomes the second special domain, migrate UI policy and the
new consumer through a finite trusted Rust contribution boundary. Contributions identify owned
components and validate/project semantic intent. Keep document mutation, revision, Undo, save and
publication under their current owners. Avoid an unrestricted action bus, dynamic ABI or widget IR.

**Acceptance:** the second domain needs no second component-name recognition tree in central
dispatch; both retain atomicity and stale-interaction guarantees. Godot keeps `can_handle` and
property handling in [Inspector plugins](../../repo-ref/godot/editor/inspector/editor_inspector.cpp#L1871).

## N05: Browser Refresh and Document Conflict Handling Are Separate Workflows

Browser Refresh starts an index scan
([browser](../../src/project_host/runtime/editor/project_browser.rs#L162)). The external-change
document action exists ([dispatch](../../src/project_host/runtime/editor/actions.rs#L259)), but
there is no demonstrated general observation-to-document reconciliation workflow. Explicit reload
rejects dirty documents and resets the session for clean documents
([workspace](../../crates/nara_tooling/src/workspace.rs#L1640)). Save checks source identity/digest
([publication](../../src/project_host/file_publication.rs#L289)); silent overwrite is not the finding.

**Direction:** one Host workflow handles manual/focus observation, classification, clean reload or
explicit dirty conflict, publication and draft/preview invalidation. A later watcher is another
event producer. Automatic merging, recovery journals and strong multi-file publication are separate.

**Acceptance:** external edits update clean open documents; dirty documents receive an explicit
choice; removal/replacement invalidates drafts; parse failure retains usable content; Browser and
document facts identify the same observed generation.

Godot separates [filesystem scanning](../../repo-ref/godot/editor/file_system/editor_file_system.cpp#L1707)
from [Editor conflict decisions](../../repo-ref/godot/editor/editor_node.cpp#L1615).

## N06: Fixed Logical Pixels Need One Runtime/Editor Presentation Owner

Runtime draws its Window target directly
([execution](../../crates/nara_render_wgpu/src/lib.rs#L655)); Editor Game View uses a same-extent
private image ([target](../../crates/nara_render/src/lib.rs#L130)). Its toolkit Integer scaling
control ([frontend](../../editor/src/frontend/imgui/game.rs#L22)) does not prove a shared GPU
logical-canvas -> effect/composite -> output path. This is existing F13, not a new defect.

**Direction:** once a game selects a fixed logical extent, add one private canvas owner shared by
Runtime and Editor, with explicit nearest/integer scaling, letterbox and input mapping. Keep UI
overlay placement explicit and simulation unsnapped. Add effects only as actual consumers require
them. No public graph, RHI or optional device profile is necessary for the basic workflow.

**Acceptance:** one scene has the same logical pixel layout in both placements through resize,
DPI changes and device recovery; picking/input inverts the actual displayed mapping.

## N07: Streaming Needs Acquisition Ownership Separate From Identity

`Handle<T>` is a Copy identity ([handle](../../crates/nara_asset/src/storage.rs#L21)); `Assets<T>`
uses [explicit removal](../../crates/nara_asset/src/storage.rs#L224). Startup content uses a shared
closure lease ([binding](../../src/project_content.rs#L1064)). That is a conservative bounded
startup lifetime, not per-region acquisition accounting. Retaining one image can retain the whole
closure's budget charge; this does not prove all other pixels remain resident.

**Direction:** before streaming, prove two regions sharing one image with domain-owned acquire/
release tokens, immutable captured content, shared-resource accounting and safe-point unload.
Source deletion and consumer unload differ. Pending completions must not resurrect an unloaded
acquisition. Keep handle identity independent; a global GC/LRU is premature.

Bevy separates [strong lifetime handles](../../repo-ref/bevy/crates/bevy_asset/src/handle.rs#L81)
from identity and tracks [asset lifecycle](../../repo-ref/bevy/crates/bevy_asset/src/assets.rs#L570).
Godot's [Resource](../../repo-ref/godot/core/io/resource.h#L52) is reference-counted. Nara need not
copy either API, but it needs an explicit lifetime owner before consumers stream content.

## N08: Optimize Active Work After a Representative Map Budget

Tilemap extraction visits every cell each frame
([extraction](../../crates/nara_sprite_render/src/extract.rs#L136)); queueing visits items per view
([queue](../../crates/nara_sprite_render/src/queue.rs#L31)). Dirty chunk markers have no renderer
consumer. This is an all-cell baseline, not broken cache invalidation. Transform completion scans
all participants ([propagation](../../crates/nara_transform/src/propagation.rs#L183)); interpolation
also scans samples ([presentation](../../crates/nara_transform/src/presentation.rs#L107)).

The existing [scale report](../benchmarks/2026-10-18-foundation-scale.md) measured 4,096-transform
one-leaf P99 at 0.30 ms and interpolation P99 at 0.57 ms. It does not establish a real map workload
or justify an immediate incremental Transform rewrite. F11's local hierarchy gate is satisfied.

**Direction:** measure a saved/reopened map, static cells, active actors, views and mutations.
Separate extraction/queue/upload/Transform/allocation costs. If a budget fails, trial view-bounds
culling before expensive preparation, then private retained chunk instances. Add dirty-subtree
Transform/interpolation membership only when that subsystem is responsible. Preserve full
algorithms as correctness oracles and test source/TileSet/device/ordering invalidation.

Godot [dirty quadrants](../../repo-ref/godot/modules/tilemap/tile_map_layer.cpp#L253) and Bevy
[visible-item queueing](../../repo-ref/bevy/crates/bevy_sprite_render/src/render/mod.rs#L438)
illustrate work selection. Do not copy mechanisms without Nara workload evidence.

## Preserve These Boundaries

- Keep the thin ECS substrate, frozen recipe, managed Runtime owner, semantic Editor transactions,
  durable identity/incarnation split and resource-complete render packets.
- Keep shared Sprite/Tilemap layer/group/Y/stable ordering; no new ordering defect was found.
- Keep ADR 0018's engine units, Y-up world, integer grid and render-only pixel policy. The old
  planning conversation's integer-only simulation and mandatory lightweight physics suggestions
  are product hypotheses, not foundation requirements.
- Use manual codecs for domain-specific data. Do not open arbitrary Rust reflection or create a
  generic importer executor merely to make an API look complete.
- Keep save-game state separate from Scene documents under ADR 0027. Its future implementation
  is not evidence that repaired Scene/identity contracts must be replaced.
- Defer general substeps/current-pose APIs, public graphs, scripting ABI, global native
  coordinators and optional device profiles until their named consumer gates apply.

The next checkpoint is a real map that can be authored/imported, saved, reopened, cold-started,
rendered, changed externally and exported. That workflow should drive foundation changes before
terrain, dialogue, world databases or streaming multiply their consumers.
