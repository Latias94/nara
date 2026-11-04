# Custom-Drawn Editor UI: Reference Boundaries

Date: 2026-10-27. Scope: supplied architecture conversation, lines 3611-4134, versus primary sources.
The user subsequently clarified the target: **Nara's own custom-drawn Editor UI**, not embedding
another toolkit. References supply concepts and counterexamples, not an adoption shortlist.
This is reference research, not runtime qualification. **Source fact** means inspected code;
**inference** means recommendation; **unverified** means no build/native/performance/unload experiment.
For the October 27 review, sparse checkouts stayed unchanged; omitted files were read online at
the same pin. The November 4 mechanism follow-up below inspects local source only.

## Baselines

| Reference | Remote | Inspected commit |
| --- | --- | --- |
| Fyrox | `https://github.com/FyroxEngine/Fyrox.git` | `f008e4c2cdfdcda7b37bf8534d0df8de155748f6` |
| Xilem/Masonry | `https://github.com/linebender/xilem.git` | `271a27a6d4a930f7878d404f9014e3c50a3a9b88` |
| egui, integration alternative | `https://github.com/emilk/egui` | `86099aca1276fe786921992c8d5a68914ee79bec` |
| Open GPUI, component/virtualizer follow-up on November 4 | `https://github.com/Latias94/open-gpui` | `689db5b49b33a5b2e64ba332bff72c98e70092bb` |
| Zed GPUI, state/invalidation/cache follow-up on November 4 | `https://github.com/zed-industries/zed` | `dd3437011469d5c2ec8c821a84cc999979625697` |

October 27 Nara context is `refactor/scene-data-representation` at `65402b9206b26555787e4da5b7f7abd1a0c048fa`.
That dependency comparison uses `winit =0.30.13`, `wgpu =30.0.1` ([local manifest][nara-deps]).
The incumbent Editor is Dear ImGui, not egui; it remains the product/control baseline.
Open GPUI denotes the pinned fork, **not** current upstream Zed GPUI.

## Verdict

**Design inference under the clarified target:** design Nara's own Editor UI runtime, separating
drawing, identity, declarative description, updates and scheduling. Borrow composition/message
ideas from Fyrox and text/focus/accessibility passes from Masonry. Their gaps are requirements
to handle explicitly, not reasons to import either framework. Dear ImGui supplies existing
workflow behavior during replacement, not the intended final toolkit. Reusing text shaping,
layout algorithms or accessibility adapters is a separate dependency choice. Nara need not
demonstrate third-party adoption failure before designing its own runtime.

## Identity Is Not A Mode Label

- **Source fact:** Fyrox stores widgets in a `WidgetPool` and retains focus/capture handles in
  `UserInterface`; its pool is generational. These are runtime references, not semantic property
  keys or saved dock identifiers ([UI owner][fyrox-owner], [pool][fyrox-pool]).
- **Source fact:** Xilem's `View` explicitly separates short-lived descriptions, retained elements
  and `ViewState`, with build/rebuild/teardown/message operations. Its ordinary `Vec` sequence
  reconciles common entries by position; generations reject stale messages after removal, not
  arbitrary model reordering ([View][xilem-view], [Vec implementation][xilem-vec]).
- **Source fact:** egui preserves state using IDs; immediate mode is not stateless ([IDs][egui-id]).
- **Inference:** distinguish model/property identity, live widget incarnation and saved panel identity.
  Preserve the first across sorting/filtering while replacing the second when needed. Generations
  alone cannot preserve the correct draft/caret/selection. Do not treat Xilem's plain `Vec` as keyed.

## Fyrox: Useful Mechanisms, Important Missing Contracts

1. **Message direction is not tree routing. Source fact:** `ToWidget`/`FromWidget` distinguish a
   request from a change response. `reverse()` preserves `destination`; `BubbleUp` starts there
   and visits its ancestors, while `Direct` only invokes that target after registered preview
   hooks. Preview is not a top-down capture pass over the target ancestry. The bubble loop does
   not automatically stop on `handled`; handlers must respect it. Invalid destinations skip widget
   processing but can still be returned to the outside caller ([message types][fyrox-message],
   [dispatcher][fyrox-routing]). **Lesson:** filter destination/direction/payload; UI responses are not document acceptance.
2. **IME/preedit is not implemented by the ordinary pinned path. Source fact:** `OsEvent` has
   keyboard text but no IME session/preedit/commit variants. `translate_event` handles keyboard
   `event.text` and falls through to `None` for IME; `UserInterface` forwards keyboard text to the
   focused node, and `TextBox` inserts its characters ([event schema][fyrox-os],
   [winit translation][fyrox-translate], [text dispatch][fyrox-text-dispatch], [TextBox][fyrox-text]).
   A commit-only adapter could insert text, but that schema cannot express composition state or
   candidate positioning. Winit requires separate preedit/commit and candidate-area handling
   ([official IME documentation][winit-ime]). This missing contract is distinct from glyph coverage.
3. **Scrolling is not virtualization. Source fact:** default `ListViewBuilder` generates every
   item container, links all into a `StackPanel`, then wraps that panel in a scroll viewer;
   `TreeRootBuilder` similarly installs the supplied items. The stack measures its children
   ([list construction][fyrox-list], [tree construction][fyrox-tree], [stack layout][fyrox-stack]).
   **Inference:** do not budget the defaults as viewport-proportional storage/layout. A custom
   panel/lazy application tree is possible, but additional work, not a supplied active-range tree.
4. **Graphics-agnostic is not resource-neutral. Source fact:** draw commands contain geometry,
   brushes, clipping, transforms, `TextureResource`, font pages and `MaterialResource`; the crate
   depends on Fyrox resource/texture/material infrastructure ([draw contract][fyrox-draw],
   [dependencies][fyrox-deps]). The consumer supplies rendering and native windows ([README][fyrox-readme]).
   **Inference:** bridge resources, text atlases, materials and clipping, not merely triangles into a wgpu pass.
5. **Do not accept prose inventories as acceptance evidence.** The README's blanket lack of
   keyboard navigation is weaker than the implemented list Up/Down selection path; conversely,
   that path does not prove complete Tab/focus scopes. The reviewed core/dependencies expose no
   AccessKit bridge comparable to Masonry's ([list input][fyrox-list-input], [Control][fyrox-control],
   [dependencies][fyrox-deps]). Native accessibility and full keyboard workflows remain unverified.

## Xilem/Masonry: Lessons From Separate Runtime, Host And Renderer

| Integration level | Source-confirmed authority | Consequence |
| --- | --- | --- |
| Xilem with `masonry_winit` | Runner creates windows, graphics context, surfaces and presentation. `on_wgpu_ready` exposes its device/queue; device options request features/limits. | Shows why a UI runner must not silently become Nara's platform/graphics owner. |
| External event-loop example | Caller forwards Winit callbacks, but still constructs `MasonryState` and its windows. The example explicitly limits current embedding scope. | Sharing an event loop is not sharing a pre-existing native window or swapchain. |
| `masonry_core::RenderRoot` plus a custom host | Constructor creates neither window nor event loop; caller feeds input/scale/size, consumes signals, redraw output and AccessKit updates. | Reference concept: keep a runtime root separate from the platform owner. |

Sources: [runner][masonry-runner], [device callback][masonry-driver], [external-loop example][xilem-external],
[RenderRoot][masonry-root]. Xilem also separates a reactive `MasonryRoot` wrapper from
`RenderRoot` ([reactive root][xilem-root]). These are conceptual references, not proposed Nara
adapters. No dependency alignment, runner embedding or third-party reconciliation integration
is planned by this study.

- **Rendering is more embeddable than the conversation suggests. Source fact:** this pin uses
  `imaging`, not only a hardwired Vello scene. `masonry_imaging::texture_render::RenderTarget`
  accepts caller-provided adapter/device/queue/texture/view; Vello initializes from that device/queue.
  ([texture renderer][masonry-texture], [official migration note][linebender-imaging]).
- **But the dependency boundary currently fails direct sharing. Source fact:** Xilem declares
  `wgpu 28.0.0` and `imaging_wgpu`'s `wgpu-28` feature, versus Nara's `=30.0.1`; both use
  `winit 0.30.13` ([Xilem manifest][xilem-deps], [Nara manifest][nara-deps]). **Inference:** Cargo
  coexistence does not make GPU handles interchangeable. This is a boundary lesson, not a Nara
  task to port or adopt the stack.
- **An external viewport is represented, not realized. Source fact:** `PaintLayerMode::External`
  emits an external layer with widget ID, transform and bounds. Compatibility replay and the
  default Winit flattening path skip external placeholders ([paint modes][masonry-paint-mode],
  [layer plan][masonry-layers], [host consumer][masonry-runner]). **Inference:** a custom compositor
  can map these to scene output, but must prove painter order, ancestor clipping, input coordinates,
  lifetime/synchronization and transparency/color space. Offscreen UI does not prove embedded Nara scene textures.
- **Text/focus/accessibility have end-to-end source plumbing.** `TextArea` uses Parley editing;
  preedit sets/clears compose state and commit inserts text. Winit IME conversion preserves all
  four variants; host signals toggle IME and update candidate geometry. Event/update passes route
  focus, handle unconsumed Tab and end/restart IME on focus transitions. TextArea publishes text
  roles/Parley accessibility data; RenderRoot accepts access actions and returns tree updates
  ([TextArea][masonry-text], [conversion][masonry-ime], [host][masonry-runner],
  [events][masonry-events], [focus update][masonry-focus], [root][masonry-root]). These facts do
  not establish native Chinese/Japanese behavior, screen-reader quality or full editor shortcuts.
- **Composition and model transactions still need an adapter. Source fact:** despite the nearby
  comment, nonempty preedit sets `edited = true` and emits `TextAction::Changed`; this alone proves neither preedit payload content nor project commit.
  `TextInput` routes Changed to its application callback and resets widget text when it differs
  from controlled contents; `reset_text` explicitly warns about lost selection and disrupted IME
  ([IME branch/reset warning][masonry-text], [controlled input][xilem-input]). **Inference:** keep
  drafts separate from document commands; trace preedit/cancel/commit before defining Undo units.
- **Virtualization exists, with explicit limits. Source fact:** Masonry `VirtualScroll` requests
  active ranges and requires the driver to add/remove rows; Xilem supplies a view wrapper.
  Its docs explicitly identify focused-child loss, incomplete virtual-list accessibility and
  missing scrollbar integration; transform and gesture caveats remain ([widget][masonry-virtual], [view][xilem-virtual]).
  Stronger than Fyrox's eager default list does not mean a ready virtualized, editable scene tree.

## Alternatives Under The Nara-Owned UI Target

| Option | Concrete benefit | Cost / rejection trigger |
| --- | --- | --- |
| Nara retained runtime with builders and explicit local updates | Own identity, edit buffers, events, invalidation and drawing; references inform mechanisms. | Leading design direction; implement and qualify these mechanisms rather than infer them from retained storage. |
| Nara immediate/hybrid runtime | Direct procedural composition; egui shows interaction state can survive rebuilt descriptions. | A possible internal technique, not a foreign toolkit embed; prove identity, composition and work bounds ([IDs][egui-id], [IME adapter][egui-ime]). |
| One shared game/Editor widget runtime | Potential common controls and rendering. | Do not make Editor replacement depend on complete Runtime UI parity; share only mechanisms whose actual consumers fit. |
| Embed a complete reference toolkit | Existing controls and platform work. | Outside the clarified target; host/render contracts remain design counterexamples, not a required experiment ([renderer][egui-renderer], [Platform][gpui-platform]). |

**Inference:** borrow selectively. Builder-plus-explicit-updates is valid; neither a universal reactive
graph nor an ECS-backed UI is required. UI events are not document transactions, accessibility
actions are not authoritative automation commands, and editor/runtime sharing needs validation.

## Falsifiable Nara Runtime Qualification Gates

Proposed experiments, **not executed results**: use a Nara-owned Editor fixture and the
incumbent's matching workflow behavior. No reference-toolkit embedding is required. Qualify
incrementally before polishing a complete widget catalogue.

| Gate | Required observation / failure condition |
| --- | --- |
| G1 Host authority | Log window, event-loop, device, queue and present ownership. Nara UI consumes admitted platform/render capabilities; it does not create a competing application host, device or surface. |
| G2 Viewport composition | Continuously render a scene between UI layers, including clipped/scrolled/transformed viewport and overlapping popup. Capture GPU submissions and compare resize/DPI/color/alpha results to a direct-scene control. No steady-state CPU readback; identify every GPU copy and synchronization owner. An external placeholder alone fails. |
| G3 Text and identity | On Windows 11 Microsoft Pinyin and a Japanese IME, record preedit/change/commit/cancel while sorting/filtering rows, refreshing Inspector data and moving focus. Zero draft-to-document commits before the chosen semantic submit; exactly one intended Undo transaction, correct caret/selection/candidate location, no viewport shortcuts while typing. Declare a cancel/preserve policy for removed rows. |
| G4 Bounded list/tree work | Compare 10,000 and 100,000 logical rows at the same viewport. Instrument live widgets, layout calls, allocation and UI dispatch. Live rows must stay within visible-plus-declared-overscan/pinned-edit rows, not grow with total count. Preserve expansion/selection by domain key and exercise focused row eviction. |
| G5 Platform semantics | Inspect actual native accessibility roles/value/focus/actions and keyboard traversal, including modal popup and deleted focused node. Repeat 100%/150%/200% DPI transitions and two native windows. Snapshot/unit tests alone do not pass; other OS claims need their own native runs. |
| G6 Scheduling and retirement | With a 60 Hz scene, count static-panel relayout/reshape and report input-to-present P95 plus memory against the control on the same machine. Freeze budgets before the run. Remove/recreate a panel during composition, capture and queued async results; zero stale actions may reach its replacement. If native unload is claimed, separately hold a callback/task/GPU use past retirement and prove physical quiescence before unloading. |

No widget tree, generation scheme, teardown hook or plugin example proves safe native unload.
Trait objects, closures, queued payloads and destructors can retain code independently of node removal.
Host-owned data/commands or process isolation are candidates, not safety receipts. No native/performance/unload claim is made.

## November 4: State, Reuse And Component Mechanisms

This bounded follow-up uses **Zed GPUI for state, invalidation and cached work**, and
**Open-GPUI for reusable component composition and virtualizer integration**. Both local HEADs
and origin remotes match the full pins above; both reference working trees were clean when
inspected. All source links in this section identify those commits. No reference was built,
downloaded, adopted or subjected to native/performance qualification.

The selected Nara route remains an **independently runnable self-drawn Editor** over existing
Application/Session, Host and render owners. Keep the actual Dear Editor separately selectable
as a model/workflow reference. A required hybrid first-party migration or an official ImGui
extension does not follow from these mechanisms. The current [UI design](../architecture/ui-foundation-design.md)
and [R11 plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
own that route; this section adds source evidence and proposed implementation implications.

### Zed: State Identity Is Separate From Element Construction

**Source facts:** these lifetimes coexist; a retained/immediate binary loses the useful distinctions.

| Layer | Actual lifetime and identity | Consequence |
| --- | --- | --- |
| Model/view state | `Entity<T>` is a strong typed handle to application-managed state; it can be downgraded. A renderable entity implements `Render::render(&mut self, ...)`. | State can outlive a description or row element. The entity owner and retained strong handles still determine lifetime ([entity][zed-entity], [render contracts][zed-element]). |
| Composition description | `RenderOnce::render(self, ...)` consumes a component recipe. Ordinary rendering constructs elements for layout/prepaint/paint. | Rebuilding a fluent description does not imply rebuilding model state; retaining state does not avoid description work ([element phases and components][zed-element]). |
| Phase-local work | `RequestLayoutState` passes to prepaint/paint; `PrepaintState` passes to paint. | These associated types describe a draw's phase handoff, not an automatic cross-frame state/cache contract ([Element][zed-element]). |
| Element-associated state | `with_element_state` uses `(GlobalElementId, TypeId)`; `use_keyed_state` retains an `Entity<S>` under an element path. `use_state` defaults to caller location. Unaccessed state is not carried into the next frame. | Keys must be unique in their containing namespace. Consecutive rendering retains compatible state; a virtualized-away element needs another owner if its state must survive. Caller location/index is insufficient for reorderable rows ([state API][zed-element-state], [frame transfer][zed-state-transfer]). |

Entity-backed views introduce a scoped element identity; using the same entity for sibling views
in the same scope collides. Nesting scopes identities, rather than making labels globally unique
([View identity][zed-view-identity]). Runtime `TypeId` provides typed lookup, not a persisted state
schema or a hot-reload migration format. **Nara inference:** separate domain item/property key,
panel/component incarnation and description position; explicitly retain or retire draft, caret,
selection and composition when an item leaves the active range.

### Zed: Notification And Cached Work Have Explicit Boundaries

**Source facts:** `Entity::update` and `cx.listener` give callbacks access to mutable state; neither
automatically calls `notify`. `Entity::write` does notify. A state-changing listener, observer or
async completion must arrange notification of the entity whose visible cached output changed.
Keyed local state installs an observer that forwards its notifications to the containing view
([update/listener][zed-update], [listener adapter][zed-listener], [write][zed-entity],
[keyed-state forwarding][zed-keyed-notify]). The supplied text editor demonstrates the dependency
edge: it observes its separate `Entity<String>`, clamps the cursor after an external write, and
calls its own `cx.notify()` because the cached editor is keyed on the editor's notification
([concrete observer][zed-editor-observer]). An arbitrary model read is not proof that every
dependent cached view will be invalidated automatically.

Reads/leases record accessed entity IDs. At draw completion, the App records which windows
currently access them; `App::notify` invalidates those live windows and schedules observers.
The window drains dirty entity IDs and marks their rendered view ancestry dirty using the prior
dispatch tree. This is entity/view identity, not field-level dependency tracking. Cached-view
replay restores its accessed-entity set so the window stays subscribed even when its render
does not run ([entity reads/leases][zed-entity-access], [access tracking][zed-access], [notify routing][zed-notify],
[dirty ancestry][zed-dirty], [cached view][zed-view-cache]). Explicit observers remain necessary
where a separate model must invalidate the consuming cached view.

| Reused artifact | Source-confirmed hit/miss boundary | Work and ownership limit |
| --- | --- | --- |
| Cached view subtree | Opt-in `Entity::cached`/`AnyView::cached`; compare bounds, content mask, inherited text style and opacity; require no dirty view or full refresh. Inspector picking disables caching. | Caller supplies a definite layout size: contents are not rendered to infer it on a hit. Props/dependencies outside the key require notification. A miss renders/layouts the subtree; it is not a general diff of all component fields ([API][zed-view-identity], [cache implementation][zed-view-cache]). |
| Recorded prepaint/paint ranges | Replay hitboxes, dispatch subtree/focus, accessed state, text-layout uses, deferred draws, listeners/input handlers, tab stops and scene primitives into the next frame. | Reuse preserves more than pixels, but copying/moving/replaying recorded ranges costs work proportional to those records. It is not a framebuffer cache or a zero-cost frame; presentation/GPU work remains separate ([prepaint/paint replay][zed-frame-replay]). |
| Layout | A cached view still requests an outer layout node; unchanged internal work can be skipped through cached output. The window clears the layout engine after drawing, and `TaffyLayoutEngine::clear` clears its Taffy tree. | This pin does **not** demonstrate a persistent whole-tree incremental layout arena across frames. Preserving a view's output and preserving Taffy nodes are different mechanisms ([cached outer layout][zed-view-cache], [draw completion][zed-draw-finish], [layout engine][zed-layout]). |
| Shaped/wrapped lines | Current/previous-frame caches use text, font size, font runs, wrap/forced width; font-generation changes invalidate cached entries. Cached-subtree replay records reused line ranges. | Exact compatible requests avoid reshaping; unused entries age out at frame turnover. This is separate from edit-buffer state and raster residency ([line cache][zed-lines], [line request/key][zed-line-key]). |
| Raster bounds and atlas tiles | `RenderGlyphParams` identifies font/glyph/size/subpixel variant/scale/rendering mode/dilation. Glyph painting asks the atlas to build/rasterize on a missing key; occupied entries return a tile. | A tile hit can avoid rasterization/upload while scene construction still occurs. Atlas removal/device-loss clearing and resource lifetime remain separate from view identity; this is no general memory-budget or GPU-time claim ([glyph key][zed-glyph-key], [glyph painting][zed-glyph-paint], [atlas lookup][zed-atlas], [Windows clearing][zed-atlas-reset]). |

**Nara inference:** use explicit, owner-local invalidation before attempting automatic fine-grained
dependency discovery. A color-only update should reach paint without invalidating compatible
measurement; text/typography/width changes must reach their actual measurement dependencies.
Custom-layout invalidation must include child intrinsic measurements and constraints, with a
conservative complete-root fallback when dependencies are not yet represented. Borrow the
notification/cache separation, not GPUI's App/Window/Entity ownership or a promise that every
local event has subtree-only cost.

### Zed Lists: Bounded Elements Do Not Mean Bounded Logical State

**Source facts:** `uniform_list` measures a representative item, derives content height from item
count and uniform height, and calls the renderer for the viewport's floor/ceil index range. It
does not infer variable heights or semantic row identity; the inspected path has no configurable
general overscan budget ([uniform construction/measurement][zed-uniform-measure],
[visible range][zed-uniform-range]). The representative measurement is additional work beyond
the visible-range callback. Application state and stable element keys must still identify items.

Variable `List` instead receives caller-retained `ListState`: an `Rc<RefCell<_>>` containing a
`SumTree<ListItem>`, measured sizes/optional uniform hints, focus handles and scroll state. Pixel
`overdraw` permits measurement beyond the viewport; optional `measure_all` explicitly pays for
the whole collection. Unknown extents can be provisional; hints converge as real rows are
measured. `remeasure_items`, `remeasure`, `splice` and `reset` communicate changed extents or
membership rather than discovering every model change ([state][zed-list-state],
[measurement/invalidation API][zed-list-measure], [active/overdraw layout][zed-list-layout]).

Its anchor is an **item index plus pixel offset**, with absolute/proportional remeasurement and
splice adjustment. A splice before the anchor shifts the index; replacing the anchor range moves
it to the replacement start. This is not a built-in domain-key reconciliation algorithm
([anchor/splice][zed-list-splice]). `splice_focusable` supplies per-item handles; layout additionally
renders an offscreen focused item so keyboard interaction can continue ([focus pinning][zed-list-focus]).
That protects a configured focus route, not arbitrary selection/draft/IME after deletion or key
reuse. Text input is installed for a focused handle for the upcoming frame, and cached replay
preserves handlers ([input handler][zed-input], [replay][zed-frame-replay]). No native IME
recycling qualification was performed here.

**Nara inference:** budget visible, overscan and explicitly pinned editing rows separately from
logical row metadata, prefix-height/index storage and filtering. Require domain-key plus
intra-item offset anchoring across reorder/filter/remeasurement, and a deliberate
pin/preserve/cancel policy for focus, selection, preedit and caret when recycling or deleting.
Do not transfer an index-associated input route to the next domain item.

### Open-GPUI: The Actual Virtualizer And Its Component Wrapper

**Verified dependency fact:** at this pin, “the virtualizer crate” is a module of workspace
`open-gpui-ui-core` **0.2.0**, not an identified independent third-party virtualizer dependency.
`open-gpui-ui-components` **0.2.0** depends on the workspace core; that core's manifest lists
`open-gpui-motion`. The root manifests and lockfile contain no separate virtualizer package.
The implementation **is locally present** in `crates/ui_core/src/virtualizer.rs`; no missing crate
implementation is being inferred ([workspace versions][open-workspace], [core manifest][open-core-manifest],
[component manifest][open-components-manifest], [locked packages][open-lock]).

**Source facts:** the reusable composition is `VirtualizedList` -> `VirtualizedListRenderPlan`
-> core `VirtualizerState`/`VirtualizerGeometryCache` -> `RowWindow` -> GPUI row elements.
The GPUI extension's `render_row` replaces row **content**, while the outer wrapper retains
virtual placement, focus, hit testing, selection and accessibility. The gallery actually uses
this hook and embeds a reusable `Button` in row content; it does not ask an ordinary author to
repeat range arithmetic ([adapter and retained runtime][open-list-runtime],
[plan/window projection][open-list-plan], [row wrapper][open-list-row],
[gallery composition][open-gallery], [nested Button][open-gallery-button]).

| Service/mechanism | Actual owner and limit |
| --- | --- |
| Ordinary author inputs | List ID, stable descriptor keys, shared item descriptors, size/theme tokens, fixed/measured mode, row-height estimate, overscan, default active/selected keys, activation/selection callbacks and optional custom content renderer. Concrete scroll handles and row renderers use the GPUI-specific extension; neutral geometry alone does not supply a native control ([builder][open-list-builder], [adapter][open-list-runtime]). |
| Persistent interaction | A list-scoped `use_keyed_state` entity retains focus, active/selected/selection-anchor keys, typeahead, row measurements, geometry and reveal operations. Keyboard navigation/selection actions explicitly notify; changed child-bound measurements invalidate geometry and notify ([runtime][open-list-runtime], [measurement notification][open-list-notify], [keyboard route][open-list-keyboard]). This supplies collection interaction, not a generic editable-row draft/IME owner. |
| Fixed extents | `resolve_fixed_window` computes geometry without materializing all row measurements; the caller passes keys for the active/overscan range. Overscan is a **total additional item budget**, divided before/after with edge redistribution ([fixed window][open-fixed], [overscan][open-overscan]). |
| Variable extents | Measured-window resolution uses keyed sizes where known and the estimate otherwise. Cache rebuild visits every current item and retains dense geometry/key metadata; variable geometry uses prefix ends and partition-point searches. A hit avoids the per-item size callback and materializes only the active window ([geometry][open-geometry], [measured cache][open-measured], [dense rebuild][open-measured-build]). Thus bounded output does not imply bounded cold work or O(visible) retained metadata. |
| Cache invalidation | Caller advances geometry revision when index-to-key/size results change; count, estimate, gap, margin and measurement authority also participate. Scroll/viewport/overscan can reuse geometry. Component runtime compares item-key order/snapshot measurements and invalidates when row measurement changes ([cache contract][open-geometry-cache], [runtime authority][open-list-authority], [notification][open-list-notify]). The inspected authority does not by itself prove complete width/font/content-change invalidation for every offscreen custom row. |
| Identity and reveal | Unique domain keys become render keys; duplicates receive collision-avoiding index-dependent render keys, and selectable positions exclude duplicates. Scroll snapshots hold pixel offset plus measured-key sizes; key-based bring-into-view first materializes then delegates physical reveal. These mechanisms do not establish general keyed scroll anchoring across arbitrary edits ([duplicate handling][open-list-keys], [snapshot][open-snapshot], [reveal][open-list-reveal]). |

**Cost limit from source:** even with fixed-window output or a geometry-cache hit, the component
plan reconstructs state metadata, counts duplicate/source keys and builds selectable positions
over the full collection; sticky-section lookup can scan backward. The active-window renderer
therefore does not prove total per-frame work independent of logical count ([plan][open-list-plan],
[metadata/keys][open-list-keys]). State identity tied to consecutive rendering also does not
automatically preserve a custom text editor when its row is omitted. This study establishes
component packaging and caller contracts, not native accessibility/IME quality, a universal
virtualized editor, or a performance advantage over Nara.

### Proposed Nara Implications Within Existing Owners

The [November 4 cost record](../benchmarks/2026-11-04-ui-work-baseline.md) owns measured results.
Its unchanged Inspector/Hierarchy calls build descriptions before returning the same `Arc<UiFrame>`;
the read-only 10,000/100,000-row projections retain 40 layout nodes but still scan logical rows.
Debug/release CPU repetitions establish that distinction, not a cache improvement or acceptance
of equally large editable source projects. This research ran no native measurements. Full R11a
remains open for its remaining attribution and fixed budgets. The moving-Game/static-Inspector
native receipts, final health/refusal controls and numbers belong in the cost/qualification record.

**Source-to-owner mapping and design inferences, not implemented changes:**

| Nara owner now | Concrete implication for R11 |
| --- | --- |
| Private [Inspector](../../editor/src/frontend/imgui/owned_inspector.rs) and [Hierarchy presentation](../../editor/src/frontend/imgui/owned_hierarchy/presentation.rs) | R11a must keep pre-reuse description/comparison and full filter/count/index work visible. R11e can gate description construction on explicit unchanged observation/local-state/style/geometry dependencies; model publication and exact action validation retain their owners. Do not substitute pointer equality or a global frame revision for those dependencies. |
| [UiCompiler/root](../../crates/nara_ui_kernel/src/compiler/root.rs), [root fallback](../../crates/nara_ui_kernel/src/compiler.rs) and [candidate-local text session](../../crates/nara_ui_kernel/src/compiler/text.rs) | Compilation currently builds a fresh Taffy tree; fallback is inert recovery. R11b's custom-layout consumer must exercise child measurement/placement without exporting Taffy storage. R11e should first compare unchanged component/root output reuse with persistent-layout work under measured pressure; the Zed source does not mandate a persistent arena or prove it faster. |
| [Widgets](../../crates/nara_ui_widgets/src/lib.rs), [text editing](../../crates/nara_text/src/editing.rs) and the selected component/attachment owner | R11b should make two same-label keyed editors, reorder/filter and custom row/canvas composition work through public scoped services. Component keys, compatible state, owned effects and invalidation belong to the existing UI/panel lifetimes; original document commands stay in Session. Borrow the Open-GPUI outer-wrapper/content split, while making editable focus/IME eviction explicit. No universal UI model owner or GPUI dependency is proposed. |
| [TextEngine window preparation](../../crates/nara_text/src/paint.rs) | Prepared sources avoid reshaping, but each window preparation still scans admitted glyphs and rasterizes distinct placed keys with call-local footprint reuse. R11e should distinguish source layout, placed raster-bounds/masks and clipping/placement dependencies; keep ordered font content, scale and subpixel identity complete, with bounded retained bytes and cache-loss fallback. |
| [Presentation planner](../../crates/nara_ui_render/src/planner/mod.rs), [atlas](../../crates/nara_ui_render/src/atlas.rs), [WGPU UI](../../crates/nara_render_wgpu/src/ui.rs) and native Host | Keep packet/atlas/upload/pipeline reuse separate from description/layout/text reuse. R11b's neutral planner consumer must preserve existing target/resource custody. R11e must account for replay/lowering, upload/copy/GPU work, retained bytes, device recovery and retirement without treating an old packet/frame as a new presentation/input authorization. |

Actionable adjustments for the owning plan/design, without expanding this research into runtime work:

1. **R11a:** finish and record the already selected matched native/CPU baseline, then freeze numeric
   budgets before optimization. Separate model refresh/filter/index, description/comparison,
   layout/measurement, editing/shaping, placed raster preparation, packet/lowering, upload/GPU
   and presentation costs. Keep the current text-counter exclusions explicit. The native probe
   passing does not fill missing allocation/retention/input-latency/GPU cells or qualify the
   independently runnable successor.
2. **R11b:** qualify scoped keyed state and explicit update/effect/compose/layout/paint boundaries
   using the independent composite and custom-layout consumers. Document which listener,
   external model result and local edit invalidates which retained work. Supply ordinary authors
   with lifecycle, theme/text/input/semantic services while allowing reusable custom row content;
   test repeated sizing without repeated business effects and focused-row eviction without
   retargeted IME/selection. Keep separate whole-Editor runnable closures for Dear and self-drawn.
3. **R11e:** begin with the measured pre-reuse description and logical-scan costs, rather than
   declaring virtualization complete at 40 nodes. Retain a filtered/keyed order/index only under
   its real model/query dependencies; refresh/rebuild cost and dense variable-height metadata
   remain charged. Compare fixed extents, measured estimates and anchored remeasurement where
   real consumers require them. Attribute cache hits/misses/evictions and invalidation reasons,
   including color, typography, width, DPI, reorder/removal and target recovery. Reuse existing
   qualification facilities; do not add a benchmark kernel, dependency or new owner for this study.

These recommendations add no implementation receipt, native qualification or dependency-adoption
decision. They refine how the existing R11a/b/e contracts can be implemented and measured.

### Zed child measurement and manual placement follow-up

At the same `dd3437011469d5c2ec8c821a84cc999979625697` pin, ordinary `Div::request_layout`
requests child IDs and delegates placement to `Window::request_layout` (`elements/div.rs:1999`,
`window.rs:5459`). The alternative `AnyElement::layout_as_root`/`prepaint_at`/`paint` sequence
permits manual allocation and placement without exposing the layout arena (`element.rs:693`).
The workspace's `PaneAxisElement` consumes this sequence for actual child regions
(`crates/workspace/src/pane_group.rs:1420`).

`request_measured_layout` registers a leaf context (`window.rs:5488`, `taffy.rs:85`), not an
arbitrary child-layout callback. During that callback the Window's layout engine has been taken
out for computation (`window.rs:5508`), so calling the same Window's child layout registration
or `layout_as_root` cannot safely reenter it. `UniformList` premeasures its sample first
(`elements/uniform_list.rs:283`, `:661`). `ContainerQuery` instead obtains final parent bounds,
then builds/measures/places a child; its contract expressly prevents that child from determining
the parent's intrinsic size (`elements/container_query.rs:13`, `:82`).

Nara therefore borrows scoped measurement/placement and repeatable phases, not the particular
GPUI callback storage. The pinned public GPUI path returns Size/Bounds without child-subtree
baseline propagation. A Nara child-layout interface must specify its own baseline and unknown
parent-constraint behavior. The pinned Taffy0.10 low-level `LayoutPartialTree` and
`LayoutFlexboxContainer` algorithms permit a private compile-local adapter to preserve current
Flex behavior while dispatching a custom container; its public `TaffyTree` leaf measure closure
does not itself provide recursive custom child placement. This source observation adds no
native acceptance, runtime-cache qualification or dependency adoption.

### Next drawing pressure after child layout

Before the geometry prerequisite, `UiPaintPrimitive` and packet lowering admit Rect/Text and
Rect/Glyph respectively.
`UiSurfacePresenter` still joins that original packet to native composite and input authority;
custom child layout adds no drawing capability. The Zed pin's `path_builder.rs:242`,
`scene.rs:786` and `window.rs:5002` illustrate tessellation before painting and path facts
carrying vertices, bounds, content mask, order and color together. That is mechanism evidence,
not a reason to import GPUI Scene or its public path vocabulary.

Two concrete Nara shapes were compared for the next drawing consumer:

| Shape | Demonstrated existing seam | Qualification and limitation |
| --- | --- | --- |
| Bounded neutral path/mesh facts inside the original UI paint stream | Kernel frame order, presentation packet lowering and WGPU `UiFeature` | Can preserve arbitrary Rect/Text/geometry painter order; needs exact geometry/retention/upload limits and draw barriers, not a graph-specific primitive |
| Independently authored native render Feature in the original program | `render-composition-consumer` typed Feature, original Host outputs and native use | Qualifies geometry/resource/output custody, but operation-level ordering alone cannot put a path between arbitrary UI primitives |

The UI path consumer must carry finite geometry, indices, bounds, clip, complete captured
transforms, color/alpha and resource identity. A mesh cannot acquire a new Device/Queue/submitter,
read live component state during encoding or bypass the exact original presentation outcome.
`UiPreparedImages` accepts CPU image snapshots; it cannot accept a `WgpuImageOutput` lease by
cast or readback workaround. Existing native outputs retain their separate Host contract.
The independent `render-output-consumer` is the complete presentation/input carrier for a future
same-stream drawing proof; `render-composition-consumer` supplies existing CPU/GPU Feature oracles.
Rectangular node/port semantics do not establish curve hit testing or canvas drag/wheel routing.
Those input-owner additions remain explicit requirements rather than hidden mesh-side effects.
The successor design selects the first shape for the independent curve consumer: bounded,
immutable triangles with per-vertex straight-alpha color in the original UI stream. Meshes use
the compiler's resolved content origin/clip and the planner's exact content transform. A single
original `UiFeature` prepares all quads once and encodes quad runs and meshes in painter order.
It preserves original packet allocation identity, preparation reservations and completion
custody. The consumer's Lyon dependency demonstrates user-selected tessellation without adding
a kernel dependency. This selection is a design, not qualification or a performance result;
the current plan records executed evidence separately. Final-size drawing callbacks and canvas
gestures remain further pressure.

## Primary Source Links

[nara-deps]: ../../Cargo.toml
[fyrox-owner]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/lib.rs#L697-L755
[fyrox-pool]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-core/src/pool/mod.rs#L21-L40
[fyrox-message]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/message.rs#L26-L373
[fyrox-routing]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/lib.rs#L2252-L2381
[fyrox-os]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/message.rs#L655-L694
[fyrox-translate]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-impl/src/utils/mod.rs#L503-L585
[fyrox-text-dispatch]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/lib.rs#L2987-L3012
[fyrox-text]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/text_box.rs#L1147-L1170
[fyrox-list]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/list_view.rs#L561-L631
[fyrox-tree]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/tree.rs#L1097-L1111
[fyrox-stack]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/stack_panel.rs#L138-L182
[fyrox-draw]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/draw.rs#L61-L144
[fyrox-deps]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/Cargo.toml
[fyrox-readme]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/README.md
[fyrox-list-input]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/list_view.rs#L467-L510
[fyrox-control]: https://github.com/FyroxEngine/Fyrox/blob/f008e4c2cdfdcda7b37bf8534d0df8de155748f6/fyrox-ui/src/control.rs
[xilem-view]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem_core/src/view.rs#L25-L102
[xilem-vec]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem_core/src/view_sequences/impl_vec.rs#L12-L167
[xilem-external]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem/examples/external_event_loop.rs
[xilem-root]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem_masonry/src/masonry_root.rs#L9-L88
[xilem-deps]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/Cargo.toml#L58-L91
[xilem-input]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem_masonry/src/view/text_input.rs#L250-L345
[xilem-virtual]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem_masonry/src/view/virtual_scroll.rs#L29-L68
[masonry-root]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_core/src/app/render_root.rs
[masonry-runner]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_winit/src/event_loop_runner.rs
[masonry-driver]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_winit/src/app_driver.rs#L51-L133
[masonry-texture]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_imaging/src/texture_render.rs
[masonry-paint-mode]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_core/src/core/paint_layer.rs
[masonry-layers]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_core/src/app/visual_layers.rs#L10-L90
[masonry-text]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry/src/widgets/text_area.rs
[masonry-ime]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_winit/src/convert_winit_event.rs#L21-L28
[masonry-events]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_core/src/passes/event.rs#L275-L328
[masonry-focus]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_core/src/passes/update.rs#L619-L685
[masonry-virtual]: https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry/src/widgets/virtual_scroll.rs#L111-L190
[egui-id]: https://github.com/emilk/egui/blob/86099aca1276fe786921992c8d5a68914ee79bec/crates/egui/src/id.rs#L14-L39
[egui-renderer]: https://github.com/emilk/egui/blob/86099aca1276fe786921992c8d5a68914ee79bec/crates/egui-wgpu/src/renderer.rs
[egui-ime]: https://github.com/emilk/egui/blob/86099aca1276fe786921992c8d5a68914ee79bec/crates/egui-winit/src/lib.rs#L764-L827
[gpui-readme]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/gpui/README.md
[gpui-platform]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/gpui/src/platform.rs#L3413-L3518
[linebender-imaging]: https://linebender.org/blog/tmil-25/#xilem-and-masonry
[winit-ime]: https://docs.rs/winit/0.30.13/winit/event/enum.Ime.html
[zed-element]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/element.rs#L9-L187
[zed-entity]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/entity_map.rs#L512-L604
[zed-entity-access]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/entity_map.rs#L287-L304
[zed-element-state]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L4586-L4696
[zed-state-transfer]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L1126-L1206
[zed-view-identity]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/view.rs#L197-L312
[zed-update]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app.rs#L3314-L3330
[zed-listener]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/context.rs#L215-L250
[zed-keyed-notify]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L7629-L7635
[zed-editor-observer]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/examples/view_example/example_editor.rs#L39-L75
[zed-access]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app.rs#L1459-L1495
[zed-notify]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app.rs#L3165-L3200
[zed-dirty]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L2478-L2491
[zed-view-cache]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/view.rs#L487-L660
[zed-frame-replay]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L4244-L4360
[zed-draw-finish]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L3711-L3752
[zed-layout]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/taffy.rs#L30-L82
[zed-lines]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/text_system/line_layout.rs#L478-L595
[zed-line-key]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/text_system/line_layout.rs#L671-L807
[zed-glyph-key]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/text_system.rs#L1099-L1131
[zed-glyph-paint]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5071-L5138
[zed-atlas]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/platform.rs#L1825-L1878
[zed-atlas-reset]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_windows/src/directx_atlas.rs#L46-L88
[zed-uniform-measure]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/uniform_list.rs#L624-L683
[zed-uniform-range]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/uniform_list.rs#L359-L509
[zed-list-state]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/list.rs#L1-L99
[zed-list-measure]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/list.rs#L307-L462
[zed-list-layout]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/list.rs#L1035-L1142
[zed-list-splice]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/list.rs#L490-L539
[zed-list-focus]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/list.rs#L1194-L1262
[zed-input]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5634-L5657
[open-workspace]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/Cargo.toml#L46-L83
[open-core-manifest]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/Cargo.toml#L1-L21
[open-components-manifest]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/Cargo.toml#L1-L36
[open-lock]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/Cargo.lock#L4551-L4575
[open-list-runtime]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/runtime.rs#L1083-L1206
[open-list-builder]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/runtime.rs#L851-L1012
[open-list-plan]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/render_plan.rs#L832-L905
[open-list-row]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/render.rs#L242-L333
[open-gallery]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/examples/ui-foundation-gallery/src/pages/components/samples/virtualized_list.rs#L140-L181
[open-gallery-button]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/examples/ui-foundation-gallery/src/pages/components/samples/virtualized_list.rs#L733-L803
[open-list-notify]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/runtime.rs#L706-L718
[open-list-keyboard]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/runtime.rs#L1509-L1630
[open-fixed]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L757-L823
[open-overscan]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L1432-L1453
[open-geometry]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L179-L347
[open-measured]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L933-L1067
[open-measured-build]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L1226-L1305
[open-geometry-cache]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L351-L465
[open-list-authority]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/runtime.rs#L53-L123
[open-list-keys]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/render_plan.rs#L1008-L1142
[open-snapshot]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L506-L551
[open-list-reveal]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/virtualized_list/render.rs#L71-L118
