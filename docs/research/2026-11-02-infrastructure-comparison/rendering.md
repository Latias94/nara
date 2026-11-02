# Rendering Infrastructure: Nara, Bevy, Fyrox, And Godot

Date: 2026-11-02. Method: offline source comparison; no builds, execution, benchmarks, or new
platform qualification. The question is which responsibilities an engine removes from a real
effect author, and which meanings must survive new renderers, delayed consumers, and device loss.
Engine breadth and interface depth are different criteria; Nara's Strategy targets an integrated
Rust-first 2D production workflow, not immediate feature parity with mature 3D engines.

| Repository | Inspected HEAD |
| --- | --- |
| Nara | `a2640a10973b6d1724176a506237bc458aa5f3bc` |
| Bevy | `ae6152152d6fb8308ad6038314b48ef295e80fbc` |
| Fyrox | `b5b151b603bba0549ac03db676568726f17e6807` |
| Godot | `95c0fa5fe8a019adc29d074c16b64469b04571f7` |

Authority is [Accepted ADR0094][adr94], including its later refinements, read alongside
[Proposed ADR0121][adr121], the [October 31 plan][plan], and [implementation ledger][ledger].
The ledger's newer 0121 row and actual consumers supersede historical closed-renderer descriptions:
typed programs/policies, shared stock composition, immutable outputs, selected history transactions,
and independent streams have landed. Full 0121, durable materials, broader View/time semantics,
and the editable Nara-drawn Inspector remain open. **Open capability is not a bug.** Existing
qualification records are evidence with their original scope, not tests rerun by this audit.

## Mechanism Matrix

| Concern | Nara | Bevy at this pin | Fyrox at this pin | Godot at this pin |
| --- | --- | --- | --- | --- |
| Frame dependency scheduling | Candidate-local versions have one writer; read/preservation edges drive deterministic serial ordering. Operation keys break ready ties. The graph validates resource meaning, not CPU task parallelism. [graph/compile.rs:20][n-graph] | `RenderGraph` is now an ECS schedule label, with Begin/Render/Submit/Finish sets. Per-view schedules compose systems; this is not the historical node/slot graph. Ordering and resource use remain distinct author obligations. [renderer/mod.rs:47][b-schedule] | Renderer orchestration calls registered HDR/LDR passes in vector order. A pass receives scene, observer, framebuffer, caches, and graphics server. Convenient integration, but branching dependencies require explicit owner choreography. [renderer/mod.rs:483][f-pass] | Product compositor callbacks sit above a physical command/resource graph. The latter derives dependencies and barriers from usages and overlapping slices; it does not provide Nara's semantic immutable versions. [rendering_device_graph.cpp:596][g-graph] |
| Logical versus physical resources | Each unbound local version gets distinct storage, including preservation destinations and unexported resources. Exact exports retain selected storage. No aliasing or dead-operation elimination is implied. [program/native.rs:478][n-native] | `ViewTarget` swaps two main textures; descriptor-keyed `TextureCache` recycles frame-local allocations. Retaining a handle is not retaining a historical pixel version. [view/mod.rs:1043][b-view], [texture_cache.rs:35][b-textures] | Per-scene render data owns G-buffer, HDR, and temporary LDR targets. Shader/texture caches and backend objects hide allocation details; scene render targets enter the texture cache for later consumers. [renderer/mod.rs:1346][f-target] | Named context/name textures belong to `RenderSceneBuffersRD`; internal/target sizes and view count drive configuration. Repeated names retrieve existing allocations, rather than immutable output generations. [render_scene_buffers_rd.cpp:328][g-buffers] |
| View, color, and time | Non-reused View identity, 2D mapping, world-raster/resolved/final semantic ports, transfer and alpha are explicit. The neutral packet has a frame index, not a shared captured shader-clock envelope. [frame.rs:89][n-frame], [stock_policy.rs:28][n-stock] | View uniforms and target formats cover renderer needs; extracted `Time` and frame count populate common GPU globals. That is useful shared vocabulary, not an independent-stream replay protocol. [globals.rs:32][b-time] | The custom-pass context includes observer/viewport and engine elapsed time, explicitly not necessarily wall time. HDR-to-LDR conversion has a concrete location between callbacks. [renderer/mod.rs:1185][f-color] | View buffers distinguish internal resolution, target resolution, and views; scene data receives renderer time/time-step. These are renderer-frame facts, not proof of retained independent capture clocks. [renderer_scene_render_rd.cpp:1447][g-time] |
| Materials and compilation | Stock material definitions are static trusted fragments; instances provide one vector and extra texture. Values are separate from the pipeline key, but durable declarations and editable shader generations remain open. [nara_material/src/lib.rs:72][n-material], [quad.rs:119][n-pipeline] | Material bindings and specialization keys separate values from pipeline variants; `PipelineCache` exposes queued/creating/ready/error states. Effects reuse established preparation infrastructure. [pipeline_specializer.rs:63][b-specialize], [pipeline_cache.rs:48][b-pipeline] | Shared shader resources describe bindings and named passes; materials store values. `RenderPassContainer` realizes those passes, and shader caches retain them. Both OpenGL and WGPU backends exist. [cache/shader.rs:213][f-shader] | Uniform and texture dirty flags queue different material updates; shader versions compile variant groups and use persistent caches. Authoring semantics and physical programs are separate layers. [material_storage.cpp:2445][g-material], [shader_rd.cpp:722][g-shader] |
| Submission and physical completion | Submission admission publishes outputs/history; original queue-prefix custody retains allocations through actual completion. Presentation and UI settlement remain separate. [host.rs:2013][n-submit], [submission.rs:354][n-prefix] | Render scheduling submits command buffers, then presents matching window targets; wgpu handles native resource lifetimes. This path does not expose Nara's output/history publication transaction. [renderer/mod.rs:82][b-submit] | WGPU `flush` submits, `finish` waits, and `swap_buffers` submits/presents. These are distinct operations, with resource/server custody rather than Nara's producer-prefix contract. [server.rs:666][f-submit] | RID disposal is deferred; frame reuse waits before pending resources are freed. Local-device submit/sync is distinct from main-device authority. [rendering_device.cpp:8131][g-submit], [rendering_device.cpp:8289][g-retire] |
| Windows, views, and streams | Producer, View, target, output, and display occurrence are different identities. Retained outputs permit differently paced streams and producer-local close; this does not imply parallel GPU queues. [output.rs:86][n-output], [streams_window.rs:14][n-streams] | Camera schedules and window-target matching support multiple views/windows within the render app. Independent retained producer lifetimes need additional application ownership. [schedule.rs:133][b-camera] | Multiple scene/camera outputs and render-to-texture support editor previews. The editor forwards a scene render target into an image widget; this is not an exact-version lease protocol. [scene_viewer/mod.rs:1035][f-editor] | Viewports order children before parents and select updates by visibility/once/always policies, then route outputs to windows. This is mature display orchestration, not arbitrary capture-version dependency solving. [renderer_viewport.cpp:98][g-viewports] |
| Recovery | Closed device requirements and epochs prevent silent capability changes. Retained CPU input reconstructs work; old GPU history becomes explicitly empty after recreation. [device.rs:151][n-device], [history.rs:61][n-history] | Recovery can reacquire resources and rerun `RenderStartup`; error policy chooses recovery, stopping, or ignoring, while the default exits. Do not claim automatic universal recovery. [error_handler.rs:19][b-recovery] | Suspend/resume destroys and reconstructs the graphics context/renderer. That proves lifecycle rebuilding, not delayed exact-capture replay after arbitrary device loss. [engine/mod.rs:1508][f-recovery] | Deferred deletion is not recovery. This pinned Vulkan queue-submit path diagnoses device loss and crashes; it does not reconstruct renderer state. This statement is backend/path-specific. [rendering_device_driver_vulkan.cpp:3331][g-loss] |

## Follow One New Effect Through Real Consumers

Use the independent separable blur, followed by independently packaged grading, as the concrete
extension. It must preserve original Scene color, keep HUD pixels crisp, and run through Runtime,
Scene View, and offscreen ownership.

1. The [collector:35][n-collector] selects a policy installation, takes shared stock capture in
   `Compose`, and transfers the original UI sidecar. Replayable drawing data and one-shot
   presentation responsibility already differ here.
2. The [policy:133][n-composition] retains stock stages plus world/Scene blur and grade declarations,
   aggregates requirements, resolves instance bindings, and builds one program. It selects effect
   placement before or after Canvas resolution, retains untouched versions, and calls stock
   `finish` for full-resolution UI. The [recipe:344][n-recipe] selects matching Runtime and Editor
   adapters; no central engine payload or encoder match is added.
3. [Blur preparation:298][n-blur] validates typed ports, reserves bytes, caches pipelines by output
   format, creates a private half-float intermediate and radius uniform, then encodes two passes.
   The intermediate stays private because no other Feature consumes it. Shared inputs/outputs
   still declare sampled/attachment accesses and initialization in the composing graph.
4. The Host performs complete preflight, realizes storage, encodes, submits, validates native
   outcomes, and publishes the group. Original prefix custody outlives candidate disposal.
   [Existing consumer evidence][n-stock-evidence] covers the actual three destinations and retained
   outputs; it does not establish unrestricted effects, every platform, or performance superiority.

Bevy's [custom_post_processing.rs:39][b-effect] packages extraction, uniform preparation, pipeline
initialization, and a per-view postprocess system. A two-pass blur adds scratch and ordering;
preserving an original beyond ping-pong reuse needs separate storage. Fyrox inserts an HDR/LDR
`SceneRenderPass`, reuses caches, and owns its scratch/pass sequence. Godot registers a compositor
callback with requested resolved buffers; [callback dispatch:298][g-effect] supplies render data,
and the effect manages its RD resources. Neither callback simplicity nor ECS freedom automatically
provides Nara's retained-output and history guarantees. Conversely, Nara's stronger guarantees do
not eliminate shader, binding, and native-resource authoring work.

## Five Architectural Judgments

**1. Keep the version graph; keep physical optimization private.** Its value is preserving meaning
across branches, retained previews, history, and independently advancing streams. That is a real
consumer requirement. However, [graph/compile.rs:20][n-graph] orders all admitted operations, and
[program/native.rs:478][n-native] allocates every unbound version. Exporting unnecessary intermediates
therefore has a concrete storage/budget consequence, even without timing measurements. This is an
explicit conservative mechanism, not a defect. Borrow Bevy's private descriptor pooling and Godot's
internal resource tracking only behind completion and last-reader proofs; never weaken immutable
leases to copy a frame-reset reuse rule. Godot's barrier graph belongs below its own RHI; wgpu
already owns that role for Nara. Copy the separation of responsibilities, not the barrier machinery.

**2. The layers mostly earn their existence, but the public choreography can become shallow.**
ECS capture freezes domain facts; policy selects composition; typed programs describe native uses;
the private graph validates/orders them; the Host owns execution. These are different duties, not
five competing schedulers. The consumer cost is visible in [composition.rs:133][n-composition]:
four declarations and bindings for two semantic spaces, requirement unions, manual operation keys,
and explicit access lists alongside captures. [collector.rs:59][n-collector-compose] adds UI refusal
handling. These are extension ergonomics risks, not demonstrated correctness failures. Extend the
existing stock builder with narrowly reusable composition conveniences; do not add a catalog,
second graph, or lifecycle manager. Keep advanced explicit construction available.

**3. Material evolution is the largest remaining authoring discontinuity.**
[nara_material/src/lib.rs:94][n-material-definition] deliberately admits static source; instances
are transient and excluded from Scene codecs. [quad.rs:119][n-pipeline] already avoids putting
parameter values in pipeline keys, while [quad.rs:360][n-material-upload] prepares per-batch material
bindings. The risk is future parallel identity systems for persistent material fields, shader
interfaces, and GPU bindings. Adopt one versioned declaration with stable semantic fields and
explicit binding/interface generations, as the [existing R9 plan][r9] proposes. Copy Bevy's specialization
and compilation-state separation and Godot's uniform-versus-texture dirty distinction; reuse
compiler reflection instead of implementing shader analysis in scripts. Fyrox demonstrates the
benefit of shared shader resources plus material values. Do not infer poor performance from current
allocation frequency.

**4. Finish clock and coordinate meaning before expanding temporal effects.**
[frame.rs:89][n-frame] carries source/frame/target/Views, and [view.rs:133][n-view] carries 2D mapping;
neither defines a shared shader time domain. Typed color ports already distinguish world raster,
resolved Scene, and composite, so radius units can be explicit. A later animated effect could still
choose Host wall time inconsistently. Freeze captured clock domain/sample/delta, pause/Step/replay
behavior, and camera-cut/reset meaning now. Require coordinate-changing effects to supply an
admitted display mapping or disable precise picking. Missing shared time and broader projection
support are open capabilities, not evidence that existing blur/grade is incorrect.

**5. Recovery and self-drawn Editor integration justify stronger ownership, not another renderer.**
[output.rs:86][n-output] correctly separates pixel retention from reconstruction and interaction;
[submission.rs:354][n-prefix] separates queue completion from publication. The cost is retaining
appropriate CPU reconstruction inputs separately and declaring temporal reset after loss. Exact
temporal checkpoint replay remains open. The Editor should consume exact image leases through the
same Host, preserving color, clipping, DPI/display mapping, designated interactive completion,
and passive mirrors. Existing clipped drawing is not an editable Inspector. UI toolkit work is
outside this audit. The previously confirmed same-window picking revocation defect is already
documented in [follow-up audit F3][known-f3]; it is a Host presentation repair, not a reason to
redesign resource graphs or repeat that investigation.

## Alternatives And Sequence

| Option | Extension/maintenance tradeoff | Decision |
| --- | --- | --- |
| Keep explicit policy/program construction everywhere | Maximum control and one correctness model; ordinary effects repeatedly assemble declarations, stock wiring, and capture transport. | Preserve as the advanced route, not the only ergonomic level. |
| Deepen shared stock composition with bounded effect helpers | Reuses existing graph, Host, and typed contracts; hides recurring wiring while retaining explicit pixel space, outputs, and failure semantics. Adds a small maintained convenience surface. | Preferred next interface improvement, driven by actual blur/grade consumers. |
| Replace composition with fully open render ECS/schedules or callback-only slots | Familiar scheduling or short callbacks; independent version retention, atomic history, and cleanup move back into authors or require another ownership subsystem. | No demonstrated reason to replace the landed owner. Use reference mechanisms selectively. |

**Freeze semantics now:** original capture/definition pairing; View and Feature incarnations;
single-writer versions and initialized/stored reads; complete publication groups; predecessor-checked
history; separate submission/presentation/completion; exact imports and epoch refusal. Complete the
clock, coordinate, material-generation, and failed-compilation/last-good contracts before their
consumers proliferate. Freezing meaning does not freeze today's type names or helper count.

**Delay mechanisms:** aliasing, pass fusion, dead-work pruning, multi-queue execution, render workers,
generalized upload rings, persistent graph templates, and dynamic native unload. Trigger them with
a named consumer or measured constraint. Keep native Feature code trusted, with real wgpu
descriptors; neither a second RHI nor an imitation compiler follows from typed ports. Layer count
and source allocations alone establish no faster/slower ranking.

## Acceptance For The Next Bounded Unit

1. **Consumer depth:** migrate the independent blur/grade composition to any convenience layer;
   retain Runtime, actual Scene View, and offscreen paths, untouched originals, crisp HUD, both
   pixel spaces, and exact typed imports. Require zero engine-specific match additions and list
   the author obligations removed. Preserve wrong-port and original-UI-binding rejection.
2. **Material/time generations:** retain A, publish compatible B, reject incompatible C, then
   realize delayed A without replacing B. Parameter-only and texture-content-only edits must not
   add pipeline variants. Replay A after a Host-time delay, paused Play, and exact Step using its
   captured clock. Compilation failure preserves same-epoch last-good data; loss requires explicit
   reconstruction/reset, not an old GPU handle.
3. **Editor/stream ownership:** interleave two windows and an image producer, preserve old outputs
   through resize/close, and keep another producer progressing. Compose the exact designated
   interactive occurrence plus passive mirror; verify clipping/DPI/input joins and F3's same-window
   replacement invalidation. Await original submitted uses before physical retirement. These are
   proposed correctness gates, not execution results of this audit or full Editor qualification.

[adr94]: ../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md
[adr121]: ../../architecture/adr/0121-composable-render-programs-and-host-execution.md
[plan]: ../../plans/2026-10-31-render-composition-and-host-execution-plan.md
[ledger]: ../../architecture/adr/implementation-status.md#L1422
[r9]: ../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r9-material-artifacts-views-color-and-domain-render-locality
[known-f3]: ../2026-11-02-refactor-follow-up-review.md#f3--p2-window-composition-can-leave-the-replaced-runtime-frame-pickable
[n-graph]: ../../../crates/nara_render_wgpu/src/graph/compile.rs#L20
[n-native]: ../../../crates/nara_render_wgpu/src/program/native.rs#L478
[n-frame]: ../../../crates/nara_render/src/frame.rs#L89
[n-view]: ../../../crates/nara_render/src/view.rs#L133
[n-stock]: ../../../crates/nara_render_wgpu/src/stock_policy.rs#L28
[n-material]: ../../../crates/nara_material/src/lib.rs#L72
[n-material-definition]: ../../../crates/nara_material/src/lib.rs#L94
[n-material-upload]: ../../../crates/nara_render_wgpu/src/quad.rs#L360
[n-pipeline]: ../../../crates/nara_render_wgpu/src/quad.rs#L119
[n-submit]: ../../../crates/nara_render_wgpu/src/host.rs#L2013
[n-prefix]: ../../../crates/nara_render_wgpu/src/submission.rs#L354
[n-output]: ../../../crates/nara_render_wgpu/src/output.rs#L86
[n-streams]: ../../../tests/fixtures/render-composition-consumer/tests/streams_window.rs#L14
[n-device]: ../../../crates/nara_render_wgpu/src/device.rs#L151
[n-history]: ../../../crates/nara_render_wgpu/src/history.rs#L61
[n-collector]: ../../../tests/fixtures/stock-composition-consumer/src/collector.rs#L35
[n-collector-compose]: ../../../tests/fixtures/stock-composition-consumer/src/collector.rs#L59
[n-composition]: ../../../tests/fixtures/stock-composition-consumer/src/composition.rs#L133
[n-recipe]: ../../../tests/fixtures/stock-composition-consumer/src/composition.rs#L344
[n-blur]: ../../../tests/fixtures/stock-composition-consumer/effects/blur/src/lib.rs#L298
[n-stock-evidence]: ../../../tests/fixtures/stock-composition-consumer/README.md
[b-schedule]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/renderer/mod.rs#L47
[b-submit]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/renderer/mod.rs#L82
[b-view]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/view/mod.rs#L1043
[b-textures]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/texture/texture_cache.rs#L35
[b-time]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/globals.rs#L32
[b-specialize]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/render_resource/pipeline_specializer.rs#L63
[b-pipeline]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/render_resource/pipeline_cache.rs#L48
[b-camera]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_core_pipeline/src/schedule.rs#L133
[b-recovery]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/error_handler.rs#L19
[b-effect]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/examples/shader_advanced/custom_post_processing.rs#L39
[f-pass]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/renderer/mod.rs#L483
[f-target]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/renderer/mod.rs#L1346
[f-color]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/renderer/mod.rs#L1185
[f-shader]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/renderer/cache/shader.rs#L213
[f-submit]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-graphics-wgpu/src/server.rs#L666
[f-editor]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/editor/src/scene_viewer/mod.rs#L1035
[f-recovery]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/engine/mod.rs#L1508
[g-graph]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/rendering_device_graph.cpp#L596
[g-buffers]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_rd/storage_rd/render_scene_buffers_rd.cpp#L328
[g-time]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_rd/renderer_scene_render_rd.cpp#L1447
[g-material]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_rd/storage_rd/material_storage.cpp#L2445
[g-shader]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_rd/shader_rd.cpp#L722
[g-submit]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/rendering_device.cpp#L8131
[g-retire]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/rendering_device.cpp#L8289
[g-viewports]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_viewport.cpp#L98
[g-loss]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/drivers/vulkan/rendering_device_driver_vulkan.cpp#L3331
[g-effect]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/rendering/renderer_rd/renderer_scene_render_rd.cpp#L298
