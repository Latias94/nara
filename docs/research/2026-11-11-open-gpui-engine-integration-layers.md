# Open-GPUI Engine Integration Layers

Status: research synthesis and selected development direction; implementation remains partial.
This is not evidence of a complete external-host adapter or Nara integration.
Date: 2026-11-11. Scope: independent framework hosting, rendering, component layers and engine use.

## Summary

For an editor embedded in an existing engine window, event loop and GPU device, GPUI currently
requires more adaptation than Dear ImGui. ImGui explicitly separates platform and renderer
backends, accepts application-owned native/GPU objects, and lets the application drive each frame.
GPUI exposes useful replacement boundaries, but an embedder must satisfy a substantially broader
runtime, window, input, text, presentation and retirement contract. [I1] [I2] [O2] [O3]

GPUI remains a plausible desktop editor host: it can own native UI windows and scheduling while
an engine produces viewport images. That arrangement still needs a real texture-composition seam.
Recording GPUI into a host encoder is another option; translating its Scene into engine-specific
draw commands is a third. These are separate choices, rather than successive required stages.
Open-GPUI is an independent general UI library; Nara is an optional consumer, not its owner.

## Version Information and Evidence Boundaries

- **Official baseline:** Zed `635026fabc38440fce5e1767d125026a17b12012`, inspected with
  `git show` in `F:/SourceCodes/Rust/nara/repo-ref/zed`; checkout HEAD is not the source authority.
- **Committed maintained fork:** Open-GPUI `424a3a0f7c2e71f6ce5a933de00138919c50893d`, branch
  `optimize/docking-graph-transaction`, at `F:/SourceCodes/Rust/open-gpui`.
- **Recording prerequisite:** Open-GPUI `0259c88f9deb26e4caa026ebd39be179d19a3ae5` adds
  `WgpuSceneRenderer`, `WgpuSceneRecording`, core/context changes and `external_scene.rs`.
  This is a qualified generic renderer module, not a complete external-host adapter or a
  selected Nara frontend. Source-adoption documentation is separately committed at `e7debc21222`.
  [W1] [W2]
- **Nara context:** initial HEAD `46ad5dd6d7d77c2e4a45ad36567d19d73f050582`; later owner-document
  reconciliation by the main lane does not turn this research into an adoption decision.
- **ImGui reference:** `d3b36a875f0f7e7446b99606ca3d6df27b7dad7a`, local `repo-ref/imgui`.
- **Supplied report:** `%TEMP%/nara-gpui-research-readable.md`, recovered readable excerpt;
  original-source SHA256 `fcffad093394908c3f6064452c0b748d2ff0afea450a167ce375154af24b036c`.
  Its viewport-first recommendation is supplied material, not verified primary-source evidence.

The six research lanes ran no Cargo or native tests. Main-thread implementation checks have
their own recorded scope below. Neither the source-adoption audit at `424a3a0f7c2` nor the
candidate's existence establishes complete fixed-baseline platform qualification.

## Key Concepts

### App hooks: baseline versus maintained fork

The official pinned baseline already has `Application::with_platform`, `run_embedded`, and
`ApplicationHandle::{update,to_async}`. They are not fork-only capabilities. Its embedded entry
still invokes `Platform::run`; a returning platform must arrange launch and external delivery.
Its handle's `update` directly calls the supplied closure under an App borrow. [Z1]

The maintained fork retains these public entry points, makes handle updates use `App::update`
to flush effects, and documents asynchronous launch, reentry, shutdown and strong/weak ownership.
`run_embedded` does not make a blocking native platform return early or provide a shared renderer.
The checked source has no public type named `EmbeddedAppContext`; use the actual symbols above.
`HeadlessAppContext` is a different, test-support context. [O1] [O6]

The official `PlatformWindow::draw(&Scene)` returns `()`; the fork returns explicit `Submitted`,
`Deferred`, `RepaintRequired`, or `Rejected`. Fork Window state distinguishes accepted and submitted
generations. A recorded command buffer cannot legitimately stand in for either submission or
GPU completion. These distinctions matter to input/semantic exposure and native close. [Z2] [O3] [O7]

### Modes and independent decisions

| Mode | Window / loop owner | UI drawing | Principal missing work | Fit and ImGui comparison |
|---|---|---|---|---|
| A: GPUI desktop host + engine viewport texture | GPUI native platform | GPUI renderer composites engine output | Ordered external-texture import, backend/device interoperability, output retirement, viewport input routing | Plausible new desktop editor shell; reverses the usual engine-hosted ImGui arrangement. |
| B: host-driven GPUI + reusable WGPU Scene recorder | Existing application/engine | Open-GPUI WGPU shaders record into host encoder/target | Production platform/window adapter, accepted Scene delivery, truthful delayed submission acknowledgement, texture import, shutdown | Closer to ImGui embedding, but current App hooks and recording candidate do not supply its ready platform/backend combination. |
| C: GPUI Scene lowered into an engine-specific renderer | Either GPUI or existing host | Consumer implements GPUI primitive and atlas semantics | Complete renderer implementation and conformance, plus B's runtime work if host-driven | Deeper than ImGui's indexed-triangle renderer contract; justified by concrete renderer needs. |

A desktop GPUI host could use a reusable WGPU renderer if its platform supports that choice;
host-driven GPUI could retain a native renderer. Therefore window ownership does not select the
renderer, shared-device recording does not require Scene lowering, and importing a viewport
does not require every toolbar to enter the engine render graph. Editor and game-runtime UI
adoption are also separate product decisions. The matrix describes options, not completed modes.

### What the renderer boundaries actually contain

`Scene` exposes typed quads, paths, shadows, underlines, sprites and surfaces, ordered by
`batches()`, with scaled geometry, clip envelopes and transforms. It is not an ImGui-compatible
vertex/index stream. Lowering must preserve these semantics, including text/atlas sampling,
overlap order, clipping, alpha/color behavior and retained resource identities. [O4] [I3]

`PlatformAtlas` accepts pixel builders and returns atlas tiles; the fork also retains exact
texture-instance leases. `WgpuAtlas::new` accepts host Device/Queue, but that alone neither imports
an engine TextureView nor maps producer output lifetimes to GPUI sampling. [O5] [O8]

The pinned and fork `SurfaceSource` accept Apple pixel buffers, not arbitrary cross-platform
engine textures. The committed WGPU dispatcher skips `PrimitiveBatch::Surfaces`; `canvas` gives
GPUI paint callbacks, not a host WGPU render pass. These APIs do not close mode A. [Z3] [O9] [O10]

## Implementation Guide: Candidate Evaluation, Not Adoption

1. Choose the required window/loop owner from the actual consumer. A new desktop shell can
   evaluate A; a non-negotiable existing engine window/loop needs B or C plus a platform adapter.
   Keep ordinary `Application::run` and native backend consumers intact. [O1] [O2]
2. For A, test engine output as a normal ordered, clipped UI item under menus and overlays.
   On Windows the existing native Window constructs `DirectXRenderer`; linking `gpui_wgpu`
   does not change that choice. Explicit interop or a platform renderer seam is required. [O11]
3. For B, use `WgpuSceneRenderer` as the reusable rendering prerequisite.
   Its host supplies adapter, enabled device, queue, encoder and attachment; recording avoids
   installing device callbacks, owning a surface, submitting, or presenting. Explicit Load/Clear
   and encoder-ordered uploads address composition and multiple scenes per submission. [W1]
4. Retain `WgpuSceneRecording` through the corresponding host submission's completion; it holds
   atlas resources and is explicitly not a presentation receipt. Resolve capability/refusal
   behavior before exposing the candidate as supported. Host device identity remains a contract,
   not something proved by accepting several GPU handles. [W1]
5. Add or qualify production Scene delivery and submission acknowledgement at their real owners.
   `PlatformWindow::draw` can be implemented by a complete custom platform, but merely copying a
   test Scene or returning `Submitted` when recording finishes is not that integration. [O3] [O12]
6. Choose C only if required engine features cannot reasonably use the reusable renderer.
   Implement against pinned Scene/atlas semantics and compare complete representative output;
   do not build a speculative universal renderer interface. [O4] [O5]

The initial candidate selected a default texture view and validated too few enabled-device limits.
Independent review identified multi-mip attachment views and insufficient bind-group/uniform
limits. The working correction selects mip zero explicitly and validates the fixed renderer's
actual device limits before resource creation. Its consumer adds a multi-mip target and
limited-device refusal controls; qualification remains tied to the main-thread results. [W1] [W2]

## Best Practices and Specific Missing Seams

| Seam | Required fact and recommended owner |
|---|---|
| Texture import | Framework renderer owns a generic sampled-image binding: device/backend identity, subresource, extent, format/color/alpha, UVs, sampler policy and lifetime. Nara owns adapting its output; no Nara type belongs in Open-GPUI core. |
| Scene/submission acknowledgement | Runtime/window adapter associates the exact accepted Scene generation with actual host submission, pending/refused outcomes and eventual completion. Synchronous `draw` outcomes do not by themselves define a later external acknowledgement API. |
| Input and native runtime | Host platform provides foreground/background scheduling, wakeups, keyboard/text/IME, focus, capture/cancellation, accessibility, clipboard and close. Recording solves none of these responsibilities. |
| Clip and DPI | Renderer preserves ordered clips/transforms; platform supplies coherent logical/physical geometry and scale. Image placement, hit testing and IME bounds must agree across resize/display migration. |
| Refusal | Reject unsupported target/subresource/device requirements before encoding where possible; preserve a truthful distinction between temporary deferral, stale-resource repaint and terminal rejection. Discard an encoder after a later recording failure. |
| Retirement | Keep exact sampled resources alive through original GPU completion; cancel pending work and quiesce presentation before native-window retirement. Recording leases and native shutdown tickets have different purposes. |

These requirements follow the fork's platform, outcome, lease and shutdown owners. Extend the
smallest owning generic seam while preserving ordinary desktop applications. Nara should own its
adapter, document/ECS commands and consumer qualification; Open-GPUI should own reusable framework
and backend behavior. Do not make ImGui's global callback tables the required GPUI architecture.
ImGui itself recommends reusing mature standard backends before rewriting them. [O2] [O3] [O5] [I1]

The local reference's WGPU backend makes the analogy concrete: `ImGui_ImplWGPU_InitInfo` accepts
the application's Device and attachment formats, while `ImGui_ImplWGPU_RenderDrawData` consumes
the application's render-pass encoder. Its Win32 backend separately accepts an HWND and forwarded
messages. GPUI's path rasterization can require intermediate passes and MSAA resolve, so its
reusable encoder interface may encompass multiple private passes rather than copy ImGui's exact
pass-only function signature. The desired similarity is application-controlled integration. [I4]

## Common Issues and Remaining Unknowns

- **Fixture admission is not production export.** Main reports an initial `external_scene` failure:
  returning `Err` from `SceneSource::render_scene` to avoid simulated submission made window open
  fail at `BeforeVisibilityPresentation`, even with hidden-window intent. Source confirms that
  open prepares initial presentation and TestWindow maps renderer failure to rejection. The
  corrected fixture explicitly simulates admission solely to generate scenes. Its screenshot
  hook copies a Scene and then fails; actual host queue/readback work is separate. [O12] [W2]
- **Headless success cannot acknowledge a future host submission.** `PlatformHeadlessRenderer`
  has `Result<()>`, not a delayed acceptance/submission protocol; it is test/bench gated. A native
  production custom platform is possible, with its complete contract still to implement. [O3] [O6]
- **Candidate coverage is limited.** Recording refuses platform video surfaces and currently uses
  a complete single-sample, single-layer 2D attachment. It does not import an engine viewport
  texture, solve platform text/input, or select GPU error/recovery ownership. [W1] [O9]
- **Qualification remains open.** Require real clipped engine images, first-window admission,
  delayed/refused submissions, continuous resize/DPI, IME/capture, close with in-flight work, and
  unaffected ordinary GPUI consumers. This lane supplies source evidence, not those test results.
- **Decision:** compare A and B with the actual host requirements; reserve C for demonstrated
  rendering constraints. The supplied report supports evaluating A, but does not settle the cost
  of native Windows interop versus a generic host-driven WGPU path.

## Selected Standalone Framework Direction

The owner's subsequent clarification selects convenient external WGPU application hosting as an
Open-GPUI development target. Preserve GPUI's retained state, declarative elements, layout, text,
input and components while separating their execution from concrete native GPU construction.
ImGui informs the platform/renderer division and application-owned frame driving; adopting its
immediate state model, global callback tables or flattened triangle representation is unnecessary.
This direction refines the earlier option comparison; it does not claim mode B is implemented.

| Layer | Responsibility | Consumer independence |
| --- | --- | --- |
| UI execution | Entities, notifications, layout/prepaint/paint, focus, text sessions, semantics and frame demand | Uses supplied platform/text services; imports no engine document, ECS or GPU type. |
| Platform adapter | Native window/events, wakeups, IME/clipboard/accessibility, geometry, close and native custody | An ordinary desktop adapter and an externally driven adapter may satisfy the same complete contract. |
| Optional WGPU renderer | Existing shaders, path passes, clip data, atlas and encoded scene resources | Accepts the application's actual enabled Device/Queue/target/encoder; no Nara dependency. |
| Components and appearance | Behavior/state, virtualized collections, layout, theme/typography, reusable controls and docking | Custom Element/canvas libraries remain ordinary framework consumers. |
| Application integration | Device creation, final submission/presentation, application data, viewports and lifecycle | Nara implements its own integration; another WGPU application can consume the same framework interfaces. |

The official Windows D3D11 construction is an implementation that may be refactored or replaced,
not a permanent public-interface requirement. Separate reusable Win32/input/DirectWrite services
from concrete renderer creation, including any device-dependent text raster work. Qualify a
normal standalone WGPU application before selecting or retiring the corresponding native default.
Shipping many GPU backends is not a prerequisite. A public WGPU renderer need not mirror WGPU
with a new RHI or expose every internal pass as a public graph node.

The complete external application interface must cover events and text, foreground work/wakeups,
frame demand, accepted-scene delivery, actual submission outcomes and cancellation/close.
`run_embedded` alone leaves most of that integration to the supplied Platform. Extend the owning
modules from a real ordinary application; do not export another test platform, mini executor or
parallel Window state machine as the product solution. [O1] [O2] [O3] [O11] [O12]

## Nara Rendering And Product Layers

Nara already has a deterministic typed resource-graph compiler, finite render programs and
`WgpuRenderFeature::{preflight,create_state,prepare,encode}`. Accepted ADR0094's finite-program
refinement and current public consumers qualify bounded Runtime, Editor Scene View and offscreen
composition. The complete ADR0121 proposal remains Proposed/partial. Its original October
comparison and ADR0094's July context do not describe today's entire implementation. [N1] [N2]

For the first engine integration, use the existing Host's final-window transaction as the control.
It already supplies the device and encoder to ImGui and the drawn frontend. A GPUI renderer can
participate in that composition without first representing every editor widget in the engine graph.
If intermediate outputs, shared dependencies or history require graph placement, wrap the renderer
in the existing typed Feature mechanism with explicit cross-module reads/writes and retained uses.
Its internal path/text/atlas passes can remain private. [N3]

| Nara owner | Framework adapter consumes or returns |
| --- | --- |
| Document/Session/history/persistence | Immutable projections, prepared commands and exact original results; no duplicate canvas-document history. |
| Workspace/panels | Panel and view identities; framework entities retain presentation state, drafts and subscriptions. |
| WgpuRenderHost | Existing device scope, predeclared requirements, window transaction, actual submission/presentation and epoch. |
| Immutable WgpuImageOutput | Exact image/version and color/alpha facts held through actual sampling; not a mutable latest-texture slot. |
| View/display occurrence | Output identity, placement, clip, DPI, interaction role and actual successful composite before picking. |
| Producer retirement | Original frozen submitted prefix; one closing view must not drain unrelated producers. |

The existing image panel attachment still binds Winit and explicitly refuses direct rendering,
outside-panel overlays, native detach and accessibility. Low-level window encoding does not
silently grant these panel capabilities. A whole-Editor native composition and a user-authored
panel attachment are separate consumers. [N4]

Separate Editor adoption from the authored game-UI decision. Keep the current `nara_ui` path as a
control and evaluate a GPUI-derived runtime against game time/pause, player/gamepad focus,
world-space/depth, resource loading/retirement, platform scope and special material requirements.
Selective algorithm/semantics/motion reuse does not require sharing all components; neither
does independent product ownership require permanently maintaining two execution engines.

## WGPU Identity And Dependency Policy

Nara currently requires registry WGPU `=30.0.1`; Open-GPUI selects the official Git revision
`cddb2f5d80dc0009b2ef6767635140cacf90a007`, whose package version is `30.0.0`.
The existing independently locked GPUI consumer selects that same Git source. `open-gpui-wgpu`
already is the optional renderer package; another package with the same responsibility is
unnecessary. Shared source identity permits compatible Rust types; sharing the actual original
Device/Queue is an additional runtime requirement. Two dependency entries alone do not prove
two devices were created. [D1]

| Option | Tradeoff and current disposition |
| --- | --- |
| Align the Nara consumer to the maintained fixed Git | Preserves known upstream Vulkan/WebGL fixes; requires deliberate Nara requirements/API/source changes and actual affected-consumer qualification. Preferred bounded compatibility experiment. |
| Move GPUI renderer to a registry release | Viable when the release contains the required fixes; registry 30.0.1 has an already reproduced Vulkan semaphore failure. Compilation alone is insufficient. |
| Application-root source overrides | Useful assembly mechanism, subject to every dependency's version requirement; a 30.0.0 patch cannot satisfy `=30.0.1`. Explicit Git edges also need reconciliation. |
| Implement Scene rendering in Nara | Avoids directly sharing renderer WGPU types, but assumes all primitive/atlas/path/text maintenance. Reserve for demonstrated needs. |
| Native cross-device/backend texture interop | Separate dependency policy is possible, with allocation, fences, format/state and dual retirement costs. Higher current complexity than same-device recording. |

Do not reinterpret WGPU handles across source versions. Record the framework revision, GPU
source/revision/package version, Naga, enabled features/limits and compiler floor for each consumed
combination. Root overrides do not propagate into independently built workspace roots: Nara has
seven relevant fixture workspaces with nine direct `=30.0.1` declarations, plus the reference
ImGui renderer's registry edge. Change affected roots intentionally when adopting a source policy.
The current research changes no Nara manifest or lockfile. [D1] [D2]

## Components, Extensions And Iteration

The supplied GPUI Base description is substantially correct: the observed GPUI Kit main branch
separates unstyled behavior from styled components. Its Base still depends on GPUI, so this is
not automatically a neutral dependency for an independently maintained fork. Dock renderer traits
produce GPUI elements for appearance; they are not GPU-renderer traits. Its current documentation
also records the missing drag-to-separate-OS-window behavior. These are useful design references,
not evidence that Nara should add a second component stack. [C1] [C2]

Open-GPUI already has `ui-core`, `ui-components`, docking, canvas and motion. Reuse their proven
models and appearance/adapter seams, then move a responsibility only when a concrete consumer
exposes inappropriate coupling. Existing `ui-core` is not claimed to be a complete headless Base.
A source-built independent Dialogue panel, extended into a custom Dialogue Graph, is a useful
next consumer for custom Element/canvas, ordinary controls, exact domain edits and cleanup. [C3]

Trusted source-built extensions can use the selected framework directly. Keep document commands,
query/preparation and exact results framework-neutral. An alternate toolkit uses explicit Host
capabilities rather than translating its entire tree into GPUI or `UiNode`. Dynamic installation,
cross-version state reconstruction and a stable Rust ABI are separate contracts; neither GPUI
entities nor captured Rust closures constitute such a protocol.

Visible automation must act on the original presented control incarnation through its controller,
with stale/hidden operations refused. A framework activation being dispatched is not a committed
Nara edit or a later presentation. Hot iteration must separately report data reload, declarative UI
reconstruction, compatible code patching, and structural restart/recovery. The current GPUI
Subsecond consumer has bounded toolchain/native evidence; `embedded_gpui` is explicitly experimental
and GPUI Shell is Preview. Neither is a ready-made Nara plugin/reload product. [C4]

## Editor And Game Reuse Boundary

The preferred target is shared UI mechanisms with separate product adapters and composition.
Game UI should not depend on the Editor package to obtain text, controls or rendering. Conversely,
maintaining a second text/layout/input implementation solely because one consumer is a game adds
cost without demonstrating a necessary boundary. Shared code does not imply shared live state:
each root/surface retains its own focus, text sessions, invalidation and resource lifetime, while
the original application arbitrates native input and GPU work.

Mature engines provide evidence that shared UI execution and separate product APIs can coexist:
Godot's plugin guide builds an Editor dock from a normal `Control` scene, while Unity's UI Toolkit
supports both Editor extensions and game/application runtime UI. Neither example establishes
that every specialized Editor control or game rendering requirement belongs in the shared layer.
These are concrete reuse precedents, not a claim that all engines use one toolkit. [P1] [P2]

| Responsibility | Preferred reuse | Product-specific responsibility |
| --- | --- | --- |
| Text, layout, scene primitives and rendering | Reuse coherent framework mechanisms and their caches; make platform and GPU services replaceable at their owners. | Engine assets/font loading, output placement, game render timing, device/submission and retirement. |
| Stateful interaction and scheduling | Reuse focus, text editing, hit testing, invalidation and semantic execution when a production externally driven surface satisfies the consumer. | Game time versus real time, seat/player mapping, pause, game commands and host wakeups. |
| Unstyled component behavior | Reuse suitable collection, selection, virtualization, scroll and control behavior without requiring an Editor theme. | Game-specific navigation, gameplay interaction and custom component behavior. |
| Appearance and composed controls | Permit shared buttons, fields and themes; keep visual recipes and higher-level libraries optional. | HUD/menu art direction, effects and materials; Editor density, Inspector, docking and workspace recipes. |
| Domain and authoring | Reuse UI extension interfaces without importing domain owners into the framework. | Scene/Prefab projection and gameplay actions versus documents, Undo, persistence and Editor tools. |

GPUI Base is primarily a separation of unstyled component behavior from styled composition; it
still depends on GPUI. This is distinct from separating UI execution from native windows and GPU
construction. Open-GPUI already has a genuinely runtime-independent `ui-core` package containing
focus-policy, virtualizer and geometry models; its `ui-components` package composes GPUI behavior.
That foundation does not yet make complete widgets, text editing or layout usable outside the
framework runtime. Both boundaries are useful and should be described separately. [C1] [C3]

Refactor the known platform/renderer coupling now through the external WGPU application consumer.
Keep unstyled behavior and appearance distinct through existing component modules. Do not first
split text, layout, focus, caching and every control into separate public crates: their shared
invalidations, font provenance, resource leases and update ordering must still have one coherent
owner. Extract a package when an independently consuming application can use it with a smaller
dependency closure and a complete lifecycle contract. Module boundaries can precede crate splits;
fewer crates are not a reason to retain native-device coupling.

For Nara, full reuse of the framework execution is the first candidate to compare, not a commitment
to ship the desktop host in a game. If no-window operation, player focus, time policy, platform
closure or measured costs expose a mismatch, extract the relevant reusable mechanism or retain
independent execution for that requirement. The HUD comparison below decides execution reuse;
Editor adoption alone cannot decide it. A common button or virtualizer also cannot establish
world-space rendering, split-screen input or arbitrary game materials.

## Development Sequence And Qualification

### Game Runtime Mechanism Review

The authored game-UI model and its execution implementation need separate decisions. Nara's
current game UI preserves Scene/Prefab/source identities and typed game commands, but its
`UiNode`/style values still depend on the current kernel. There is no already-qualified drop-in
GPUI executor interface. Preserve the authored semantics and owner contracts while allowing
internal types and dependencies to change when the comparison justifies replacement. [G1]

| Concern | Implemented Nara control | GPUI-derived comparison requirement |
| --- | --- | --- |
| Layout/tree lifetime | ECS projection runs during Extract; each surface compiles through a compilation-local Taffy tree. Text layouts can be reused, but this is not a persistent incremental layout tree. | Measure retained root/key state and localized invalidation; count projection, layout, text and paint work rather than assuming framework maturity implies a cost win. |
| Text and IME | Authored game components provide text display and buttons. Shared widgets have field/preedit machinery, but the game component/input timeline does not yet provide complete editable-text/IME authoring. | Production text services and clear composition/commit ownership; do not count Editor text qualification as a game capability. |
| Input/focus | Directional navigation, activation, modal blocking and cancellation exist. Kernel focus can be seat-scoped; runtime currently binds one seat/pointer to a surface and one surface per window. | Prove player/seat ownership and the exact presented generation; physical gamepad, split-screen and multiplayer focus require their own consumer. |
| Coordinates and depth | Screen-space DPI/safe-area/letterbox mapping exists. The UI pass writes color; no complete world-space/depth/ray interaction contract is established. | A UI image rendered on a world plane can reuse 2D controls, but world hit-to-UV mapping, depth and pixel density belong to the engine adapter. |
| Time and pause | Real/Virtual/Fixed time already differ; paused presentation continues, while the HUD is updated by game systems. The UI has no complete selectable animation-clock mechanism. | Game-time animation and real-time text/IME responsiveness need explicit adapter policy; a process-wide paused desktop clock is insufficient. |
| Appearance and materials | Authored colors, images, borders, radii and button state styling exist; kernel custom triangle geometry exists separately. | Preserve authored styles; prove any required custom material/effect through a named consumer. Postprocessing a whole UI texture is not arbitrary per-control material support. |
| Resources and outputs | Original frame/image/glyph snapshots and exact presentation facts survive in retained packets. Rendering can be omitted for CPU execution. | Maintain the same original content/output/input and retirement obligations in native, offscreen-GPU and no-render execution. |

These are current limits of both the comparison and the control; they do not establish that the
existing stack already solves every game requirement which GPUI would lack. In particular, world
UI, authored editable text and richer player focus remain genuine future engine work. [G1] [G2]

Compare three implementations against the same source and behavior: the current Nara execution
as a control; a coherent GPUI-driven runtime surface; and selective neutral-module reuse if
complete GPUI execution has a demonstrated lifecycle, dependency or cost mismatch. Prefer the
coherent mechanism experiment before copying many isolated internals whose caches and lifetimes
would have to be maintained again. Do not make two complete control/text/layout stacks a
permanent default without measuring their adaptation and maintenance costs.

The first named consumer is `reference-game/prefabs/reference-ui.prefab.json`: dynamic HUD,
terminal modal and Retry. Project supported authored nodes into GPUI elements with retained state
per root/surface and stable identity mappings; a persistent GPUI Entity per ECS entity is not
required. Run the same authored width/action changes and activation trajectory through a
production no-window layout/semantic path, an offscreen GPU target and the existing desktop Host.
Test pause/resume, modal precedence, resize/capture cancellation, refused submission, retained old
resources and close. Neither a TestPlatform nor an offscreen recording alone qualifies that
production UI surface. [G3]

Each running composition has one interaction executor and one semantic action outlet into
existing game commands. A GPUI candidate owns its hit/focus/control execution; the old kernel
remains a separately selected control, not a second live executor to synchronize. Editor edits
continue through the existing document/history owner. Any required source/schema or module
dependency migration is explicit; framework class names do not become authored source identities.
Extend capability to a text-entry/player-focus or world-space consumer only when that workflow
is named, retaining the broader design requirements without claiming them from the HUD.

The generalization is useful if the framework's built-in desktop host and a normal external WGPU
application can share the same elements/components while selecting their platform/render owners.
Measure that result before expanding runtime unification. Reconsider the shared execution scope
if a game adapter must duplicate Window's state machine, import ECS/material/world concepts into
the framework, synchronize two live focus/IME/hit systems, or exceed the matched workload budget.
Those findings would justify a narrower shared module or independent execution, not an automatic
rejection of reusable GPUI mechanisms or an unconditional commitment to two permanent stacks.

### Framework And Engine Sequence

1. Finish the generic external-device recorder with ordinary public GPU tests for two scenes before
   one application submit, Load, clips/path/image, capability refusal and exact resource retention.
   This is the current implementation prerequisite, not full external hosting.
2. Build the production external-host driver and generic texture import through existing owners.
   Establish real scene/submission acknowledgement, events/IME, frame demand and close without
   using `test-support` as the application runtime.
3. Qualify a standalone non-Nara WGPU window with its own loop/device/encoder and ordinary
   components. Decouple concrete Win32 rendering as needed; compare the native default through the
   same consumer before switching it. The renderer module must remain usable independently.
4. Resolve Nara's concrete GPU source combination in a bounded source variant, then run the real
   content/window consumer through its original Host. Sample changing engine images in GPUI painter
   order beneath clipped/overlapping controls and retain exact View input, refusal and retirement.
5. Build the independent GPUI source-editing composition and external Dialogue Graph consumer.
   Preserve complete text/draft/composition, command results, Undo/Redo and Save/Reopen workflows.
6. Measure static Inspector beside a running viewport, large lists/graphs, local edit, theme/font
   and resize changes. Record UI reconstruction/layout/text/raster work, allocations/retained bytes,
   CPU/P99/input latency, GPU copies/submits and build/reload costs before selecting optimizations.
   Leave numerical budgets unqualified until actual values and workloads are fixed.

The initial changed-renderer cohort executes 118 tests: all 117 existing WGPU library controls
pass, while the new public consumer fails at headless initial admission. A later run exposes its
obsolete no-submission assertion. These are fixture failures, not proof of the unexecuted mip
scenario. The host background pass then exposes its own all-mips attachment error. After those
corrections and the five independently reviewed target/limit fixes, the actual two-scene,
one-submit public GPU consumer passes 1/1 (`6ee302a4-2050-4849-b2bf-99307fff4e45`).
It retains exact clip/path/image/Load pixels, four capability-refusal controls, the host callback
and original atlas lifetime. Affected correctness Clippy, production wasm and actual Windows
Rust 1.93 checks, formatting and existing static gates pass. Open-GPUI's verification record
owns the detailed results. The design review finds no blocking ownership/feasibility conflict.
No native Editor, game runtime, independent external-host driver or performance completion follows.

## References

Line numbers below refer to the pinned source or the inspected candidate snapshot. `O` paths are
relative to `F:/SourceCodes/Rust/open-gpui`; `Z` paths to `nara/repo-ref/zed`; `I` to `nara/repo-ref/imgui`.

- [Z1] Zed pin above, [`crates/gpui/src/app.rs:211–242,312–355`](https://github.com/zed-industries/zed/blob/635026fabc38440fce5e1767d125026a17b12012/crates/gpui/src/app.rs#L211-L355): App handle and embedded launch.
- [Z2] Same pin, [`crates/gpui/src/platform.rs:366–374,1246–1332,1797–1803`](https://github.com/zed-industries/zed/blob/635026fabc38440fce5e1767d125026a17b12012/crates/gpui/src/platform.rs#L1246-L1332): runtime, draw and atlas contracts.
- [Z3] Same pin, [`crates/gpui/src/elements/surface.rs:13–35`](https://github.com/zed-industries/zed/blob/635026fabc38440fce5e1767d125026a17b12012/crates/gpui/src/elements/surface.rs#L13-L35); [`canvas.rs:10–25`](https://github.com/zed-industries/zed/blob/635026fabc38440fce5e1767d125026a17b12012/crates/gpui/src/elements/canvas.rs#L10-L25).
- [O1] `crates/gpui/src/app.rs:243–378`: public strong/weak lifetime, effect-flushing update, returning-platform requirement.
- [O2] `crates/gpui/src/platform.rs:3705–3719,5855–5938,6072–6126`: platform services, geometry, IME, events, close and retirement.
- [O3] `crates/gpui/src/platform.rs:6129–6142,6258–6289`: synchronous draw outcomes and gated headless renderer.
- [O4] `crates/gpui/src/scene.rs:108–123,156,642–660,794,1050–1068,1415–1498`: Scene batches, clip data and renderer transforms.
- [O5] `crates/gpui/src/platform.rs:6846–6929`: pixel atlas API and exact texture-instance leases.
- [O6] `tests/fixtures/dependency-consumer/tests/embedded_app.rs:1–37`; `crates/gpui/src/app/headless_app_context.rs:1–65`: public API probe versus headless context.
- [O7] `crates/gpui/src/window.rs:8417–8452`; `docs/adr/0031-open-gpui-renderer-surface-lifecycle-authority.md:16–53` at committed pin: accepted/submitted and shutdown separation.
- [O8] `crates/gpui_wgpu/src/wgpu_atlas.rs:26–84`: host Device/Queue and internal presentation lease.
- [O9] `crates/gpui/src/elements/surface.rs:13–35`; `crates/gpui/src/scene.rs:1480–1486`: Apple-only source.
- [O10] Committed `crates/gpui_wgpu/src/wgpu_renderer.rs:309,1727,1840`: surface-owning renderer, private submitting record path, skipped surfaces.
- [O11] `crates/gpui_windows/src/window.rs:822,1007,8735–8742`; `platform.rs:2490–2497`: native DirectX construction and blocking loop.
- [O12] `crates/gpui/src/app.rs:1895–1916`; `crates/gpui/src/platform/test/window.rs:2472–2501`; `crates/gpui/src/window.rs:3682–3689`: initial admission and simulated headless outcomes.
- [W1] Recording commit `0259c88f9deb26e4caa026ebd39be179d19a3ae5`, `crates/gpui_wgpu/src/wgpu_renderer/scene_renderer.rs:4`, `wgpu_renderer.rs:49`, `gpui_wgpu.rs:20`: recorder, target/limit checks and resource receipt.
- [W2] Same recording commit, `crates/gpui_wgpu/tests/external_scene.rs:1`: explicit simulated scene production and separate actual GPU submission/readback; Open-GPUI `docs/verification.md#application-owned-wgpu-scene-recording` owns executed results and limits.
- [I1] ImGui pin above, [`docs/BACKENDS.md:25–67,129–172`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/docs/BACKENDS.md#L25-L67): platform/renderer split and custom-engine guidance.
- [I2] Same pin, [`examples/example_win32_directx11/main.cpp:74–75,101–109,134–136,176–183,267`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/examples/example_win32_directx11/main.cpp#L74-L183): existing HWND/device, application loop, draw and present.
- [I3] Same pin, [`docs/BACKENDS.md:214–287`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/docs/BACKENDS.md#L214-L287): texture updates, indexed triangles, clip offset and framebuffer scale.
- [I4] Same local ImGui pin, `backends/imgui_impl_wgpu.h:49–79`, `backends/imgui_impl_wgpu.cpp:466–617`, `backends/imgui_impl_win32.h:24–35`: injected Device, host render-pass encoding, external texture views, clipping and forwarded native events.
- [N1] [Finite programs](../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs), [current guide](../guides/render-programs.md), `crates/nara_render_wgpu/src/program/mod.rs:467`, `graph/compile.rs:19`, `program/feature.rs:34`.
- [N2] [ADR0121](../architecture/adr/0121-composable-render-programs-and-host-execution.md), [implementation status](../architecture/adr/implementation-status.md): bounded implemented refinements and full proposal status.
- [N3] `crates/nara_render_wgpu/src/host.rs:969,1229,2214,2384,2901,3071`, `host/imgui.rs:102,172,297`, `output.rs:86`; `editor/src/platform/drawn/graphics.rs:329`.
- [N4] `editor/src/native/images.rs:82,108`, `native/images/presentation.rs:22,203`; `src/project_host/runtime/editor/presentation.rs:159`.
- [D1] `nara/Cargo.toml:139`; `open-gpui/Cargo.toml:243`, `crates/gpui_wgpu/Cargo.toml:26,68`, `tests/fixtures/dependency-consumer/Cargo.toml:43`; sibling Open-GPUI `docs/verification.md#wgpu-dependency-shaders-and-webgl`.
- [D2] [Cargo dependency overrides](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html): source/version selection and root-only patches.
- [C1] [GPUI Base](https://gpui-kit.com/base/), observed November 11 [workspace manifest](https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml), [Base manifest](https://github.com/longbridge/gpui-kit/blob/main/crates/base/Cargo.toml): main-branch evidence, not a latest-release claim.
- [C2] [Dock documentation](https://gpui-kit.com/base/dock/), [DockAreaRenderer](https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/dock/dock_area.rs), [TabGroupRenderer](https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/dock/tab_group.rs).
- [C3] Open-GPUI `crates/ui_core/src/lib.rs:9`, `crates/ui_components/README.md`, `docs/ui/component-contract.md`, `crates/gpui_docking/README.md`, `crates/canvas/README.md`, `crates/motion/README.md`; Nara [independent documents](../guides/editor-documents.md) and `tests/fixtures/module-consumer/editor-panel-consumer/tests/dialogue_panel.rs:85`.
- [C4] Open-GPUI `crates/ui_components/src/activation.rs:54`, `tests/fixtures/hot-reload-consumer/README.md`; Nara [panel operations](../guides/editor-panels.md#observe-and-operate-presented-controls), [iteration contract](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md); [embedded GPUI](https://github.com/zed-industries/embedded_gpui), [GPUI Shell](https://gpui-kit.com/shell/).
- [G1] Nara `crates/nara_ui/src/runtime.rs:218`, `runtime/targets.rs:8`, `codec.rs:19`, `persistent_key.rs:19`, `authoring_model.rs:365`, `interaction/timeline.rs:143,228`; `crates/nara_ui_kernel/src/compiler/layout.rs:25`, `compiler.rs:55`, `interaction/state.rs:96`.
- [G2] Nara `crates/nara_app/src/lib.rs:2102`, `src/ui_presentation/surfaces.rs:108`, `crates/nara_render_wgpu/src/ui.rs:244`, `crates/nara_ui_kernel/src/style.rs:530`, `mesh.rs:38`, `crates/nara_ui_render/src/packet/mod.rs:378`.
- [G3] Nara `reference-game/src/runtime_ui.rs:64,143,311`, `reference-game/prefabs/reference-ui.prefab.json`, `reference-game/tests/ui_authoring_roundtrip.rs:32`, `reference-game/tests/ui_product.rs:252,286`; the existing test presenter explicitly distinguishes its evidence from native presentation.
- [P1] [Godot plugin guide](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html), [Control](https://docs.godotengine.org/en/stable/classes/class_control.html), observed November 11; local `repo-ref/godot/editor/docks/editor_dock.h:40` and `scene/gui/control.h:45` provide additional source context.
- [P2] [Unity 2023.2 UI Toolkit overview](https://docs.unity3d.com/2023.2/Documentation/Manual/UIElements.html), observed November 11: retained-mode UI for Editor extensions and game/application runtime; other Unity UI systems remain appropriate for some use cases.
