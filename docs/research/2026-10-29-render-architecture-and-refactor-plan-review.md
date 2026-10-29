---
title: Rendering Architecture And Refactor Plan Review
date: 2026-10-29
status: Research evidence frozen for parent review
---

# Rendering Architecture And Refactor Plan Review

**Initial audit verdict, before the parent refinement below.** Retain R8's separation of captured
producer data, renderer policy and a runner-owned
GPU Host. Trial typed feature integration over a finite, validated resource-use plan, with a
convenient post-process slot at the product boundary. Do not freeze a public graph, feature
trait family or material ABI from the present prose. The important unresolved contracts are
physical resource versions, typed capture/adapter registration, stream retirement, presentation
semantics and shader binding authority. These determine whether the design actually replaces
the central Sprite/UI path.

The two-pass example justifies an extension seam; it does **not** by itself justify a general
resource graph. A good slot implementation can execute both passes using private scratch
textures. Prefer an ordered plan that checks declared resource use before adding a scheduler.
Keep R9a with R8, R9b behind the selected seam and relevant asset-product contracts, and R9c as
an independent measurement. This agrees with the dependency shape already recorded in
[the rebaseline plan, lines 78–99][plan-dependencies].

**Parent design refinement.** The user's subsequent direction keeps pinned Bevy and Godot as
the primary architecture references, permits a render graph and prioritizes avoiding future
large architectural replacements over minimizing initial code. The parent therefore takes an
explicit finite resource-dependency graph with typed Features and runner-owned execution as the
preferred full-workflow design. Slots lower into that graph. Ordered-plan/slot controls in this
initial audit remain useful architectural falsification, not a requirement that the smallest
initial implementation win. Include branching/shared-image and cross-view consumers and design
identity/dependency/lifetime boundaries for foreseeable resource evolution before interface freeze.
The owning plan and Proposed ADR record this refinement; no implementation is implied. Exact wgpu
documentation below constrains native behavior rather than replacing the engine references.

The follow-up challenges the initial append-only recommendation: declaration order must not
become dependency authority. The preferred graph can execute serially in deterministic topological
order while expressing explicit edges. Design texture and buffer access together, including
sampled/attachment/storage/copy and legal read-write distinctions, with conservative initial range
tracking. Qualify a small compute-statistics/storage-buffer/custom-histogram consumer before core
interface freeze. A reserved future enum case is not evidence that the model extends correctly.

History imports a previous producer/View/Feature-instance/slot version; current graphs remain
acyclic. Define initialization, producer-output publication/advance and resize/camera/policy/epoch
invalidation. Repeated display does not advance history, and Host composition cannot require all
Worlds to capture a fresh frame in lockstep. The existing multi-stream, device-capability, material
identity/ABI and custom-Editor boundaries below remain part of the full design. These amendments
design foreseeable ownership now while leaving allocator/scheduler optimizations for later evidence.
Specifically, the initial ordered-only/forward-reference rejection recommendation below is
superseded by an explicit dependency graph whose declaration order is not authority. The closing
list's open buffer/compute/history items refer to concrete public API and implementation
qualification; resource identity, access vocabulary and history ownership are now design work,
not postponed foundations. Shared shader-input/artifact invalidation is required; WESL import
and missing-resolution cases apply when that frontend is selected, not as a prerequisite to WGSL.

**Scope and evidence.** This is a source/design audit, not an implementation or qualification
result. ADR 0121 is [Proposed][adr0121]; [R8/R9 remain not started][plan-render]. No Cargo command,
test, formatter, branch change or commit was run. The only repository write is this report.
Concurrent Runtime Scene work, existing documentation edits, `vendor/` and reference checkouts
were left untouched. Parent integration may revise plans and Proposed decisions separately;
the source findings and proposed experiments below do not establish production qualification.

| Evidence | Inspected baseline |
| --- | --- |
| Nara | Initial and pre-write HEAD `3c623072fbb293da5b108cc3310af4f33baee6bd`; rendering worktree sources read on 2026-10-29. Concurrent changes prevent treating the whole checkout as an immutable snapshot. |
| Dependency cohort | [Root manifest][manifest]: Rust `1.95`, `bevy_ecs =0.20.0`, `wgpu =30.0.1`, Dear ImGui WGPU `=0.21.0`. This audit did not execute the toolchain. |
| Bevy reference | `85503941e488152711a4695bbe248e9ed9f453fc`, manifest `0.20.0-dev`; inspected checkout had no tracked modifications. This is source precedent, not a stable-release compatibility claim. |
| Godot reference | `81ab961f91066a63bcbf3ec340962bb1a1ada558`; inspected checkout had no tracked modifications. |
| Additional WGPU reference | `efcf445784dd31537f09bc61bad27431de713fce`, manifest `30.0.0`. Version-specific behavior was also checked against official `30.0.1` API documentation; these revisions are not treated as identical. |
| Supplied conversation | `D:/Frankorz/Downloads/Rust游戏引擎架构选型 (2026-10-23).md`, especially lines 257–287, 2627–2726, 3922–4052 and 4391–4421. Its recommendations are hypotheses, not primary evidence of engine behavior. |

Pinned upstream links below identify files read locally under `repo-ref/bevy/`,
`repo-ref/godot/` and `repo-ref/wgpu/`; their relative source paths are preserved in each URL.
Official API/specification pages were checked on 2026-10-29. Dated specification links preserve
the revision consulted; they do not imply support for every feature in those drafts.

The review follows [STRATEGY.md][strategy], [the architecture authority map][architecture-map],
reassessment [D14–D16][reassessment] and the affected Accepted ADRs cited below. The October 27
clarification selects Nara-owned Editor controls, layout, interaction and drawing.
[ADR 0118:68–75][adr0118-ui] and [the existing custom-UI study][custom-ui-gates] already record
that direction. No foreign-toolkit embedding experiment is a prerequisite.

**What the current implementation actually proves.**

| Current fact | Consequence for the review |
| --- | --- |
| [`WgpuFramePayload` names Sprite/UI and `WgpuFrameSet` has separate collection stages][nara-payload]. [Capture][nara-capture] and [target preparation][nara-prepare-target] assemble these domains centrally. | The extension obstacle is real. Public labels alone would leave the payload and encoder closed. |
| [`RenderFramePacket`][nara-frame] captures an ordered list of at most 16 views for one target. Its builder rejects multiple runtime windows and mixed destinations. | One-target capture is a useful boundary. Multi-target Host execution need not become one giant multi-target packet. |
| [`ExtractedView`][nara-view] already has an invertible affine 2D mapping, viewport, order and clear/load semantics. [ADR 0117][adr0117-view] already admits bounded ordered views and fixed-canvas presentation. | R9a should extend these facts, not rediscover camera transforms or describe all existing rendering as single-view. |
| [`WgpuEditorExecution`][nara-editor-host] already owns a view map; [each view has separate source/frame admission][nara-editor-admission]. | Editor-local GPU ownership and several independently identified image views have implementation precedent. They do not prove the requested two-producer/two-window/offscreen contract. |
| The ordinary [runtime backend][nara-backend-owner] still has one source/frame-admission pair and [rejects image packets][nara-runtime-image]. Device creation [requires a window surface][nara-device-init]. | Lifting only the `Device` out of the World would leave important runtime assumptions behind. Window-free GPU execution needs an actual initialization path. |
| [Prepared images retain exact identity and pixels together][nara-image-capture]; [Sprite batches retain prepared image/material snapshots][nara-sprite-capture]; [UI packets retain frames, images and glyph pages][nara-ui-packet]. | Preserve exact old captures. The proposal must not replace them with live asset lookup through an erased registry. |
| [The independent palette consumer][palette-consumer] supplies real custom shader code, parameters and a texture. [The current material API][nara-material] explicitly restricts it to one vec4, one extra texture and transient instances. | It proves a useful narrow extension, not durable materials, shader editing, arbitrary bindings or renderer replacement. |
| [Editor image submission and final window presentation][nara-editor-submit] are distinct; [Scene promotion][nara-scene-presenter] joins content, texture, mapping and composite authority. | A successful resource plan is not a replacement for presented-frame authority. |

**Priority amendments before an interface freeze.**

| Priority | Amendment | Owning work |
| --- | --- | --- |
| P0 | Specify the supported resource-use model and the physical meaning of versions; compare viable slots and schedules on the same consumer. | R8 + R9a |
| P0 | Specify typed capture-to-adapter registration and the exact authority of native encoding; migrate stock features through it. | R8 |
| P0 | Separate stream closure, target finalization, submission completion and whole-Host drain; scope identity and budgets accordingly. | R8, existing Host/close owners |
| P0 | Choose transparent-image alpha semantics, captured clocks and final display mapping before admitting the effect. | R9a + Editor presentation |
| P1 | Separate semantic material Schema from GPU layout; define the supported shader-author workflow and tracked dependency publication. | R9b + relevant R6a inputs |
| P1 | Qualify one renderer-policy replacement against the same Host and presentation contract. | R8 |
| P2 | Admit retained domain geometry only after attribution shows a material win. | R9c |

These are missing specifications or unproven choices, rather than evidence that every existing
mechanism is incorrect. The following recommendations describe changes for parent review.

**1. Use one useful two-pass consumer to compare three honest alternatives.**

Use an independently authored, separable Scene-color blur. It takes captured radius/strength
and time, reads the completed Scene color `S0`, writes horizontal blur `H0`, then writes vertical
blur `B0`. Runtime HUD and Editor controls remain crisp. The same package runs in a runtime
window, Scene View and a window-free GPU image target. A zero-radius mode provides an identity
control. An outline implementation would additionally need a defined selection/mask producer;
silently adding that producer would make the comparison more than the advertised two passes.

For the composition pressure, let an Editor comparison view retain `S0` alongside `B0` and let
a second independently registered image effect consume `B0`. This is a before/after preview
workflow, not an arbitrary graphics-feature catalogue. It exposes ownership of an earlier
version and cross-package composition without changing the blur algorithm.

| Alternative | Same two-pass implementation | What it establishes and what it costs |
| --- | --- | --- |
| Named post-process slot | A typed `BeforeRuntimeUi` contribution receives Scene-color facts, owns two encode steps and scratch allocation through the Host, and returns its completed output. Ordered contributions can chain outputs. | Small author surface. It can solve the initial consumer completely, including private ping-pong. It needs explicit retention/import rules for the before/after branch and explicit ordering between packages. A slot is not inherently limited to one pass. |
| Finite resource-use plan | The package declares `S0 -> H0 -> B0` with image contracts and typed captured inputs. The Host validates a finite ordered list, realizes declared resources, then invokes the native encoders. Product slots can construct this list. | Shared validation and transient lifetime can replace repeated package-specific bookkeeping. Cost: a resource vocabulary, capture/adapter erasure and diagnostics. Resource handles alone do not establish initialization or physical non-aliasing. |
| Open render schedule | Two systems encode the same passes using prepared per-view resources. The package registers extraction/preparation and orders both systems before UI; the Host still owns final submission/presentation. | Broad Rust control and existing ECS scheduling machinery. Authors must maintain GPU resource sequencing, readiness and lifetime themselves unless another owner supplies them. A render World or dedicated thread is optional, not intrinsic to this alternative. |

Bevy at the inspected revision really uses `RenderGraph` as a **Schedule**, with
Begin/Render/Submit/Finish sets, not the historical Node graph
([local `renderer/mod.rs:32–76`][bevy-schedule]). Its custom post-process registers extraction,
uniform preparation, bind-group preparation and a Core3d system
([`custom_post_processing.rs:38–71`][bevy-post-registration]). The example deliberately checks
pipeline readiness **before** flipping the target; [`ViewTarget::post_process_write`][bevy-pingpong]
then gives the caller an obligation to fill the destination. This demonstrates a viable open
schedule with explicit author responsibilities. ECS read/write conflicts do not by themselves
describe the image versions used inside bind groups.

Godot's [`CompositorEffect` slots and resolved-resource flags][godot-compositor] coexist with
[ordered callback invocation][godot-compositor-run] and a separate low-level
[RenderingDevice resource tracker][godot-rd-usage]. The tracker builds dependencies from actual
resource usage and subresources ([`rendering_device_graph.cpp:423–452,650–715`][godot-rd-deps]).
Nara should borrow the distinction between product effect placement and resource obligations.
It should not copy Godot's native barrier machinery above wgpu.

**Recommendation:** trial the finite **ordered** plan as the common implementation, with a
small named-slot authoring helper. Reads can reference earlier outputs and writes create fresh
versions; forward references can be rejected. This avoids needing a general dependency
scheduler merely to execute the example. If slots plus explicit leases remain simpler on the
composition case, selecting them is a valid outcome. No source review establishes a performance
winner. Compare registration steps, manual lifetime code, required engine edits and measured
costs; do not compare a complete graph implementation to intentionally crippled slots.

**2. Resource versions need physical semantics, not another RHI.**

[ADR 0121:19–23][adr0121] names initialized inputs, conflicts and cycles. It does not yet define
what a version means when a native adapter records commands. A fresh logical ID is not proof
of fresh storage or preserved contents. For example, `B0` cannot overwrite `S0` while a later
before/after consumer still expects the original pixels.

Admit a small initial model:

- Plan-local opaque resource IDs carry a plan identity; foreign or retired IDs reject. Each
  output has one declared producer and a bounded descriptor. Registration order cannot silently
  replace a producer.
- Shared image obligations include extent, sample count, color/depth role, required usages,
  transfer/alpha meaning and supported subresource scope. Start with full 2D mip-0/layer-0 images.
  Unsupported mips, layers and storage hazards reject rather than receiving nominal support.
- New attachments initialize by clear, admitted import, or a specifically trusted complete-write
  operation. A small viewport does not initialize the whole target; a draw declaration cannot
  prove shader coverage. The current [`ViewLoadOp` explicitly clears the entire attachment][nara-view].
- `Load` is a read of existing contents plus a write. Sampling and rendering to the same
  physical subresource in one pass is invalid even if the two logical handles have different
  names. A version still needed by another consumer must retain distinct storage.
- Initially allocate distinct transient outputs, with narrowly controlled attachment accumulation.
  Do not introduce automatic aliasing, pass fusion or a general liveness allocator. An ordered
  builder can make cycles unconstructible; hostile internal plans should still reject.
- Imported targets and final consumers remain Host-owned. A pass declares its use of an acquired
  view; it cannot acquire, reconfigure, publish or present the target itself.

The logical plan need only describe resources exchanged between operations. Sprite vertex
buffers, UI glyph uploads and material uniforms can remain private to their exact-wgpu adapters
when those adapters own their complete preparation, use and completion lifetime. A shared mutable
buffer crossing adapters would require an explicit resource obligation; hiding it behind two
features would invalidate the plan's claims. Do not advertise universal compute/indirect-buffer
composition from a color/depth-only model.

Use exact wgpu descriptors in the exact-wgpu integration. Keep only the semantic and shared
lifetime facts needed by the neutral boundary. “No mirrored RHI” does not mean “no resource
descriptors”; it means avoiding a second Device/Queue/command/format universe.
[ADR 0012:88–95][adr0012] already supports that distinction.

Depth attachment load/store, comparison convention and projection/upload layout need a bounded
depth tracer before becoming supported APIs. D16's 3D sample is useful convention evidence;
it should not gate the first 2D effect on a general 3D renderer or turn meter/Reverse-Z into
already selected requirements.

**3. Make the capture/adapter split usable in Rust and honest about trust.**

“Typed Features” leaves several different things unnamed: a code registration, a feature
instance in a view, an immutable frame value and device-local mutable state. Conflating them
would move the existing central match into a generic registry without fixing ownership.

| Owner | Input/output responsibility |
| --- | --- |
| Domain producer | Captures geometry, settings, exact resource generations and the selected clock values at one defined cut. Output owns its data and retained CPU resources. It does not encode or look up current World state later. |
| Renderer policy / Render Program | Chooses the domain composition, view stages, effect placement and required semantic outputs. It selects active feature instances and constructs the finite plan. It does not create a second GPU owner. |
| Exact-wgpu feature adapter | Prepares pipelines, bindings and private resources from that captured type and current target/capability facts; records commands into Host-provided scope. Its state belongs to one Host/epoch and code registration. |
| GPU Host | Admits work and limits, imports/allocates targets, owns submission order, completion retention, finalization and device transitions. |
| Presentation owner | Joins completed image use with the actual destination presentation and display mapping; commits the existing domain-specific UI/picking authority. |

For the first prototype, use generic registration to pair a captured Rust type with its native
adapter. Erase that pair **inside** the Host once, at feature/operation granularity. Avoid a
public `Any` bag that requires every package to downcast, an enum containing every domain,
or a trait object per sprite/glyph. Associated capture/state types can stay behind the typed
registration; consumers should not solve object-safety or lifetime plumbing themselves.
The experiment must reveal the actual trait/object shape before ADR 0121 promises it.

Keep installation short: an effect author supplies owned settings/capture, its two resource
operations, pipeline preparation and encoding; a consumer installs the paired contribution and
selects it on a view. The engine supplies collection/sealing, epoch checking, limits and terminal
outcomes. Ordinary gameplay should not orchestrate Reset/Collect/Seal/Submit or manufacture a
presentation receipt. First-party Sprite and Runtime UI must use exactly this registration and
lifecycle path. They may keep different packet formats and private shared quad code.

Owned data and `Arc` are not proofs against interior mutation or a callback consulting global
state. Likewise, a scoped `&wgpu::Queue` still has `submit(&self)` and can be cloned; scoped
`Device` access can also escape through a clone and exposes `destroy(&self)`
([official Queue][wgpu-queue], [official Device][wgpu-device]; pinned local
[`queue.rs:280`][wgpu-queue-source], [`device.rs:21`][wgpu-device-source] and
[`device.rs:836`][wgpu-device-destroy-source]).
The current [Editor scope][nara-native-scope] already exposes these capabilities to trusted
first-party integration.

Therefore withhold Queue and target-finalization entry points from ordinary feature encoding
where practical; let the Host execute owned uploads/recorded copies and submit completed work.
Do not build a full Queue wrapper to imitate wgpu. Document raw native access as a trusted-code
protocol. “Reject unauthorized target actions” means ordinary API authority and Host-token
validation, not a sandbox against arbitrary native Rust. Do not add source-analysis scripts
that pretend to prove the behavior of every extension.

Host-local adapter state need not implement `Send + Sync`. Add `Send` or other bounds only at
an actual transport/parallel-preparation boundary. Captured data must not retain an acquired
surface, mutable World borrow or native encoder. If code-provided captures, trait objects,
destructors or callbacks can outlive registration, retain their code owner until they retire.
Start with source-built trusted Rust; this does not qualify dynamic native unloading or a
language-neutral render ABI.

**4. A shared Host needs stream-local identity, budgets and completion.**

The existing [Editor view admission][nara-editor-admission] is a better starting point than the
runtime backend's single `source_generation/last_frame_index` pair. Preserve the distinctions:

| Identity/lifetime | Required meaning |
| --- | --- |
| Producer/stream incarnation | Reject work from a replaced producer; closing this stream does not close the GPU Host. |
| View identity | Stable within the producer's declared view lifetime; independent of vector position, frame number and native window. Recreated views get a new incarnation unless continuity is explicitly preserved. |
| Capture sequence and policy revision | Identify exact captured data and the adapter/policy that interprets it. A policy replacement cannot reinterpret queued old captures. |
| Target/configuration generation | Fences resize, format/sample/alpha changes and target reallocation. A document edit alone need not recreate its texture. |
| Host identity + device epoch | Fences every GPU realization and device-dependent asynchronous result, without rewriting device-independent content identity. |
| Submission/presentation identity | Separates encoded/accepted GPU work, published images, individual destination presentation and later physical completion. |

Do not collapse these into a global frame counter or make each caller reconstruct the entire
tuple. Existing [image receipts][nara-editor-submit] and [Scene presentation tokens][nara-scene-presenter]
already join several axes; deepen those owners instead of introducing parallel “render success”
receipts.

Declare a finite pending/in-flight allowance per stream plus a Host-wide reservation owner.
Latest-frame replacement is reasonable for some visual captures, but replacing a capture that
owns a move-only UI submission must produce its exact terminal outcome.
[ADR 0112:126–149][adr0112-ui] already requires this. A missing/minimized window must not
silently retain an unbounded queue or block another image/window stream.
Keep one cumulative upload/instance admission scope over a Host drive, with per-stream
attribution and a fixed simple admission order. Per-frame demand and physical retained bytes
are different budgets; [ADR 0096:52–69][adr0096-budget] already makes this distinction.
No global fairness scheduler is needed for the first experiment.

On stream close: stop its admission, terminalize unsubmitted UI work, revoke its input routes,
and retain its submitted resources until their completion boundary. Continue servicing other
streams. The current [GPU retirement token waits for queue-empty][nara-gpu-drain], which is
appropriate for **whole-Host** shutdown. Requiring queue-empty to close one stream can prevent
that stream from completing while other streams keep submitting. Use retained submission-prefix
completion for the departing stream; current [texture retention callbacks][nara-texture-retain]
already illustrate a smaller completion boundary. Device loss alone is still not completion.

A Host can execute several one-target transactions in a declared order. If window A presents and
window B fails, A remains presented. An image publication can succeed while its later window
consumer fails; neither event authorizes a new UI presented session for B. Candidate publication
can be atomic before submission, but external GPU execution and multiple native presents cannot
be rolled back as one transaction.

**5. Close capabilities before device creation, and qualify window-free initialization.**

The direction in [ADR 0121:36–55][adr0121] is correct but needs a concrete late-install policy.
[`ensure_device`][nara-device-init] currently selects against one window surface and requests a
default descriptor. A new external renderer cannot assume its requirements were enabled merely
because the Adapter supports them.

Before initialization, the Host closes requirements from the selected renderer, installed
features and target set. Record required features/limits, selected optional paths, rejected
requirements, the exact request and actual enabled facts. Official
[wgpu 30.0.1 DeviceDescriptor documentation][wgpu-device-descriptor] states that resulting
validation uses the requested features/limits, not all Adapter capabilities. Limits require
field-appropriate comparison; do not merge every numeric field with `max`. The inspected
[Bevy device planning][bevy-device-plan] distinguishes max/min constraints too, but its
functionality-priority policy is Bevy's choice, not a Nara requirement.

For late installation or a new stream, accept only requirements satisfied by the current device
and target formats. Otherwise return a typed capability/restart requirement, leaving existing
streams running. Reconstructing the Host is an explicit owner operation; installing one effect
must not silently reset the Editor. Removing a feature does not need to shrink the live device.
Recovery reuses the closed request or reaches a truthful Unavailable state under bounded policy.
[ADR 0078:124–160][adr0078-init] already owns much of this contract.

Check each admitted surface against the selected Adapter; compatibility with the first window
is not authority over an arbitrary later window. An offscreen GPU profile must initialize from
an Adapter without requiring a hidden dummy window. Content-only Headless continues to omit
rendering under [ADR 0106:100–124][adr0106-products].

Realization results bind the Host/epoch, relevant capability/target facts and the original
shader/material/policy revision. Reject late results after any relevant supersession.
“Complete candidate” means the pipelines/resources required by that selected bounded candidate,
not every theoretical material variant. Finish validation scopes before publishing those
realizations; keep active resources untouched during failed candidate preparation.

Keep same-epoch compatible last-good as a complete interpretation of its original inputs.
Allocation pressure is retryable policy, shader/layout rejection is content failure, and device
loss invalidates the entire namespace. Current [pipeline preparation][nara-pipeline-errors]
and [pipeline-cache admission][nara-pipeline-cache] already distinguish these cases.
Uploaded immutable captures can rebuild; temporal/generated
contents reset unless explicitly rerendered from captures; imported external resources reacquire.
Do not relabel old physical objects as new-device last-good.

**6. R9a must define transparent composition, clocks and spatial interpretation now.**

The current [material fragment contract][nara-material] returns linear RGB with straight alpha,
and [the quad pipeline uses straight-alpha source-over][nara-blend].
[ADR 0092:80–109][adr0092-composition] explicitly warns that an accumulated transparent render
target is not automatically a reusable straight-alpha texture. Thus the gap is already
recognized; this audit does not establish a new bug in the currently qualified opaque
Scene/Editor path.

For the first reusable transparent image, select **linear RGB with premultiplied accumulated
color** at the compositing boundary. Keep authored tint/color values semantically unassociated;
declare source-image alpha representation and shader output representation explicitly.
This choice does not require a universal premultiplied asset format or an HDR policy.
Premultiply in linear space. A filtered effect must process premultiplied RGB and alpha
consistently; its final source-over uses color factors One and OneMinusSrcAlpha. Straight
source shaders can still blend correctly into the declared accumulated representation.

A small algebraic oracle makes the failure visible. Straight red with alpha 0.5 over transparent
black stores accumulated `(0.5, 0, 0, 0.5)`. Reusing that stored RGB as straight multiplies alpha
again: over opaque white the red channel becomes 0.75. Correct premultiplied composition gives
linear `(1, 0.5, 0.5, 1)`. This follows from the dated
[W3C source-over equations][compositing-spec]. Measure/read back the selected numeric format
with stated quantization tolerance. An all-opaque screenshot cannot test this contract.
Any straight export/consumer conversion needs an explicit alpha-zero rule.

Introduce distinct linear/sRGB value entry points in the breaking design, but keep transfer,
primaries, alpha representation and numeric texture format as different facts. A non-sRGB
texture format alone does not tell a consumer whether its values are encoded output or linear
working data. Do not premultiply gamma-encoded bytes or apply an extra sRGB decode to an already
linear Scene texture. The same facts must enter target compatibility and pipeline keys.

Capture the chosen clocks with the frame. [`RealTime` and `VirtualTime`][nara-clocks] already
exist; [`RenderTime`][nara-render-clock] currently holds interpolation/remainder rather than a
complete shader time. The [captured WGPU frame][nara-payload] includes an `Instant` for work measurement,
not an admitted shader clock. Define which real/virtual/fixed-derived values each producer
captures, including pause, exact-step and interpolation semantics. Replaying one capture at a
later Host wall time must not advance its shader animation. An Editor cursor/effect can use an
Editor real clock while a paused Play stream uses unchanged virtual time.

A shared View envelope should carry producer-scoped view identity, target/viewport facts,
captured clock values and color/alpha contract, with a typed 2D mapping payload. Retain
`ViewMapping2d` as 2D. Preserve [`DisplayMapping2d`][nara-display-mapping] for physical/logical
DPI, letterboxing and input inversion; a raw Mat4 is no replacement for these facts.

Finally, image effects can change spatial meaning. The first blur tracer is coordinate-preserving.
A later distortion/crop effect must supply an admitted display/input mapping or explicitly
disable precise geometric picking. An exact color-image receipt cannot automatically prove the
old camera inverse is still valid. Do not attempt to infer an inverse from arbitrary shader code.

**7. R9b needs one material declaration with separate semantic and GPU projections.**

The direction in [D15][reassessment] is stronger than enlarging `SPRITE_FRAGMENT_V1`, but “one
Schema” must not conflate persistent field meaning with shader memory layout.
[ADR 0119:23–39][adr0119] already distinguishes semantic Schema from domain binding descriptors.

| Contract | Authority and publication |
| --- | --- |
| Material data | Stable material/type/field identity, semantic values, defaults, reference traversal and save/inspect behavior. Durable image/shader references follow existing asset-product reference contracts; process-local `Handle` or `TypeId` is not persistent identity. |
| Binding declaration | Generated from the same selected typed material declaration: binding numbers, uniform encoding, texture/sampler types, stages, optional bindings and supported specialization vocabulary. This is a domain-owned projection of Schema, not the entire persistence Schema. |
| Shader artifact/interface | Tracked source/resolver closure, frontend/version/options, compiled WGSL and the used entry-point interface. Check compatibility with the binding declaration; use the selected compiler/validator rather than hand-parsing WGSL or Rust. |
| Pipeline realization | Shader artifact/interface, entry points, binding ABI, bounded specialization, vertex layout, primitive/depth/blend/sample and target requirements, inside the Host/epoch. Uniform values, animation time and ordinary texture content changes do not create pipeline variants. |

For example, a semantic vector or matrix does not imply its native Rust memory can be uploaded
unchanged. The dated [WGSL layout specification][wgsl-spec] gives `vec3<f32>` alignment 16/size 12
and `mat3x3<f32>` size 48. Uniform layout, padding and strides need an actual shader-layout
encoder and validation. Avoid a material-specific ABI compiler implemented by repository scripts.

The pinned [Bevy Material2d example][bevy-material] combines typed material data,
`AsBindGroup` and `ShaderType`, with shader declarations and specialization. It supports the
value of a typed author path; it does not provide Nara's durable Schema or publication proof.
Godot instead exposes shader-derived uniform properties and named Variant parameters
([`material.cpp:250–254,390–452`][godot-material]). That is a different legitimate authoring
choice, not evidence of weaker performance.

There is a product decision to state before freezing R9b: can a designer add/change an arbitrary
shader binding layout in a prebuilt generic Editor, or only edit shader code/values against
admitted layouts? A Rust derive alone proves the latter. For the initial slice, qualify two
declared layouts and unchanged-layout shader editing; reject unsupported interface changes
without destroying the active material. Broader shader-authored layouts need one admitted
interface/schema route, not a separately maintained string property bag. Do not claim this
broader workflow is implemented by typed Rust materials.

Convert the palette into durable Save/Reopen/Check/Export data, then add a genuinely different
UV-transform/dissolve material: mixed scalar/vector/matrix parameters and two auxiliary
texture/sampler bindings for a noise mask and edge ramp.
Use existing asset-reference encoding, as illustrated by
[the Sprite codec's `AssetRef` conversion][nara-sprite-codec]; do not serialize runtime handles.
State whether values are shared material data or per-instance overrides, and avoid forcing each
Sprite to duplicate a shared immutable material.

WGSL can be the first frontend; WESL is a useful tracked module frontend, not the resource
identity model. If WESL is qualified, use its resolver/compiler dependency results, including
failed/missing resolution dependencies and relevant options. Rebuild reverse dependency edges
from each admitted closure so removed imports do not cause permanent over-invalidation.
The inspected [Bevy shader cache][bevy-shader-deps] records dependencies and invalidates dependent
pipelines. That supports the mechanism, not a promise that Nara can copy its cache or last-good
behavior unchanged.

Bind candidate results to source/interface revisions and publish a complete compatible material
generation. A shared dependency edit invalidates the affected shader/pipeline closure; unrelated
materials remain usable. A rejected shader/layout keeps only a compatible same-epoch last-good
generation and records the failed candidate separately. Repairing source must retry under the
new artifact identity. The current [static material definition equality][nara-material] is
content-based through Rust's derived `Eq`, not pointer-address identity; the observed limitation
is startup-only text and a fixed ABI, not a demonstrated string-pointer bug.

R9b needs immutable shader inputs, typed Runtime decoding, stable references and dependency
capture from relevant R6a work. It does not need the full recipe index/CAS, DLC or remote cache.
The parent is still delivering processed Scene and a later base catalog; this report assumes
neither is finished and does not change their owners or current admission.

**8. Integrate the custom Editor as a real independent drawing consumer; defer retention.**

Nara-owned Editor drawing should consume the same Host/feature execution lifecycle, but it need
not consume the runtime game's entire `UiFrame` kernel. [ADR 0107][adr0107] explicitly permits
selective sharing and independent product state. The current [paint packet][nara-ui-packet]
contains Rect/Glyph primitives tied to a runtime `UiFrame`. Treating that as a universal Editor
paint language would make future timelines, node canvases and custom controls depend on
unproven runtime-UI vocabulary.

For the rendering experiment, add only a Nara-owned drawing tracer: a clipped custom canvas,
one live Scene image and an overlapping control/popup layer. Its own interaction/layout owner
produces the immutable paint data. Test real image import, clip order, resize and DPI, and preserve
the existing content/composite/picking token chain. The renderer does not take document history,
field drafts, focus or input-method ownership. A manual paint tracer qualifies that drawing
handoff only; full Editor controls, IME, accessibility and virtualization remain R11 work.

An unchanged Inspector should be able to retain layout/text/paint data while a Scene view
continues drawing. GPU compositing cadence need not force recompilation of every panel.
Do not pre-emptively cache every panel into a texture. These are already explicit pressures in
[the custom-UI qualification study][custom-ui-gates] and the supplied conversation.

Renderer replacement must be **policy** replacement: an independently authored simple renderer
produces the same required color/presentation outputs and supports the same blur/UI consumer
under the existing Host. It cannot call Stock2d internally and count as replacement, nor create
its own surface/queue authority. Unsupported domain outputs should fail at policy selection.
Whole-Host replacement and another RHI remain separate decisions.

Also preserve process placement. [ADR 0114:46–62][adr0114-placement] selects supervised project
processes for ordinary Play/Run; the in-process Game View is a trusted adapter. Two local producer
streams do not prove that arbitrary project Rust runs in the generic Editor or that GPU objects
can cross a process boundary. R8 should qualify local sharing without inventing a remote-image
transport or undoing ordinary Play isolation.

For R9c, first measure the current [complete extraction and conservative queue culling][nara-culling].
Current Sprite instances are already projected to clip space; retaining those across camera
changes without a view key would be incorrect. If retention wins, keep immutable **domain-owned**
geometry before view-dependent projection, or include every view-dependent input in its key.
Content/settings/material/image/spatial revisions must establish validity; dirty hints alone do
not. Old packets retain their old blocks and resource generations. Keep invalid-invisible-content
diagnostics and painter order. No global retained render scene or resync protocol follows.

**A bounded prototype and qualification sequence.**

The following is proposed work, not work executed by this audit. Use one independently compiled
consumer with shared blur shaders/settings across the comparison drivers. Keep disposable
alternatives outside supported public APIs. Set numeric latency/memory regression budgets at
experiment entry from the selected machine and baseline, as
[the plan already requires, lines 53–60][plan-experiments]. This report invents no benchmark results.

| Stage | Concrete result | Failure that must stop admission |
| --- | --- | --- |
| A. One output chain | Slot, ordered-plan and open-schedule drivers execute the same `S0 -> H0 -> B0` consumer with existing Sprite content and crisp Runtime UI. | A driver depends on core package-specific fields, skips UI completion, or compares a different image algorithm/workload. |
| B. Composition pressure | Retain original `S0` for before/after display; independently register a second effect consuming `B0`. Compare the required author/owner choreography. | Earlier pixels are overwritten, package order changes meaning silently, or the public path needs engine-private stages. |
| C. Shared Host | Two explicit local producer streams feed two real windows and one image target; retire one while the other continues. Offscreen-only boot uses no native window. | One stream's close destroys the device, waits for unrelated continuous work to stop, leaks its permits, or blocks healthy targets. |
| D. Epoch and policy replacement | Replace renderer policy under the same Host; inject device loss/reconstruction and late realization results. | Old adapters reinterpret new captures, old-epoch objects/results become current, or loss alone is reported as physical completion. |
| E. Editor handoff | Scene image inside Nara-owned clipped drawing with an overlapping popup; repeat resize/DPI/presentation failures. | Image-only success mints picking/UI authority, a stale mapping admits input, or the tracer creates another GPU Host. |
| F. R9b follow-through | Durable palette plus second binding layout, a shared shader dependency, failed edit and repair through Check/Export and the selected Runtime decoder. | Source fallback is hidden, a runtime handle is serialized, unrelated pipelines invalidate, or a bad candidate destroys compatible last-good. |

Correctness cases should be small and adversarial:

- Reject uninitialized loads, foreign-plan handles, duplicate producers, unsupported descriptor
  combinations and read/write aliasing of one physical subresource. Reject cycles/forward uses
  at the chosen builder boundary; do not expose a cycle-creation API solely to test it.
- Resize or replace a target after capture; vary transfer/alpha/sample requirements. A packet
  cannot silently inherit the latest target. Use two overlapping views with explicit clear/load,
  and a partial viewport to expose whole-attachment initialization assumptions.
- Hold capture A while publishing new image/material/geometry B. Render A from its retained facts,
  or reject it for a stated admission reason; never mix A geometry with B resources.
- Fail the second pass's pipeline/allocation preparation. Preserve a complete compatible prior
  candidate before submission; do not publish a half-prepared output or charge failed allocations.
- Present window A, then fail/occlude B. Observe per-window outcomes. A's receipt remains truthful;
  B receives no new composite/input authority. Image-only output is not physical GPU completion.
- Close a stream with queued UI work, an in-flight image and a pending realization. Each permit
  terminates once; resources/code remain retained as needed; late results cannot attach to a
  replacement stream with the same user-visible name.
- Request a feature supported by the Adapter but not enabled on the live Device. Refuse/restart
  explicitly without resetting unrelated streams. Also reject a second incompatible surface.
- Replay a captured animated material with the Host clock advanced. Pixels remain tied to captured
  time. Pause Play while Editor animation continues; exact-step advances only its selected clock.
- Read back the transparent red oracle, a small fixed blur kernel and sRGB/linear conversion
  controls. Include alpha-zero texels and non-opaque accumulated alpha; declare expected tolerance.
  Record actual copies and submissions, without steady-state CPU readback in normal presentation.

Measure the costs that distinguish the designs:

| Measurement | Interpretation |
| --- | --- |
| Author concepts, registrations, manual phase edges, core edits and duplicated lifetime code | Whether the public seam is actually simpler for an independent effect, renderer and UI adapter. Count production integration as well as the demo's few lines. |
| Capture/preparation, plan construction/validation, encoding and Host occupancy P50/P95/P99 | Whether the extra model adds bounded CPU work; separate cold startup, resize, cache-hit frames and edited frames. |
| Cold/warm shader and pipeline readiness, edit-to-ready, failures and variant counts | Whether R9b improves iteration without unbounded specialization or hiding failed candidates. |
| Allocations/reallocations, uploads, transient/retained bytes and in-flight high-water marks | Physical cost of distinct versions and retained old captures; report per stream and Host aggregate. |
| GPU pass timing where timestamp queries are enabled, plus submission/copy count | GPU work and synchronization cost. Timestamp support is an optional measured path, not a new minimum device feature. |
| Capture/input-to-final-present latency, dropped/deferred frames and idle panel compile/layout counts | Whether local sharing and live viewports improve the actual Editor/runtime workflow. Distinguish submission from presentation and GPU completion. |

Use identical scenes, effect settings, target sizes, adapter, build profile and validation mode
for comparisons. Include small and larger static/edited workloads, one and multiple views, and
the stated two-stream target mix. Existing Host occupancy, frame-completion and resource telemetry
can be extended narrowly; no universal benchmark framework is required.

**Phased refactor and deletion plan for parent integration.**

| Phase | Dependencies and implementation focus | Displaced design to remove after matching qualification |
| --- | --- | --- |
| 0. Amend the contract | Resolve the P0 items and record which portions of Accepted decisions the experiment challenges. Keep ADR 0121 Proposed during comparison. | Any implication that immutable capture requires permanently closed topology, or that two passes already prove a graph winner. |
| 1. Qualify the seam | R8 + minimum R9a; implement the same consumer and select one execution shape. Material source can initially be captured immutable WGSL. | Losing prototype drivers; do not keep all alternatives as public compatibility paths. |
| 2. Move ownership once | Lift actual device/cache/target state to runner/Editor Host, migrate runtime/direct-App integration, use stream-local admission and completion. Reuse the present target leases, exact captures and retirement owners. | The replaced per-World GPU owner and its independent submit/shutdown route. Keep one coherent production Host path, not an old backend plus a new service forwarding to it. |
| 3. Dogfood and integrate | Move Sprite, Runtime UI and the bounded Editor drawing tracer through the selected registration; qualify multi-stream/window/image and actual renderer-policy replacement. | Central `WgpuFramePayload` domain fields, CollectSprite/CollectUi assembly and domain dispatch in target preparation/encoding. Keep proven caches/batching internally where still useful. Generalize the fixed canvas/Scene-image operations through the seam instead of retaining competing execution routes. |
| 4. Material artifacts | Selected seam + relevant immutable R6a shader/reference/decoder contracts; qualify durable palette, second layout and tracked invalidation. No full CAS dependency. | Fixed `SPRITE_FRAGMENT_V1`/static-text material path, transient-only registration and manual duplicate binding metadata once their real consumers migrate. Retain useful sampler/alpha semantics in the owning module. |
| 5. Measure locality | R9c after cost attribution, independently of basic R8/R9b completion. | Losing retained-cache experiments; obsolete dirty-hint authority if a complete revisioned projection replaces it. No universal retained scene. |

Do not mechanically delete every render compile-fail fixture. Existing
[`render-topology-fail.rs`][topology-negative] checks historical symbol absence,
[`render-frame-plan-fail.rs`][frame-plan-negative] forbids a packet `pass_plan()` method, and
[`render-frame-phases-fail.rs`][frame-phases-negative] rejects an old builder signature.
Those old APIs need not return. Replace assertions whose rationale is “all external topology
must be impossible” with positive independent-feature consumers and negative authority/capture
tests. Keep ordinary preludes small and half-built extraction/queueing state private.

**Narrow ADR reconciliation, not a new taxonomy.**

| Owning decision | Recommended disposition |
| --- | --- |
| 0017 / 0094 / 0105 | Partially replace static/closed-topology prohibitions with the qualified feature/plan contract. Preserve owned captures, deterministic painter order, target ownership and epoch rejection. In 0094, revise the blanket two-frontend material-admission requirement if one concrete tracked frontend and two layouts prove the selected workflow. |
| 0012 / 0032 | Clarify that admitted exact-wgpu companion adapters may own feature realization under Host custody. Preserve domain/backend direction and wgpu as the only RHI; no universal backend trait follows. |
| 0040 / 0054 / 0078 / 0096 | Refine realization custody, cumulative Host/stream budgets, late capability admission, target-local failures and stream-versus-Host retirement. Preserve physical completion and native/browser affinity rules. 0096's measured retention policy remains appropriate. |
| 0106 | Replace first-party-only contribution language for the admitted render seam; retain compiled ceiling versus requested/installed closure and content-only Headless. Add an explicit GPU-offscreen composition rather than silently changing Headless. |
| 0092 | Define reusable transparent-output representation and conversions; introduce explicit linear/sRGB entry points. Retain coherent SDR arithmetic. Its unreleased compatibility language must not prevent a deliberate replacement, and a compositing decision is not universal HDR/asset policy. |
| 0117 | Retain ordered 2D views, shared canvas/display mapping and visibility diagnostics. Amend the closed canvas execution and startup-only fixed material clauses only as their replacements qualify. |
| 0111 / 0112 | Generalize the private fixed edge and source/target cardinality where R8 now has evidence. Preserve exact content/image/sample/window joins, move-only UI terminal outcomes and input authority. Read through current 0117/0118 refinements; do not reimpose obsolete single-view restrictions. |
| 0102 | Preserve immutable Runtime UI capture, domain semantics and receipt-based interaction. Clarify that native execution through the shared seam does not require the custom Editor to adopt the runtime's Rect/Glyph/`UiFrame` vocabulary. |
| 0107 / 0118 | Keep independent UI products and Nara-owned Editor destination. Link the qualified rendering handoff and ownership if needed; do not make all Editor UI implementation part of R8 or require a foreign toolkit. |
| 0114 / 0119 / relevant asset ADRs | Reuse process-isolation, semantic Schema and resource-product authority. No broad replacement is justified by this rendering audit. Any shader/material publication refinement belongs with its actual owner. |

Keep these ideas open in ADR 0121 or dedicated owner design until evidence selects them:
the public registration/erasure shape; slots versus an exposed resource-use builder; broader
buffer/compute/subresource composition; shader-authored arbitrary layouts; native module
hot-unload; portable render transport; history/retained geometry; and 3D/HDR conventions.
Mandatory render threads, a second RHI, automatic aliasing, pass fusion, multi-queue scheduling,
a universal GPU compiler and ADR 0077's full family/recipe taxonomy remain unnecessary for this
workflow.

The immediate parent action is to resolve P0 contract details and run the bounded R8/R9a
comparison after the current production work reaches its own checkpoint. This report supplies
research evidence only; it neither amends an ADR nor declares a rendering milestone complete.

[strategy]: ../../STRATEGY.md#L13-L24
[architecture-map]: ../architecture/README.md#L8-L20
[manifest]: ../../Cargo.toml#L59-L137
[plan-dependencies]: ../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L78-L99
[plan-experiments]: ../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L49-L68
[plan-render]: ../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L529-L567
[reassessment]: ../architecture/foundation-architecture-reassessment.md#L708-L810
[adr0121]: ../architecture/adr/0121-composable-render-programs-and-host-execution.md#L17-L80
[adr0012]: ../architecture/adr/0012-render-crate-boundaries.md#L86-L95
[adr0078-init]: ../architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md#L124-L160
[adr0092-composition]: ../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md#L80-L109
[adr0096-budget]: ../architecture/adr/0096-evidence-gated-render-scaling-and-upload-policy.md#L50-L73
[adr0106-products]: ../architecture/adr/0106-content-and-presentation-product-composition.md#L100-L124
[adr0107]: ../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L29-L82
[adr0112-ui]: ../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md#L126-L149
[adr0114-placement]: ../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md#L46-L62
[adr0117-view]: ../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md#L220-L260
[adr0118-ui]: ../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L68-L75
[adr0119]: ../architecture/adr/0119-schema-first-semantic-metadata-authority.md#L23-L39
[custom-ui-gates]: 2026-10-27-custom-drawn-editor-ui-boundaries.md#L131-L161
[nara-payload]: ../../crates/nara_render_wgpu/src/frame.rs#L35-L73
[nara-capture]: ../../crates/nara_render_wgpu/src/lib.rs#L295-L360
[nara-prepare-target]: ../../crates/nara_render_wgpu/src/backend.rs#L1603-L1679
[nara-frame]: ../../crates/nara_render/src/frame.rs#L9-L259
[nara-view]: ../../crates/nara_render/src/view.rs#L30-L128
[nara-editor-host]: ../../crates/nara_render_wgpu/src/editor_execution.rs#L776-L859
[nara-editor-admission]: ../../crates/nara_render_wgpu/src/editor_execution.rs#L719-L756
[nara-backend-owner]: ../../crates/nara_render_wgpu/src/backend.rs#L485-L521
[nara-runtime-image]: ../../crates/nara_render_wgpu/src/backend.rs#L946-L1003
[nara-device-init]: ../../crates/nara_render_wgpu/src/backend.rs#L1295-L1348
[nara-image-capture]: ../../crates/nara_image/src/prepare.rs#L112-L175
[nara-sprite-capture]: ../../crates/nara_sprite_render/src/types.rs#L177-L199
[nara-ui-packet]: ../../crates/nara_ui_render/src/packet/mod.rs#L334-L398
[nara-material]: ../../crates/nara_material/src/lib.rs#L72-L179
[palette-consumer]: ../../tests/fixtures/module-consumer/material-consumer/src/lib.rs#L11-L35
[nara-editor-submit]: ../../crates/nara_render_wgpu/src/editor_execution.rs#L1109-L1279
[nara-scene-presenter]: ../../editor/src/platform/imgui/scene.rs#L159-L236
[nara-native-scope]: ../../crates/nara_render_wgpu/src/editor_execution.rs#L466-L521
[nara-gpu-drain]: ../../crates/nara_render_wgpu/src/editor_execution/imgui/retirement.rs#L12-L91
[nara-texture-retain]: ../../crates/nara_render_wgpu/src/texture.rs#L724-L748
[nara-pipeline-errors]: ../../crates/nara_render_wgpu/src/quad.rs#L234-L290
[nara-pipeline-cache]: ../../crates/nara_render_wgpu/src/backend.rs#L1880-L1923
[nara-blend]: ../../crates/nara_render_wgpu/src/quad.rs#L509-L522
[nara-clocks]: ../../crates/nara_app/src/lib.rs#L332-L413
[nara-render-clock]: ../../crates/nara_app/src/lib.rs#L650-L672
[nara-display-mapping]: ../../crates/nara_render/src/presentation.rs#L62-L181
[nara-sprite-codec]: ../../crates/nara_sprite/src/lib.rs#L333-L354
[nara-culling]: ../../crates/nara_sprite_render/src/queue.rs#L14-L143
[topology-negative]: ../../tests/fixtures/public-prelude/src/bin/render-topology-fail.rs#L1-L7
[frame-plan-negative]: ../../tests/fixtures/public-prelude/src/bin/render-frame-plan-fail.rs#L1-L5
[frame-phases-negative]: ../../tests/fixtures/public-prelude/src/bin/render-frame-phases-fail.rs#L1-L11
[bevy-schedule]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_render/src/renderer/mod.rs#L32-L76
[bevy-post-registration]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/shader_advanced/custom_post_processing.rs#L38-L71
[bevy-pingpong]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_render/src/view/mod.rs#L1028-L1053
[bevy-device-plan]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_render/src/renderer/mod.rs#L319-L364
[bevy-material]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_sprite_render/src/mesh2d/material.rs#L74-L183
[bevy-shader-deps]: https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_shader/src/shader_cache.rs#L322-L430
[godot-compositor]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/compositor.h#L39-L95
[godot-compositor-run]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/servers/rendering/renderer_rd/renderer_scene_render_rd.cpp#L298-L317
[godot-rd-usage]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/servers/rendering/rendering_device_graph.h#L155-L213
[godot-rd-deps]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/servers/rendering/rendering_device_graph.cpp#L423-L715
[godot-material]: https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/material.cpp#L250-L452
[wgpu-device-descriptor]: https://docs.rs/wgpu/30.0.1/wgpu/type.DeviceDescriptor.html#structfield.required_features
[wgpu-device]: https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html#method.destroy
[wgpu-queue]: https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html#method.submit
[wgpu-device-source]: https://github.com/gfx-rs/wgpu/blob/efcf445784dd31537f09bc61bad27431de713fce/wgpu/src/api/device.rs#L21-L25
[wgpu-device-destroy-source]: https://github.com/gfx-rs/wgpu/blob/efcf445784dd31537f09bc61bad27431de713fce/wgpu/src/api/device.rs#L836-L838
[wgpu-queue-source]: https://github.com/gfx-rs/wgpu/blob/efcf445784dd31537f09bc61bad27431de713fce/wgpu/src/api/queue.rs#L280-L290
[wgsl-spec]: https://www.w3.org/TR/2026/CRD-WGSL-20261026/#alignment-and-size
[compositing-spec]: https://www.w3.org/TR/2024/CRD-compositing-1-20240321/#simplealphacompositing
