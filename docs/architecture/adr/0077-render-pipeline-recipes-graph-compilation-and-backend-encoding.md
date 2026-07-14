# ADR 0077: Render Pipeline Recipes, Graph Compilation, and Backend Encoding

**Status**: Accepted
**Date**: 2026-07-11
**Refines**: [ADR 0017](0017-render-graph-policy.md),
[ADR 0032](0032-render-backend-integration-boundary.md), and
[ADR 0040](0040-render-resource-lifetime-and-submitter-ownership.md)

## Context

nara already separates gameplay authoring, backend-neutral render extraction, domain-owned queueing,
and the wgpu adapter. `RenderPassPlan` makes the current clear/world/UI/gizmo order explicit, but it
is intentionally a static phase plan. It does not model logical resource reads and writes,
transient or history lifetimes, computed ordering, cross-target dependencies, pass culling, or
capability lowering.

The next architecture boundary must satisfy several product goals at once:

- simple projects get a complete default renderer without authoring a graph;
- projects may select different pipeline policies per view, similar to choosing a renderer or
  pipeline asset;
- trusted plugins may add portable render features without taking ownership of submission;
- advanced integrations retain a narrow raw-wgpu escape hatch;
- browser WebGPU, native desktop, mobile, editor viewports, and future render workers use the same
  product-facing contract;
- pipeline configuration, shader changes, and feature parameters can be validated, inspected, and
  switched atomically;
- the engine can later add a real resource graph without changing gameplay components.

Unity demonstrates that useful programmability is a permission gradient rather than one unlimited
callback: pipeline assets configure a renderer, renderer features contribute declared passes,
RenderGraph owns scheduling and resources, and unsafe passes trade optimization and observability
for control. Current Bevy demonstrates that ECS schedules are useful for pass execution order, but
schedule topology alone does not define resource lifetimes or a data-driven pipeline asset. Godot
similarly separates product-facing compositor effects from its lower-level rendering device graph.

This ADR fixes the ownership and vocabulary before nara fixes a public Rust graph API.

## Terminology

| Term | Meaning |
|---|---|
| Render backend | The concrete GPU implementation. nara uses wgpu rather than defining a second RHI. |
| Pipeline family | A code-provided rendering policy with compatible material, lighting, feature, and frame-topology assumptions, such as the portable 2D family or a future high-fidelity family. |
| Pipeline recipe/profile | Versioned, inspectable project data selecting a family, feature instances, parameters, quality policy, and capability fallbacks. |
| Render feature | A stable-ID code provider that contributes parameters, pass declarations, queue inputs, and capability requirements to a family. |
| Frame pass / graph pass | A declarative unit with stable identity, logical resource access, ordering constraints, side effects, and portability metadata. |
| Render phase | A category of queued render items, such as opaque, transparent, UI, or gizmo. A phase is not a frame pass. |
| Material technique | A future material/shader implementation for a family, phase, and feature set. It is not the pipeline recipe. |
| Backend render pass | A wgpu command-encoding concept owned by `nara_render_wgpu`; it is not persistent authoring data. |
| Compiled pipeline template | A reusable backend-neutral result derived from recipe, providers, and semantic capabilities. Frame packets do not participate in its identity. |
| Frame execution plan | A backend-neutral frame-local instantiation of a captured active pipeline-set snapshot against current views, targets, batches, and dynamic extents. |
| Device-epoch backend realization | The wgpu shader, pipeline, layout, retained physical resource, and cache state paired with a logical template for exactly one live host/device epoch. |

## Decision

nara adopts an engine-owned pipeline compiler with data-driven recipes, declared feature/pass
contributions, backend-neutral frame packets, and one wgpu execution backend.

```mermaid
flowchart TD
    World[Gameplay World] --> Extract[Domain extraction and queueing]
    Extract --> Packet[Owned RenderFramePacket]
    Project[nara.toml project authority] --> Recipe[Resolved pipeline recipe slots]
    Providers[Rust feature and pass provider catalog] --> Compiler
    Recipe --> Compiler[Engine-owned logical compiler]
    Caps[Semantic render capabilities] --> Compiler
    Compiler --> Candidate[CompiledPipelineTemplate candidates]
    Candidate --> Realize[Device-epoch backend realizations]
    Realize --> Active[Active pipeline-set generation]
    Active --> Instantiate[Frame instantiation]
    Packet --> Instantiate
    Instantiate --> Plan[FrameExecutionPlan]
    Plan --> Host[WgpuRenderHost execution authority]
    Packet --> Host
    Host --> Target[Acquire, encode, submit, present]
    Host --> Observation[Structured render observations]
    Observation --> Tooling[nara_tooling and headless diagnostics]
```

### wgpu is the RHI

- nara does not define a parallel `Device`, `Texture`, `Buffer`, `CommandEncoder`, or `Queue`
  abstraction that mirrors wgpu.
- `nara_render_wgpu` owns exact `wgpu::Features`, `wgpu::Limits`, downlevel capabilities, surface
  capabilities, native handles, command encoding, and submission.
- Core and domain crates expose stable rendering intent and semantic capability requirements, not
  raw wgpu types.
- A speculative generic render-backend trait remains rejected. A second concrete backend would
  require a new ADR and measured product need.
- The browser contract targets WebGPU. nara does not maintain a separate WebGL2 compatibility
  pipeline or silently lower to WebGL2.

### Pipeline recipes and families

- A project has an effective default pipeline recipe resolved through the existing `nara.toml`
  project-settings authority. A view may explicitly select another compatible recipe.
- A recipe selects a stable pipeline-family ID, feature instances, parameter values, quality
  policy, and capability fallback policy. Each resolved recipe/template slot has stable identity
  and generation. Every feature instance has an ID unique within that recipe.
- Recipe order provides deterministic iteration and tie-breaking only; it does not override declared
  dependency topology.
- Recipes store stable public IDs and semantic asset references. They do not store Rust `TypeId`,
  function pointers, runtime `Entity`, frame-local view indices, wgpu enum discriminants, or GPU
  handles.
- The persistent recipe envelope and exact Rust data types remain deferred until a real recipe is
  implemented. They must follow ADR 0049 parse budgets and ADR 0051 envelope policy. Compilation
  also enforces decoded expansion budgets before backend allocation, including bounded feature
  instances, passes, logical resources, edges, fallback variants, specializations, and diagnostics.
- A family owns compatibility rules between its material techniques, feature catalog, view inputs,
  and default frame topology. A family is not a GPU backend.
- The first product family is portable and WebGPU-baseline oriented. A high-fidelity family is
  introduced only when real material, lighting, and frame-topology differences justify it.

### Feature and pass provider catalog

- Rust plugins register providers under stable engine- or plugin-owned IDs.
- A static contributed pass/resource declaration identity is derived from recipe/template identity,
  pipeline family, provider ID, feature instance ID, and provider-local ID. Repeating one provider
  is valid only through distinct feature instance IDs.
- A frame execution instance adds the owned packet's view identity and any target/resource-scope
  occurrence to its static declaration identity. Multiple views or recipe slots therefore cannot
  collide while instantiating the same provider declaration.
- A provider declares its recipe parameter schema, pass contributions, logical resource accesses,
  capability requirements, fallback variants, queue inputs, and diagnostic identity.
- Recipe data selects registered providers; it never serializes executable callbacks.
- Missing, duplicate, conflicting, or incompatible providers fail validation with structured
  diagnostics. They do not silently disappear from a compiled pipeline.
- Rust provider code changes require recompilation or a future separately designed native-module
  boundary. Recipe parameters and shader/material data may use normal asset reload policy.

### Owned frame packet

- Extraction and domain queueing produce an owned, backend-neutral `RenderFramePacket` boundary.
- A packet may contain owned views, domain batches, upload payloads, semantic resource keys,
  generations, and frame metadata. Large immutable payloads may use bounded shared ownership.
- A packet does not borrow the gameplay `World` and does not contain `Surface`, `SurfaceTexture`,
  `Device`, `Queue`, command encoders, backend cache entries, or platform window objects.
- Packet ownership preserves the option to execute on the browser's local GPU executor, a future
  native render worker, or an editor-owned render host without changing gameplay authoring.
- The current main-world extraction model remains valid. A retained second render ECS world is an
  internal optimization, not a public pipeline contract.
- Packet producer, consumer, budget, rejection, and retirement rules must follow ADR 0036 before
  packets cross an asynchronous or threaded boundary.

### Template compilation and frame instantiation

`nara_render` owns validation and compilation from recipe plus providers plus semantic capabilities
into an inspectable `CompiledPipelineTemplate`. A frame packet does not participate in template
identity. When recipe, provider-catalog, schema, shader-interface, or semantic-capability generation
has not changed, static template validation and compilation are reused rather than repeated per
frame.

Each active recipe slot holds one complete logical-template generation plus its device-realized
generation. At the frame boundary, the execution coordinator captures an immutable snapshot of all
recipe slots selected by that frame's views. Publication during execution affects only a later
frame; one frame never mixes old and new generations of a slot.

Each frame instantiates that pipeline-set snapshot against current packet views, targets, batches,
dynamic extents, and enabled frame-local work to produce a backend-neutral `FrameExecutionPlan`.
Exact type names and whether the plan owns or indexes packet sections remain implementation
decisions; it may not borrow the gameplay `World` or contain GPU objects.

The compiler owns:

- stable pass and logical-resource identity validation;
- ordering, cycle detection, and target/view compatibility;
- declared read, write, attachment, copy, and side-effect validation;
- required, optional, and fallback capability resolution;
- logical lifetime and external-ownership declarations, with possible future forms including
  transient, retained/history, imported target, or persistent-cache resources;
- optimization eligibility and reasons work cannot be culled, reordered, aliased, or combined;
- deterministic template and specialization identity;
- decoded expansion budgets and bounded compilation diagnostics.

The listed lifetime and optimization forms are non-exhaustive vocabulary, not frozen enum variants
or algorithms. The first graph-only vertical slice decides the concrete resource schema, aliasing,
merging, barrier, and execution-queue models.

The compiler does not expose physical textures, barriers, heap offsets, bind groups, or command
encoders as project data. Backend lowering maps logical resources and passes onto wgpu objects.

The current `RenderPassPlan` remains a Phase 1 transitional static phase-order plan. It is neither a
persistent recipe nor the frame-local `FrameExecutionPlan` defined by this ADR, and it must not grow
compatibility scaffolding into a partial graph. When the first graph-only vertical slice lands,
nara may replace or delete it rather than preserve parallel pre-1.0 APIs.

### Frame and target transactions

One frame execution coordinator owns the global pass/resource dependency order, command-encoding
order, and queue-submission order across all targets. A target lease owns only external-target
acquisition/import, its final-consumer boundary, and presentation/publication.

1. capture the immutable selected pipeline-set snapshot and instantiate one frame-wide dependency
   plan;
2. resolve every ordered view and pass that contributes to each target;
3. acquire or import each external target at most once;
4. apply explicit clear/load, viewport, scissor, and composition semantics;
5. encode cross-target dependencies and perform one or more globally ordered submissions;
6. present or publish each external target once after its final consumer.

An external target handle never enters `RenderFramePacket` or recipe data. Multiple views targeting
one window do not independently acquire, clear, and present that window. Offscreen images and editor
viewport targets use the same transaction vocabulary as surfaces.

### Capability model

- The backend retains the complete exact wgpu capability snapshot for lowering and advanced
  diagnostics.
- `nara_render` defines only stable semantic requirements needed by recipes and providers.
- A requirement is classified as required or optional and carries an explicit fallback policy:
  disable the feature, substitute a named variant, or reject compilation.
- Capability lowering is deterministic and inspectable. A pass is never silently omitted.
- Raw `wgpu::Limits` and feature bitsets are not serialized into project files.
- Capability tiers are added only from measured pipeline needs; nara does not invent a comprehensive
  hardware taxonomy in advance.

### Extension permission levels

1. Recipe configuration is the default extension path and remains fully portable and inspectable.
2. Declared feature/pass providers may contribute graph work but do not own target acquisition,
   graph compilation, global resource lifetime, queue submission, or presentation.
3. An advanced wgpu escape hatch may receive a short-lived command context after declaring its
   resource scope and ordering contract. The standard escape hatch does not expose `Device`,
   `Queue`, target leases, engine caches, owned engine resource handles, or cloneable wgpu resource
   views. It exposes a non-cloneable callback-scoped encoding facade plus host-managed opaque keys
   for the declared resource bindings.

Opaque keys are bound to the captured plan generation and current host/device epoch. The facade
rejects undeclared, stale, cross-generation, cross-epoch, or out-of-callback use before encoding;
provider code cannot resolve a key into an independently retained backend handle.

Raw passes are marked in compiled observations. The compiler must conservatively disable or explain
optimizations it can no longer prove, including culling, aliasing, pass merging, or reordering. Raw
wgpu access is not part of gameplay preludes or the portable plugin ABI. An integration requiring
direct device/queue ownership or native interop is a trusted backend adapter with explicit epoch,
teardown, and portability policy, not a normal raw pass.

### Reload, failure, and observation

- Recipe, parameter, provider, schema, semantic-capability, or shader-interface changes first build
  a backend-neutral logical template candidate. A shader-content reload whose validated interface
  is unchanged reuses the logical template and starts only a backend-realization candidate.
- The host realizes every required statically knowable shader module, pipeline, layout, and
  generation-retained backend resource for the current device epoch. Only after logical compilation
  and required backend realization both succeed does nara atomically publish one complete recipe
  slot generation containing the template and its backend realization. A coordinated multi-slot
  change publishes one pipeline-set generation only after all required slots succeed.
- Dynamic target formats or extents, frame-transient resources, and explicitly lazy specializations
  are resolved during bounded frame instantiation/execution. Their failure skips the affected
  target or frame with a structured reason, or selects an already declared same-epoch fallback; it
  never partially mutates the active generation.
- During ordinary reload on the same live device epoch, failure at either candidate stage preserves
  the previous complete active generation. After a device-epoch change, the previous backend
  realization is invalid: nara may retain the logical template, but affected rendering remains in
  recovering/skipped state until a complete realization for the new epoch succeeds. Optional lazy
  variants must use declared fallback behavior and cannot partially replace the active generation.
- Template generation, backend-realization generation, device epoch, selected variants, fallback
  decisions, disabled features, pass/resource edges, lifetimes, cache pressure, and CPU/GPU timing
  eligibility are structured observations.
- `nara_tooling` consumes stable `RenderPlanSnapshot` and `RenderExecutionSnapshot`-style data, not
  GPU handles or backend pointers. Exact type names remain an implementation decision.
- Runtime and editor viewports consume the same compiled pipeline semantics. Editor overlays are
  explicit feature/pass contributions rather than a second hidden renderer.
- Important failures bridge into the ADR 0048 runtime diagnostics bus and remain observable in
  headless operation.

### Crate ownership

| Owner | Responsibility |
|---|---|
| `nara_render` | Recipe vocabulary, semantic capabilities, owned frame packet, provider catalog contract, plan/graph validation, compiled observations. |
| Domain render crates | Extract domain data, queue/sort/batch items, register domain feature/pass contributions. |
| `nara_render_wgpu` | wgpu host, exact capabilities, physical resource allocation, pipelines, encoding, submission, presentation, and device-domain caches. |
| `nara_app` | Declared extraction/prepare/queue/render stages and runtime lifecycle; it does not own GPU policy. |
| `nara_tooling` | UI-agnostic plan/execution inspection and editor commands; it does not own graph compilation. |

New crates are introduced only when implementation size or dependency isolation creates real
pressure. The decision does not require an empty `nara_render_graph` crate now.

## Alternatives Considered

### Option A: Keep one hard-coded renderer with fixed insertion hooks

**Pros**: Smallest initial implementation and easiest default path.

**Cons**: Hooks accumulate implicit resource and ordering contracts, make cross-target composition
fragile, and cannot provide Unity-like pipeline families without backend changes.

**Decision**: Rejected as the long-term extension model. The current `RenderPassPlan` remains a
transitional implementation.

### Option B: Expose raw wgpu as the primary scriptable render pipeline

**Pros**: Maximum Rust control with almost no wrapper code.

**Cons**: Couples plugins and project policy to wgpu versions, prevents safe graph optimization,
weakens WebGPU portability, and makes editor/headless inspection incomplete.

**Decision**: Rejected as the default path. Retained only as a restricted advanced escape hatch.

### Option C: Copy Bevy's RenderApp, retained render world, and schedule as the public model

**Pros**: Mature extraction and native pipelined-rendering precedent; pass-as-system composition is
familiar to ECS users.

**Cons**: Adds entity synchronization and a second ECS lifecycle before nara needs them. A schedule
does not itself define logical resources, transient lifetimes, or a serializable recipe. Bevy's
SubApp ownership also conflicts with nara-owned app lifecycle.

**Decision**: Rejected for the public model and deferred as a possible internal optimization.

### Option D: Engine-owned compiler with recipes, declared passes, and a raw escape hatch

**Pros**: Gives simple projects a complete default, preserves controlled customization, keeps
project data stable, supports WebGPU and editor inspection, and leaves physical optimization in one
deep module.

**Cons**: Requires a provider catalog, validation model, stable identities, diagnostics, and an
eventual real resource graph.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Default usability | The portable default family renders without project-authored graph nodes | Windowed example and plugin-group test |
| Backend isolation | No core/domain recipe, packet, or persistent type imports `wgpu` | Dependency-boundary search |
| Target transaction | One external target is acquired and presented at most once per target frame despite multiple views | Multi-view backend integration test |
| Global execution order | An offscreen-to-surface dependency is encoded and submitted in frame-wide order; target leases cannot submit independently | Cross-target integration and API-boundary tests |
| Capability transparency | Every unsupported required feature rejects or selects its declared fallback with a structured reason | Compiler capability matrix tests |
| Atomic reload | On the same live epoch, candidate failure leaves the prior complete slot or coordinated pipeline-set generation active | Last-good reload and backend-realization tests |
| Epoch recovery | A new epoch never executes the prior realization; logical templates may be reused only while rendering waits for complete new-epoch realization | Device-loss recovery tests |
| Per-view policy | Two views can select different compatible recipes without replacing the GPU backend | Multi-view pipeline test |
| Frame coherence | A frame captures one immutable pipeline-set snapshot and never mixes generations during concurrent reload publication | Reload-race integration test |
| Inspectability | Compiled observations expose stable pass IDs, resource edges, selected variants, and raw-pass limitations without GPU handles | Tooling snapshot tests |
| Template reuse | Unchanged recipe, provider, schema, shader-interface, and semantic-capability generations do not revalidate or recompile the static template each frame | Compile-count instrumentation test |
| Expansion bounds | Encoded and decoded recipe expansion limits reject oversized candidates before backend allocation | Boundary and hostile-input matrix tests |
| Escape containment | The normal raw-pass API exposes no device, queue, target lease, cloneable resource view, or owned engine handle; undeclared/stale keys and escaped facade borrows are rejected | API review, runtime scope checks, and compile-fail tests where practical |
| Third-party feature parity | Independent packages can contribute extraction/packet data, material/queue policy, post-process passes, and editor-only gizmo/overlay work without editing `nara_render_wgpu` or requiring a public second render World | Clean-room external-package tracer and dependency/API audit |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Recipe schema freezes before a real pipeline exists | High | Medium | Freeze ownership and semantics now; defer exact persistent and Rust shapes to the first implementation. |
| `RenderPassPlan` grows into a second partial graph | High | High | Keep it transitional and replace it under the pre-1.0 policy when a graph-only slice arrives. |
| Semantic capabilities merely rename every wgpu feature | Medium | Medium | Add semantic requirements only from concrete family/provider needs; retain exact values in the backend snapshot. |
| Raw passes become the normal plugin path | High | Medium | Keep them out of normal preludes, require declared scope, and expose lost optimizations in diagnostics. |
| Recipe flexibility makes defaults difficult to support | High | Medium | Ship explicit engine-owned portable family bundles and validate family/feature compatibility. |
| Legal recipes amplify into excessive graph work | High | Medium | Enforce decoded feature/pass/resource/edge/variant/diagnostic budgets before realization. |
| Graph compilation adds per-frame cost | Medium | Medium | Separate cached templates from bounded frame instantiation and key them by recipe, provider, schema, shader-interface, semantic-capability, and generation identity. |
| Editor requirements fork the runtime renderer | High | Medium | Require editor targets and overlays to consume the same captured template/backend-realization generations, backend-neutral `FrameExecutionPlan`, and target-transaction semantics. |

## Consequences

- `RenderPassPlan` remains valid for the current renderer but is explicitly transitional.
- The next submitter-extension work should target provider/packet contributions rather than add
  sprite/UI/text/3D parameters directly to `nara_render_wgpu` systems.
- Omitting a public Bevy-style `RenderApp` is justified only after the third-party feature-parity
  tracer proves equivalent extension capability through Nara's provider/packet/graph seams.
- The first real graph implementation must be driven by at least two concrete target/resource
  flows, such as the current SDR surface path plus HDR/offscreen editor composition.
- Project-facing pipeline data can evolve independently from wgpu versions and native backends.
- Pipeline family selection, logical template compilation, device-epoch realization, frame
  instantiation, and execution become separate observable states rather than one backend draw
  function.
- This ADR does not implement a full graph, shader DSL, material file format, high-fidelity family,
  async compute, pass fusion, physical alias allocator, render worker, or second render world.

## Deferred Decisions

- Exact recipe envelope and Rust API, triggered by the first configurable portable pipeline.
- Concrete logical resource and graph node schema, lifetime enum, execution queues, aliasing,
  merging, and barrier algorithms, triggered by the first intermediate texture or cross-target
  dependency that static planning cannot express.
- Color space, transfer function, HDR display, tone-mapping, and alpha contracts, triggered before
  the first HDR output family or persistent color-bearing project format.
- Shared editor/process render-host ownership, triggered by the first offscreen editor viewport that
  must outlive or be shared across isolated Play runtimes.
- Native parallel encoding or retained render world, triggered only by profiling evidence.
- Material-technique and shader-reflection contracts, triggered by reusable material assets and a
  second real family/technique combination.

## Citations

- [ADR 0017: Render Graph Policy](0017-render-graph-policy.md)
- [ADR 0040: Render Resource Lifetime and Submitter Ownership](0040-render-resource-lifetime-and-submitter-ownership.md)
- [Unity URP RenderGraph introduction](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/render-graph-introduction.html)
- [Unity URP unsafe passes](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/render-graph-unsafe-pass.html)
- [Bevy renderer schedule at `f6c6e6eebb94`](https://github.com/bevyengine/bevy/blob/f6c6e6eebb94/crates/bevy_render/src/renderer/mod.rs)
- [Bevy per-view core schedules at `f6c6e6eebb94`](https://github.com/bevyengine/bevy/blob/f6c6e6eebb94/crates/bevy_core_pipeline/src/schedule.rs)
- [Godot compositor effect contract at `c939bf3791`](https://github.com/godotengine/godot/blob/c939bf3791/doc/classes/CompositorEffect.xml)
- [Godot rendering device graph at `c939bf3791`](https://github.com/godotengine/godot/blob/c939bf3791/servers/rendering/rendering_device_graph.h)
