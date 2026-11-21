# Cooked Player Delivery

The finite delivery workflow joins concrete processed content with one explicitly built Player.
It produces one relocatable directory containing the actual captured executable, `content.pack`
and `package.json`. Source-carrying Export remains a separate workflow. The
[delivery plan](../plans/2026-10-31-cooked-player-delivery-plan.md) records current qualification;
this API guide alone does not establish platform support or Runtime readiness.

## Ownership And Build Order

Use the same game-owned `ProductRecipe` in the compiled cooker and Player. The cooker selects
the content target/configuration, actual importer/cook definitions and source observations.
Cargo package, binary, profile, default features, requested features, target and tools are a
separate explicit selection. A content profile need not equal a Cargo profile.

| Value | Guarantee and owner |
| --- | --- |
| `VerifiedArtifactGroup` | Original encoded member bytes, Product mappings, formats, dependencies and build facts; its original retained lease survives cache deletion. It can be historically valid without being the current selection. |
| `CookedContentBuilder` | Finite concrete assembly through Scene, Input and TileSet production encoders plus verified groups; no importer registry or directory publication. |
| `PreparedRuntimeContent` | Original admitted captured base and independent expected catalog facts; every delivered member is covered by deep validation. |
| `GeneratedPlayerBootstrap` | Exact Rust source bytes, logical name and digest carrying independent expected catalog identity and fixed pack location. The original Cargo request owns and supplies this input without writing project source. |
| `CapturedCargoExecutable` | Actual selected executable bytes and checked finite code-input/tool provenance. Building does not execute it. |
| `PreparedPlayerPackage` | Exact content/bootstrap/code binding and bounded three-file candidate; no Runtime activation. |
| `PlayerPackageReceipt` | Actual complete directory publication and filesystem guarantee tiers. A later callback failure cannot erase this physical result. |

The order avoids a digest cycle: prepare content, generate independent expectation input, build
the Player, bind actual build provenance, then publish the whole directory. The catalog never
contains the final executable digest. The final manifest records code, catalog and pack facts,
and cannot authorize its own expected identity.

For source projects, first ingest the selected manifest/profile, resolve the game's recipe and
call `ProjectContentLoader::load_delivery`. That original source path checks the selected
authoring Schema against the compiled owners and returns the complete admitted delivery members.
Carry their actual paths and digests into the cooker's existing source fence, together with its
importer observations, and revalidate them at the original build/publication boundaries. Do not
assume the bundle lives at the starter's default path. A valid bundle at another manifest-selected
location is supported; missing, corrupt or divergent selected bundles reject before generating
the Player bootstrap. These source checks do not run in the installed source-free Player.

## Assemble Complete Content

For an ordinary source project, use `project_tooling::project_cook` from its compiled cooker.
Construct `ProjectCookRequest` with the original root capability, admitted settings candidate,
game recipe, content target, existing artifact store, actual implementation digest and limits.
Call `prepare_project_cooked_content` and explicitly contribute game-specific build facets:

```rust
let prepared = prepare_project_cooked_content(request, |context| {
    context.add_source_assets::<GameSoundBankDomain>()?;
    context.add_source_assets::<GameAnimationDomain>()?;
    Ok(())
})?;
```

The shared entry consumes the source loader's actual compiled Scene and Prefab results,
Image pixels, Input definitions, Material and TileSet data. It preserves source identity and
derives travel roots from the selected manifest, including additional authored roots. Dependency
Scenes do not become public travel roots. Every selected Scene needs an adjacent `.meta` with a
stable identity, including dependency-free roots in `project_location` projects. That identity
names the cooked Product. The source document keeps its configured identity policy. Stock Images
retain their admitted extent and color space. A device-specific transform remains an explicit
separate artifact policy. Each additional domain uses its selected `SourceAssetBuild` and complete
verified group. A source-only importer does not automatically opt into that build contract.
Stock payloads enter the pack through a delivery root or a Required or Reference edge declared by
the cooked content. An importer can embed PNG data in a custom Product and keep the PNG as a
tracked build input. If the codec declares a link to the Image Product, the pack also contains
the Image payload. A build result can also request a stock Image, TileSet or Material by its
primary Product, even when no Scene snapshot loaded it. TileSet and Material definitions add
the Image Products named by their actual encoded dependencies. For Material, enable the stock
source domain and the Runtime codec in the recipe. The Material importer reads its Shader files
as source inputs and stores the admitted shader in the cooked Material. Each selected UUID has
one source across the configured catalogue roots. Newly selected definitions, PNGs, sidecars
and Shader files count once per path against the delivery file limit. Before publication, Cook
verifies the original files and checks the selected UUIDs again. A PNG embedded solely by path
continues to participate as a build input.

Decoded roots retire sequentially; the unpublished builder accumulates their processed members
and performs the existing final complete validation. `PreparedProjectCook` retains source and
presence-aware build observations. Construct an explicit `CargoBuildRequest` covering the actual
Rust inputs (the supplied `project_rust_input_inventory` covers the finite default tree), plus
any extra compiler inputs the game uses. `build_player(request, limits)` selects the generated
bootstrap through the original Cargo owner, and the returned package's `publish` retains source
and code checks at the original publication checkpoints. Keep typed Cargo/package failures until
their native cleanup custody completes. Neither the inventory helper nor the Cook entry resolves
Cargo dependencies or analyzes Rust includes.

The generated `desktop-2d/v1` project includes a headless `cook` and `player` alongside its existing
source `headless` and `desktop` binaries. Follow its generated README for the explicit Cook command.
Its Player reads only its compiled executable-relative pack; `--frames N` selects a finite run
length, without overriding content expectations. This headless entry does not replace the source
desktop Export or establish foreign-target/native-library support.

Consumers that exhaustively match `ProjectMember` must handle `BuildScript`, `CookSource` and
`PlayerSource`. The starter publishes twelve members and four binary targets. Source-carrying
Export continues selecting its original `headless` and `desktop` products; Cook and Player are
additional project-owned entry points, not extra source Export executables.

The cooker uses `project_workflow::cooked_content` under the `project-workflow` feature.
Construct `CookedContentRequest` with the complete startup/travel Scene set, explicit addressable
roots, `RuntimeConfiguration` and `RuntimeContentTarget`. Add actual verified groups with
`add_group`, saved/expanded Scenes with `add_scene`, immutable global Input with `add_input`,
and concrete TileSet descriptors with `add_tileset`. Scene references are resolved through the
actual `AssetReferenceResolver`; encoded Runtime references use complete Products.

`finish` consumes the unpublished builder. Required and Reference dependencies must both be
delivered; Immediate/Deferred determines current preparation demand. Soft metadata alone does
not require membership. Add an actual addressable root when an otherwise unreferenced Product
belongs to this delivery. Surplus members reject rather than silently evade validation or become
implicit roots. A compound member preserves all of its Products without duplicating its bytes.

Deep validation runs the selected production decoders and actual reference inventories, including
Image/TileSet/Tilemap CPU constraints. Material texture slots, the stock Sprite's optional
`material.image` and the stock `UiVisual` image brush's `background.image` must target members
selected with the Image publication role. Choose `ProductRecipe::add_runtime_image_codec` for an
Image used by these native bindings. The generic `add_runtime_asset_codec` registration remains
available for direct typed CPU consumption. An Image transform that invalidates an atlas rejects
assembly. Pack startup admits the catalog, extents and current demand. Later original-file
corruption can reject a Deferred acquisition while preserving the current Scene.

### Cook A Dedicated Server

Use `prepare_project_server_content` when a game ships a server derived from the same authored
Scenes. Pass the ordinary `ProjectCookRequest` for source validation and a `ProjectServerCookTarget`
with the server candidate, executable recipe and component types to omit. Both candidates come
from the same opened project manifest and select the same authored content roots. Select the
`server` preset and clear an inherited source action map with `clear_action_map = true` in the
profile's `input` section. The target recipe installs semantic gameplay commands without the
raw Input plugin.

List only component types the game has verified it can run without. The cooker retains every
entity, its hierarchy and provenance, and validates surviving references against the target
Schema. The target can select a subset of the source Schema owners. Each selected owner's
current and predecessor definitions must agree with the source. Artifacts needed by the retained
components follow the ordinary dependency closure. Game-specific build contributions remain
explicit. The admitted source profile still validates all of its authored inputs, and the
existing publication fence protects those inputs during the build.

Build and publish the exact server executable through the same generated expectation and
captured package flow. Audit the shipping Cargo selection, delivered member table and an actual
running World separately. Two-Level Game exposes this path as `cook --runtime server`. The
source-free server opens its compiled pack relative to the executable.

Compile a processed server against `nara` with default features disabled and `runtime-core`
and `serde` enabled. Use `HeadlessSession::from_runtime_content` for an explicitly driven session
or `HeadlessRun::from_runtime_content` for a bounded run. Both use the same processed Scene and
CPU asset admission as the local Player. Source-backed constructors and stock Image, TileSet and
Material publication use `runtime-2d`. Keep a separate Cargo selection for the cooker, since it
reads the authored project using the client feature set.

The published Two-Level `server --serve` drives the original session at the authored frame
cadence, accepts local semantic controls from standard input and reports admission, observed
state and final cleanup. An optional absolute checkpoint path selects the game's existing save
slot. [Run the installed server](../../samples/two-level-game/README.md#operate-the-server)

For exact game continuation, the installed Server captures from a paused boundary and checks a
fresh candidate before it reports Ready. The game-owned state includes scheduling phase, settled
room and relic state, and the remaining supported command stream. The caller retains the
incumbent until the candidate is ready and chooses which process receives new work.
[Continue in a fresh Server process](../../samples/two-level-game/README.md#continue-in-a-fresh-server-process)

`ProductRecipe::add_runtime_asset_codec` and its `ProductRecipeError::RuntimeCodec` variant are
available with `runtime-core` and `serde`. Exhaustive matches in that configuration need to
handle codec selection failures.

## Bind The Player

Native construction and publication live in `project_tooling::player_package`:

1. Create `GeneratedPlayerBootstrap::new(&content, canonical_input_name, limits)`.
2. Keep ordinary code files in the `CargoBuildRequest` inventory. Select its single generated
   source with `with_generated_rust_input(bootstrap.name(), bootstrap.bytes())`.
3. Make the trusted build script read `NARA_CARGO_GENERATED_RUST_SOURCE`, always declare
   `cargo::rerun-if-env-changed` for it and write only Cargo's `OUT_DIR`. The Player includes that
   module and uses its constants with its own game recipe. No input must preserve an explicit
   refusal, even when an older `OUT_DIR` file exists.
4. Call `build_player_package(content, bootstrap, &request, limits, control)`. The wrapper retains
   content through Cargo, reserves the next owner's work promise and binds the actual capture.

The binder compares the whole requested Cargo selection and the generated input's exact canonical
logical name/digest/length in the separate generated-input provenance fact. A matching ordinary
source-file observation cannot replace it. Resolved compiler features and tool observations
remain separate facts. Old Player A cannot bind content B by sharing a startup Product or label.
The separately available `PreparedPlayerPackage::bind` accepts already captured historical facts
without asserting that mutable authoring inputs remain current.

The Player's inclusion and use of the generated module is a trusted Rust implementation contract.
The binder does not analyze Rust inclusion, scan executable code or prove executed branches.
Do not offer command-line or manifest overrides of the compiled expected catalog/target. This
finite workflow does not establish hermetic compilation, remote authenticity or a reusable
prebuilt-Player trust scheme.

The generated source slot is invocation-wide, nonempty NUL-free UTF-8 and bounded to 16 KiB,
in addition to the request's existing total input and work limits. It is immutable after selection;
there is no general environment-variable or package-routing API. The command clears an inherited
reserved value for ordinary requests. Trusted Cargo configuration must not define or override
this variable. Native environment/spawn errors retain the original process failure contract.
The existing target-directory lease serializes cooperating builds; `rerun-if-env-changed` makes
serial A/B/A content selections invalidate the actual build script without another target tree.

The canonical unpublished `player_package` version-one payload now records `generated_input_name`
and `build.generated_rust_input`. The superseded `generated_input_path` shape and physical-source
proof are removed. The separate code inventory and source/content publication fences remain.

## Keep Current Selection With The Host

Retain the actual finite source, metadata, auxiliary-read/presence and processed-group selection
envelope used to prepare this content. The control callback revalidates that original envelope
around Cargo, before staging and before publication. Do not create a fresh baseline inside the
callback: that would accept the changed selection being checked. Scene, Input and TileSet source
observations participate alongside import/cook inputs.

An importer may read the same action map that the manifest selects for global Input. Check and
Cook retain one member with the Input role when both reads observe the same bytes. Before
publication, the Host checks the selected Product owners along with their original file facts.
This includes the source UUID shared by a compound group's primary and auxiliary Products.

Code inputs and content observations are different inventories. Cargo revalidates its finite code
inventory; group validity alone cannot extend content freshness across the later Player build.
The Host retains content observation custody through publication. A failed fence returns
`SelectionChanged` or `Cancelled`; callback panics become finite package failures. The check
envelope does not prove simultaneous cross-file state, ABA detection or globally latest inputs.

## Publish And Run

Call `prepared.publish(&parent, &destination, control)` with an existing writable directory
capability. The owner streams the original admitted pack, writes captured executable and manifest
bytes, synchronizes files, checks the exact three-file regular-member inventory and verifies
staged digests before existing no-replace whole-directory publication. It does not scan Cargo
output for dependency libraries. Additional native libraries need explicit finite support before
that product can claim a complete package.

Use an output parent on the supported local filesystem for the build host. The native publication
tiers are x86-64 Windows on local NTFS and x86-64 Linux on local ext4 with Linux 5.6 or later.
On Linux the output parent uses `TrustedLocal` cooperative-writer authority. The same-parent
rename protects the destination from overwrite, and the receipt reports name-bound source
identity. [Filesystem transaction authority](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#filesystem-transaction-authority)

Handle every `PlayerPackageOutcome`. Clean rejection, orphaned nonempty candidate and unresolved
begin custody are distinct. No recursive name-based cleanup occurs. `Published { receipt, error }`
always means publication happened; an optional postpublication error does not make the destination
absent. `package.json` is finalized before publication; physical receipts stay in the return value.
Publication neither promotes executable last-good nor establishes Runtime Ready.

Executable-relative opening is available with `runtime-core` and `serde`. The local Player's
recipe also selects `runtime-2d` to publish its selected stock assets.

```rust
let expected = RuntimeContentExpectation::new(
    ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
    RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)?,
);
let base = open_executable_relative_runtime_pack(
    &RelativePath::new(generated::PACK_FILE)?,
    &expected,
    game::recipe(),
    RuntimeContentLimits::default(),
)?;
```

`project_host::open_executable_relative_runtime_pack` uses the actual executable parent, independent
of the working directory. It opens one original pack and delegates all admission to Runtime
content. Failure has no authoring-source, metadata, importer or cache fallback. Continue through
the existing processed `HeadlessRun`/`DesktopRun` Host rather than installing a second loader.

## Bounds And Evidence

`CookedContentLimits` accounts for live group custody, copied captured bytes, metadata and decoder
promises. `PlayerPackageLimits` separately bounds bootstrap/manifest encoding, retention, overlapping
work and the complete staged disk payload. Pack writing is streamed. Shared owners keep original
leases; equal digests do not merge independent allocations. Borrowed Host observations, escaped
acquisitions and compiled recipe construction keep their own existing owner allowances. These
logical ledgers do not measure allocator capacity, subprocess RSS or arbitrary trusted native code.

The [independent Cargo consumer](../../tests/fixtures/cargo-build-consumer/README.md) contains the
complete compiled cooker/Player workflow and explicit serial commands. It uses real tracked Image
import/target cook, an auxiliary compound Product, global Input with production driver-event
admission, shared Image/TileSet,
Deferred Scene travel, whole-directory relocation and unavailable source/cache/Cargo at execution.
Its actual qualification remains in the owning plan/evidence; foreign builds, native presentation,
server stripping and additional platforms require their separate consumers.

The [Two-Level Game's audible placement](../../samples/two-level-game/README.md#source-free-audible-player)
adds a concrete game-owned Sound Bank through ordinary `SourceAssetBuild`. A project Cook caller
can build the selected sources with `add_source_assets::<D>()` or reuse verified bytes through
`add_group::<D>()`, which checks the current typed build seed before admission.
Both Scenes declare the same primary Product; the catalog includes its source-free PCM member.
The cooker binds the actual import groups' raw observations through publication, including a
renamed WAV selected by the descriptor. CPU preparation requires no audio device. Only the
explicit Windows `audio-desktop` recipe opens the original WASAPI device at Runtime startup.
Its complete native delivery acceptance, observation costs and platform limits belong to the
[game's current unit](../plans/2026-11-03-joint-motion-execution-plan.md#source-owned-sound-bank-and-source-free-audible-player).

The same game selects native controllers separately with `--controllers none|gilrs`. The default
is `none`; Gilrs requires the cooker's optional gamepad feature, a Windows target and a desktop
placement. Silent and audible desktop recipes compose the same fixed native Input configuration.
The finite game-owned `PlayerSelection` supplies the selected Cargo features, binary, content
profile and recipe to the existing build/catalog owners. Those owners retain actual compiler
features and fingerprints; they do not infer backend policy. A Player uses its bound selection,
so enabling all Cargo features does not itself acquire a controller context. The original three
profile names remain unchanged, and controller profiles carry distinct names. No native device
state enters the content pack or save format. See [game usage](../../samples/two-level-game/README.md#optional-native-controller)
and the [delivery qualification](../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#explicit-controllers-through-cooked-player-delivery).

The game's `--flow exploration|round` choice is separate from runtime and controller selection.
Round desktop and audible desktop profiles use the existing UI composition and game-owned menu
policy. They publish a usable presented Title before the first fixed tick; pending first-tick Save
intent is reported separately and is consumed by the original gameplay transaction after Start.
The cooked Input definition and exact menu derivation retain their original immutable handles.
The Player retains the tick-zero Title submission. READY can report a newer current HUD if Enter
starts the round before Host Resume confirmation is delivered. HUD observations join the original
UI and native submission facts. The cooker defaults to exploration. It rejects headless Round,
and the Round Player rejects incompatible scripted verification selections before opening the pack. See the
[game command](../../samples/two-level-game/README.md#cooked-round-flow) and
[owning unit](../plans/2026-10-20-persistent-two-level-game-plan.md#cooked-round-flow-successor).

The same game's [explorer animation](../../samples/two-level-game/src/animation.rs) uses one
shared four-frame Image Product in both travel Scenes. Atlas regions are evaluated Sprite state
derived from the existing logical clock and held intent. The
[animation workflow](../plans/2026-11-03-joint-motion-execution-plan.md#explorer-atlas-animation-through-runtime-and-delivery)
owns source-free frame selection and native pixel evidence.

The marker Prefab uses a separate badge Image reached through the retained marker Scene. The three
Player placements package two travel Scenes, the marker Scene, two Deferred rooms, five Images,
the shared region Material and TileSet, Sound Bank, Sprite clips and global startup Input. The
catalog contains fifteen members. Both rooms prepare when requested from Meadow and use the same
relic Image, Material and TileSet. The Material requires the existing Meadow Image as a palette;
the TileSet requires the relic Image as its atlas. The source-free closure retains both edges.
Their eight-cell Tilemaps use distinct checkerboard colors. Shared live TileSets retain their
original version, and independent close or primary travel releases the last region's declaration.
[Marker Prefab workflow](../plans/2026-10-20-persistent-two-level-game-plan.md#textured-prefab-runtime-successor) · [Play the side rooms](../../samples/two-level-game/README.md#play-the-side-room)
