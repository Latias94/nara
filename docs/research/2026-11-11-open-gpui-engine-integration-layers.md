# Open-GPUI Engine Integration Layers

Status: research synthesis and selected development direction; implementation remains partial.
The implemented checkpoints qualify scoped Windows hosting, deferred submission, hosted logical
roots and the first Nara-owned Host/Scene consumer. Complete engine/frontend and game-specific
acceptance remain open.
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

The initial comparison uses the fixed official baseline. Later fork-owned work has implemented
[external Windows hosting](#implemented-synchronous-external-win32-host) and
[deferred Scene submission](#implemented-deferred-scene-submission). Those checkpoints qualify
their named consumers; they do not establish a standalone game UI runtime. The
[Editor/game reuse boundary](#editor-and-game-reuse-boundary) distinguishes shared execution,
unstyled control behavior, appearance and product adapters.

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
- **Image/native-renderer successor:** Open-GPUI `a5d7dea101ca643a9fd462f1c0476499967c9276`
  and `f0a51dd041b1312b87724d6b6f4f9be48aeb3d01` implement retained GPU images and a
  supplied synchronous Win32 renderer with an ordinary application. The implemented checkpoint
  below supersedes the earlier missing-import/native-selection observations; external HWND/loop
  driving and delayed Scene publication remain incomplete. [W3]
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

The [reuse map](#editor-and-game-reuse-boundary) and
[runtime comparison](#game-runtime-mechanism-review) define that decision. Include shipped UI
resource loading and retirement through the Player's actual content providers or explicitly
executable-owned resources. Existing [cooked delivery](../guides/cooked-player-delivery.md)
does not qualify a GPUI asset/font adapter or justify falling back to Editor source loaders.

## Selected Nara Renderer Adapter

The November 11 clarification selects Nara-owned rendering adaptation for the first actual
engine consumer. This resolves the application-side owner without changing Open-GPUI's status
as an independent framework or selecting the Editor/game-runtime default.

| Owner | Selected responsibility |
| --- | --- |
| Open-GPUI App/Window | UI state, invalidation, layout/text, input, ordered Scene production and exact prepared/submitted publications. |
| Framework rendering contract | Backend-independent Scene and atlas semantics, original candidate delivery and exact submission/shutdown authority. |
| Nara renderer adapter | Implement `HostedSurfaceRenderer`, adapt exact engine outputs and retain recordings/imports through the original engine GPU use. |
| Nara `WgpuRenderHost` | Original enabled device/queue, window/target acquisition, encoder transaction, actual submit/present, composite receipts and retirement. |
| Optional `WgpuSceneRenderer` | Reuse existing primitive shaders, atlas and private passes inside the Nara-owned encoding transaction. |

This resembles ImGui's application-owned rendering integration. The runtime remains retained:
consuming a Scene does not require rebuilding UI interaction in Nara or changing GPUI to an
immediate-mode API. Scene delivery and renderer resource custody are explicit because recorded
commands can wait while the last-submitted controls remain active.

Use the qualified optional WGPU recorder for the first consumer. Nara can later replace it with
an engine-specific Scene renderer without replacing components or layout/input execution.
That replacement must implement the complete admitted primitive/atlas contract, including
paths, glyphs, shadows, exact clips, painter order, color/alpha and resource lifetime. Avoid
introducing a second RHI or a universal render-graph wrapper to conceal those obligations.

Record inside the existing final-window transaction first. Declare actual engine-output samples
with their original display occurrence and join View input only to the successful Host composite.
Complete the framework's original candidate from the Host's actual outcome, preserve the
submitted UI on refusal, and retain renderer/import resources through the original GPU prefix.
One closing UI root must not drain unrelated producers. A graph placement is warranted when
real cross-feature reads/writes, intermediate outputs or history require it; internal UI passes
need not become public graph nodes.

The source check confirms that `WgpuWindowDeviceScope` already exposes its original adapter,
device and queue. The window encoding callback's `TextureView::texture()` supplies the original
target required by `WgpuSceneRenderer::record`; no new attachment accessor or intermediate copy
is needed for that access. The concrete missing handoff is per-attempt window submission custody:
the Host internally freezes a `WgpuSubmissionPrefix`, but its public outcome exposes only
presentation/composite receipts. Preserve that exact prefix on successful and failed attempts,
including writes before a paint refusal or callback panic. Establish resource custody before
native work; release UI-thread recordings only after their original prefix settles. A producer's
earlier render prefix, a later latest-prefix query or whole-device idle cannot substitute for
that window use. [N5]

The WGPU-source decision below is still required for recorder reuse; a custom Scene backend
avoids shared renderer types but does not remove the native-host, atlas and lifecycle work.

## WGPU Identity And Dependency Policy

At the ownership-decision checkpoint, Nara required registry WGPU `=30.0.1`; Open-GPUI selected
the official Git revision
`cddb2f5d80dc0009b2ef6767635140cacf90a007`, whose package version is `30.0.0`.
The existing independently locked GPUI consumer selects that same Git source. `open-gpui-wgpu`
already is the optional renderer package; another package with the same responsibility is
unnecessary. Shared source identity permits compatible Rust types; sharing the actual original
Device/Queue is an additional runtime requirement. Two dependency entries alone do not prove
two devices were created. [D1]

| Option | Tradeoff and current disposition |
| --- | --- |
| Upgrade both consumers to a common registry release | Prefer a supported common release when it contains the required fixes. Check source and affected integration behavior; registry 30.0.1 has an already reproduced Vulkan semaphore failure. |
| Align the Nara consumer to the maintained fixed Git | Current bounded alternative while no qualifying newer release is available. Preserves known upstream Vulkan/WebGL fixes; requires deliberate Nara requirements/source changes and affected-consumer qualification. This is not a permanent Git-source requirement. |
| Application-root source overrides | Useful assembly mechanism, subject to every dependency's version requirement; a 30.0.0 patch cannot satisfy `=30.0.1`. Explicit Git edges also need reconciliation. |
| Implement Scene rendering in Nara | Avoids directly sharing renderer WGPU types, but assumes all primitive/atlas/path/text maintenance. Reserve for demonstrated needs. |
| Native cross-device/backend texture interop | Separate dependency policy is possible, with allocation, fences, format/state and dual retirement costs. Higher current complexity than same-device recording. |

Do not reinterpret WGPU handles across source versions. Record the framework revision, GPU
source/revision/package version, Naga, enabled features/limits and compiler floor for each consumed
combination. Root overrides do not propagate into independently built workspace roots. The source
inventory found seven fixture workspaces with nine direct `=30.0.1` declarations, plus the main
workspace and reference ImGui renderer's registry edge. Change affected roots intentionally when
adopting a source policy. That research checkpoint changed no manifest or lockfile. [D1] [D2]

A read-only crates.io query on November 11 reports `30.0.1` as the newest non-yanked release,
published August 22. The maintained Git revision is dated September 15 and includes the
September 12 surface-drop fix `3b3506ea31154edc04de64debda0e780cba2639c`, although its package
still says `30.0.0`. Replacing it with published `30.0.1` is therefore not an upgrade to that
correction. The existing unchanged Vulkan controls already reproduced the registry failure
and passed on the fixed source; do not rerun the entire official framework to establish this
known source difference. A later corrected release can replace the Git pin in both projects.
Align WGPU/Naga and affected independently locked roots deliberately, retaining compiler and
feature requirements rather than changing a version string alone. [D1] [D3]

The subsequent engine implementation selects the existing fixed Git revision in Nara's main
workspace and all nine independent direct declarations. It also resolves the fifteen affected
lock roots, including source games and presentation/prelude consumers. Application roots that
build the reference Dear renderer patch its `wgpu = "30.0"` registry edge to that same source;
the dependency accepts the Git package version without modifying Dear. The separate
`gpui-engine-consumer` workspace owns the independent framework closure; ordinary image-output
consumers do not resolve sibling framework paths. An optional path dependency in their existing
manifest would still impose that resolution requirement and is therefore not used.
Its actual compilation and native qualification remain implementation evidence, separate from
this dependency disposition. Future corrected registry releases remain eligible replacements.

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
The [pinned Base/module comparison](#editor-and-game-reuse-boundary) distinguishes public state
from private behavior bindings and defines the consumer criterion for further extraction.

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

The source check retrieved on **2026-11-11** pins official `longbridge/gpui-kit` main to
`8650093ccee70317fe96d48d53ead459620d38f0`, committed at `2026-11-11T10:11:33+08:00`.
Its manifests declare `gpui-base`/`gpui-component` `0.9.1`; mandatory `gpui` resolves to
`gpui-pre =0.3.12`, alongside `gpui_macros` and text/parser dependencies. The optional
`gpui-fast` backend does not turn Base into runtime-neutral state. Base also declares native
platform dependencies; `gpui-component` depends on Base, with no reverse component dependency.
These are inspected source versions, not a latest-release or fork-compatibility claim. [ERB1]

Two implementations show the actual boundary:

- `Checkbox` accepts controlled `CheckboxState`, computes the next activation value and reports it
  through a callback; the application supplies the value again. Its otherwise unstyled control
  retains GPUI `Stateful<Div>`, keyed `FocusHandle`, accessibility mapping and a callback receiving
  `Window`/`App`. Reusing that complete behavior still requires GPUI execution. [ERB2]
- `TabGroup` retains active-panel, focus and drag state, emitting layout-edit intents for its
  container. `TabGroupRenderer` customizes appearance through `Stateful<Div>`/`AnyElement` and
  `Window`/`App`, while the group installs structural layout and interaction. This separates
  docking behavior from its skin; it does not replace the platform or GPU renderer. [ERB3]

The bounded Open-GPUI comparison uses unchanged package sources at
`43a36754102858deea8bf1668e6cdaabab40f0d6`, inspected on the same date:

| Existing package | Present boundary and reuse limit |
| --- | --- |
| `ui-core` | Only production dependency is neutral `motion`; controlled state, focus policy, geometry and virtualizer models do not require GPUI. They are not complete control execution. |
| `motion` | No dependencies; caller-supplied elapsed time, shared spring sampling and frame-demand values. The adapter owns the clock and scheduling. |
| `ui-components` | GPUI elements, themes and adapters share a package with resolved component state; the state/adapter separation is a module contract, not a separately consumable unstyled package. |
| `canvas` | Document/store/tool owners and a GPUI `adapter` module are distinct, but the package still has a mandatory GPUI dependency. |
| `docking` | Durable layout/model and runtime/visual owners are distinct modules; the package still depends on GPUI, `ui-core` and `motion`. It is optional workspace infrastructure, not a game-UI prerequisite. |

The manifests and source boundaries support selective reuse already. Accepted Open-GPUI ADR0008
keeps the existing component packages as the active product boundary and defers another headless
package; ADR0018 already supplies the independently reusable motion owner. [ERB4] [ERB5]
This does not establish Base-equivalent public behavior primitives. For example, resolved button
state and the semantic activation handle are public, while the focus/input/publication binding
used by official controls remains crate-private. A custom skin must be able to reuse complete
keyboard, pointer, focus and semantic behavior through a supported interface; exporting state
structs alone does not prove that capability. Use the custom Editor/HUD composition to identify
the needed module/API extraction before selecting a new package. [ERB7]

Existing dependency and feature choices are the first levers. GPUI Kit's `default-features = false`
omits its optional styled `component` and `assets`, while retaining Base, GPUI and, on desktop,
the platform dependency. In Open-GPUI, depending only on `ui-core`/`motion` avoids GPUI entirely;
using core plus `gpui_wgpu` instead of the desktop platform selector keeps renderer selection
explicit but still needs a production host adapter. The workspace already disables GPUI's default
features. However, `ui-components`, `canvas` and `docking` all require GPUI, so disabling their
defaults cannot make them runtime-independent. Neither a new crate nor another umbrella feature
is required to express these conceptual boundaries. [ERB6]

Refactor the known platform/renderer coupling now through the external WGPU application consumer.
Keep unstyled behavior and appearance distinct through existing component modules. Do not first
split text, layout, focus, caching and every control into separate public crates: their shared
invalidations, font provenance, resource leases and update ordering must still have one coherent
owner. Extract a package when an independently consuming application can use it with a smaller
dependency closure and a complete lifecycle contract. Module boundaries can precede crate splits;
fewer crates are not a reason to retain native-device coupling.

For the host-driven comparison, clarify three concrete owners within the existing modules:

1. The production `Platform`/`PlatformWindow` adapter delivers host events, text/IME, wakeups and
   frame demand into the existing App/Window lifecycle, including exact Scene admission and later
   submission acknowledgement. Keep one focus/input/semantic executor. [O1] [O2] [O7]
2. Existing `gpui_wgpu` owns Scene recording, atlas/sampled-image bindings and resource receipts;
   the application owns device/queue, encoder/target, submit/present and completion. Texture import
   is a renderer resource contract, not a reason to create another UI foundation. [O5] [O8] [W1]
3. The native platform owns windowing and OS services; its default renderer construction must allow
   an injected renderer choice. Preserve the normal desktop composition while qualifying the
   external driver. This is an ownership seam, not a crate for each algorithm. [O11]

Keep element state, invalidation, layout/text caches, hit geometry and Scene generation coherent
inside the framework. Keep canvas transactions and docking layout authority with their existing
owners; extract further only for an actual independent consumer. Headless behavior tests, Scene
recording and texture import each prove their own boundary, not game UI support or a Nara product
integration choice.

For Nara, full reuse of the framework execution is the first candidate to compare, not a commitment
to ship the desktop host in a game. If no-window operation, player focus, time policy, platform
closure or measured costs expose a mismatch, extract the relevant reusable mechanism or retain
independent execution for that requirement. The HUD comparison below decides execution reuse;
Editor adoption alone cannot decide it. A common button or virtualizer also cannot establish
world-space rendering, split-screen input or arbitrary game materials.
Distinguish a logical UI root/surface from an OS window in that comparison. A HUD, split-screen
player surface or world-plane texture must not require an extra native window/message loop merely
to obtain layout, controls and text. Reusing Window internals is viable only if their lifecycle
can represent those surfaces through the production host contract, without simulating native
presentation or synchronizing a second interaction tree. This is an acceptance requirement,
not an already-implemented standalone-root API. [G1] [G2]

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

## Implemented Image And Native Renderer Prerequisites

The November 11 continuation implements two reusable framework seams after the recording unit:

| Implementation | Qualified capability | Remaining ownership boundary |
| --- | --- | --- |
| `a5d7dea101ca643a9fd462f1c0476499967c9276` | WGPU imports an explicit mip/layer/texel region of an application texture without copying its pixels. Ordinary Window painting preserves order, clips, transforms, filtering and sampled transfer/alpha. External recordings and built-in submissions retain the original allocation through GPU completion. | Caller supplies the same actual device, orders producer writes and sampling, and prevents premature explicit destruction/reuse. This does not snapshot mutable pixels or select HDR/color policy. |
| `f0a51dd041b1312b87724d6b6f4f9be48aeb3d01` | Win32 native services accept supplied graphics and text through the existing Window, geometry and retirement owners. An ordinary non-Nara WGPU application produces a changing texture and composes normal controls. | GPUI still owns the HWND/message loop and uses synchronous submission. Application-owned windows/loop and delayed Scene admission remain separate work. |

Changed-renderer/public GPU tests pass 125/125; the final affected native selection passes 18/18;
the independently locked ordinary image consumer passes 1/1. The actual standalone application
submits its changed image and normally shuts down after quiescing original prefix 3. Its direct
Windows dependency initially omits the existing manifest feature and fails in the OS loader;
enabling `windows-manifest` corrects that normal application configuration. Its resolved production
graph includes no test/bench-support runtime. Affected correctness Clippy, formatting and existing
import/API/document/release checks pass. [W3]

Production wasm compilation exposes the new retention path's non-Send browser resource capture.
Correction `7c5c0cd810f15c3e4c3591c400e42aa72f25a2b4` keeps resources in a browser-local
future and sends only queue completion across the callback boundary. Locked production wasm
compilation and correctness Clippy then pass;
the independently locked production Web smoke composition also compiles after adding the same
existing dependency edge. The new browser image/completion trajectory is not executed. This is
a fork portability fix, not additional upstream source adoption. [W3]

The cache-only native fixture had also assumed accessibility inactive. Clean pre-change source
reproduces its failure; `43a36754102858deea8bf1668e6cdaabab40f0d6` establishes the disabled
precondition while retaining every exact counter and four combinations. A separate unchanged
synthetic-accessibility startup test still fails under a preconnected observer, also reproduced
with the pre-change executable. The passing 18-test selection is not a complete native aggregate.
Desktop automation was unavailable; no screenshot or manual physical-input claim follows. [W3]

These are fork-owned general framework extensions, not newly imported official implementations.
The synchronous host-driven Window/loop successor below supersedes that missing prerequisite;
the exact delayed Scene/submission/interaction lifecycle remains open. Keep the current Nara UI
as the control until the actual Host/View and authored HUD consumers qualify an arrangement. [W3]

## Implemented Synchronous External Win32 Host

Open-GPUI `6ad133d36622851025eee882d63bb8a47787d524` adds an explicit App platform-window
factory inside the original reservation, resolved options, display snapshot, first-presentation
and rollback transaction. `911e628c0e9cd66cf63f5c04e7d46c0d87c67fdd` attaches the existing
Win32 services to an application-created hidden top-level HWND and returns launch to the host's
message loop. The ordinary non-Nara WGPU application owns its window, loop, device, encoder and
actual synchronous submission; normal elements and native services retain their existing owners.
Successful binding detachment quiesces the original GPU prefix and leaves the host HWND alive,
with its userdata, original IME association and message chain intact. It is a different terminal
fact from OS destruction. [W4]

Final affected Core/public tests pass 27/27; native attachment/input/retirement controls pass
22/22; independently locked consumers pass 1/1 without test support and 2/2 with ordinary
headless execution. The final ordinary external smoke changes GPU images, routes native button,
F6 and UTF-16 messages, checks actual resize and quiesces original prefix 6 before detachment.
Host private messages and its quit marker remain usable. The original GPUI-owned application
also passes its changed-image and prefix-3 shutdown trajectory. Correctness Clippy, production
Core/WGPU/Web wasm compilation, root/consumer formatting and existing static/document checks
pass; qualification is recorded at `af68cbad1133d40c59f8f0cd342156a12ad475e8`.
These are fork-owned framework extensions; no additional official
source-adoption credit follows. The framework record retains initial fixture failures, a real
COM access violation and its CDB diagnosis, and the reviewed exact-authority/IME/resource fixes. [W4]

This checkpoint accepts hidden, non-child normal/floating top-level windows and submits
synchronously. It does not provide delayed Scene delivery, last-submitted interaction/text/
accessibility publication, a production no-window logical root, physical IME/DPI qualification,
performance budgets or Nara Host/View acceptance. A game HUD or world-plane texture must not need
an extra native window merely to obtain controls. Keep those runtime requirements separate from
the Base-style public behavior/skin consumer and subsequent crate-extraction decisions. [W4]

## Implemented Deferred Scene Submission

Core `797372013abf6da74b301972515dc02a9e596801` and native/application
`69d0992f2a083f7d3ad7161700d3e2735cc8fc41` implement deferred submission through the original Window,
not a parallel UI executor. One prepared candidate retains its Scene, text/cache resources,
authority overlay and unpublished effects. The last-submitted frame keeps pointer/key handlers,
focus, native text and accessibility dispatch while the host waits. A non-reusable token carries
the original full WindowId, frame generation and attempt; actual submit/present returns that
same token through ordered App ingress. Renderer/resource and native target invalidation prevent
later submission of stale work. A clone of a Scene or a mutable latest-generation slot is not
this authority. [W5]

Initial open may return a still-hidden pending window. The existing creation transaction retains
its native reveal command until the original nonempty submission. Rejection, empty-frame retry,
callback unwind and close retain the same lifecycle. Ordinary synchronous platforms preserve
their default mode and share the prepared/commit implementation. Profiling freezes CPU draw
completion before the host wait and uses the original completion report timestamp; it does not
infer GPU duration or scanout. [W5]

Final focused Core/public tests pass 57/57; affected Window/input/accessibility/profiler controls
626/626; native regression controls 22/22. The ordinary external deferred WGPU trajectory submits
actual retained commands, reads original A/B producer pixels as `[217,89,38,255]` and
`[38,140,217,255]`, and preserves submitted controls/text through pending replacements. It covers
hidden initial reveal, resize/same-size ABA, rejection, minimized/hidden recovery and close with
an unsubmitted candidate, then quiesces original prefix 13 and leaves the host alive. Both original
synchronous smoke modes pass. Close's completion observation is already at prefix 13: the result
proves exact recording custody through the wait, without claiming hardware remained busy then.
The framework record retains actual failed fixtures, review corrections and precise scope. [W5]

Independently locked feature-free interfaces pass 1/1. Affected correctness Clippy, production
Core/WGPU/Web wasm compilation, root/consumer formatting and existing static/document gates pass.
The framework qualification checkpoint is `c9fc89bd5fc0f0b438c396be067fd2a1356de74d`.
Standalone Win32 raw-handle feature correction is `ec576a20dc177616b1ba6b70d66181251c8def92`;
no dependency version, WGPU source or compiler floor changes. These results qualify Windows-owned
desktop delivery and shared compilation, without browser deferred-host execution. [W5]

This closes the selected desktop delayed-delivery prerequisite. Production logical roots without
native windows, other host/platform combinations, physical IME/DPI, numerical budgets and actual
Nara GPU/Host/View consumption remain open. Preserve the authored HUD comparison and the public
two-skin control-behavior consumer as distinct next steps. No Nara dependency or UI default changes.

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
- [W3] Local Open-GPUI image commit `a5d7dea101ca643a9fd462f1c0476499967c9276`, native/application commit `f0a51dd041b1312b87724d6b6f4f9be48aeb3d01`, and browser custody correction `7c5c0cd810f15c3e4c3591c400e42aa72f25a2b4`, qualified at `12f9bb50e8e`; `crates/gpui/src/{image_texture.rs,window/imported_image.rs}`, `crates/gpui_wgpu/src/wgpu_atlas/imported_image.rs`, `crates/gpui_windows/src/renderer.rs`, `examples/wgpu-native/` and the public/native consumers. Open-GPUI `docs/external-wgpu.md` and `docs/verification.md#retained-gpu-images-and-supplied-win32-renderers` own public usage, actual results and failures. These local revisions are not remotely published.
- [W4] Local Open-GPUI App factory/detachment commit `6ad133d36622851025eee882d63bb8a47787d524` and Win32/application commit `911e628c0e9cd66cf63f5c04e7d46c0d87c67fdd`, qualified at `af68cbad1133d40c59f8f0cd342156a12ad475e8`; `crates/gpui/src/{app.rs,platform.rs,app/cell.rs}`, `crates/gpui_windows/src/{platform.rs,window.rs,direct_manipulation.rs,native_attachment_tests.rs}`, `examples/wgpu-native/` and the two public `platform_window_factory` consumers. Open-GPUI `docs/external-wgpu.md#drive-an-application-owned-win32-window` and `docs/verification.md#supplied-platform-windows-and-external-win32-driving` own usage, actual results/failures and current limits. Local commits only.
- [W5] Local Open-GPUI Core `797372013abf6da74b301972515dc02a9e596801`, native/application `69d0992f2a083f7d3ad7161700d3e2735cc8fc41` and standalone feature correction `ec576a20dc177616b1ba6b70d66181251c8def92`, qualified at `c9fc89bd5fc0f0b438c396be067fd2a1356de74d`; `crates/gpui/src/{window.rs,window/deferred_frame.rs,window/candidate_transfers.rs,platform/frame_delivery.rs,profiler.rs}`, existing App native ingress, Windows renderer/Window forwarding and `examples/wgpu-native/`. Framework `docs/verification.md#deferred-scene-submission-and-submitted-publications` owns exact results, failures and limits; `docs/external-wgpu.md#defer-an-original-candidate` owns public usage. This is fork-owned hosting work after the fixed-source audit.
- [I1] ImGui pin above, [`docs/BACKENDS.md:25–67,129–172`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/docs/BACKENDS.md#L25-L67): platform/renderer split and custom-engine guidance.
- [I2] Same pin, [`examples/example_win32_directx11/main.cpp:74–75,101–109,134–136,176–183,267`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/examples/example_win32_directx11/main.cpp#L74-L183): existing HWND/device, application loop, draw and present.
- [I3] Same pin, [`docs/BACKENDS.md:214–287`](https://github.com/ocornut/imgui/blob/d3b36a875f0f7e7446b99606ca3d6df27b7dad7a/docs/BACKENDS.md#L214-L287): texture updates, indexed triangles, clip offset and framebuffer scale.
- [I4] Same local ImGui pin, `backends/imgui_impl_wgpu.h:49–79`, `backends/imgui_impl_wgpu.cpp:466–617`, `backends/imgui_impl_win32.h:24–35`: injected Device, host render-pass encoding, external texture views, clipping and forwarded native events.
- [N1] [Finite programs](../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs), [current guide](../guides/render-programs.md), `crates/nara_render_wgpu/src/program/mod.rs:467`, `graph/compile.rs:19`, `program/feature.rs:34`.
- [N2] [ADR0121](../architecture/adr/0121-composable-render-programs-and-host-execution.md), [implementation status](../architecture/adr/implementation-status.md): bounded implemented refinements and full proposal status.
- [N3] `crates/nara_render_wgpu/src/host.rs:969,1229,2214,2384,2901,3071`, `host/imgui.rs:102,172,297`, `output.rs:86`; `editor/src/platform/drawn/graphics.rs:329`.
- [N4] `editor/src/native/images.rs:82,108`, `native/images/presentation.rs:22,203`; `src/project_host/runtime/editor/presentation.rs:159`.
- [N5] `crates/nara_render_wgpu/src/host.rs:728,943,2901`, `submission.rs:275,440`, `retirement.rs:30`; WGPU 30 `src/api/texture_view.rs:30`. At the renderer-selection checkpoint the window attempt retained its own prefix internally, while its public resource handoff was missing. The implemented Host/Scene successor below supplies that original observation.
- [D1] `nara/Cargo.toml:139`; `open-gpui/Cargo.toml:243`, `crates/gpui_wgpu/Cargo.toml:26,68`, `tests/fixtures/dependency-consumer/Cargo.toml:43`; sibling Open-GPUI `docs/verification.md#wgpu-dependency-shaders-and-webgl`.
- [D2] [Cargo dependency overrides](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html): source/version selection and root-only patches.
- [D3] [crates.io WGPU API](https://crates.io/api/v1/crates/wgpu), read November 11, 2026: newest `30.0.1`, published `2026-08-22T02:53:38.229945Z`, not yanked. Official Git [surface-drop correction](https://github.com/gfx-rs/wgpu/commit/3b3506ea31154edc04de64debda0e780cba2639c) and [maintained dependency revision](https://github.com/gfx-rs/wgpu/commit/cddb2f5d80dc0009b2ef6767635140cacf90a007); commit dates read from the local official dependency checkout.
- [C1] [GPUI Base](https://gpui-kit.com/base/), observed November 11 [workspace manifest](https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml), [Base manifest](https://github.com/longbridge/gpui-kit/blob/main/crates/base/Cargo.toml): main-branch evidence, not a latest-release claim.
- [C2] [Dock documentation](https://gpui-kit.com/base/dock/), [DockAreaRenderer](https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/dock/dock_area.rs), [TabGroupRenderer](https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/dock/tab_group.rs).
- [C3] Open-GPUI `crates/ui_core/src/lib.rs:9`, `crates/ui_components/README.md`, `docs/ui/component-contract.md`, `crates/gpui_docking/README.md`, `crates/canvas/README.md`, `crates/motion/README.md`; Nara [independent documents](../guides/editor-documents.md) and `tests/fixtures/module-consumer/editor-panel-consumer/tests/dialogue_panel.rs:85`.
- [C4] Open-GPUI `crates/ui_components/src/activation.rs:54`, `tests/fixtures/hot-reload-consumer/README.md`; Nara [panel operations](../guides/editor-panels.md#observe-and-operate-presented-controls), [iteration contract](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md); [embedded GPUI](https://github.com/zed-industries/embedded_gpui), [GPUI Shell](https://gpui-kit.com/shell/).
- [G1] Nara `crates/nara_ui/src/runtime.rs:218`, `runtime/targets.rs:8`, `codec.rs:19`, `persistent_key.rs:19`, `authoring_model.rs:365`, `interaction/timeline.rs:143,228`; `crates/nara_ui_kernel/src/compiler/layout.rs:25`, `compiler.rs:55`, `interaction/state.rs:96`.
- [G2] Nara `crates/nara_app/src/lib.rs:2102`, `src/ui_presentation/surfaces.rs:108`, `crates/nara_render_wgpu/src/ui.rs:244`, `crates/nara_ui_kernel/src/style.rs:530`, `mesh.rs:38`, `crates/nara_ui_render/src/packet/mod.rs:378`.
- [G3] Nara `reference-game/src/runtime_ui.rs:64,143,311`, `reference-game/prefabs/reference-ui.prefab.json`, `reference-game/tests/ui_authoring_roundtrip.rs:32`, `reference-game/tests/ui_product.rs:252,286`; the existing test presenter explicitly distinguishes its evidence from native presentation.
- [P1] [Godot plugin guide](https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html), [Control](https://docs.godotengine.org/en/stable/classes/class_control.html), observed November 11; local `repo-ref/godot/editor/docks/editor_dock.h:40` and `scene/gui/control.h:45` provide additional source context.
- [P2] [Unity 2023.2 UI Toolkit overview](https://docs.unity3d.com/2023.2/Documentation/Manual/UIElements.html), observed November 11: retained-mode UI for Editor extensions and game/application runtime; other Unity UI systems remain appropriate for some use cases.
- [ERB1] Official GPUI Kit main pinned by `git ls-remote` on 2026-11-11 to [`8650093ccee70317fe96d48d53ead459620d38f0`](https://github.com/longbridge/gpui-kit/commit/8650093ccee70317fe96d48d53ead459620d38f0), commit timestamp `2026-11-11T10:11:33+08:00`; retrieved 2026-11-11. Pinned [workspace manifest](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/Cargo.toml#L51-L80), [Base manifest](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/Cargo.toml#L27-L82), [Component dependency](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/component/Cargo.toml#L120-L131), [Base backend alias](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/lib.rs#L7-L10); compare the [official overview](https://gpui-kit.com/base/) and pinned [README](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/README.md).
- [ERB2] Same GPUI Kit pin and retrieval date, [`checkbox.rs`](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/checkbox.rs#L16-L181), especially controlled activation/state, callback and keyed focus; [`RenderOnce`](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/checkbox.rs#L370-L408) installs the actual GPUI interaction/accessibility adapter.
- [ERB3] Same GPUI Kit pin and retrieval date, [`TabGroup` state](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/dock/tab_group.rs#L136-L178), [`Render`](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/dock/tab_group.rs#L683-L757), and [`TabGroupRenderer`](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/base/src/dock/tab_group.rs#L902-L964): presentation hooks return GPUI elements and retain GPUI context.
- [ERB4] Open-GPUI local source pin `43a36754102858deea8bf1668e6cdaabab40f0d6`, committed `2026-11-11T13:49:17+08:00`, inspected 2026-11-11; the five compared package trees had no working diff. Paths are relative to the local Open-GPUI repository: `crates/{ui_core,ui_components,canvas,gpui_docking,motion}/Cargo.toml`, `crates/ui_core/src/{lib,controllable_state,virtualizer}.rs`, `docs/ui/component-contract.md:79`, `crates/canvas/src/lib.rs:38`, `crates/gpui_docking/src/lib.rs:165,191`, and `crates/motion/README.md:3`. This is a bounded source comparison, not executed runtime evidence or a remotely published revision.
- [ERB5] Same local Open-GPUI pin and inspection date, Accepted `docs/adr/0008-open-gpui-ui-component-productization-roadmap.md:23` and `docs/adr/0018-open-gpui-motion-crate-boundary.md:22`: existing component product boundary and neutral motion ownership respectively.
- [ERB6] GPUI Kit pin/date from ERB1, [`gpui-kit` manifest](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/kit/Cargo.toml#L18-L27) and [mandatory dependencies](https://github.com/longbridge/gpui-kit/blob/8650093ccee70317fe96d48d53ead459620d38f0/crates/kit/Cargo.toml#L78-L92). Open-GPUI local pin/date from ERB4, `Cargo.toml:68`, `crates/gpui/Cargo.toml:21`, `crates/gpui_platform/Cargo.toml:42`, and `crates/gpui_wgpu/Cargo.toml:20`; required component/domain GPUI edges are recorded in ERB4. These source declarations do not prove a production no-window host or the complete resolved feature union of an embedding application.
- [ERB7] Same local Open-GPUI source pin/date as ERB4, `crates/ui_components/src/public_api/common.rs:20,36`, `crates/ui_components/src/activation.rs:139,328`, and `crates/ui_components/src/button.rs:268`: public resolved state/activation handle versus private concrete activation binding and the current styled adapter.

## Reuse Decision For Editor And Game UI

Source check: local Open-GPUI `c9fc89bd5fc0f0b438c396be067fd2a1356de74d`, clean when inspected
on 2026-11-11. The source citations below use paths relative to that repository at this revision.
This is a bounded source review; it adds no executed qualification or dependency/default decision.

Prefer shared framework mechanisms with separate Editor and game adapters when the authored-game
comparison succeeds. Game controls must be usable without Editor documents, workspaces, docking
or Editor visual recipes; applications may explicitly choose optional shared controls and skins.
Keep the current executor as a control; retain independent execution for demonstrated lifecycle,
platform or cost requirements.

| Reuse unit | Current boundary and consequence |
| --- | --- |
| Runtime-neutral models | `ui-core` depends only on dependency-free `motion`; controlled values, focus policies, virtualizer models and motion samples can already be consumed without GPUI. They do not supply complete widgets, text editing or layout execution. [RD1] |
| Complete unstyled widget behavior | Pointer/keyboard activation, focus, accessibility, accepted-publication binding and control lifetimes require GPUI execution. These can be reusable with different skins while remaining dependent on `App`/`Window`. Public state alone is insufficient. [RD2] |
| Execution | Entities, layout/text caches, input and submitted-frame authority remain coherent App/Window responsibilities; platform and rendering adapters supply services. Neither a Base-style split nor removing a theme creates another execution host. [RD3] [RD5] |

The useful Base-like improvement is a supported behavior/composition API, not more neutral structs.
Today `ButtonState` and `ActivationHandle` are public, but `ActivationBinding` and its pointer,
keyboard and publication installation are crate-private. Prove one complete control under distinct
Editor and HUD skins before choosing its public module boundary; include disabled/cancellation,
focus and semantic activation, not just appearance overrides. The pinned Base examples above
demonstrate this behavior/skin distinction while retaining GPUI execution. [RD2] [ERB2] [ERB3]

Start with module/API extraction. Extract a crate when a named independent consumer uses the complete
contract with a smaller mandatory dependency closure and preserved lifecycle, without copying a second
interaction executor. Measure build/runtime benefits. `ui-components`, `canvas` and `docking` still
require GPUI even with defaults disabled; pure internal modules do not make their packages neutral.
An unstyled crate can avoid styled dependencies without removing App/Window needs. Revisit ADR0008
explicitly if consumer evidence justifies extraction; motion already proves a separate owner. [RD1] [RD7]

Distinguish a GPUI `Window` from an OS window. The supplied App factory creates a `PlatformWindow`;
the implemented Win32 attachment requires a real HWND and native services. Ordinary Linux headless
mode already opens `HeadlessWindow` without compositor surface/native handles and runs layout/text
shaping. Its draw discards Scenes, its atlas uploads nothing and its frame callback is a no-op.
This refutes a universal native-window requirement, without qualifying portable engine-driven roots,
offscreen GPU/player input. Test-gated `HeadlessAppContext` is separate. [RD3] [RD4]

The external WGPU seam accepts the host's device/queue/encoder/target, imports retained GPU images,
and provides recording leases the host retains through GPU completion. Deferred delivery preserves
the original Window's submitted interaction while a candidate waits, then admits its exact token
after actual submission, separately from GPU completion. Reuse these owners; the Win32 trajectory
does not qualify game surfaces or Nara's exact WGPU/source and Host integration. [RD5] [W5]

| Game requirement | Remaining consumer proof and owner |
| --- | --- |
| No native window | Engine-driven logical roots must run layout/semantics without an extra OS window; separately prove offscreen GPU output and no-render execution with truthful frame admission, cancellation and close. Reuse the existing Window authority rather than fake native submission. [RD3] [RD4] [G3] |
| Time and pause | `MotionClockSample` accepts caller elapsed time, but element animation uses the background executor clock. Define game/real-time policies per relevant root or animation; paused gameplay must preserve responsive menus, text/IME and host work. Playback pause alone is not a game clock. [RD6] |
| Player and input | Current activation binds a Window and focus publication; key events carry keystrokes, not player identity. The engine must define seat/player routing, gamepad navigation, modal precedence and capture cancellation, with one live interaction executor per selected root. [RD2] [RD6] |
| Shipped resources | Adapt fonts/images through actual Player content providers or declared executable resources; preserve original version/device identity, producer ordering and retirement. `AssetSource`, font injection and imported-image leases are useful seams, not cooked-content integration. [RD5] [RD8] |
| World space | A recorded 2D UI texture can be placed on an engine-owned plane; the engine still owns projection, pixel density, depth/occlusion and ray-to-UV input. The recorder accepts a 2D color target, with no depth attachment contract or arbitrary per-control material API. Require a named workflow before extending it. [RD5] [G2] |

Use the existing authored HUD/modal/Retry workflow for this comparison before selecting execution
reuse. Godot and Unity supply bounded precedents for sharing mechanisms; neither those precedents
nor `gpui-base` settle these game requirements or require every Editor control to ship. [G3] [P1] [P2]

- [RD1] `crates/motion/Cargo.toml`; `crates/{ui_core,ui_components,canvas,gpui_docking}/Cargo.toml:[dependencies]`; `crates/ui_core/src/controllable_state.rs:ControllableState`; `crates/ui_core/src/virtualizer.rs:VirtualizerState`.
- [RD2] `crates/ui_components/src/public_api/common.rs:COMMON_PUBLIC_EXPORTS`; `crates/ui_components/src/button.rs:ButtonState,Button`; `crates/ui_components/src/activation.rs:ActivationHandle::request,ActivationBinding::{new,bind,publish_accepted_runtime}`.
- [RD3] `crates/gpui/src/app.rs:App::open_window_with_platform_window,headless_app_context`; `crates/gpui/src/window.rs:Window::new,Window::platform_window`; `crates/gpui/src/platform.rs:PlatformWindow`; `crates/gpui_windows/src/platform.rs:WindowsPlatform::attach_window`.
- [RD4] `crates/gpui_platform/src/gpui_platform.rs:headless,current_platform`; `crates/gpui_linux/src/linux/headless.rs:HeadlessConnection`; `crates/gpui_linux/src/linux/headless/window.rs:HeadlessWindow` and its `HasWindowHandle`, `draw`, `sprite_atlas`, `on_request_frame` implementations.
- [RD5] `crates/gpui_wgpu/src/wgpu_renderer/scene_renderer.rs:WgpuSceneRenderer::{new,record},WgpuSceneRecording`; `crates/gpui_wgpu/src/wgpu_atlas/imported_image.rs:WgpuAtlas::import_image`; `crates/gpui/src/platform/frame_delivery.rs:PlatformFrameSubmission::{is_pending,complete}`; `crates/gpui/src/window/deferred_frame.rs:Window::defer_candidate_frame`.
- [RD6] `crates/motion/src/controller.rs:MotionClockSample::from_elapsed`; `crates/motion/src/frame_host.rs:MotionFrameHost::sample_elapsed`; `crates/gpui/src/elements/animation.rs:AnimationElement::request_layout`; `crates/gpui/src/interactive.rs:KeyDownEvent`; `crates/gpui/src/window/focus.rs:FocusId::is_focused`.
- [RD7] Accepted `docs/adr/0008-open-gpui-ui-component-productization-roadmap.md:Decision`; `docs/adr/0018-open-gpui-motion-crate-boundary.md:Decision`.
- [RD8] `crates/gpui/src/assets.rs:AssetSource`; `crates/gpui/src/app.rs:Application::with_assets`; `crates/gpui/src/text_system.rs:TextSystem::add_fonts`; `crates/gpui/src/image_texture.rs:ImageTexture`.

## Implemented Hosted Logical Surfaces

The next fork-owned successor implements `App::open_surface`, `HostedSurface` and its renderer
interface through the original App/Window at Core commit
`2910b52424ea39c28a330056d7ea04e4f5beaa6f`; ordinary GPU consumers are
`8809f12929eb1aeca4db5d42f06a9c58381b8a4e`, qualified at
`5925a34f920563592cd6115e6f97867699f8dae6`. These are local framework commits. A logical root
captures the hosted adapter's own capability profile; desktop opening keeps its original
platform validation. Layout/text caches, input and prepared/submitted publications retain their
existing owners. The host supplies local
geometry, directed input, frame demand, the atlas/renderer and exact presentation shutdown.
Native handles and desktop mutations remain unsupported, native creation/visibility facts stay
false, and retiring the logical binding reports detachment without OS destruction.

Four ordinary production WGPU trajectories pass on Windows: owned, external-host and externally
deferred native compositions with an offscreen root, plus isolated demand followed by native
entry close. They share the original Device/Queue without another UI HWND or native surface.
Actual final UI readbacks cover background, button/glyph/image and field changes. Frozen A/B,
submitted input while a candidate waits, geometry/scale ABA, zero extent, refusal/recovery and
original-prefix quiescence remain complete paths. Logical close preserves native sibling input
and redraw; native-entry close instead requests normal App quit with the logical root still live.
The isolated request advances logical prefix 1 to 2 through two host wake turns, with a timer
only observing rather than driving it.

Final affected CPU/public controls pass 45/45, profiler-disabled capture 1/1, independently
locked interfaces 5/5 and affected native lifecycle 14/14. Correctness Clippy, production
Core/WGPU/Web wasm compilation, root/independent formatting and document checks pass. The
independent fixture distinguishes executed constructor/value controls from unexecuted typed App
closures. The ordinary application supplies GPU execution evidence. No aggregate component,
remote CI, browser runtime or renewed compiler-floor qualification is inferred.

The framework qualification records the actual failures and corrections: retained marked-text
cleanup belongs to original Window retirement; renderer metadata must be captured with its Scene
rather than read from newer mutable source; Windows can lazily create IME helper HWNDs; recording
a deferred packet must wake the next host turn. Exact non-IME UI HWND identities and independent
renderer/surface creation counters remain unchanged. CPU/public and independent checks have
separate evidence from these GPU executions. The framework's existing
`docs/verification.md#host-driven-logical-surfaces-without-native-windows` owns final revisions,
results and limits; its `docs/external-wgpu.md#drive-a-logical-surface-without-a-native-window`
owns public usage.

This supplies the selected no-extra-native-window prerequisite. It does not select the Nara
executor or establish actual Nara GPU-source/Host/View, physical IME/accessibility, game clocks,
player/gamepad focus, cooked resources, world-space/depth/materials or numerical budgets. The
independent framework remains a normal application dependency; no Nara type enters its core.
Keep the public two-skin behavior consumer and authored HUD comparison separate from hosting.

## Implemented Nara-Owned Host And Scene Consumer

The maintained framework support is local Open-GPUI
`7bfe8022f68169879464296e0d6d8ad09d89c7e3`. Its two generic API additions are fork-owned hosting
support after the fixed official-source audit, not additional upstream-adoption credit.
Nara's source-alignment, original window-prefix, surface-recovery and stable capture prerequisite
is local commit `3095fa5b291ed895188f8c4fc4d20007c6081bcd`.
The independently assembled adapter/consumer and usage are local
`8d0849da1bdee7d18f07a99ef89462a4a5c07b6e`.

The independently locked [engine consumer](../../tests/fixtures/gpui-engine-consumer/README.md)
implements `HostedSurfaceRenderer` in Nara. Its ordinary Winit window and `WgpuRenderHost` own
the device, window attachment, command encoder, submission, presentation and retirement.
Open-GPUI retains its original App/Window, text/layout/input and candidate/publication execution.
The optional WGPU Scene recorder uses that exact original enabled device and records directly
to the acquired Nara window attachment. It creates no second UI window, device, surface, submitter
or Scene executor. Ordinary Nara renderer roots resolve no framework packages or sibling paths.

The real `EditorProjectSession` opens a trusted compiled source project. One prepared construction
creates the Sprite through source-owner templates and property writes; later movement uses the
existing prepared property edit, followed by the original Undo. Its real preview candidate and
stock policy produce an immutable `WgpuImageOutput`. Importing the original texture and retaining
its display lease adds no pixel copy. A frozen Scene keeps its exact atlas lease, geometry and
display snapshot while waiting for Nara's attachment. The reusable framework helper follows
actual Scene batches, including directly assembled public sprite arrays.

After real window submission, the adapter completes only the original GPUI attempt. Its normal
accepted-frame publication must match that attempt and the actual Nara image/occurrence/window
composite before `EditorPreviewPresentation` admits picking. Capturing a new unpresented preview
does not revoke still-valid displayed content merely because it is no longer latest. Actual
source or geometry changes still invalidate its authority. A scene clone, texture registration,
successful recording or GPU completion alone cannot admit View input.

`WgpuWindowFrame::submission_prefix` exposes an observation before uploads. It shares the Host's
one-time original index binding, including failure paths, while the adapter keeps non-Send
recordings on its UI thread. The same mechanism preserves custody through paint refusal/panic,
validation, finalization/submission failure and pre-present failure. Final root shutdown cancels
pending work and waits only for its retained original window uses. Another producer submits
before and after that root closes; this is not a forced hardware-busy or bounded-latency proof.

The common WGPU source migration updates fifteen existing locks without unrelated package
identity changes. New framework dependencies live only in this integration workspace. An initial
attempt to use an optional path dependency in the old render-output fixture still imposed sibling
checkout resolution and conflicted with its exact Lyon pins. The final consumer has a separate
workspace; the old fixture and its original pins remain independent. Feature unification also
exposed a private capture method whose tuple shape depended on stock UI. Its render-only method
now has one return type and explicitly refuses UI permissions, returning their original completion
rather than dropping product authority silently.

Actual Windows/DX12 and Vulkan trajectories execute Scene pixels, glyphs, overlay exclusion,
selection, prepared source editing/Undo, 4096-byte field presentation, IME preedit/commit/cancel,
ordinary spaces, submitted input while a successor waits, stale picking, paint refusal/panic
recovery, resize/ABA/zero-size, fractional-root geometry and pending close. Qualification renders
the same frozen Scene into an offscreen diagnostic attachment in that original window submission;
the normal path has no readback or intermediate UI copy. The diagnostic is not swapchain readback
or scanout evidence. Both backends observe center RGBA `[255,0,0,255]`, 1,679 nonbackground glyph
samples, nine successful UI submissions, seven discarded candidates and two intentional failures.

Initial DX12 runs exposed an over-strict latest-preview check and an extra event-loop drive after
successful close; both were corrected without weakening the workflow. Vulkan then exposed a
native acquired-image lifetime issue: the pinned backend's discard operation is a no-op and left
an acquire semaphore signaled after failed drawing. The existing Nara surface owner now keeps
reconfiguration required from acquisition until successful present. Failed encoding, validation,
submission or pre-present paths therefore rebuild the same swapchain before another acquire;
normal successful frames retain their fast path and suboptimal acquisition retains its policy.
Vulkan's unchanged complete trajectory passes after this owning correction. Error-level native
validation logs fail the independent child-process test.

The independently locked Reference Game all-target/all-feature check additionally finds two
pre-existing non-exhaustive fixture matches after Mesh entered the UI primitive enums. Their
glyph-only captures now explicitly exclude Rect and Mesh while retaining every original glyph,
page-identity and product-state assertion. The corrected locked check passes; its existing UI
product/planner run initially passes 8/11 because disabled Retry controls expose no executable
actions and old fixture lookups mistake that for missing control identity. Test-only correction
`db122deb75058678510497e10fe57d1daf44b308` observes the exact role/name, preserves key/focus and
repeat-refusal assertions, and explicitly checks disabled and enabled action exposure. All eleven
original product/planner controls then pass; the plan retains both run results and cleanup-abort
facts separately from native GPU qualification.

Final affected qualification and exact commits are recorded in the foundation plan. This scoped
consumer does not select the full Editor frontend/game executor, close the processed-image and
translucent-color matrix, prove physical IME/DPI migration, device-loss/reconstruction, numeric
budgets or continuous busy-producer retirement, or complete R11/R12/R11f and maintenance handoff.

## Color Composition And Future Display Boundaries

The user requested an early color-space decision for larger games. Accepted
[ADR0092](../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md) already selects
linear sRGB/Rec.709 working primaries, straight authored `Color` and explicit sRGB Image decode.
Its November 12 refinement separates reusable accumulated-alpha targets from authored sources
and makes the external UI composition domain explicit. This is an SDR contract, not an HDR
selection or a declaration that every mature engine blends every UI layer in linear light.

The inspected upstream sources demonstrate why these boundaries matter:

| Source | Observed implementation | Consequence for Nara |
|---|---|---|
| Godot `7a37606a433c0e7735525037cdf774f043146a21`, `servers/rendering/renderer_rd/renderer_canvas_render_rd.cpp` | Canvas enables attribute-to-linear conversion according to whether its render target uses HDR. Ordinary 2D and HDR 2D therefore have distinct working-domain policies. | Preserve an explicit adapter choice; importing a framework does not silently select the engine's composition domain. |
| Same Godot pin, `servers/rendering/renderer_rd/shaders/blit.glsl` | Final blit distinguishes source sRGB from target Rec.709 linear/nonlinear, adjusts SDR reference luminance for linear output and clamps to the declared output range. | Storage transfer, final display encoding, range and reference luminance are separate decisions. A texture-format suffix is not a complete output policy. |
| Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`, `crates/bevy_render/src/view/composition.rs` | Absent or explicit Linear compositing resolves to one linear policy; other requested spaces participate in camera-stack resolution and pipeline keys. | A future composition policy belongs to the actual target/pass owner, including cameras sharing that target. It must not be guessed from an individual control. |
| Same Bevy pin, `crates/bevy_window/src/display_target.rs` | Camera HDR intermediates are distinct from requested surface gamut/transfer and luminance. Requested and supported display output can differ. | A float Scene texture alone does not establish HDR display support. Future capability negotiation must report the selected display policy explicitly. |
| Same Bevy pin, `crates/bevy_ui_render/src/lib.rs` | UI is scheduled after PostProcess and before upscaling in the inspected Core2d/Core3d paths. | UI placement relative to tone mapping/upscaling is a product rendering decision, separate from reusable control/layout mechanisms. |

These are pinned source observations; no Godot/Bevy native execution or complete HDR-output
qualification is claimed. The relevant sources were inspected locally through read-only
`repo-ref/` trees on November 11.

For the current Nara consumer, ordinary PNG RGB is sRGB encoded and alpha is linear. Stock
rendering samples decoded linear RGB and accumulates associated RGB in its transparent output.
The exact retained output descriptor carries that transfer/alpha meaning into the GPUI adapter.
The framework's existing numeric-sRGB working domain remains its default. The Nara adapter
selects a fork-owned linear-sRGB Scene recorder so UI authored colors decode before blending
and the final sRGB attachment encodes once. Imported Scene pixels retain their original declared
meaning throughout; relabeling an Image is not a color conversion.

Future HDR/wide-gamut work must distinguish source/working primaries, transfer, alpha association,
precision/range, scene- versus display-referred stage, tone mapping and display capabilities.
Neither a generic `Hdr` Image flag nor a default ACES pipeline is introduced before a complete
authoring/output workflow supplies acceptance evidence.

Intermediate precision is a separate remaining gate. Non-Canvas stock image rendering currently
uses a linear `Rgba8Unorm` target, while the Canvas world raster uses sRGB storage. Eight-bit
linear storage has coarse dark-color steps even when its transfer/alpha contract is correct.
The current PNG oracle explicitly includes that intermediate quantization; its midtone result
does not qualify a complete dark-gradient palette. Before a high-fidelity Scene/postprocessing
claim, compare target-matched sRGB SDR storage and floating-point linear intermediates through
the actual stock policy, effects and output path, with declared dark/gradient/alpha quality and
bandwidth/retained-byte budgets. Resource descriptors and exact import meanings keep this choice
in rendering rather than changing authored Image, persistent `Color` or control semantics.

## Implemented Source And Cooked Image Color Composition

The explicit generic recorder is local Open-GPUI
`909f510d18bd0d1e537faf73eb57947cee47f927`, extending the previously qualified hosting baseline.
It is fork-owned composition support after the fixed official-source audit, not a new upstream
adoption or a framework-default switch.
The Nara stock-alpha/descriptor prerequisite is local
`08e65be65540942668d140f43b1ca902aee76438`.

This successor extends the same independently assembled engine consumer with a real 12x4 sRGB
PNG: RGB `[128,64,192]` and four vertical alpha bands `[0,64,128,255]`. A Source loader imports
the PNG and metadata, and the actual Image and Runtime Scene cook produce a two-member catalog
with member digests, schema/plugin/codec fingerprints and a separate producer expectation.
Two managed headless runs execute the ordinary stock capture workflow. The Processed base is
admitted before both temporary backings are destroyed; its Runtime then consumes that immutable
base. Both Runtime owners retire before rendering. Public prepared Image observations verify
original allocation/retention, identity and content, then are dropped before only the original
`WgpuRenderInput`s escape. The consumer does not inspect the hidden Sprite batch representation.

Those inputs render into two image outputs through the original native `WgpuRenderHost`, without
a second GPU Host or replacement captures. The adapter obtains stored transfer and sampled-alpha
meaning from each exact output descriptor, preserving the nominal resource meaning. Candidates
retain the actual sampled displays and atlas instances through their original window prefix.
Source and Cooked panels join the existing complete Editor/View/text/failure/close trajectory.

The old GPUI recording mode actually fails the new native midtone oracle: the `#404040` background
reads `[137,137,137,255]` instead of `[64,64,64,255]` when encoded again by Nara's sRGB target.
Selecting the framework's explicit `LinearSrgb` recorder fixes that mismatch without relabeling
imports or changing expected pixels. A separate reviewed dark-Path regression fails with an
8-bit linear intermediate (`#060606` becomes black). Restoring target-matched Path storage
retains the original MSAA/format policy: sRGB hardware storage/sampling preserves SDR precision,
and the final Path shader forwards already decoded associated RGB. The strengthened framework
GPU controls pass on both selected backends.

The unchanged complete Nara child passes on **DX12**
(`1f803269-dba8-4abc-b268-51fb8427f285`) and **Vulkan**
(`c591aaef-55b2-4adb-b043-8c91b22c2eef`). Source and Processed both read the same band pixels:
`[64,64,64,255]`, `[86,63,115,255]`, `[103,65,146,255]`, `[128,64,192,255]`.
Translucent overlay and clipping checks pass at the original and fractional-root scales. Each
backend still completes nine UI submissions, seven discarded candidates, two intentional paint
failures and five diagnostic replays, while another engine producer remains usable after UI
close. Diagnostic scope remains a same-Scene offscreen replay in the original window submission,
not swapchain readback/scanout or the normal rendering path.

The stock owning alpha/publication controls pass **7/7 per backend**, covering nonblack and
alpha-zero camera/Canvas clears, Blend, Opaque partial/zero-alpha source/tint/custom materials,
raw finite-program clear semantics, retained old pixels and detached exact descriptors. A raw
clear test initially chooses a half-byte boundary and observes 127 instead of 128 on DX12;
byte-aligned input retains exact assertions without imposing a device rounding convention.
The renderer's broader library selection passes **405/405**, with 60 ignored native controls
kept separate. The first fixture compile catches explicit submodule path resolution; the normal
locked graph needs only the new `nara_winit` package/edge for logical Window capture, preserving
all previously locked external package versions. Independent review corrects startup's nested
`RefCell` import borrow and removes private batch observation before actual native acceptance.

The final affected Canvas/UI/View/legacy-oracle selection passes **14/14 on DX12**
(`e39ad089-7786-4dd2-9f51-24d610b488c1`); its Canvas/program-UI subset passes **7/7 on Vulkan**
(`c0dc9f7a-67d5-4391-a104-f912069a6204`). The unchanged independent translucent Scene/blur/grade
consumer also passes **1/1 per backend** (`be219276-ef42-4040-a8a0-cbc4740dbd3d` DX12;
`35ca781b-cac7-45aa-9b10-ed2ed946c231` Vulkan). These overlapping selections do not form a new
aggregate framework/engine total. Renderer, framework WGPU and independent engine-consumer
all-target/all-feature correctness Clippy pass. The no-default renderer library and production
Core/WGPU/Web wasm compile pass; root/framework and isolated consumer formatting succeed.
Added local links/fragments, changed manifests/lock and workflow YAML validate. Remote CI,
compiler-floor reruns and unchanged aggregate xtask gates are not claimed by this unit.

Exact final revisions/build/static evidence belong in the foundation plan. This closes the
selected Source/Cooked midtone/alpha prerequisite; device-loss/reconstruction, forced second-content
admission failure, continuous busy-producer retirement, physical input/DPI, broader color/HDR
and numerical budgets, full frontend/game adoption and maintenance handoff remain open.

## Compositor And Cache Research Direction

The user supplied `open-gpui-nara-compositor.md`, v1.0 dated November 12, SHA-256
`cb231c7649ec464e0a3b519b0bfb81ec7d264900c66bb2a19fba08b29424a3d6`.
It explicitly describes design suggestions without inspecting the current repositories. This
comparison uses Nara `7040fe5c25d45b3eaefed134923f7b2b7e316cf3` and local Open-GPUI
`909f510d18bd0d1e537faf73eb57947cee47f927`. The supplied document is background research,
not an Accepted ADR, evidence of implemented features or a replacement execution plan.

Its distinction between dependency/layout reuse, paint/display-list reuse, retained GPU geometry,
raster textures, final composition and a render thread is useful. These reduce different work
and have different validity and lifetime boundaries. An unchanged CPU description can still
produce repeated uploads or GPU rasterization. Retained geometry can avoid upload while still
executing shaders. A raster cache adds texture allocation, rebuild and sampling costs; its final
composition can still touch the whole attachment. Partial texture repaint does not establish
partial swapchain presentation. Neither a separate thread nor fewer draw calls establishes a win.

### Current Source Disposition

Paths in the framework column are relative to the pinned Open-GPUI checkout above.

| Suggested mechanism | Actual source and disposition |
|---|---|
| Dirty/read tracking and cached subtrees | `app.rs::{detect_accessed_entities, record_entities_accessed, notify}`, `window/invalidator.rs` and `view/element.rs::{request_layout_view, prepaint_view, paint_view}` already implement bounded notification and cached View islands. Hits skip internal render/layout and reuse prepaint/paint. This is not a universal field-to-subtree reactive graph. `AnyView::cached` documents the remaining non-View dependency limitation; the owner still explicitly invalidates when required. |
| Cross-frame Taffy nodes | `TaffyLayoutEngine::clear` runs during original candidate commit/refusal. Cached View hits can avoid rebuilding their internal layout while retaining an outer placeholder. Persistent nodes are a possible new optimization, not a prerequisite inferred from the fact that the frame layout arena is cleared. |
| CPU paint/Scene replay | `Window::{reuse_prepaint, reuse_paint}` and `Scene::{replay, finish}` already retain/replay original input structures, primitives and resource leases. Replay/remapping/sorting still cost work; these are not cached pixels. |
| Static GPU upload reuse | `wgpu_renderer.rs::{write_instances, write_instance_binding, prepare_path_draw, encode_frame}` retain buffer capacity/atlas bindings but process and upload primitive data on recording. External recordings stage uploads in the original host encoder. Capacity reuse is not unchanged-content upload reuse. Measure this seam before adding a new GPU Scene owner. |
| Raster layers, damage and scroll tiles | `paint_layer`/`Scene::push_layer_for_render` manage drawing order; `CommittedVisual` retains a CPU Scene, and Path intermediates clear/redraw. They do not establish a general cross-frame subtree raster cache. Glyph/image atlas tiles are distinct from scroll-content tiles. |
| Engine texture/hosting bridge | The actual Nara consumer already shares device/queue/target/encoder, exact immutable output displays, deferred original attempts and submitted input. The supplied P2 bridge is therefore partly completed; full multiple-View/device-loss/cost acceptance remains open. |
| Threaded compositor | Deferred Scene delivery exists, but Scene geometry validity and original submission tokens contain UI-thread `Rc`/`Cell` custody. A clone is neither a Send transport nor submission authority. Threading requires a concrete frozen-data/resource/completion protocol through the original owners if measured owner occupancy justifies it. |

The supplied G1/P1 consequently mixes existing mechanisms with possible new work. Preserve the
official implementation and current fork contracts as the baseline. Consider three bounded
successors according to measured cost, without freezing the proposed module tree or cache enum:

1. **Static upload reuse in the existing WGPU recorder.** An ordinary public application paints
   two cached large panels and an independent spinner, then edits/reorders one item. Compare
   staging allocation, upload bytes and encoding cost; preserve exact pixels and refusal/resource
   recovery. Original GPU rasterization may remain unchanged.
2. **Persistent layout nodes only if layout dominates.** A variable-height list and editable
   panel exercise insertion/deletion, text measurement, sibling notification and continuous
   resize. Keep nodes in the original layout/Window lifecycle, preserve original hit/focus and
   candidate refusal, and measure warm work plus bounded retained-node occupancy.
3. **One explicit raster boundary if GPU work justifies it.** Use an expensive static text/vector
   panel, dynamic content and a popup/input control. Compare direct versus raster behavior,
   total GPU work, rebuild/sampling and payload pressure. Include eviction, scale/font/image
   changes and original submission refusal. Tile scrolling and an independent thread follow only
   if these consumers expose a remaining bottleneck.

Current `with_element_opacity` distributes opacity to primitives. Raster grouping must preserve
that meaning; compositing an isolated group with one final opacity is a distinct explicit
semantic. The same rule applies to complex masks/effects: cache-off must already implement the
same semantics before cache-on can be its optimization. Rounded clips require full clip/transform
facts rather than their AABB. Raster bounds include ink/shadow/filter support, not only layout
bounds. Background-dependent LCD glyph pixels cannot be assumed reusable on a transparent layer.

Use validity facts appropriate to each retained representation. CPU Scene/geometry replay needs
its original atlas leases; a completed independent raster may no longer depend on glyph-slot
residency. Font content/metrics, source Image content, physical scale, working-domain/transfer/
alpha and original device/configuration remain relevant when they change that raster. The
document's large key tuple is a conservative checklist, not a new global generation authority
or a requirement to hash every field for every cache level.

### Engine Composition And Lifetime Constraints

The same pinned Nara source already supplies a finite render program, typed Image ports,
Features and exact resource/display observations. Reuse those owners when an actual offscreen
UI contribution needs graph participation. The UI runtime decides whether semantic content is
reusable; the engine decides physical dependencies, execution and GPU retirement. A proposed
texture-plus-generation handle alone cannot replace source/Host/epoch identity or their leases.
See [program imports](../../crates/nara_render_wgpu/src/program/mod.rs),
[native admission](../../crates/nara_render_wgpu/src/program/native.rs) and
[Feature execution](../../crates/nara_render_wgpu/src/program/feature.rs).

An immutable [output](../../crates/nara_render_wgpu/src/output.rs), its
[display occurrences](../../crates/nara_render_wgpu/src/presented.rs), a composite receipt and a
[submission prefix](../../crates/nara_render_wgpu/src/submission.rs) represent different facts.
Cache, display and in-flight leases may share one allocation without duplicating allocation or
accounting. Eviction ends reuse, while the last original lease and GPU use decide release.
Producer retirement cannot release another submitted sample. Normal prefix completion can be
independent of another producer; abandoned-callback recovery may require `QueueEmpty` on the
original device. Do not promise universal independent completion for exceptional recovery.

Transform/scroll-only reuse also needs common visual and interaction admission. If A/M0 is
displayed while B/M1 is pending, pointer/drag/text/IME/accessibility still use A/M0 until the
original B transaction is admitted. Extend that transaction rather than adding a second hit
tree or changing only inverse pointer coordinates. Existing
[preview redisplay](../../src/project_host/runtime/editor/presentation.rs) requires a distinct
display occurrence when mapping changes and returns a replacement presentation token after
actual redisplay is confirmed. The WGPU Host issues display occurrences.
Current [DisplayMapping2d](../../crates/nara_render/src/presentation.rs)
is rectangular fit/UV mapping, not arbitrary affine or world-space support.

Engine Image-to-UI sampling is qualified above. A UI-layer-to-engine-effect-to-window graph is
a subsequent consumer: intermediate raster completion cannot publish final display authority.
The final receipt must describe the effect output actually sampled and preserve the UI content's
lineage/interaction policy. Existing `record_image_sample` in the
[Host](../../crates/nara_render_wgpu/src/host.rs) observes actual non-fully-clipped sampled use;
it does not prove shader coordinates or invent the final transform. Two interactive Views,
pending transform cancellation/ABA and device-loss reconstruction supply further bounded
consumers when those capabilities are selected.

Thread transport is a separate unimplemented seam. Current framework submission tokens use
`Rc<Cell<_>>`, and recordings retain a non-Send atlas interface; the engine Feature path has
Send requirements. Do not pass these objects through it by asserting Send. A future frozen GPU
packet must retain encoded/upload/in-flight resources and return exact completion to the
original UI owner. Latest-wins replacement may cancel unsubmitted work, not discard already
encoded or submitted custody. No transport API or compositor thread is selected here.

### Measurement And Admission

Start from one current-source native workload with a real changing engine View beside static
UI, plus a change-dense control. Compare the existing direct path, retained geometry if profiling
justifies it, and one explicitly selected expensive raster boundary. Fix hardware, source,
device/format/working domain, physical extent/scale, fonts, accessibility, build mode and warmup.
Keep cache hits, misses, cold/rebuild, eviction pressure and source/geometry changes in the sample
population. Predeclare acceptable CPU/GPU/retained-byte and regression thresholds before the
optimization; the document's reuse break-even equation is a model, not measured acceptance.

- Measure update/description, layout, prepaint and paint separately from Scene finishing,
  staging/upload, GPU raster and final composition. Keep reached work after refusals rather
  than counting only accepted frames as all attempted work.
- Existing Nara Host native-work counters exclude arbitrary encoding callbacks and third-party
  renderer commands, including GPUI recording. Zero in that counter is not zero GPUI upload.
  Attribute work at the actual recorder boundary before adding or trusting a counter.
- A queue-completion interval includes scheduling/backlog and is not GPU duration or scanout.
  Use actual supported timestamps for the measured pass scope; distinguish unsupported or
  unmeasured cells from zero. Estimated rectangle coverage is not actual shaded-pixel work.
- The earlier Dear/Nara kernel benchmark remains its original control. Its timing, warm reuse
  counts and memory cannot be transferred to the current GPUI consumer. Diagnostic offscreen
  replay/readback proves correctness and must stay outside the normal measured path.
- Qualify CPU isolation independently of GPU isolation. A direct/geometry-only renderer may
  legitimately redraw static primitives at every presentation. Require reduced original raster
  work only for the selected raster-cache path, accounting for its composite work and memory.
- Compare cache-off/on pixels with the same group-opacity/effect semantics, original input,
  IME/focus/accessibility and exact accepted publications. A performance fallback must preserve
  those semantics; missing capabilities are explicit rather than silently changed visuals.

The current [work baseline](../benchmarks/2026-11-04-ui-work-baseline.md) and
[output guide](../guides/render-image-outputs.md) own those observation exclusions. No new
performance numbers, cache implementation, public layer API or thread transport are qualified
by this research update.

## GPUI Critique And State Invalidation Research

Reviewed November 12, 2026 against local Open-GPUI
`909f510d18bd0d1e537faf73eb57947cee47f927` and Nara
`7040fe5c25d45b3eaefed134923f7b2b7e316cf3`. The supplied article, **A Critique of GPUI**,
is dated November 8 and was retrieved November 12 with HTTP 200; the downloaded HTML SHA-256 is
`2f7fde2be0925183965271170ee24abd5ba0bc20e43a7a09c8a2fc113c26810e`. This is a source review,
not a new benchmark, runtime qualification or Nara domain-model recommendation. [GCR1]

The article identifies a real cost: ordinary mutable state and entity-wide notification do not
express which field changed or which displayed projection needs rebuilding. Its preferred
run-once Signal architecture is a design position; the supplied experiments do not establish it
as the best choice for Nara. The distinction between Entity granularity, dependency tracking,
retained trees, layout and rendering is necessary: improving one does not automatically improve
the others.

### What The Current Fork Tracks

1. **Writes remain explicit.** `Entity::update` supplies `&mut T`; `Context::notify` sends an
   `EntityId`, without a field identifier or value comparison. Pending notification effects
   coalesce by entity, which does not eliminate equal-value writes. An equality guard can avoid
   notification work, but its semantics belong to the writer or a suitable state API. [GCR2]
2. **Entity access tracking already exists.** Reads and leases record entity identities.
   `detect_accessed_entities` captures scoped accesses; `record_entities_accessed` associates
   them with windows. `App::notify` filters invalidation to currently tracking windows, while
   cached views restore their recorded accesses on reuse, including paint dependencies.
   Calling this entirely untracked or indiscriminate application-wide invalidation would be
   inaccurate. It is not field-to-element dependency tracking. [GCR3] [GCR4]
3. **Window invalidation is weaker than projection invalidation.** A notified View identity
   dirties its rendered ancestor path. A non-View model read can subscribe a window without
   making that model's notification dirty the cached View that read it. The existing
   `cached_canvas_nested_window_reads_remain_window_local_on_replay` regression explicitly
   expects the correct window to redraw while its cached content stays unchanged until refresh.
   Notify the owning View, connect an observer/event to that notification, or refresh when
   rebuilding is required. Read collection alone does not close this edge. [GCR3] [GCR4]

The `notify`/zone.js analogy therefore describes uncertainty below the notified entity; it should
not erase GPUI's entity identity, live-window routing and explicit observation mechanisms.
Conversely, those mechanisms do not establish automatic selective invalidation for arbitrary
reads from a shared model. Missing notifications can produce stale cached output; a later unrelated
redraw is not a correctness contract. [GCR1] [GCR2] [GCR4]

### Retention, Layout And Presentation Are Separate

| Concern | Current-source distinction and consequence |
|---|---|
| Retained tree | Entities, scoped element state, dispatch structures and cached prepaint/paint records survive appropriate frames; ordinary declarative Element objects are rebuilt. This is different from retaining every layout/Element node and updating individual bindings. `RenderOnce` consumes a component recipe each invocation; its name does not promise one execution per mount. [GCR4] [GCR5] |
| Cached View | Caching is explicit through `.cached(style)` and requires a definite outer size independent of contents. Active accessibility or inspector picking bypasses it. Reuse also depends on bounds, inherited text/style, clips, opacity, presentation, transforms, resource validity and refresh state. This explains real layout/behavior restrictions; it does not establish how frequently applications use caching. [GCR4] |
| Incremental layout | The wrapper creates Taffy nodes and clears its tree on candidate completion/refusal. A cached View requests an outer placeholder; a hit skips its internal render/layout and replays prepaint/paint, while a miss lays out the internal subtree as a root. Clearing this arena does not prove every cached subtree is relaid out. It also says nothing universal about Taffy's own incremental capabilities or another framework's integration. [GCR4] [GCR6] |
| Render caching | CPU Scene replay still copies/remaps primitives and finishes ordering. The WGPU recorder writes instance/path data and encodes drawing for the supplied Scene. A View cache hit therefore does not prove unchanged upload or raster work was skipped. The preceding [compositor study](#compositor-and-cache-research-direction) owns the fuller distinction; no general subtree raster or scroll-tile cache follows from these mechanisms. [GCR7] |
| Custom nodes | `Element` exposes layout, prepaint, paint and accessibility participation; `canvas` requests an outer layout and invokes custom callbacks. A specialized editor/canvas can replace thousands of generic descendants with its own layout/drawing. It still participates in framework execution and invalidation. This is a supported escape hatch with author-owned responsibilities, not evidence that all GPUI semantics are bypassed or that arbitrary rich controls become cheap. [GCR8] |
| Docking and multiple views | Docking retains stable panel handles/lazy factories and selects visible panel content; it also explicitly observes controller/surface-owner notifications. This proves composition and lifetime mechanisms, not field-level dependencies or isolated rendering cost. Entity tracking can route to multiple current windows; independent occurrences still require suitable scoped identities and window-local interaction/geometry. Multiple views of shared data neither require one giant invalidation boundary nor automatically acquire precise projection tracking. [GCR3] [GCR5] [GCR9] |

### Ergonomics And The Run-Once Alternative

The article's badge example is a plausible way to filter a broad model notification: retain a
projection, compare it, and notify a smaller View only when that projection changes. The stored
scalar, guard and subscription demonstrate the burden of selective projection management. They
are not mandatory boilerplate for every label: a View can read an Entity directly, own local
state, or observe typed events. Direct reading still needs correct invalidation, and typed events
require correct publication; neither infers field dependencies from ordinary Rust. [GCR1] [GCR2]

Two current-fork interfaces narrow the article's API generalizations. `View` supports a consumed
wrapper combining parent-supplied props with an existing backing entity; parents need not always
mutate child entities merely to pass render arguments. `Window::use_keyed_state` can retain keyed
state across consecutive rendered frames, so a handwritten entity map is not the only local-state
tool. Its observer notifies the containing View, and disappearance ends that continuity; it is not
a general persistent keyed collection or a finer binding boundary. `RenderOnce` remains without
an independent entity identity. None of these interfaces supplies automatic prop equality or
field-level selective recomputation. [GCR5]

Run-once signals make another contract explicit: reactive labels, conditionals and collections
must be expressed through tracked closures/dynamic containers rather than ordinary one-time reads
and top-level `if`/`for`. Dependency setup, retained nodes, disposal, captures and effect ordering
have costs and failure modes. The author acknowledges creation/memory/effect costs and that coarse
signals still perform poorly. Adding signals that merely notify existing GPUI Views can improve
state authoring, but cannot by itself turn ephemeral Elements into persistent per-binding sinks.
That stronger behavior needs an explicit execution/Element contract and lifecycle. [GCR1] [GCR5]

### What The Benchmarks Establish

The Method names GPUI `ddc9eda925`, Longbridge gpui-fast `86a8228229` and the author's zgui
`082337db`; none is this Open-GPUI revision. It reports medians of apply/settle, selecting the best
of three passes on one i9-13900K/RTX 3080 Ti Linux/Vulkan machine, excluding compositor and input
costs and supplying no Windows execution evidence. The harness is unpublished and maintainers
have not reviewed the adapters. The author discloses that zgui was tuned on this harness. These are useful sparse
update observations under the author's adapters, not Nara measurements or native latency. [GCR1]

The marked correctness failures are Floem label/keystroke and Slint font-size cells; they cannot
serve as equivalent correct-output wins. Other cells passed the author's oracle, without an
independent reproduction here. The 60-panel experiment uses gpui-fast's modified `retained_bench`
in test mode with no GPU work, and retention-off is still gpui-fast. Quoted Zed retention results
come from experimental branches/plans, separately from the article's default-GPUI measurements.
Neither experiment establishes that those retained engines are present in this fork. [GCR1]

A reproducible fork comparison still needs the harness/adapters, complete source/dependency and
toolchain/build pins, sampling/warmup protocol, phase boundaries, and matched layout, font,
accessibility and virtualization settings. The article does not establish those controls for
this consumer. Its own discussion of expensive layout/engine walks under fine tracking supports
separating notification precision from downstream execution, rather than selecting signals from
charts alone. [GCR1]

For follow-up evidence, keep three questions distinct: **correctness**, whether every relevant
change invalidates stale output; **efficacy**, which description/layout/replay/upload/raster work
actually disappears; and **authoring cost**, how much identity, subscription and lifetime code
the application must maintain. Explicit broad refresh can satisfy correctness while failing the
efficacy goal. A precise signal graph can still leave expensive layout or rendering. Sparse,
equal-value and shared-model changes plus structural/change-dense controls are useful cases for
testing those claims; this research supplies no new performance acceptance result.

### Sources For This Section

All framework paths below refer to Open-GPUI `909f510d18bd0d1e537faf73eb57947cee47f927`.
Test references describe inspected expectations, not tests rerun for this documentation update.

- [GCR1] [A Critique of GPUI](https://blog.zortax.de/posts/a-critique-of-gpui), especially [GPUI](https://blog.zortax.de/posts/a-critique-of-gpui#gpui-notify-is-zonejs), [precision](https://blog.zortax.de/posts/a-critique-of-gpui#making-gpui-precise-defeats-gpui), [signal limits](https://blog.zortax.de/posts/a-critique-of-gpui#signals-do-not-automatically-make-your-app-fast) and [Method](https://blog.zortax.de/posts/a-critique-of-gpui#method). Its references identify experimental Zed `312a04c83`/`c15d4b8e4b`; their timing claims are attributed, not independently reproduced here.
- [GCR2] `crates/gpui/src/app/context.rs:62-118,229-232`; `app.rs:2503-2518,2664-2670,3815-3849,3943-3962`: observation/events, explicit notification, effect coalescing and mutable updates.
- [GCR3] `crates/gpui/src/app/entity_map.rs:230-264,279-335`; `app.rs:1508-1545,3815-3849`; `window.rs:4044-4055,8479-8496`: access scopes, current-window subscriptions and View ancestor invalidation.
- [GCR4] `crates/gpui/src/view.rs:17-33,54-66,255-263`; `view/element.rs:217-365`; `window/cache_dependency_tests.rs:273-335,550-658`: cache admission/replay, retained accesses and non-View notification limits.
- [GCR5] `crates/gpui/src/element.rs:8-17,168-191`; `view.rs:216-263`; `window.rs:11068-11125`; `crates/gpui_macros/src/derive_into_element.rs:10-25`: transient descriptions, RenderOnce, props with identity, keyed state and embedding.
- [GCR6] `crates/gpui/src/taffy.rs:31-107,183-249`; `window.rs:7724-7744,8119-8120`: node creation/computation and candidate-lifetime clearing; compare GCR4's cached-layout bypass.
- [GCR7] `crates/gpui/src/scene.rs:531-594`; `window.rs:9485-9528,9723-9783`; `crates/gpui_wgpu/src/wgpu_renderer.rs:1918-1950,2044-2080,2464-2508`: CPU reuse versus per-recording instance preparation/upload.
- [GCR8] `crates/gpui/src/element.rs:19-32,50-149`; `elements/canvas.rs:8-90`: custom layout, drawing, semantics and the canvas outer-layout contract.
- [GCR9] `crates/gpui_docking/src/panel_view.rs:8-126`; `host.rs:923-935`; `host_render_session.rs:166-205`; `render_tabs.rs:397-442`: panel identity/lazy lifetime, explicit observers and selected-content composition.

## Implemented Shared Model Editor Projections

This first implementation of the state-locality direction extends the same independently locked
[native engine consumer](../../tests/fixtures/gpui-engine-consumer/README.md). It consumes the
framework runtime qualified at `909f510d18bd0d1e537faf73eb57947cee47f927`; later Open-GPUI
`92ffbf5d815c4962ddbd68d518133bb5342dc03d` adds the public CPU locality fixture and its CI/docs,
with no production framework mechanism, manifest or lock change. This is fork-consumer evidence,
not new official-source adoption or full Editor migration.

The [engine adapter](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/engine.rs) runs
the existing bounded selection-independent entity query and original property read. One immutable
observation becomes a shared UI Entity. The
[panel implementation](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/panels.rs)
retains one source-declaration Hierarchy and two Inspector entities with independent draft,
preedit and conflict state. Their existing subscriptions compare each relevant projection;
visual equality does not replace current operation-basis validation. Full entity/source identity
keys row state. One shared row-identity snapshot avoids cloning the entire list per row; this
finite consumer is not a virtualized collection or a total O(visible) work claim.

The original Session prepares/commits X edits and owns Undo/Redo. An invalid draft queues no
mutation. A changed source value updates clean drafts, while another panel's dirty draft remains
and refuses Apply until explicit Reload. A changed source/selection binding resets its local
field under the selected policy. Window-aware observation and Reload issue the original native
text content/selection notifications immediately. Source removal is an explicit absent projection,
so Undo of initial Sprite construction keeps the Viewer usable for Redo. This does not qualify
every source/schema/preedit replacement path, continuous property gestures or generic field types.

Early native execution identifies actual fixture/integration mistakes. The Hierarchy click
first misses its row; fixed coordinates then exercise the real row handler. Review separates
focus activation from text injection with an actual accepted frame, and moves programmatic
text notifications into the original Window rather than relying on repaint. Locality assertions
also expose forced refresh in the existing adapter. Scene images now have a separate definite-size
cached View, ordinary source changes notify that owner and request non-forced demand, and panel
projections retain their own notification boundaries. Forced recovery/geometry refreshes still
rebuild as required. A separate command-path source edit isolates locality from pointer/focus
changes; it does not stand in for the earlier real Inspector click/IME/Apply workflow.

The public framework fixture passes **5/5**
(`2ae1f4fa-8e62-4817-92b3-7940c5abd6b2`), using real CPU callback publication, shared Entity
observations, broad-refresh versus narrow-projection render/paint controls, equal visual values
with a changed operation basis, changed constraints/source, entity/subscription release and late
result rejection. Test-platform Submitted facts are not GPU/OS execution.

The final complete Nara child passes **1/1 on DX12**
(`66e31aef-c76b-4972-83b5-c71f656f99d9`) and **1/1 on Vulkan**
(`2b8e95b9-fc9c-4991-945b-17e035cf0dc5`). Both select/report RTX 4090 and the original
`Bgra8UnormSrgb` Host attachment. The native input path selects the source row, edits independent
Inspector drafts, refuses invalid/conflicted writes, Reloads, commits through the Session and
Undo/Redo restores values/absence. Isolated source publication reuses unchanged Hierarchy rendering.
The original pixel, Source/Cooked alpha/color, long draft/IME, submitted input, refusal/panic,
geometry, fractional-scale and close/other-producer assertions remain in that one trajectory.
Each backend observes 20 actual UI submissions, ten discarded candidates, two intentional paint
failures and five diagnostic replays; the diagnostic remains same-Scene offscreen work in the
original submission, outside the normal rendering/performance path.

Independent reviews close focus/text-notification and absent-Sprite findings. Focused framework
and independent-consumer all-target/all-feature correctness Clippy pass; existing warning policy
is retained. Root/framework and isolated-consumer format, document links/fragments and workflow
YAML checks pass. No Nara product default/dependency switch, additional GPU or command owner,
new Signal runtime, persistent layout, upload/raster cache, thread transport, physical input or
numeric budget follows. R11a/e still require phase attribution, current-source allocation/
retention/upload/GPU/latency costs and broader sparse/structural/virtualized consumers.

## Implemented Variable-Height Source Collections

This successor starts from Nara `c66a8b049367fd28c346ee570f3f6f843cd6e1eb` and reuses the
unchanged production framework at Open-GPUI `92ffbf5d815c4962ddbd68d518133bb5342dc03d`.
The public collection fixture and its qualification/CI are committed separately at local
Open-GPUI `f75c10b2a9bf7b4dcb2c8c2aa446203997601369`, with no production mechanism or GPU
dependency change.
The independently locked consumer's temporary Scene contains 10,000 real empty source records
plus its constructed Sprite. The
[source adapter](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/engine.rs) follows
every bounded entity-query continuation under the same original basis, reports query pages and
scanned records, and returns one immutable identity observation. Pinning the Sprite first is an
explicit fixture display policy; all other declarations retain source order. This is a finite
source-declaration list, not effective Prefab topology or a new general Hierarchy contract.

The extracted [Hierarchy module](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/hierarchy.rs)
owns filtering, original `ListState` and its consumer-side key reconciliation. Empty filtering
uses the source indices directly; a committed nonempty filter scans the complete observation
and retains matching indices. The first changed key span is spliced through ListState, while a
surviving key retains its absolute intra-row offset across filtering/structural replacement.
If that key disappears, this native consumer falls back to the first row with zero offset.
The public framework fixture separately exercises forward-then-backward fallback as a different
consumer policy. Source/Schema incarnation replacement resets the list/filter binding. Details
changes row heights with original proportional remeasurement. No second layout or height index
is implemented, and this finite module does not add a universal keyed-list interface.

The native [complete trajectory](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/smoke.rs)
checks complete source paging, bounded startup/deep-scroll row construction, key/offset retention,
real filter text input, row-height changes, exact source selection and restoration of the Sprite.
PixelDelta/LineDelta enter the ordinary Winit event conversion; at logical scale 1.25, 6.25
physical delta pixels become five logical pixels. This is injected input and logical-root scaling,
not physical monitor DPI migration or a wheel/trackpad latency claim. Original Source/Cooked
pixels, color/alpha, Inspector draft/conflict/Undo/Redo, long text/IME, deferred input, paint
refusal/panic, resize/ABA/zero extent and pending close/other-producer retirement remain in the
same native trajectory. No native aggregate or unrelated official implementation is requalified.

Early native attempts fail on row selection despite valid list bounds. MouseDown reaches the
row but Click does not: the Filter field's intrinsic height overflows its 30px wrapper and takes
pointer capture inside the first row. Correcting the recipe to header 24px, field 48px and list
58px, with explicit nonshrinking allocation, removes the overlap. The unchanged full trajectory
then passes (`ab8a9c57-ae23-4123-9dea-3d0d3236d1bf`). A new ordinary List-row Click control also
passes without an intervening frame, so no GPUI production workaround is introduced. Independent
review identifies that the initial wheel probe bypasses Winit conversion; the final trajectory
replaces it with actual Winit PixelDelta/LineDelta and fractional-scale delta assertions.

Final native qualification passes **1/1 on DX12**
(`7bfccc3e-d4c9-4326-8cb9-6a1c00704c28`) and **1/1 on Vulkan**
(`6cb98443-f88f-4f6b-8198-6d24c19c6ea1`). Both report RTX 4090, original `Bgra8UnormSrgb`,
32 UI submissions, eleven discarded candidates, two intentional paint failures and five
correctness-only same-Scene diagnostic replays. At the named collection checkpoint, both observe
10,001 source rows, 86 row callbacks, 180 query pages/180,016 scanned records, 10,001 filter visits
and 40,021 reconciliation checks. Later scale/recovery work is outside that checkpoint count.

The public framework collection fixture supplies five CPU controls, combined with the five
previous state-locality controls for **10/10**
(`72777591-cc96-4d43-b0c5-17d6b8d5b3f8`). It covers 10k/100k logical rows, wrapped/variable
heights, clipped input/bounds, insertion/deletion/reversal, offscreen same-key content/width/font
remeasurement, stale pointer/keyboard/source publications and release-time Click. Its 96px
viewport/16px minimum row recipe bounds callbacks at 14 in those cases; this is CPU callback/
publication evidence, not GPU execution or an engine-authoring throughput result.

Independent bounded reviews find no remaining P1/P2. Framework-fixture correctness Clippy and
independent engine-consumer all-target/all-feature correctness Clippy pass. Formatting and
document/workflow checks retain their final recorded scope. Source, filter/key metadata and
ListState remain O(logical rows); equality/string comparisons, snapshot allocation, framework
traversal, upload/GPU work and old-frame retention lie outside the added counters. No measured
latency, heap/leak bound, general O(visible) cost, cache/persistent-layout implementation or
complete Editor/game/default/handoff acceptance follows. Those R11a/e requirements remain open.

## Cost Observation Interfaces And Limits

Open-GPUI source pin: `f75c10b2a9bf7b4dcb2c8c2aa446203997601369`; the official Zed baseline already exports its optional profiler/frame collector. [CO1]

### Public Capture In An Ordinary Hosted Consumer

- Enable `open_gpui`'s `profiler` dependency feature and gate consumer profiling code accordingly.
  Observe the original Window without `test-support`, a benchmark App or a second profiler. [CO1] [CO2]
- Before the workload, call `open_gpui::profiler::set_trace_enabled(true)` and create
  `FrameTimingCollector::new()`; periodically call `collect_unseen()`. Read `FrameEvent::Draw(t)`'s
  `phases`, `draw_duration()` and `invalidations`, or `FrameEvent::Present(p)`'s `present_duration()`;
  filter both by the original `window_id`.
  The process-global ring retains at most `16 MiB / size_of::<FrameEvent>()` entries; each
  collector starts at creation and copies unseen entries independently. Eviction is silent.
  Drain before disabling tracing: the final trace owner clears task/frame buffers. Preserve
  the prior manual trace setting; it is process-global. [CO2]
- On the actual App, call `cx.foreground_journal().collector()`, then `collect_unseen()` and
  inspect `ForegroundJournalEntry::Event(ForegroundEvent::View(t))`. This profiler-feature
  journal is independent of manual tracing; its fixed ring targets 4 MiB. Preserve
  `DrainedEntries::lost`/`Discontinuity`; retain an `IntervalSealer::new(start)` and feed its
  `push_entries(...)` successive drains for interval analysis. [CO3]
- Call `window.frame_duration_snapshot()` and `window.input_latency_snapshot()` for per-window
  histograms (nanoseconds) and outcome/refresh counters without manual tracing.
  `reset_input_latency_measurements()` resets input diagnostics, not frame-duration histograms. [CO2]

### Attribution And Thresholds

- `request_layout` includes View rendering and layout requests; root layout computation runs in
  `prepaint`. Cached Views request an outer placeholder; misses render/lay out their subtree in
  prepaint, while hits reuse prepaint/paint data. Neither phase measures pure layout or render. [CO4]
- Window phase intervals are sequential portions of drawing; their sum does not cover all draw
  bookkeeping or the external recorder. View `end - start` spans nest within those intervals;
  `self_duration` subtracts nested View scopes. Do not add View spans to Window phases, or sum
  task/input/action/draw spans as independent CPU work. `FrameSnapshot::occupancy()` unions
  recorded spans; folded short polls make its result an approximation. [CO3] [CO4]
- `VIEW_TIMING_FLOOR` is 500 microseconds of **self** time per phase. A missing View event can
  mean below-threshold work, cache reuse, lost data or interrupted sampling; it never proves zero.
  View events name a type, not an Entity/row identity. Task polls below 100 microseconds are folded.
  Measure smaller render/projection/list operations with scoped consumer counters/timers. [CO3] [CO4]
- `FrameTiming::budget()` uses the known refresh interval with an 8.333333 ms floor, or
  16.666667 ms if unknown; `work()` requires a frame signal. These classification rules do not set
  a consumer target. Draw events cover accepted frames; account for refused/unwound work separately. [CO2] [CO4]
- Deferred Window acceptance publishes the prepared draw, and submission acknowledgement uses
  equal original `settled_at` timestamps. Its zero `present_duration()` is an unmeasured renderer
  interval. Keep host recording/submission timers separately; neither Present timing nor
  input-to-submission histograms establish GPU execution duration or scanout. [CO2] [CO5]

### Allocator And Benchmark Scope

- Independently depend on `open-gpui-bench-metrics` (`open_gpui_bench_metrics` in Rust); install
  its `CountingAllocator::new(the_shipped_allocator)` as the binary's single global allocator.
  `allocation_stats()` returns optional process totals. Before/after `allocations`, `allocated_bytes`,
  `freed_bytes` and `live_bytes()` differences include every thread using that wrapper. Reallocation
  counts as old free/new allocation; sharded relaxed reads are not an atomic concurrent snapshot. [CO6]
- `AllocationCounter::new(AllocationMetric::Allocations, MetricScope::CallingThread)` and
  `AllocatedBytes`, measured through Criterion's `Measurement::start/end`, isolate that thread's
  allocation churn; thread-local frees are not tracked. Requested/live-byte deltas do not establish
  peak/RSS, per-View/cache retention, leaks or allocations bypassing the wrapper, including GPU memory.
  Keep allocating snapshot/trace/reporting work outside the named interval; label background noise. [CO6]
- `bench-support` enables the GPUI Criterion harness and profiler; `bench_main!` installs the
  counting allocator unless `allocator = none`. `BenchMeasurement::from_env()` supports
  `BENCH_MEASUREMENT=allocations` / `foreground-allocations`; default mode adds available secondaries,
  `wall-time` disables them. Unavailable explicit primaries fail; Windows hardware/getrusage
  counters are unavailable, not zero. [CO1] [CO6] [CO7]
- `BenchAppContext::{bench_renderer,bench_renderer_session}` run unpaced work, not native latency.
  `bench_platform(None, text_system)` discards Scenes; the current Windows default has no renderer.
  Name an injected GPU renderer; CPU benchmarks do not qualify the ordinary hosted GPU path. [CO7]

### GPU And Host Limits

`WgpuSceneRenderer::record(...)` stages instance/path data and encodes copies/draws in the Host
encoder; its `WgpuSceneRecording` retains resources, not upload-byte or GPU-time statistics.
Nara's `frame_transaction_stats().native_work()` excludes direct callbacks/third-party commands,
including this GPUI recorder: zero there is not zero GPUI upload. `submission_prefix().poll()`
establishes resource settlement. Wall time to that observation includes host/queue/polling delay;
it is not GPU duration. Use a supported timestamp scope for GPU pass time; absent upload,
GPU-time and retained-memory observations remain **unmeasured**. [CO8] [CO9]

Source map: `CO2` through `CO8` use sibling paths at the Open-GPUI revision above;
`CO9` is Nara `64946fee8211884e13083a6d0d9a576bc1ef0ee6`. No Cargo/native measurements were run.

- [CO1] Open-GPUI `crates/gpui/{Cargo.toml:21-45,src/gpui.rs:61-62,154}`; official Zed `635026fabc38440fce5e1767d125026a17b12012`, `crates/gpui/{Cargo.toml:49,src/gpui.rs:25,138,src/profiler.rs:1582-1654}`.
- [CO2] `crates/gpui/src/{profiler.rs:835-873,918-1144,2038-2042,2144-2191,2220-2275,2378-2450,window.rs:8728-8744}`; `tests/fixtures/dependency-consumer/tests/profiler.rs:15-69` (inspected, not rerun).
- [CO3] `crates/gpui/src/{app.rs:2855-2860,profiler/journal.rs:36-69,71-94,301-420,670-684,781-801,1106-1144,1399-1528}`.
- [CO4] `crates/gpui/src/{window.rs:7794-7846,8772-8787,8822-8834,8884-8923,view/element.rs:92-211,217-350,profiler.rs:999-1021,1735-1779}`.
- [CO5] `crates/gpui/src/{window/deferred_frame.rs:380-392,438-451,profiler.rs:2152-2158}`.
- [CO6] `crates/bench_metrics/{Cargo.toml:1-18,src/bench_metrics.rs:44-54,188-204,832-861,899-965,1003-1202,1233-1293,1404-1444}`.
- [CO7] `crates/gpui/src/{bench.rs:35-64,app/bench_context.rs:31-69,776-791,832-855}`.
- [CO8] `crates/gpui_wgpu/src/{wgpu_renderer/scene_renderer.rs:4-21,158-209,wgpu_renderer.rs:1733-1775,1918-1950,2464-2508}`.
- [CO9] `crates/nara_render_wgpu/src/{work.rs:1-30,host.rs:953-990,telemetry.rs:181-192,573-588}`; `tests/fixtures/gpui-engine-consumer/examples/gpui_view/renderer.rs:264-280,347-364`.

## Implemented Hosted UI Cost Observation

The November 12 successor adds opt-in observation to the same independently locked Nara
consumer, starting at `64946fee8211884e13083a6d0d9a576bc1ef0ee6`, while maintained Open-GPUI
stays at `f75c10b2a9bf7b4dcb2c8c2aa446203997601369`. No production framework mechanism, domain
owner or default frontend changes. The existing public `FrameTimingCollector` supplies accepted
draw phases; original renderer timing stays local to the existing Host contribution. A second
optional mode uses the already maintained `CountingAllocator` and calling-thread counters.
The [cost record](../benchmarks/2026-11-04-ui-work-baseline.md#hosted-gpui-source-and-frame-cost-probe)
owns exact phase meanings, build/hardware/geometry, distributions, work counts, initial failure
and remaining unmeasured scope. The consumer README owns commands.
The local implementation commit is `c11e131b5dc705ca73c68221e4716c7fa966805d`.

Five cases each keep eight warmups and 64 samples over the real 10,001-record source: equal
observation, static panels beside refreshed Scene output, an original property edit, list-height
remeasurement and the same edit with explicit broad refresh. They distinguish mutation/drive,
complete query/snapshot, projection publication, preview capture/import, hosted settlement,
GPUI draw phases and nested recorder work. They never substitute a CPU-only test platform for
the original native GPU Host or add qualification readbacks to timed samples.

Both optimized DX12 and Vulkan complete the new probe and original visual/recovery workflow,
2/2 each. Local edits rebuild both changed Inspectors but no Hierarchy rows; broad refresh also
rebuilds Hierarchy and visible rows. Equal observations draw nothing but still query and compare
the complete observation. Across 64 samples, local edit queries scan 1,280,128 records because
the internal selection check and explicit query each enumerate the source. This is not an
idle-per-display-frame scan: the interactive consumer publishes panels after commands.

The combined source mutation/drive P50 is 330.441 ms on DX12 and 326.777 ms on Vulkan; GPUI
request-layout/prepaint/paint and recorder work are much smaller in this finite fixture.
The mutation interval also reaches source preparation/validation/history, Editor frame drive,
preview compilation/Runtime and Editor model publication. It cannot identify their individual
contribution. Thus the next measurement seam is that original path, before selecting a new
Signal system, persistent layout arena or raster cache. The evidence does not rank general
toolkits, qualify Shader Graph/Timeline workloads or close the R11a/e cost matrix.

The separate optimized allocation-enabled DX12 run also completes probe plus original smoke,
2/2. Complete explicit query/snapshot requests 10,030 allocations and 1,342,250 bytes per
observation; the combined local mutation/drive averages approximately 1.236 million requests
and 200.913 million requested bytes per edit. These are allocation churn, not live heap or
per-View memory. Process gauges include history, traces and background activity. Original native
candidate occupancy reaches zero at retirement. Full GPU/upload/heap and numeric-budget
qualification stays open; these observations strengthen attribution of the original source path.

## Measured Source Drive And Hierarchy Batch Locality

Local implementation: attribution `9715832a0b701beacb90f274a1eb1680fb600d73`, indexed
hierarchy/Scene `f1330617bc1c1d4ab41383b52bb547bb19cd6ea5`.

The next attribution keeps maintained Open-GPUI unchanged and subdivides Nara's original
property operation. The first four-scope optimized DX12 probe passes 1/1 and identifies frame
drive rather than selection/preparation/commit as the largest interval. Optional standard
`tracing` spans then locate the dominant reached subinterval in Scene materialization:
223.628/243.931 ms P50/P95, compared with source compilation 80.251/84.762 ms and Editor model
publication 1.189/1.755 ms. The [attribution record](../benchmarks/2026-11-04-ui-work-baseline.md#original-source-and-editor-drive-attribution)
owns exact runs, inclusive timing and observer limits. This evidence supersedes attributing
the previous combined ~330 ms interval to source preparation alone.

Source inspection finds quadratic repeated sibling membership, anchor lookup, removal and
sequence movement in the original runtime hierarchy candidate/commit path. The complete
10,001-root source exercises replacement and retirement, so root membership matters as well as
initial append. The selected owning refactor uses private indexed candidate links only for
touched sibling sets, then materializes final ordered projections through the same prepared
batch. The original root owner maintains complete-Entity membership beside its ordered slice;
Scene uses one retirement set when selecting the replacement occurrence. No GPUI mechanism,
domain command, Runtime or identity owner is replaced.

ADRs0100/0101 retain ordered runtime authority, full sequential validation, exact anchors,
failure atomicity and the prepared no-flush/no-hierarchy-mutation interval. Lifecycle admission
still visits every reached operation target even when final values cancel. A nonempty batch
invalidates completion and advances the original epoch; unchanged final parent values need no
synthetic remove/reinsert. The new private links are preparation data, not linked entity lifetime
or a second structural relation. Root-index storage and transient touched-sibling state remain
explicit costs. Public slices, cycles, independent/unmanaged state and exact source retirement
remain mandatory acceptance.

The predeclared selected-consumer target is materialization P95 below 150 ms for both edit
controls, with unchanged source/render work counts and complete original native recovery/input/
retirement workflow. This is a local optimization target, not a general Editor frame SLA.
Source compilation, source preparation and remaining whole-source work need their own evidence;
the UI state/layout/compositor choices stay independent of this materialization correction.

The indexed replacement's full optimized smoke/probe passes on DX12 and Vulkan, 2/2 each, and
in a separate DX12 allocation build, 2/2. Both edit controls meet the declared materialization
P95 below 150 ms: DX12 104.706/94.851 ms, Vulkan 90.708/109.762 ms, with identical query,
projection, View/row, draw/record/submission counts and exact native retirement. The traced
DX12 local-edit baseline was 243.931 ms P95. Complete hierarchy/Scene/Transform coverage and
selected storage/large-root tests retain their separately reported scope in the cost record.
No general engine SLA or incremental-source claim follows.

Allocation churn remains high, approximately 202.379 MB requested per local edit. New deep
observer coverage and indexing both differ from the preceding allocation run; absolute
outer-source steps identify remaining drive/preparation churn without attributing all delta
to the index. Process gauges are not native/peak/cache memory. Preview compilation and remaining
materialization are still roughly 80/85 ms P50, so the next owning attribution stays in Nara.
This correction changes no production Open-GPUI mechanism or selected Editor/game default.

The subsequent [Scene compilation/publication attribution](../benchmarks/2026-11-04-ui-work-baseline.md#scene-compilation-and-publication-attribution)
at `a55043b3b32ff702e579d948807095f217d8e34e` extends the same default-off standard tracing,
without installing an engine subscriber. Both optimized DX12/Vulkan smoke/probe pairs pass 2/2;
the seventeen reached intervals and original five-workload source/render counts are verified.
Structural closure is about 32.7 ms P50 within approximately 80 ms preview compilation. The
remaining approximately 85 ms materialization is distributed across preparation, binding and
publication; the latter two include their necessary revalidation. These are inclusive elapsed
observations, not exclusive CPU time, new allocation data or GPU measurements.

The selected next owner change reuses an already-built unpublished structure when the complete
pending instance-selection list succeeds without adding any scope. Real instance entry still
rebuilds; subsequent reference-demand retries and final entity/working limits remain. Removing
actual scratch work may admit a source at a formerly failing tight working-byte boundary; the
budget policy remains, without retaining phantom charges. The predeclared consumer target is
compilation P95 below 80 ms for both source-edit cases on both selected backends. This bounded
compiler change leaves GPUI state/layout/raster choices, receiving-World lifecycle checks and
the independent Editor/game/runtime decision intact.

The resulting [stable-structure qualification](../benchmarks/2026-11-04-ui-work-baseline.md#stable-structural-closure-qualification)
at `969c6f52db2` passes Scene library/public 1178/1178, independent Scene 21/21, and original
DX12/Vulkan smoke/probe pairs 2/2 each. With identical workloads and observer, structural P50 is about
17 ms instead of 32.7 ms; compilation P95 meets the predeclared below-80-ms target in all
four backend/edit cells. Source/render/call vectors remain identical, with exact native
retirement. Materialization remains about 85 ms P50 and full owner samples about 178 ms;
no Editor latency SLA, allocation/memory or GPU/cache result follows. This is a measured
Nara compiler correction, not GPUI incremental UI layout or a Signal architecture result.

The [materialization owner attribution](../benchmarks/2026-11-04-ui-work-baseline.md#materialization-owner-attribution)
at `9d1e667c3da159f885e6b7268e138b74214bd042` qualifies 25 optional standard intervals through
the same optimized native consumer, with both backend smoke/probe pairs passing 2/2. Repeated
predecessor/identity preparations and retirement hierarchy are counted explicitly; timings
for repeated intervals are per-sample inclusive sums. Decode/value admission is about 11.7 ms
P50; identity adoption is about 1.9 ms. The source review does not select a new adoption API.

The next selected reuse belongs to the existing compiled-document/validation owner. Its sealed
immutable rooted forest and ID-sorted entities can serve receiving placement/membership without
rebuilding duplicate indexes. Receiving Native Visibility projection eligibility and lowest-ID
child diagnostics remain, as do definitions, values/limits, references/assets, actual codecs and
all target-World admission. Raw drafts/claims continue complete topology validation. The
predeclared decode P95 target is below 5 ms in both cases/backends; original source/render
vectors and complete smoke remain acceptance. This does not select a UI reactive/layout/cache
mechanism, alter the separate exact Runtime-definition witness or close Editor/game budgets.

The resulting [compiled-topology receiving reuse](../benchmarks/2026-11-04-ui-work-baseline.md#compiled-topology-receiving-reuse)
at `9b1376f75b5` passes full Scene library/public 1180/1180, independent Scene 21/21 and unchanged
DX12/Vulkan smoke/probe pairs 2/2 each. All four backend/edit decode P95 observations fall below 0.7 ms against
the matched 25-span baseline's 12.3–14.3 ms, with complete source/render/call vectors unchanged.
Materialization P50 is about 75 ms and entire owner P50 about 167/168 ms. Whole-owner local
P95 increases; unchanged compiler tails also exceed the earlier 80-ms observation. The report
preserves those values and source/observer limits instead of claiming a stable Editor latency
budget. Sealed structural reuse adds no retained index, broad pure-validation exemption or
new identity owner. UI state/layout/render, full Editor/game selection and remaining budgets
stay independent and open.
