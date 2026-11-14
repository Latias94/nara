# Durable Materials And Tracked Shader Artifacts

Status: active. This is the complete R9b successor of the
[foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md#r9-material-artifacts-views-color-and-domain-render-locality),
starting from `4bd02b9`. Editor/component and Dialogue UI development remain paused. Native
qualification shares the original serial Cargo owner with Runtime work.
The declaration/Shader/native prerequisite is implemented at `899d64b` with the evidence below;
the complete persistent product outcome remains active.

## Product Outcome

The Palette material is authored with stable typed parameters and resource references, saved
and reopened through the existing Scene/session owner, checked, exported and run independently.
A second materially different uniform layout uses the same declaration, compiler and native
preparation path. Shader edits publish only a compatible complete generation. Invalid edits
retain the same-epoch last-good, and retained old captures use their original values, resources
and Shader artifacts even when first realized after a newer generation.

This unit replaces the startup-only static-text, one-vec4/one-texture extension. Merely adding
serde to `Material2dInstance` or keeping an old pipeline cache entry does not complete it.
No visual Shader Graph, new renderer, universal publication manager or parallel asset registry
is selected. R9a View/history, R9c measured geometry locality and broader 3D retain their scopes.

## Current Source Findings

- `nara_material` stores static Shader text in `Material2dDefinition`; `Material2dInstance`
  explicitly excludes durable Scene codecs and stores process-local Image handles.
- `nara_sprite_render` owns extraction and `PreparedMaterial2d`. Its immutable captures retain
  exact Image snapshots. Both Sprite and Tilemap already share this path.
- `nara_render_wgpu::quad` fixes the material group to a 16-byte uniform and one D2 texture.
  `StockQuadResources` caches Ready and Rejected pipelines by the full static definition.
  Value/Image-content updates do not enter that key, but no current material publication or
  dependency-directed Shader update exists.
- The original Host, native preparation, allocation preflight and submission prefix already
  retain resources independently from cache eviction. Reuse them. The same-producer successful
  frame watermark rejects old frames; delayed A must use an admissible independent producer
  in the same Host/epoch rather than bypassing frame admission.
- Scene schema/codecs, capability-bound source import, Check/source Export, cooked Runtime
  codecs and physical content residency already exist. Imported products' typed dependencies
  still need a complete closure: copying a raw dependency file alone does not import/publish
  the Image product referenced only inside a Material.

## Selected Boundaries

One material declaration owns stable semantic field IDs/version, uniform field types and
texture references. Its binding projection owns actual uniform size/offsets and group bindings;
its Shader artifact owns actual compiled source/interface and tracked source observations.
Rust parameters produce this declaration and encoder from one field declaration. The same
validated declaration constructor is the future Shader-authored admission seam: Rust `TypeId`
is neither persistent identity nor a substitute for layout compatibility.

Use actual uniform-layout/encoding and Shader compiler libraries, with the repository's
existing pinned WGPU/Naga source. Do not implement WGSL parsing, a name resolver or guessed
alignment rules in a script. WGSL qualifies first; a future WESL frontend must report its real
resolved and failed dependency closure through the same artifact boundary.

The pinned Bevy reference is `03ed5a27c92715c8fa2188f31f04e454ec39942f`, especially
`crates/bevy_render/macros/src/as_bind_group.rs`, `crates/bevy_encase_derive/src/lib.rs` and
`crates/bevy_shader/src/shader_cache.rs`. Reuse its separation of field declaration, real
uniform encoder and actual compiler dependencies. Do not copy its invalidation policy as proof
of Nara's retained-capture/current-publication contract. Encase 0.12.1's upstream source is
`561121cd17298355de74c07a7206eb8329fd8347`; `encase_derive_impl::derive_shader_type` accepts the
generated runtime support path, while merely reexporting its ordinary derive would still require
a consumer dependency named `encase`. Glam 0.33.12 supplies the matching `encase-012` feature.
Naga remains at WGPU's `cddb2f5d80dc0009b2ef6767635140cacf90a007`.

The small `nara_material_derive` crate is justified by the two actual parameter layouts, not
another runtime layer: one field traversal supplies stable IDs, typed values and Encase's
generated layout/encoder. Arrays are not WGSL vectors. Runtime Shader/parameter code remains
inside `nara_material` behind an explicit feature so sampler-only users need no Shader compiler.
The existing Reflect type is an optional generated tooling projection, not a separately
maintained material field table. General reflection/App dependency restructuring is not part
of this material workflow.

Semantic fields, binding ABI and Shader interface have distinct fingerprints, joined in one
immutable admitted generation. A prepared edit retains its expected current generation and
cannot replace a newer publication. Invalid source/interface leaves current last-good intact.
Captured instances retain the exact complete generation, encoded values and referenced resource
versions. A late old capture can realize its own pipeline but cannot publish itself as current.
Value-only and ordinary Image-content-only edits change bindings/resources without multiplying
pipeline variants. Unrelated material declarations remain usable.

Persistent Scene values contain semantic parameters and durable resource references, never GPU
handles, process-local Image handles or serialized Rust type identity. Reuse component schema,
reference traversal and codec ownership. Resource import/Runtime codecs and source/processed
residency use existing project content owners; current replacement must not resolve candidate
values against the incumbent's old domain collection.

Native layout, Shader and uniform resource preparation stays in the original stock quad
adapter. Extend preflight, exact layout/pipeline keys and submission-held resources together.
Cross-Feature texture inputs must join existing graph resource versions and declared reads;
an opaque bind group alone does not satisfy that contract. No second Queue, submit or epoch
owner is introduced.

## Implementation Order

1. Build the concrete two-layout typed declaration, parameter encoder and compiler interface
   with stable semantic identity, bounded input and inspectable validation errors.
2. Replace static material captures and fixed group construction through Sprite/Tilemap,
   original native preflight/cache and actual same-epoch GPU acceptance.
3. Add complete persistent schema/reference/source/Runtime content plumbing and extend the
   independent Palette consumer through Save/Reopen/Check/Export/run.
4. Qualify compatible B, rejected C and delayed A; exact dependency-directed updates; value/
   texture controls; actual pixel and original retirement evidence. Remove superseded APIs.
5. Reconcile ADR0033/0094 and relevant guides/ledger, review independently and commit precise
   changes. Full R9b remains open until its product and interleaving acceptance pass.

## Verification

- Owning CPU tests cover unique stable IDs, type/value/ABI disagreement, finite value and
  bounded input rejection, dynamic/typed encoding equality and current-generation revalidation.
- Two actual layouts include mixed scalar/vector alignment and a materially different resource
  set, rather than another name for the original fixed vec4 contract.
- Shader validation uses actual Naga module/layout results. Malformed source, missing/wrong
  resources and incompatible interfaces retain the compatible current generation.
- An independently locked consumer authors, edits, saves, reopens and checks the Palette
  material, then runs exported/source-free content with a texture referenced only by Material.
  Missing/wrong Product and format/target inputs reject before gameplay publication.
- One valid native Host/device epoch retains A, publishes compatible B, refuses C, then first
  realizes A through an independently admitted producer. Readback proves A/B pixels; variant
  counters separate Shader changes from value/texture-content controls. Existing submitted
  resources remain held to their original completion prefix.
- Record actual cold compile/edit-to-ready/variant observations without claiming a performance
  win from uncontrolled build time or test duration.
- Serial locked/offline Cargo reuses root `target`, jobs one and explicit Rust 1.95.0. Match
  tests to changed owners, preserve paused UI work, and record failures and qualification limits.

## Implementation And Qualification In Progress

The first implementation replaces static definition/instance APIs with `Material2dBinding`,
exact immutable generations and two-layout parameter/Shader admission. The library's current
and failed values delegate to original `Assets<Material2dGeneration>`/`AssetStates` and captured
`AssetMutationBaseline`; the first draft's separate `Entry.current` was removed after independent
ownership review, before consumer integration. Source/processed residency must use these same
actual stores rather than keep a synchronized second current. Source dependency observations
are explicit requested/read inputs, including missing reads, not invented WGSL import results.

Actual Encase 0.12.1 derives the layout/encoder and Naga's original pinned compiler validates
the assembled stock/material interface. Pure sampler users retain a compiler-free feature path.
Both declaration digests are computed once on admission. Native pipeline identity excludes
publication sequence, semantic IDs and parameter/texture content; its exact source/ABI remains
retained by the old capture. The original stock WGSL is moved to the material artifact owner and
shared by ordinary/native paths instead of maintaining a validation-only copy.

The owning all-feature production library check passes on Windows/Rust 1.95.0. The first CPU
test build fails in a new test: storing Encase's move-only `Metadata` in a local and calling its
consuming `offset` method repeatedly. Reading the associated constant separately corrects the
test without changing the ABI contract. The second run passes **82/82**, zero skips, in
`00b75977-87fd-4439-b216-cd788d0ec396` (4.551 seconds after a 1m13s release build). It includes
declaration/parameter, actual compiler-interface, derive, five public generation workflows and
the complete Sprite/Tilemap consumer suite. The no-default-feature material library check also
passes (17.71 seconds). Independent consumption and native qualification are still in progress.

These results do not complete the persistent schema/source/Runtime product plumbing, full
R9b, graph-shared material inputs or the foundation Goal. No numerical performance improvement
is inferred from build/test durations.

The independent correctness review finds two concrete derive defects outside the first green
inputs: a supported field named `reader` is shadowed by Encase's unrelated generated reader,
and a scalar alias named `ArrayLength` triggers its syntactic runtime-array special case. Actual
public compile counterexamples reproduce both. The adapter now retains official ShaderType,
ShaderSize, WriteInto and constraint generation, omits unused buffer reader implementations,
and wraps the cloned field type in a transparent syntax group so Rust resolves its actual type.
Semantic reconstruction continues through the original material values. The extended owning
suite passes **83/83**, zero skips, in `a06bc9e8-2e35-411c-9768-ecea181bc3f3` (4.535 seconds
after a 1m12s build). Independent rereview closes both P2 findings. Correctness Clippy for the
material, derive and Sprite/Tilemap owners passes; existing dependency/style warnings remain.

The independently locked material consumer compiles and passes **1/1** in
`683c981a-bf7b-4431-ab68-faed097edfbd` (0.042 seconds after 22.42 seconds). Its first run
`9efbb0a6-d55f-4b0b-8642-01bc398a0863` fails a new test's guessed variable-name assertion from
another Palette fixture; comparing the actual complete input and assembled source corrects that
oracle. Old typed values, signed-zero bytes, handles, Shader artifacts and failed-edit last-good
are retained through library retirement. The no-default normal dependency tree contains only
`nara_material`; sampler policy therefore needs neither ECS nor a compiler. The production
all-feature native library check passes (1m09s), while actual native tests remain pending.

One further ownership review identifies a new sustained-edit retention defect: the former static
pipeline cache appends every distinct Shader artifact until device-epoch replacement. After
ordinary old captures, outputs and original submissions retire, cached Ready/Rejected keys still
retain those old generations, source bytes and native pipelines. Single-candidate budgets do not
bound that cross-call history. The selected repair is a private 64-entry LRU cache, trimmed after
the entire preparation call on both success and failure, with orphaned binding-layout lookup
entries removed. Prepared/submitted draws keep their actual handles independently. One call may
prepare more than 64 variants; the bound governs retained acceleration state after return, not
product admission or a complete native-memory budget. Qualify both multi-call edit churn and a
single legal call exceeding the retained cache, including late original capture realization.

The native adversarial review closes that cache-retention P2 after the common success/failure
exit trims Ready/Rejected LRU entries and orphaned binding layouts. It confirms that trimming
occurs after complete draw resource preparation; old native pipelines/bindings remain held by
prepared work and original submission custody. The active test history is retained:

- The first native test compile finds one old helper invocation missing the newly explicit
  Image handle; the retained TileSet's own Image handle corrects that fixture.
- The second compile catches a new negative-test closure returning the success shape expected
  by neither the original `with_preparation` nor its asserted rejection; returning the actual
  ContentRejected error corrects that fixture.
- The first complete ordinary native-owner run `2f0b6650-acf0-41ba-898f-6dd9ca50e7fb` executes
  415 cases: 414 pass and one new rejection-cache assertion fails, with 65 explicit skips.
  The third private statistic is layout count, not Ready count. The correction keeps one shared
  layout, bounds entries and separately checks that each cached entry is Rejected.
- The first Vulkan focused run passes 12/13. The new churn trajectory incorrectly allocates a
  new source generation at every call and is truthfully refused by the unchanged exact source
  gate. Reusing one source generation for that trajectory corrects the fixture without widening
  admission. Its original independent producer, increasing frame index and old-output controls
  remain intact.

The corrected focused run with Vulkan selected for the production Host **passes 13/13** in
`a07d9528-64f4-4eef-b502-013a1226c35f` (11.031 seconds after a 2m26s release test build),
with 467 cases outside the explicit selection. It executes all five ignored native material
Host cases plus material capacity, budget, identity and cache controls. The production Host
reports NVIDIA GeForce RTX 4090, Vulkan, NVIDIA driver 621.14, with forced fallback disabled.
The separate private `ReadbackGpu` controls retain their original default adapter/fallback
selection; the environment setting does not establish their individual backend or hardware.
Evidence
includes nonzero mixed scalar/vector parameters with pixel tolerance one, integer-mode control,
both texture contributions, compatible B/current, interface-incompatible C refusal, first
realization of retained A, same original Host/device epoch, unchanged pipeline variants for
value/texture-content updates and exact producer/source admission.

The same focused run proves shared Image deduplication across multiple captures while retaining
per-occurrence buffers/custody; allocation demand one byte short rejects before uploads and exact
demand succeeds. Partial texture OOM retains prior work through the original prefix, cache clear
does not refund live holders, frame watermark remains unchanged and the same frame retries.
Cache controls execute a single 66-variant working set, further edits, late evicted-A realization,
old output readback and 66 rejected native Shader variants. Retained cache entries remain at
64 and the shared binding layout remains one; original output/resources retire normally.

One sample records CPU initial compile 645.7 microseconds, compatible edit compile 327
microseconds and first Host submission 3.4823 milliseconds. These are one fixture observation
per interval, not P50/P95, pure GPU compile duration, scanout or an optimization claim; the Host
interval includes its actual device/pipeline preparation and submission, excluding later readback.
No general large-game material cost or foreign native qualification follows.

The full workspace release all-target check passes after the owned formatter runs (5m42s).
Seventeen owned lockfiles retain every original package version/source and add only the required
material encoder/compiler closure. Sixteen independent Windows metadata graphs resolve locked;
the paused GPUI consumer is explicitly excluded. An initial unfiltered offline metadata attempt
reaches an unrelated uncached `cstr` foreign dependency; the target-filtered Windows resolution
passes without downloading or changing that package. No runtime execution is inferred from
metadata. The ordinary stable formatter preserves the already-qualified Winit source; its
nightly-option warnings do not indicate changed vendor behavior.

Representative owning commands, with the original absolute target and jobs one, are:

```text
cargo +1.95.0 check --locked --offline --release --workspace --all-targets
cargo +1.95.0 nextest run --locked --offline --release -p nara_material -p nara_material_derive -p nara_sprite_render --all-features --lib --test generation_workflow --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-material-module-consumer --test workflow --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --offline --release -p nara_render_wgpu --all-features --lib --run-ignored all -E 'test(stock_quad::material_tests::) | test(host::program_tests::material::)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

The focused native invocation sets `WGPU_BACKEND=vulkan` and `NARA_WGPU_FORCE_FALLBACK=0`.
The [material guide](../guides/materials.md) describes current explicit source admission and
retained bindings without claiming the pending persistent authoring/import workflow.

### Final Owning And Native Results

After formatting and retaining the original lockfile package selections, the combined owning
run passes **498/498**, with 65 explicit skips, in
`85b905ec-b338-4d58-a132-cd6f54c9fb09` (140.491 seconds after a 13m32s release build). This
includes all 83 material/derive/Sprite controls and all 415 ordinary native-owner cases, closing
the corrected rejection-cache assertion from the earlier aggregate. The ignored native material
Host cases retain their separately executed selection rather than being counted as ordinary
passes. Counts from focused/aggregate runs overlap.

The same five production Host trajectories also pass on **DX12: 5/5**, in
`a1899417-37b1-4bde-bd94-eec08e91e551` (4.924 seconds, 558 outside the selection). The Host
reports RTX 4090, discrete GPU, DX12, driver 32.0.16.2114. The sample records CPU initial
compile 652.1 microseconds, compatible edit compile 278.7 microseconds and first Host submit
call 28.5046 milliseconds. These timestamps bracket `publish_material` and `submit_program`
calls, after `begin_frame`; they do not attribute earlier device setup, readback or hardware
execution. Neither these samples nor the Vulkan sample establish a backend performance winner.

The first DX12 command selected only the native package, creating a different feature union and
unnecessarily rebuilding dependencies. Its exact owned compiler/launcher descendants were
identified, cancelled and confirmed terminal before another Cargo invocation. No test result
is claimed for that cancelled build, and no other session's process was touched. Reusing the
same four-package/all-feature/target selection as the owning run completes the DX12 test build
in 2.50 seconds. Final all-target correctness Clippy for those four owners passes (1m13s),
with existing dependency/style warnings. All verification remains serial and uses the original
target; no clean or parallel Cargo is introduced.

The independently locked renamed-root material test passes **1/1** in
`69e7125d-f728-4a71-b6df-6a6b8fc4eb21` (0.041 seconds after an 8m07s release build).
Its manifest still declares only `engine`; the macro locates the facade's runtime support
without a direct encoder dependency. The same actual test combines `reader`, the scalar
`ArrayLength` alias, a vector's aligned offset, exact bytes and semantic reconstruction.
The direct independent Palette package also passes all-target strict Clippy with `-D warnings`
(34.71 seconds). Neither result is inferred from token parsing or a typed but unexecuted closure.

Final root, module-consumer and renamed-root `cargo fmt --all -- --check` pass. Added local
document links/fragments, affected TOML/lock parsing and normal staged/working whitespace checks
pass. The imported Winit nightly-option warnings retain their known scope; vendor and physical
DPI source have no new changes. The pre-1.0 static definition/input/instance APIs are removed
after migrating all real Rust consumers, with a concrete guide and Conventional Commit breaking
change note rather than compatibility aliases.

All eleven scoped research/implementation/review agents are closed, including the two reused
native-worker/reviewer threads. The source findings, two derive P2s and sustained-cache P2
have been reconciled and independently rereviewed. No remote publication or main/branch
movement is part of this unit. Complete persistent material delivery, shared graph texture
inputs, numerical production budgets, foreign native execution and full R9b/R12/foundation
completion remain open.
