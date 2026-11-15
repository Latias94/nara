# Admitted 2D Materials

The `material` feature of `nara_material` supplies declarations, typed parameters, CPU-validated
WGSL artifacts and exact generation bindings. Stock Sprite/Tilemap rendering and the engine's
`runtime-2d` composition select it explicitly. Sampler/alpha policy remains available without
the Shader compiler. This is the active
[R9b replacement](../plans/2026-11-14-durable-material-and-shader-plan.md); persistent material
assets and the complete Save/Reopen/Check/Export workflow retain that plan's acceptance.

Pre-1.0 consumers replace `Material2dDefinition`/`Material2dDefinitions`/`Material2dInput` with
the declaration and explicit prepare/publish workflow below. Replace `Material2dInstance`
construction with an exact generation's `bind` or `bind_values`; those static interfaces and
their old error type are removed without aliases.

## Declare And Bind

One parameter struct supplies stable semantic IDs, values and actual Encase uniform layout.
The derive supports `f32`, `i32`, `u32` and `Vec2`/`Vec3`/`Vec4`; arrays are not vectors.
Do not derive `ShaderType` separately. The generated support path also works through a renamed
direct dependency or Nara's `material` facade; an explicit `#[material(crate = "engine::material")]`
can select the latter.

```rust
use std::{collections::BTreeMap, sync::Arc};
use nara_asset::AssetPath;
use nara_material::{
    AlphaMode2d, Material2dDeclaration, Material2dLibrary, MaterialParameters, Vec4,
};

#[derive(MaterialParameters)]
struct TintParameters {
    #[material(id = "tint")]
    tint: Vec4,
}

let mut materials = Material2dLibrary::default();
materials.declare(Material2dDeclaration::from_parameters::<TintParameters>(
    "game.tint", 1, AlphaMode2d::Blend, vec![],
)?)?;
let path = AssetPath::new("shaders/tint.wgsl")?;
let source: Arc<str> = Arc::from(r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.tint;
}
"#);
let prepared = materials.prepare_update("game.tint", &[path], |_| Ok(Arc::clone(&source)))?;
let generation = materials.publish(&prepared, |_| Ok(Arc::clone(&source)))?;
let binding = generation.bind(&TintParameters { tint: Vec4::ONE }, BTreeMap::new())?;
```

Attach `binding` alongside the Sprite or Tilemap component.
The reader in this compact example returns explicit immutable memory. A file-backed Host must
use its admitted source capability and return the requested file's actual bytes. Compilation
does not read ambient paths. An ordered manifest can contain shared WGSL helper files followed
by the material function; it is explicit concatenation, not an import parser. Inputs, assembled
source and compiler diagnostics have finite limits. `publish` re-reads the captured inputs and
refuses drift or a changed publication baseline.

Declare extra textures with stable semantic ID, WGSL name and group-1 binding 1 through 8.
`generation.bind` accepts a complete map from those semantic IDs to typed Image handles. The
render domain resolves every handle into its exact prepared Image version before queuing. A
missing texture omits only the affected draw; current library lookup cannot retarget old work.

The stock fragment receives the sampled primary Image, linear tint and UV coordinates and returns
linear RGB with straight alpha. Alpha is not gamma encoded. Image import owns its explicit transfer
interpretation; the selected render target/output policy owns conversion and composition. A
material does not infer display transfer from byte texture format or bypass
[the SDR color contract](../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md).

## Sample A Graph Texture

A stock renderer policy can replace one extra Material texture with a typed texture produced in
the same program, or an exact retained output imported through `stock.graph().import_texture`.
The policy retains the original `Material2dBinding` from its captured draw and uses the texture's
declared semantic ID:

```rust,ignore
let selection = WgpuMaterialTextureBinding::new(&material, "palette", &palette)?;
let world = stock.world_with_material_textures(10, &[selection])?;
```

Here `stock` is the existing `WgpuStockStages::begin` builder and `palette` is a typed port from
that builder. Declare its writer through the original program API; operation keys break ready
ties, while the automatically declared sampled read determines dependency order. Continue the
policy's Canvas/UI/effect composition and finish through the same stock builder.

Matching requires the original generation, exact uniform bytes and complete original Image
handle map. Every nonempty captured batch with that exact binding receives the selection.
Duplicate, unknown and unused mappings refuse the whole world operation. The operation restores
its own resource reservations, metadata and draw facts on error, so a caller may correct the
selection and retry in the same builder. Missing writers, foreign graph ports and feedback retain
the graph compiler's ordinary refusals. Imported outputs keep the original Host/device-epoch
validation and lifetime.

The source may have a different extent. It must be a whole single-mip, single-sample baseline
filterable float D2 texture with `TEXTURE_BINDING`; its transfer must match the native format.
The original Sprite's sampler still determines nearest/linear sampling. The descriptor's alpha
interpretation is explicit and receives no implicit conversion. For example, a Shader sampling a
premultiplied palette must unpremultiply it before returning the Material's straight-alpha output.
Only unreplaced asset Images enter the Image upload cache; the program retains selected graph
textures through its original native submission.

The independent [stock composition consumer](../../tests/fixtures/stock-composition-consumer/README.md)
contains a generated palette, an independent grading branch, explicit sampling/color oracles and
retained original output use. Its recorded qualification limits remain separate from this API.

## Edit And Observe

`current(id)` returns the actually published generation. `prepare_update` builds an effect-free
candidate; `publish` is the original asset-baseline commit. Two candidates from the same baseline
cannot both replace current. A foreign or stale update is refused before source reads. Failed
compilation records the latest attempted inputs and preserves the usable old value through the
existing asset state owner. A read/drift refusal during publication leaves the candidate intact
for explicit retry against the same original baseline.

`affected_by(path)` selects declarations that depend on the source in their current or latest
attempted cut, including a missing requested input. Successful publication replaces recovery
edges with the actual new closure. This API grants no watcher, task or automatic compilation
authority: the caller feeds changes through its existing asset/source work owner.

Bindings retain immutable declaration, Shader, parameter bytes and typed references after a new
generation or library retirement. Resource resolution and native preparation must retain the
matching captures. Shader compilation proves the CPU interface; successful GPU pipeline creation,
device lifetime and physical submission completion remain separate original render authorities.
Uniform values and ordinary texture-content changes are not pipeline specialization inputs.

The native stock adapter bounds its cross-call Ready/Rejected pipeline cache to the most recent
64 entries after each complete preparation. A single preparation can use more variants: it first
captures all required native pipelines and buffers, then trims acceleration state. Prepared draws
and original submissions retain their own handles; cache eviction neither releases those holders
nor changes current material publication. Evicted old captures can recreate their original
pipeline. This entry bound is not a total GPU-memory or compilation-time budget.

Semantic field IDs/version, Shader symbols/offsets and source-body identity are different facts.
Renaming a Rust field need not change its stable ID; changing its semantic type requires a new
compatible declaration or explicit migration. The validated dynamic declaration constructor is
the future Shader-authored seam. Naga cannot infer persistent field identity from a symbol.

Run the Palette example through the normal desktop product:

```text
cargo +1.95.0 run --locked --offline --release --example palette_material --features presentation-2d,desktop-winit,render-wgpu
```

This code-first example supplies its Shader source explicitly. Persistent file-backed products
use the separate workflow below; neither path installs an automatic file-watching material panel.

## Persistent Products And Scene Values

The `asset` feature adds `Material2dSource`, `Material2dDomain`, `Material2dAsset` and the
`runtime_material2d` version-one `Material2dRuntimeCodec`. The `persistent` feature adds the
single native `Material2d` carrier, `Material2dRecord` and `Material2dPlugin`. The product
workflow's selected source/processed delivery has integrated qualification in the owning R9b
plan. Automatic source watching, live Editor preview hydration and a material Inspector remain
separate product work.

`Material2dSource::new` receives the same validated declaration, ordered Shader input paths,
complete default parameters and a complete map of default texture selections. `encode()` emits
bounded versioned JSON. Source metadata supplies its actual source/Product identity and selects
`AssetSourceKind::Other("material2d")` with unit settings. Shader reads use the supplied source
capability. Default Images are typed product dependencies; reading raw WGSL does not substitute
for importing them.

The root `runtime-2d` plus `serde` combination includes the native carrier as an optional
`Runtime2dPlugins` default. The first-party Editor preview and generated starter metadata know
that same carrier; selecting a render recipe does not execute arbitrary Schema plugins in the
Editor. Material families, source import and processed codecs remain explicit selections.
Existing Runtime2d projects must refresh their authoring bundle after adopting the new default
carrier: run `nara schema <project> --write`, then `nara schema <project> --check` through the
normal project tool. Missing selected owners remain a Runtime-agreement error. Default-based
recipes must not also add the same plugin explicitly; deliberate duplicate selection still
rejects. A direct custom group can disable `Material2dPlugin` or install its explicit contribution.
This admits source editing of the carrier; it does not add automatic Material asset hydration
or Shader recompilation to the Editor Scene preview's existing Image/TileSet import owner.
The independent runtime and Export workflows select the actual Material source domain.
For a custom plugin group, select the material plugin and its schema through ordinary compiled
composition:

```rust
use nara::{ProductRecipe, SchemaContribution};
use nara::material::{MATERIAL_SCHEMA_PROVIDER, Material2dDomain, Material2dPlugin};

let recipe = ProductRecipe::new()
    .add_contribution(
        SchemaContribution::<Material2dPlugin>::for_default([MATERIAL_SCHEMA_PROVIDER])?
    )?
    .add_asset_domain::<Material2dDomain>()?;
```

A Scene stores `Material2dRecord::new(material_reference, &declaration, parameters,
texture_overrides)?.to_component_value()?` under `nara.material2d`. Parameters are complete
semantic values in six required typed maps: `f32`, `i32`, `u32`, `vec2`, `vec3` and `vec4`.
The literal path is `parameters.<kind>.<semantic-id>`, and a semantic ID may occur in exactly
one map. Empty kind maps remain present, so the carrier has a complete typed data shape that
the existing preservation-safe Editor field operation can validate. Native decoding additionally
checks exact vector lengths and material declarations validate the complete actual key/type set.
Texture overrides are optional per declared texture ID. Missing overrides use the Product's
defaults. Handles, local sequences, Rust type identity and encoded GPU bytes are absent from
this record. All semantic IDs are bounded to 128 ASCII bytes and follow persistent field-ID
grammar. Declaration-derived tooling fields retain full literal paths; their namespaced digest
IDs fit even when the original semantic key uses the whole allowance.

Source and processed preparation validate each record against its own loaded material Product
before startup or Scene replacement. The original Scene transaction reserves handles and
publishes `Assets<Material2dAsset>` with Images through the shared states/events. Extraction
captures one exact binding after checking actual Product association and published slots for
the material and effective textures. A raw write revokes this managed derivation; explicit
code-first `Material2dBinding` remains a valid separate input. Supplying both inputs on one
entity is ambiguous and skips that entity with a `missing_materials` observation.

For processed delivery, resolve default texture selections to complete Products through the
admitted catalogue, select the material Runtime codec in the recipe, and call the existing
`CookedContentBuilder::add_material` alongside the real Image artifact groups and Scene. The
Runtime material format stores the original user WGSL body, declaration, defaults and complete
Product dependencies. It compiles that body through the same Naga authority without inventing
source paths or observations. Its source/processed generation uses sequence zero; sequence is
not persistent identity.

Public material, generation, Shader and input snapshots retain the original source byte/
generation leases or Runtime acquisition budget. Cloning a Shader alone preserves that custody.
The extra read-only views and canonicalized default maps are reserved before allocation. Owners
retain leases and the original immutable base without pointing back to a collection containing
themselves. This does not give a clone publication authority or exempt it from native resource
and submission checks.

## Source Scene Updates

A hosted source runtime accepts semantic notifications through
`ProjectScenes::notify_source_change(&AssetSourceChange)`. Paths use the original project-root
namespace, such as `assets/palette.wgsl`; metadata observations name the principal source path.
Feed notices from the project's selected source route. The call performs no file I/O and does
not publish an asset or replace the current Scene. Its boolean reports related or conservatively
queued interest, rather than exclusive ownership of an event.

The original TaskUpdate prepares one successor of the current Scene. Observe
`source_update_status().request`, then use the ordinary request `state`, `prepared` and
`diagnostics` APIs. Inspect Ready content to construct owned game data; explicitly call
`commit_prepared_project_scene` with the game's overlay, or
`begin_prepared_project_scene_travel` when native domains are installed. A source notification
does not preserve arbitrary gameplay data automatically. A terminal result occupies the original
slot until `acknowledge` receives its exact request identity.
Status observation retains an automatic request ID even if its current-instance or Runtime
generation basis becomes stale. That visibility permits terminal acknowledgement; it does not
authorize another notification, preparation or commit against the stale basis.

Ready content is immutable. A later notification queues one successor intent without changing
that candidate or allocating a third prepared generation. Notices during automatic preparation,
Ready inspection or managed native travel conservatively queue unknown dependency paths as well.
After terminal acknowledgement, the next TaskUpdate can prepare the queued successor. Cancellation
still waits for the original worker's physical return before releasing its slot. Admission errors
remain observable in `source_update_status().error` and leave pending intent available for retry.
An accepted pending intent also survives an intervening explicit Scene switch, so a shared
dependency edit or full rescan observed while the target was frozen can update the actual new Scene.

Successful publication replaces the dependency interest with its actual content closure. A valid
failed automatic preparation retains the latest complete attempted-read cut as additional retry
interest. For example, changing a Material from `a.wgsl` to missing `b.wgsl` preserves the usable
old Scene, and a later creation notice for `assets/b.wgsl` can trigger recovery. Once the new
closure publishes, obsolete paths stop triggering work. Failed-read paths use the original
content budget and retain no decoded generation. An incomplete observation cut is refused and
cannot replace the last complete retry interest.

The Material importer propagates required Shader and Image dependency failures. A generic
importer that handles a missing optional input and successfully returns a fallback has a different
contract: that absent file is not a member of the successful content closure. Automatic recovery
when it appears is not guaranteed by this route; supply a full rescan for that availability change.
Successful fallback availability needs an explicit content read-interest contract before it can
be qualified as dependency-directed watching.

Call `request_source_rescan()` after queue overflow, broad observations or translation failure.
Catalogue membership and identity availability also require a rescan: a newly created metadata
file can introduce or conflict with a Product whose source path was previously unknown. Exact
file-path interest does not subscribe to directory absence or unresolved UUID availability.
The existing `AssetWatcher`, bounded `AssetWatchEventQueue` and `AssetWatchTranslator` can supply
notices; this API does not install a native watcher. The stock `AssetWatchPlugin` drains into the
generic per-asset reload route. Do not also send Scene-managed publications through that route:
its independent asset-state changes would conflict with the Scene residency transaction.
Composition must select the route for each publication; mixed routing needs explicit arbitration.

Processed runtimes refuse this source ingress. Global Input and project settings retain their
original startup selection and compatibility checks; a Scene source update does not replace them.
Stop fences new notices and drops source interest while the original task/native owners retain
unfinished cleanup.

## Native Source Scene Watching

With `asset-watch`, opt into the Scene route through your existing product recipe:

```rust,ignore
use nara::project_host::ProjectSceneWatchPlugin;

let watching = ProjectSceneWatchPlugin::new(absolute_project_root)?;
let recipe = recipe
    .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(watching, Clone::clone)?;
```

The root is explicit and absolute. Actual source Host admission compares its directory identity
with the original loader, then starts one native worker for selected source directories. Recipe
resolution itself starts no watcher. The project root is nonrecursive; configured Assets, Scenes
and Prefabs directories are recursive, with uncovered schema/Input parents added. Missing optional
directories use the nearest existing parent until a structural event rebuilds coverage. Normal
Cargo output directories are not recursively observed. Uncovered intermediate ancestors are
watched nonrecursively so replacing a nested source parent rebuilds coverage instead of leaving
watchers on its old directory objects. Coverage admits at most 128 native paths on one worker;
an excessive selection refuses before native acquisition rather than silently omitting paths.
Processed Hosts reject this plugin, and
ordinary unbound Runtime startup refuses it. Do not also add the generic `AssetWatchPlugin`.

Coverage starts before a mandatory full source rescan. The admitted initial Scene stays usable;
observe `ProjectScenes::source_update_status()` and explicitly inspect, overlay, commit and
acknowledge the original request as above. The initial rescan closes the capture-to-watch gap.
File changes feed dependency interest; metadata and catalogue changes request full revalidation.
Global settings, schema selection and Input remain startup-owned rather than being silently
reconfigured by this observer.

Some backends provide only undifferentiated changes. notify's Win32 events use `Any` kinds;
the existing conservative adapter marks them as incomplete observation. This path therefore
rebuilds coverage and requests a full Scene rescan for those Windows edits. It does not establish
dependency-directed native filtering or a numerical edit-cost budget. Exact semantic notices
and backends with proven file scope retain the existing dependency-interest path.

Queue overflow, backend errors and uncertain translation start fresh coverage before requesting
another full rescan. The old queue's loss flag stays sticky; it is not reset in place. A failed
replacement exposes `ProjectSceneWatchState::RetryRequired` and its error through
`source_watch_status()`. After fixing the cause, call `retry_source_watch()` to request one
TaskUpdate retry. `Watching` describes native coverage, not committed Scene freshness.
An unsupported lifecycle access returns a classified Runtime fault; a watcher retry does not
override the Runtime's fault/close decision or reopen an already closed Host.
`rescan_pending` describes undelivered full-rescan intent; once delivered, pending/request/result
state belongs to `source_update_status()`. Native counts and `restarts` distinguish coverage
replacement from Scene publication.

Before Stop, retain `source_watch_retirement()` if the application needs to observe physical
worker completion after its World or Host receipt is retained. The read-only observer provides
`stats()`: a successful completed close has no active/retiring sessions and equal started/joined
counts. Every session, including failed watch setup and superseded coverage, remains with the
original Runtime close participant until the original worker is joined. A joined panic remains
an error; observing callback destruction alone is insufficient. Explicit Close/Retry and deadline
semantics stay with Runtime. The maintained [notify source note](../../vendor/notify/NARA-PATCH.md)
records the bounded Windows/Linux join patch and its OS cleanup/platform limits. Managed watcher
registration on other platforms currently refuses before native acquisition.

## Live Source Updates And GPU Execution

The independent Material consumer also provides an opt-in offscreen GPU trajectory. It uses
the same physical file edits, original Scene requests and eight explicit commits as the
CPU-content control. Its caller drives `HeadlessSession::frame`, observes the immutable CPU
input taken by a `WgpuPolicyCaptureSet::Publish` system, then submits and reads that frame
through one external `WgpuRenderHost`. The native owner stays outside the gameplay World.
The project's requested `render-wgpu` and `presentation-2d` capabilities do not imply
`desktop-winit` or a native-window presentation consumer. Its Window component is logical
32-by-24 target data, with no OS window or native handle provider.

The pixel oracle independently decodes the fixture's sRGB Images into linear RGB, applies the
selected Shader gain and straight-alpha source-over blending, and compares the linear RGBA8
output. Every committed state and every refusal has a fresh captured, submitted and read-back
frame; a retained old output alone cannot qualify last-good rendering. The original startup
input is first submitted after a Shader successor, through a separate producer in the same
Host. Another unsubmitted input is retained across an Image successor. Release each old CPU
input after its original pixels are verified: Source snapshots retain their original content
budget, whose two-generation limit deliberately refuses a third retained generation. GPU
outputs have their own retained-byte custody and can remain across all later publication.
Each producer retains its original monotonic frame sequence. Retained outputs are read
again after Runtime Stop and producer retirement. Final cleanup requires the original native
watchers joined and the GPU Host's retained logical bytes and cleanup faults at zero.

Run serially with the original absolute `CARGO_TARGET_DIR`, `CARGO_BUILD_JOBS=1`, an actual
adapter, and `WGPU_BACKEND=vulkan` or `WGPU_BACKEND=dx12`:

```text
cargo +1.95.0 nextest run --locked --offline --release --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-material-module-consumer --features native-source-watch-gpu --test source_watching --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

Missing adapters and timeouts fail this ignored test. This is live offscreen source/Runtime/GPU
evidence, not OS-window presentation, scanout, device reconstruction or a numerical edit-cost
budget. The CPU-only `native-source-watch` feature remains independently usable.

## Windowed Source Update Control

The ordinary `source_watching_window` example drives the same physical edits through a source
`DesktopRun` on its native main thread. It creates one normal 320-by-240 window, configures a
game-owned Camera/Sprite and leaves batch consumption with the original window Publish owner.
A `WgpuPolicyFrameReader` inspects the sealed stock input before Publish and retains only copied
frame/content facts. The game waits for a matching actual presentation receipt before advancing
each representative committed/refused state. No GPU Host or native resource is stored in World.

Select the original target directory, one Cargo job, `WGPU_BACKEND=vulkan` or `dx12`, and explicit
`NARA_WGPU_FORCE_FALLBACK=0` for a hardware run (`1` is a separately labeled software control):

```text
cargo +1.95.0 run --locked --offline --release --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-material-module-consumer --features native-source-watch-desktop --example source_watching_window
```

The example creates and edits its own temporary source project. It does not edit an existing
game or require Editor/Dialogue UI. It joins each receipt's Runtime, App frame, original render
capture, target/surface incarnation and backend/device epoch with the exact captured Material,
Shader and Image facts. After the final Done state actually presents, the existing owner seals
bounded completion telemetry before exit. The recorded duration is App-frame-start to original
GPU completion; it is neither GPU-only duration nor source-edit latency. Final checks require
watcher join before World destruction, original native target/provider/surface retirement and
one-time recovery of the original physical event loop.

Deadlines are cooperative and cannot interrupt synchronous source admission or a graphics-driver
call. Automated execution should also bound and reap the ordinary example process. A deadline,
missing adapter or incomplete retirement is a failure. These receipts establish original surface
submission/present calls, not window-pixel equality, human visibility or physical scanout. The
separate offscreen control above retains the fixed-tolerance pixel and old-input replay evidence.
Windows native notifications still use the documented conservative full-rescan route.

For fixture failure controls, set `NARA_SOURCE_WINDOW_FAULT` to `update`, `capture` or `observer`.
Each injects a callback panic after earlier states have actually presented. The fixture retains
the original panic within its callback boundary, closes the original desktop owners, verifies
the native target and watcher shutdown, then resumes the panic. Expected exit code 101 is only
qualified when the `native source fault cleanup` marker also reports successful physical loop,
native target and join-before-World-drop checks. An abort, timeout or nonzero exit alone is not
cleanup evidence. This contains the fixture's assertions; production engine panic policy is
unchanged.

## Independent Delivery Controls

The independently locked Palette consumer exercises prepared source edits, Undo/Redo, exact
Save/Reopen/Check, source material residency, source-free processed running and rejected content:

```text
cargo +1.95.0 nextest run --locked --offline --release --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-material-module-consumer --test workflow --test persistent_workflow --test processed_workflow --test source_updates --test-threads=1 --no-fail-fast --no-tests fail
```

The separate ignored delivery controls build actual executables through the public Cargo and
publication owners. Source Export exercises headless and desktop product observations before
moving the distribution and deleting its source project; it then runs the relocated headless
product from an unrelated working directory. Cooked delivery imports and cooks the actual PNGs,
generates a Player bootstrap, binds the captured executable to the content, moves the three-file
package and deletes the source/cache before sealed headless launch. It checks decoded pixel
values and live Scene/material Product identities, not GPU scanout or a source-free desktop run.
The source Export control requires an interactive Windows desktop; both require the pinned
Rust toolchain and offline dependencies. Run them serially with `CARGO_BUILD_JOBS=1` and the
absolute repository `target` selected through `CARGO_TARGET_DIR`:

```text
cargo +1.95.0 nextest run --locked --offline --release --manifest-path tests/fixtures/cargo-build-consumer/Cargo.toml --test material_source_export --test material_cooked_delivery --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

Actual outcomes and remaining scopes belong in the
[R9b qualification](../plans/2026-11-14-durable-material-and-shader-plan.md#persistent-product-successor).
