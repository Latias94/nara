# Rendering And Custom-Drawn Editor UI: Implementation Boundaries

Date: 2026-10-30. Scope: rendering, materials, images, windows, and the future Nara-owned Editor UI.

This is a source review, not implementation or native/GPU qualification. It preserves the user's
selected **finite resource-dependency graph** direction. Declaration order must not determine
dependencies; deterministic serial topological execution remains a valid initial implementation.
The earlier ordered-only recommendation is not reinstated.

The [October 29 render review](../2026-10-29-render-architecture-and-refactor-plan-review.md) and
[October 27 custom-UI study](../2026-10-27-custom-drawn-editor-ui-boundaries.md) are prerequisites,
not findings to repeat here. [Proposed ADR 0121](../../architecture/adr/0121-composable-render-programs-and-host-execution.md)
already addresses explicit resource versions, preserving writes, history publication, contribution
incarnations, exact imports/exports, independently paced producers, and device epochs.
[R8/R9](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r8-external-render-feature-and-host-local-execution)
and [R11](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11-ui-content-association-text-admission-and-platform-qualification)
remain **not started** at this baseline. Design coverage is not implementation evidence.

## Evidence And Authority

| Repository | Inspected commit | Qualification of evidence |
| --- | --- | --- |
| Nara | `75e1284ed28bc5a96686609f7dae8805d1924788` | Requested main baseline; Nara line references below refer to this commit. |
| `repo-ref/bevy` | `ae6152152d6fb8308ad6038314b48ef295e80fbc` | Clean reference checkout when inspected; implementation precedent, not a release compatibility claim. |
| `repo-ref/godot` | `7be59b463a4798834d72d6d0a2942ddd01d920ee` | Clean reference checkout when inspected. |
| `repo-ref/fyrox` | `5495d012b32145674501af2e67d2fd06a06f04dd` | Clean reference checkout when inspected; differs from the October 27 study's pin. |

Read together with STRATEGY and the architecture authority map, the relevant Accepted contracts are
ADRs 0012/0032/0033/0040/0078/0092/0094/0105 for render ownership and content;
0031/0041/0102/0107/0108 for text, input and UI;
and 0111/0112/0118 for Scene/Game presentation and Editor ownership.
The proposal must reconcile affected Accepted contracts before changing them. This report changes
none of those documents. The OQ010 navigation wording issue is being handled by the parent review
and is deliberately not a finding here.

No Cargo commands, native runs, subagents, reference updates, implementation changes, ADR/index/memory
edits, or commits were performed. Only this report is written. The pre-existing untracked
`render-ui-apply-foundation-implementation-codex-root.md` is left untouched.
The forward-looking recommendations are design inferences from pinned source, not unverified claims
about the industry's latest releases or an inevitable future architecture.

## Five Conclusions

| ID | Conclusion | Relation to existing work |
| --- | --- | --- |
| F1 | Replace the closed domain/material execution boundary with real typed Features and renderer-policy composition under one native Host. | Known R8/R9 implementation debt; no request to reopen graph selection. |
| F2 | Exact output leases need a separate policy for repeated display and one-shot Runtime UI completion across independently paced producers. | Pixel custody and placements are already proposed; the concrete completion/interaction cross-contract below needs explicit closure. |
| F3 | Window-free initialization and producer-local retirement must be demonstrated independently of the existing Editor Host shutdown path. | Already specified in R8/ADR 0121; still unimplemented in the ordinary Runtime path. |
| F4 | Give the Editor its own retained UI/drawing and docking owners while preserving document, view and Host authority. | Accepted separation is sound; the custom drawing consumer and native detach path remain unqualified. |
| F5 | Reuse mature text/layout algorithms, but qualify an editable-text and platform-semantic boundary rather than promoting display glyphs into an editor toolkit. | R11 covers the workflows; the concrete editable-layout and font-policy successor is still to be selected. |

### F1 — A Resource Graph Must Carry Real Renderer Contributions, Not Re-label Closed Families

**Source evidence.** Nara's private [`frame.rs:36–55`](../../../crates/nara_render_wgpu/src/frame.rs#L36)
names `CollectSprite`, `CollectUi`, `sprite_batches`, and `ui_packets` centrally.
[`backend.rs:1603–1675`](../../../crates/nara_render_wgpu/src/backend.rs#L1603) prepares target draws
through that payload and the stock quad path. The ordinary domain stages already exist:
[`nara_sprite_render/src/lib.rs:59–68`](../../../crates/nara_sprite_render/src/lib.rs#L59) installs
extraction and queueing. Their existence does not make the captured payload extensible.
[`nara_material/src/lib.rs:75–99,108–117,153–161`](../../../crates/nara_material/src/lib.rs#L75)
admits only `SPRITE_FRAGMENT_V1`, one parameter vector and one additional texture, using startup
shader text and transient instances outside durable Scene codecs.

**Trigger.** An external package supplies a blur, a compute histogram consumed by custom Editor
drawing, or a renderer policy that composes different domain outputs. A separate dissolve material
needs a matrix and two auxiliary textures, durable Save/Reopen, and shader dependency invalidation.
Neither a new phase label nor a larger fragment parameter array supplies these workflows.

**Mature mechanisms.** At the inspected Bevy pin,
[`renderer/mod.rs:32–76`](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/renderer/mod.rs#L32-L76)
defines `RenderGraph` as a schedule with Begin/Render/Submit/Finish; its
[`RenderSystems:186–238`](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/lib.rs#L186-L238)
distinguishes asset preparation, queueing, sorting, bind-group preparation and rendering.
Godot separately exposes
[`CompositorEffect` placement:42–90](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/resources/compositor.h#L42-L90)
and tracks concrete buffer/image accesses in
[`rendering_device_graph.h:155–180`](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/servers/rendering/rendering_device_graph.h#L155-L180).
These precedents support separate composition and resource obligations; neither selects Nara's API.
For materials, Bevy's
[`AsBindGroup` example:45–64](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/examples/shader/shader_material_2d.rs#L45-L64)
and Godot's
[`ShaderMaterial` property projection:250–253](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/resources/material.cpp#L250-L253)
illustrate different authoring routes, not a reason to equate persistent Schema with GPU layout.

**Counterevidence and limits.** The stock path is intentionally closed under Accepted ADR 0105;
this is an extension limit, not evidence that stock Sprite/UI output is incorrect.
Fyrox's useful
[`SceneRenderPassContext`/callbacks:484–588](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-impl/src/renderer/mod.rs#L484-L588)
expose scene, caches and HDR/LDR callbacks, but do not by themselves prove complete renderer-policy
replacement. Likewise, raw native access is trusted integration: Nara's current
[`WgpuEditorWindowFrame::encode:639–675`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L639)
passes Device and Queue references. A scoped borrow is not a security boundary against trusted Rust
retaining native capabilities. Do not replace this honesty with a second RHI or a source verifier.

**Break point, coverage and delay cost.** R8 already selects paired immutable captures/native
adapters, first-party parity and a second renderer policy; R9b already selects durable material
declarations, separate semantic/binding/interface identities and tracked shader inputs. Implement
those seams before extending the closed family. Extraction captures domain meaning; queueing
selects/orders draws; GPU preparation realizes exact captured inputs; the finite graph determines
shared dependencies; only the Host submits and finalizes targets. These responsibilities do not
require a fixed public sequence of half-built mutable stages or a separate render World.
Keep ordinary encoding authority narrower than whole-Host replacement, and retain wgpu as the RHI.
Deferral makes each new renderer/material/UI family depend on central fields and private caches,
increasing the number of consumers that must migrate together. This is existing planned work,
not a newly discovered graph-design omission.

### F2 — Output Reuse And Runtime UI Completion Are Different Lifetimes

**Source evidence.** There are three concrete constraints to join:

- [`editor_execution.rs:210–225,1180–1195`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L210):
  a View owns one reusable target and accepts a receipt only for its last submitted frame.
  The next image draw writes that same target. Retaining a receipt or cloned TextureView does not
  retain an earlier pixel version.
- [`editor_execution.rs:567–594`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L567):
  composite receipts are deduplicated by image within a window. Display occurrence/clip/transform
  is not an identity in that low-level record. The Scene presenter joins the actual display mapping
  separately in [`editor/src/platform/imgui/scene.rs:159–205`](../../../editor/src/platform/imgui/scene.rs#L159).
- [`WgpuEditorUiImageSubmission::presented:382–405`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L382)
  consumes one UI submission when its image matches a final composite.
  [`integration.rs:465–474,504–512,663–695`](../../../crates/nara_ui_render/src/integration.rs#L465)
  refuses another backend reservation or outcome drain while previous permits remain outstanding.
  These permits return to their originating Runtime, not to a global frame counter.

**Trigger and additional counterexample.** World A produces at 30 Hz, World B at 60 Hz, and the
Editor composites at 144 Hz. A's output includes Runtime UI and appears in an interactive Game
panel plus a clipped comparison mirror. The mirror is hidden or its native window fails while
the Game panel remains visible. Holding the original one-shot attempt until *all future displays*
finish would prevent A's next UI handoff. Completing it separately for every occurrence would
duplicate terminal authority. Completing on image publication would grant authority before a
window displays it. Merely introducing immutable GPU output leases chooses none of these policies.
This is a source-derived failure of a naive generalization, not a reproduced failure of today's
single Game bridge.

**Mature mechanisms.** Godot's
[`SubViewportContainer:104–152,196–230`](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/gui/subviewport_container.cpp#L104-L152)
separates texture display, visibility/update policy and focus/input forwarding, including coordinate
conversion. Fyrox's
[`render_ui:820–900`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-impl/src/renderer/mod.rs#L820-L900)
can render UI to a texture and register it for subsequent use. Neither mechanism establishes Nara's
exact immutable versions or one-shot presented-session contract; copying their texture handles
would not settle this issue.

**Counterevidence and contract status.** Nara already separates Host and View frame indices in
[`editor_execution.rs:719–755,848–858`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L719).
It is therefore inaccurate to call the existing Editor globally single-view or to claim that
every repeated display necessarily requires a new World frame. Existing fail-closed checks are
valuable. ADR 0121's exact output and occurrence boundaries already solve the *proposed pixel and
mapping model*. However, Accepted
[ADR 0108:53–68](../../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md#L53)
allows one active Surface Slot per target, and
[ADR 0112:115–149](../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md#L115)
requires exactly one terminal outcome before the next candidate or retirement. Its image-only
success must not commit UI presentation. ADR 0121's display-occurrence text does not yet select
the Runtime UI fan-out completion/interaction policy. Its listed refinements also do not explicitly
name those two contracts. This is the main additional cross-contract closure identified here.

**Recommended break and delay cost.** Separate immutable producer output custody from destination
display attempts and from the original Runtime UI attempt. A conservative initial policy is
passive mirrors plus one explicitly active interactive placement per logical UI surface/input seat.
Name the designated display attempt that terminalizes the original UI handoff; later repainting an
already completed output does not re-complete that handoff. Independent mirrors receive their own
display/mapping facts and retain pixel custody, but do not clone a Runtime UI permit or widget
session. A failed/hidden designated attempt must reach an explicit terminal outcome, leaving future
capture eligibility and other producers observable. Interaction handoff requires exact compatible
presented geometry/session authority, not merely the same texture.

Before freezing R8/R11's public bridge, reconcile this policy with ADRs 0108/0112 and exercise the
following *single additional interleaving* alongside the existing R8 tracers:

| Event | Required observation |
| --- | --- |
| A publishes A42; B publishes B77. | Each output retains its exact resources, producer identity and captured clock. |
| Editor repaints A42 in two placements while B advances. | No recapture or history advancement for A; per-placement clip/DPI/input facts; no second completion of A42's original UI attempt. |
| One destination hides/fails; A43 preparation is pending/rejected. | Original attempts terminalize according to the selected policy; compatible A42 can remain visible without granting unpresented A43 interaction; B continues. |
| A retires while B keeps submitting. | A's pending permits return once, input authority is revoked, and retained output pixels survive only for their authorized physical consumers. |

All rates are test inputs, not measurements. If simultaneous independent interactive mirrors are
later required, they need an explicit session/aggregation successor; they cannot be obtained by
cloning a receipt. Delaying this decision embeds conflicting notions of “frame complete” in the
renderer, UI planner, platform input router and Runtime teardown at once.

### F3 — A Shared GPU Host Needs Window-Free Boot And Producer-Local Completion

**Source evidence.** Ordinary
[`WgpuRenderBackend:485–502`](../../../crates/nara_render_wgpu/src/backend.rs#L485) retains one
source/frame admission pair. Its Runtime submit path rejects image targets at
[`backend.rs:999–1003`](../../../crates/nara_render_wgpu/src/backend.rs#L999), and
[`ensure_device:1295–1332`](../../../crates/nara_render_wgpu/src/backend.rs#L1295) requires a surface
and requests a default device descriptor. The Editor has a useful separate owner and View map at
[`editor_execution.rs:760–788`](../../../crates/nara_render_wgpu/src/editor_execution.rs#L760).
Its whole-Host retirement token explicitly requires stopped admission and queue-empty completion:
[`imgui/retirement.rs:12–16,56–89`](../../../crates/nara_render_wgpu/src/editor_execution/imgui/retirement.rs#L12).

**Trigger.** Close producer A while producer B and the Editor keep submitting; run a thumbnail or
render test without any native window; install an effect requiring capabilities not enabled by the
live device. Reusing whole-Host drain for A can wait for unrelated continuing work. Requiring a dummy
window or silently rebuilding the shared device hides rather than solves the ownership mismatch.

**Mature mechanism.** Bevy's
[`renderer/mod.rs:243–274,319–364`](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/renderer/mod.rs#L243-L274)
allows adapter selection without a primary-window surface and explicitly derives requested features
and limits. Its functionality-priority policy is not automatically the right Nara policy.

**Counterevidence.** Nara already retains exact Image identity and pixels together in
[`nara_image/src/prepare.rs:19–38,118–168`](../../../crates/nara_image/src/prepare.rs#L118), and GPU
image use is held through submission completion by
[`texture.rs:730–748`](../../../crates/nara_render_wgpu/src/texture.rs#L730).
Do not replace these with live asset lookups, or report a general missing GPU resource lifetime
model. Queue-empty is appropriate for the existing whole-Host teardown contract. Submission
completion also does not establish native window destruction or future recovery eligibility.

**Break point, coverage and delay cost.** These changes are already specified by R8/ADR 0121 and
the accepted Host/content contracts. Move ordinary execution ownership to the runner/Host, retain
separate producer/View/target/device identities, close enabled capabilities before initialization,
and finish a retiring producer's submitted prefix while unrelated work continues. Keep exact CPU
captures or same-generation reconstruction leases for any promised recovery; GPU completion alone
must not retire the only recovery input. Imported assets and GPU-generated outputs need distinct
reconstruction policies. Offscreen GPU remains separate from content-only Headless. Ordinary
supervised project-process Play is not an implicit cross-process GPU-sharing facility.
Delaying this work proliferates World-owned devices, hidden-window requirements and global-drain
assumptions across previews and custom UI. It does not justify implementing multi-queue scheduling
or a universal allocator now.

### F4 — Own An Editor UI Scene Without Making It A Game World Or A Universal Toolkit

**Source evidence.** The Runtime kernel is already renderer-neutral, but its current
[`UiPaintPrimitive:343–364`](../../../crates/nara_ui_kernel/src/frame.rs#L343) is Rect/Text, and
[`UiPaintPacket:367–397`](../../../crates/nara_ui_render/src/packet/mod.rs#L367) retains a Runtime
`UiFrame`, prepared images and glyph pages. This is a good concrete consumer, not a demonstrated
universal drawing language. Editor
[`panels.rs:66–83`](../../../editor/src/workspace/panels.rs#L66) already separates panel placement,
panel instance, render view and native-window identities. Its
[`layout.rs:18–37,71–82`](../../../editor/src/workspace/layout.rs#L18) keeps toolkit layout as
replaceable user preferences, and
[`layout.rs:345–353`](../../../editor/src/workspace/layout.rs#L345) currently restores secondary
window placements into the primary window with an explicit reason.

**Trigger.** A custom node canvas or timeline displays a live Scene result between clipped UI
layers, with a popup above it. An Inspector remains unchanged while that producer renders, and a
panel is reordered, hidden or transferred to a detached native window. Forcing every control through
Runtime Scene/ECS authoring or rebuilding the entire UI at the producer's frame rate couples owners
that have different state, identity and invalidation needs.

**Mature mechanisms and counterevidence.** Fyrox keeps widget/focus/capture/drawing ownership in
[`UserInterface:736–770`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/lib.rs#L736-L770),
with clip, geometry, transform and resource-bearing
[`draw::Command:61–69,129–143`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/draw.rs#L129-L143).
Its dock manager's
[`floating_windows:164–170`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/dock/mod.rs#L164-L170)
are handles to `fyrox_ui::window::Window` widgets, not proof of OS-window detach support.
Godot explicitly distinguishes embedded/native window behavior through
[`Window:42,324–325,401`](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/main/window.h#L401).
Nara's existing neutral kernel and stable panel identities are positive evidence: no new universal
Editor kernel or mandatory ECS UI model follows from this study.

**Break point, coverage and delay cost.** Accepted ADRs 0107/0118 and R11 already select independent
product layers. Use an Editor-owned retained UI scene: live widget identity, local edit state,
layout/paint/semantic invalidation and a drawing stream. Its concrete renderer Feature consumes
exact output leases under the shared Host; document/session owners retain edits, history and
selection. Keep display placement separate from producer View identity. Preserve source-built
semantic tools and panel identities; replace toolkit-specific widgets and docking blobs as needed.
Do not require all Editor controls to consume Runtime `UiFrame`, and do not create another GPU Host
inside the UI runtime. A simple canvas/image/popup tracer qualifies rendering integration only.
Native detach additionally needs focus/capture cancellation, IME relocation, DPI, accessibility and
window/surface retirement evidence. Retain layout/text work across unrelated viewport redraws before
considering broad panel-texture caching. Deferral is costly when toolkit-specific text buffers or
dock IDs become document identity, not merely because disposable widget code will be rewritten.

### F5 — Self-Drawn Does Not Mean Reimplementing Text Shaping, Rasterization And Layout

**Source evidence.** Nara already uses Parley and Swash in
[`nara_text/src/engine.rs:3–23`](../../../crates/nara_text/src/engine.rs#L3), and Taffy inside
[`nara_ui_kernel/src/compiler/root.rs:4–14,82`](../../../crates/nara_ui_kernel/src/compiler/root.rs#L4).
The current text engine deliberately registers one face with system fonts disabled
([`engine.rs:27–48`](../../../crates/nara_text/src/engine.rs#L27)), then projects the layout into
lines, runs and positioned glyphs
([`engine.rs:114–190`](../../../crates/nara_text/src/engine.rs#L114)). That output is useful for
display; it does not expose the retained cluster/caret/hit-testing and edit-composition state of a
complete text editor. The limitation agrees with Accepted
[ADR 0031:20–30](../../architecture/adr/0031-text-and-font-strategy.md#L20), which explicitly excludes
editable text and fallback sets from this slice.

**Trigger.** Edit a Chinese/Japanese name mixed with Latin and bidirectional text; keep the caret,
selection and preedit while filtering Inspector rows; move the focused panel across DPI/windows;
expose the field and its actions to a screen reader. Glyph drawing alone cannot decide cluster-safe
movement, composition ownership, native candidate coordinates or authoritative document submission.

**Mature mechanisms.** Godot's
[`LineEdit:124–188`](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/gui/line_edit.cpp#L124-L188)
separates composition, commit/cancel and window-transformed IME geometry; it asks TextServer for
caret and hit-test information at
[`line_edit.cpp:1958,2028–2044`](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/gui/line_edit.cpp#L1958).
Owning the widget does not require owning every text algorithm. Conversely, Fyrox's inspected
[`OsEvent:655–694`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/message.rs#L655-L694)
has keyboard text but no IME composition variants, and
[`translate_event:512–595`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-impl/src/utils/mod.rs#L512-L595)
falls through for IME. Its mature widget catalogue is not evidence of complete East Asian input.
Its eager
[`ListViewBuilder:562–629`](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-ui/src/list_view.rs#L562-L629)
also shows why scrolling alone is not virtualization.

**Counterevidence and remaining choice.** Current Dear ImGui input has dedicated
[`ime_tests.rs:77–144`](../../../editor/src/frontend/imgui/ime_tests.rs#L77), and Nara has an existing
[`WinitAccessibilityPublisher:74–110,181`](../../../crates/nara_winit/src/accessibility.rs#L74).
These are reusable mechanisms and test baselines, not proof that a new Editor runtime inherits
native candidate placement or accessibility. Do not weaken deterministic game text by silently
enabling ambient system fonts everywhere, or assume absent Editor use of an accessibility bridge
means the whole engine has no accessibility work.

**Break point, coverage and delay cost.** R11 already requires composition, virtualization,
invalidation and platform qualification. Before its text API freezes, select a retained editable
layout/edit-buffer boundary using mature text facilities, with explicit font/fallback identity and
an Editor policy separate from deterministic Runtime font admission. Keep Nara-owned controls,
identity, commands and scheduling; reuse shaping, rasterization, layout algorithms and platform
bridges where suitable. Preserve cluster-aware hit/caret data rather than reverse-engineering it
from glyph quads. Native preedit stays in the frontend; only the chosen semantic commit updates the
document and creates an Undo unit. Stable property identity, live widget incarnation and saved panel
identity remain separate. Accessibility semantics/actions and keyboard traversal must be produced
by the Editor surface itself under Host focus/input arbitration.
Delaying this boundary until after a large widget catalogue forces changes through every text field,
virtualized row, font cache and platform adapter. It does not justify writing a new shaping engine,
layout solver, complete reactive framework or universal shared game/Editor toolkit.

## Integration Recommendation

Keep the selected graph and existing R8/R9/R11 ordering. The highest-value addition to the parent's
report is **F2's exact completion/interactive-placement interleaving**, explicitly joined to
ADRs 0108/0112; visual two-panel sampling alone is too weak an acceptance case. The remaining
conclusions identify implementation obligations or concrete design choices already implied by the
plans. They are not five newly discovered production bugs.

Before render API freeze, demonstrate the external Feature and alternate policy using one Host,
then independently paced producers with retained old outputs, stream-local retirement and real
window-free GPU initialization. Carry the same lease boundary into a small Nara-owned drawing
consumer. Develop the editable-text/platform and retained Editor interaction owner without waiting
for complete Runtime widget parity. Preserve the working Dear frontend until corresponding complete
workflows pass. Measurements and native qualification remain future work; this review claims no
performance improvement, graphics correctness run or platform support expansion.
