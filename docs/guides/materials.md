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

Its Shader source remains an explicit startup input until the persistent/source workflow in
R9b completes; this guide does not imply an automatic file-watching Editor material panel.
