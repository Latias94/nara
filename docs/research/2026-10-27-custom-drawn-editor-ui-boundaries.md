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

### November 5: Phase Validity And Measurement Retention

This bounded local-source follow-up uses the same Zed and Open-GPUI pins above. It adds three
acceptance cases to the existing comparison; it runs no native or performance experiment.

**Source fact:** Zed's cached view compares bounds, clip, inherited text style and opacity in
prepaint. Final opacity from ancestor hover/drag-over styling can become known during paint.
After a hit, the inspected path can replay its recorded paint despite changed opacity and
request another frame to correct it ([prepaint and paint][zed-view-cache]). **Nara inference:**
an early cache hit does not establish final artifact validity. Dependencies discovered in a
later phase require revalidation and regeneration or refusal of the affected candidate before
publication. A next-frame visual correction is not evidence for Nara's exact frame/input
publication contract. This extends the existing conservative invalidation rule without adding
a reactive owner or requiring a persistent layout arena.

**Source facts:** Open-GPUI keeps item-order geometry authority separately from
`row_measurements`; rebuilding geometry reads retained measurements before snapshot estimates
([authority][open-list-authority], [measurement selection][open-list-keys]). **Nara inference:**
advancing the geometry revision alone can rebuild an index from stale offscreen heights. A
measurement record must depend on the item incarnation, content, measurement width and
typography. Invalidate that record or return to an estimate before rebuilding the index.
The combined acceptance case changes an offscreen row under the same key, changes width/font,
reorders the collection and scrolls back; check actual height, keyed anchor and input association.
Existing focused-row pin/preserve/cancel and same-index replacement requirements still apply.

**Source fact:** the inspected list runtime inserts/overwrites measured rows and invalidates
geometry, but no eviction of historical measurement keys accompanies collection replacement
([measurement storage update][open-list-notify], [retained runtime][open-list-runtime]).
**Source-derived risk, unmeasured:** a continuously mounted measured list can accumulate
historical entries as successive distinct keys are measured. Nara's retention acceptance must
therefore include repeated key replacement without unmounting, with separately charged pinned
edit state and a declared bound/eviction policy for measurement storage. Attach/close tests alone
cannot establish that plateau. Cold/warm comparisons must also distinguish a cold geometry index
with retained measurements from empty measurement/snapshot state; the core's tests separately
exercise initial measurement callbacks and later geometry reuse ([measured cache tests][open-measured-tests]).

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
| Private [Inspector](../../editor/src/frontend/controls/owned_inspector.rs) and [Hierarchy presentation](../../editor/src/frontend/controls/owned_hierarchy/presentation.rs) | R11a must keep pre-reuse description/comparison and full filter/count/index work visible. R11e can gate description construction on explicit unchanged observation/local-state/style/geometry dependencies; model publication and exact action validation retain their owners. Do not substitute pointer equality or a global frame revision for those dependencies. |
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

### Presented pointer follow-up

At the same Zed GPUI `dd3437011469d5c2ec8c821a84cc999979625697` pin,
`elements/div.rs:299` limits ordinary motion to hovered Bubble listeners, while
`window.rs:3602`/`:778` makes a captured hitbox count as hovered outside its bounds. Capture-phase
mouse observation in `window.rs:6212` follows listener registration order; it does not itself
grant pointer ownership. `interactive.rs:520` preserves Lines/Pixels, event position, modifiers,
phase and source; its `pixel_delta(line_height)` leaves conversion to the caller. Windows
ordinary wheel events and Direct Manipulation supply different source units
(`gpui_windows/src/events.rs:577`, `direct_manipulation.rs:320`).

GPUI key-down/up retains repeat and logical keystroke facts separately from its text input
handler (`interactive.rs:23`, `platform/keystroke.rs:16`, `input.rs:26`). Its keyboard path
(`window.rs:6433`, `:6550`) may consume a shortcut before raw propagation. These mechanisms
support preserving source meaning, not deriving desktop logical keys/repeat from Nara's
gameplay physical `KeyCode`/held-state projection. `nara_input` also brings nonoptional App/ECS
dependencies, so it is not selected as a neutral kernel type dependency.

Nara's existing pointer-route equivalence cancels all gestures when any hit geometry changes.
That protects ordinary widgets but interrupts a viewport whose own pan moves descendants. The
selected successor makes event samples explicit and admits opt-in owner/ancestor geometry
continuity under unchanged Surface facts; other geometry changes retain their own cancellation.
It selects one nearest motion/scroll listener, not GPUI's live callback registry, automatic
global drag state or arbitrary propagation. Dynamic stop handling and general key/text services
remain further pressure. This pinned source study is not native or performance evidence.

### Cached composition and state membership follow-up

At the same Zed pin, `window.rs:4256` carries accessed element-state records into the next frame
while `view.rs:560` restores accessed entity IDs on a cached prepaint hit
([replay][zed-frame-replay], [view cache][zed-view-cache]). This keeps retained state and later
dependency notifications alive even when composition is skipped. Nara's membership transaction
must distinguish such reuse from actual child removal before adopting the parent pass. This
is a future reuse acceptance requirement, not a demonstrated bug in the current conservative
recomposition consumers.

At Open-GPUI `689db5b49b33a5b2e64ba332bff72c98e70092bb`, geometry reuse does not skip the
wrapper's complete key preparation. `render_plan.rs:856` rebuilds duplicate/source key data;
`:1008` inserts keys into a `BTreeMap` ([plan][open-list-plan], [keys][open-list-keys]). Distinguish
O(n) logical-entry visits/storage from up to O(n log n) ordered-key comparisons and key-byte
copying. Neither geometry reuse nor a bounded number of rendered rows proves O(visible) total
work. The current design/plan now name these separate costs without selecting this module as
a dependency.

## November 5 style, layout and component-library readiness

**Scope and evidence:** the visible Editor's poor appearance motivates this bounded source study.
This is research/design pressure, **not an implemented polished component library**, an adoption
decision or a visual/native/performance qualification. Existing execution-model and virtualizer
findings above stand; this follow-up concerns visual consistency, layout and extension ergonomics.
Local `rev-parse HEAD`, origin and clean worktrees were checked on November 5, 2026:

| Mandatory primary input | Verified local HEAD |
| --- | --- |
| Zed GPUI, `repo-ref/zed` | `dd3437011469d5c2ec8c821a84cc999979625697` (same as above) |
| Open-GPUI component companion, `repo-ref/open-gpui` | `689db5b49b33a5b2e64ba332bff72c98e70092bb` (same as above) |
| Upstream C++ Dear ImGui, `repo-ref/imgui` | `9d0b72573a27d860280811d58ef9ed62f4d46c05` |
| Godot, `repo-ref/godot` | `fefec82d74b46f5e6cc31f373071058f80a5cbee` |

Native toolkit source facts below refer to those pins, not release names or current upstream
behavior. Official frontend documentation and W3C specifications cited in the contract inventory
were consulted on November 5, 2026.

### Nara: existing mechanisms versus visual prerequisites

These are November 5 working-tree observations, including concurrent successor work, not a release claim.

| Area | Source fact now | Nara inference / readiness gap |
| --- | --- | --- |
| Layout and drawing | [`UiStyle`](../../crates/nara_ui_kernel/src/style.rs#L175-L200) has Flex/Absolute, Auto/Px/Percent, grow/shrink, gap/padding, stacking and Visible/Clip overflow; [`UiVisual`](../../crates/nara_ui_kernel/src/style.rs#L520-L585) carries border/radii separately. [`UiLayout`](../../crates/nara_ui_kernel/src/layout.rs#L7-L59) already exposes intrinsic constraints and a real first baseline. | Use these mechanisms; settle min/max sizing, per-axis overflow/scroll, border-box accounting and baseline-aligned component recipes through a real consumer. A new layout engine is not the prerequisite. |
| Theme | [`UiThemePalette`/`UiTheme`](../../crates/nara_ui_kernel/src/style.rs#L870-L968) are ten color roles plus an immutable revision. | Missing library-level semantic metrics, typography/elevation roles and scoped resolution cannot be supplied by changing accent RGB alone. Keep a renderer-neutral snapshot; do not require the game's `nara_ui` adapter. |
| Text and controls | [`UiText`](../../crates/nara_ui_kernel/src/style.rs#L680-L713) accepts explicit size/brush/alignment and Wrap/Clip. [`UiCompiler::with_fonts`](../../crates/nara_ui_kernel/src/compiler.rs#L94-L119) already admits multiple fonts; [`UiButton::authored_node`](../../crates/nara_ui_widgets/src/button.rs#L64-L92) takes caller-supplied style, visual, brush and size. | Font fallback and controlled behavior exist. An integrated inherited typography/density/locale scope, measured truncation policy and coherent variant/state recipes remain separate work. |
| Drawn Shell | [`shell.rs`](../../editor/src/frontend/drawn/shell.rs#L29-L34) fixes inset/gap/text at 8/6/14; [`compile`](../../editor/src/frontend/drawn/shell.rs#L542-L578) refuses less than 960 by 272 logical pixels and can reuse an unchanged frame. [`Theme construction`](../../editor/src/frontend/drawn/shell.rs#L867-L887) uses the default palette; [`buttons`](../../editor/src/frontend/drawn/shell.rs#L952-L970) use absolute rectangles and conflate pressed/focused colors. | This is useful workflow plumbing, not evidence of responsive layout or a finished visual system. Keep its exact controls/results while replacing presentation recipes; retain the corrected shared controller paths above. |

### Tokens, density and variants: inspect the recipes, not only the palette

**Source facts:**

- Zed distinguishes surface/elevated surface, filled/ghost element states, text/icon emphasis and
  focused/selected/disabled borders ([roles][n5-zed-colors]). `DynamicSpacing` supplies density-specific
  tuples; `ButtonSize` resolves compact/default/medium/large to 18/22/28/32 base pixels in rems.
  Button padding/gap consume those tokens ([spacing][n5-zed-spacing], [sizes][n5-zed-sizes],
  [button recipe][n5-zed-button]). Filled, tinted, outlined, subtle and transparent are distinct
  emphasis variants; selected style is a separate input ([variants][n5-zed-variants],
  [selection recipe][n5-zed-button]).
- Open-GPUI's `ThemeDesignScales` owns typography, spacing, radius, elevation, density and motion.
  Explicit component size wins over the density default; Button and TextInput resolve the same
  metric families ([scales][n5-open-scales], [recipes][n5-open-recipes]). Button exposes primary,
  secondary, outline, ghost and destructive variants ([variants][n5-open-variants]); rendering uses minimum height, padding,
  line height, semantic disabled/selected state and keyboard-visible focus ([button][n5-open-button]).
- ImGui separates window padding, frame padding, inter-item, inner-item and table-cell spacing;
  its constructor uses `(8,8)`, `(4,3)`, `(8,4)`, `(4,4)` and `(4,2)` respectively
  ([defaults][n5-imgui-style]). `ButtonEx` measures label plus frame padding, selects normal/hover/
  held colors and draws navigation focus separately. `SmallButton` removes vertical frame padding
  while preserving text baseline alignment ([button][n5-imgui-button]).
- Godot's generated `EditorTheme` selects Modern/Classic recipes and merges an optional custom
  Theme ([generation][n5-godot-generation], [merge][n5-godot-merge]). Compact/default/spacious
  spacing presets use base/extra spacing `(2,2)`/`(4,0)`/`(6,2)`, not uniform bitmap scaling
  ([spacing][n5-godot-spacing]). Modern Button installs normal/hover/pressed/focus/disabled
  StyleBoxes and text/icon colors, with `align_to_largest_stylebox` enabled
  ([recipe][n5-godot-button]); Button caches largest state margins ([margins][n5-godot-margins]).
  `Theme::set_type_variation` defines a visual type variant; `Control::set_theme_type_variation`
  selects it independently of interaction state ([definition][n5-godot-variation],
  [selection and subtree Theme assignment][n5-godot-theme-assignment]).

**Nara inference:** choose a small dense desktop recipe before expanding the catalogue: background/
panel/elevated surfaces; normal/muted/disabled/on-accent text; border/divider/focus/selection/error;
control height, inline/block padding, icon gap, row indent, section gap, radius and stroke.
Use compact/default metrics independently of monitor DPI. Define primary/secondary/quiet/destructive
variants and the combined hover/pressed/selected/focus/disabled/read-only/invalid matrix.
Preserve focus and error as independent indicators, reserve their geometry to prevent layout jumps,
and distinguish visible stroke bounds from hit targets. Reference numbers are comparison inputs,
not a prescribed Nara palette or a promise to implement every token family immediately.

### Scoped overrides, ownership and invalidation

**Source facts and ergonomic tradeoffs:**

| Mechanism | Actual scope and update behavior | Extension implication |
| --- | --- | --- |
| Zed GPUI | `StyleRefinement` applies base, focus, keyboard focus-visible, hover and active refinements in explicit order. `with_text_style` pushes/pops inherited text refinements ([states][n5-zed-states], [text scope][n5-zed-text-scope]); `TextStyle` includes font/fallbacks/features/size/line height ([typography][n5-zed-typography]). | Fluent composition is backed by explicit property/state precedence; arbitrary box properties are not thereby inherited like text. |
| Open-GPUI | Resolution selects nearest subtree, window, app, then builtin. `ThemeScope` restores scope across layout/prepaint/paint and forces cached-child refresh when effective context changes ([resolver][n5-open-resolver], [scope][n5-open-scope]). Source revision is metadata excluded from effective-content equality ([snapshot][n5-open-snapshot]). | A local preview can override siblings safely; cached children and detached content still need the correct dependency/captured context. |
| ImGui C++ | `PushStyleColor`/`PopStyleColor` and `PushStyleVar`/`PopStyleVar` save and restore `ImGuiContext::Style` ([colors][n5-imgui-color-stack], [metrics][n5-imgui-var-stack]). `BeginDisabled` also sets item flags and restores alpha when leaving the disabled scope ([disabled][n5-imgui-disabled]). | Overrides are lexical call scopes, requiring balanced cleanup; dimmed paint alone does not disable behavior. Product variants can wrap these APIs without changing the toolkit owner. |
| Godot | `Theme` groups fonts, sizes, colors, constants, icons and StyleBoxes. `Control::add_theme_*_override` supplies local exceptions before cached type/variation lookup, then branch Theme owners, context and fallback ([types][n5-godot-types], [lookup][n5-godot-lookup], [owners][n5-godot-owners]). Theme changes clear caches, redraw and update minimum size; mutable StyleBox/font overrides subscribe to resource changes ([override lifecycle][n5-godot-overrides], [invalidation][n5-godot-invalidation]). | Composed custom Controls inherit editor styling with local exceptions, but shared mutable resources and broad notification fan-out have real ownership/work costs. |

**Nara inference:** propose explicit defaults < nearest scope < component variant/size < local
override precedence, with declared state-layer ordering and a reset/removal rule. Scope values
must be immutable or owner-versioned and restored on early return; composition resolves them
without document mutation, listener installation or a new global UI authority.
Keep style dependencies separate from component incarnation, edit buffer and semantic document
identity. Color-only changes should not reshape text; font/line-height/density/locale changes
must invalidate affected intrinsic measurements, layout, hit/semantic geometry and paint.
Existing unchanged-frame reuse must include effective scoped inputs. A detached popup must choose
live inheritance or an opening snapshot explicitly: Open-GPUI captures the opening theme and uses
the visible committed anchor's displayed/layout bounds ([overlay adapter][n5-open-overlay]).
That is a useful policy example, not Nara's adopted popup lifetime or a cache-performance result.

### Layout, text overflow and popup placement are component contracts

**Source facts:**

- GPUI exposes min/max size, padding/margin, flex basis/wrap/grow/shrink and per-axis overflow
  ([box style][n5-zed-box]). Its text measurement receives known dimensions plus available
  space, separates wrapping from truncation and rejects truncated-layout reuse for an unconstrained
  intrinsic probe ([measurement][n5-zed-measure]). This is a concrete failure mode to test.
- ImGui offers `SameLine`, `BeginGroup`, `SetNextItemWidth`, `GetContentRegionAvail` and
  `AlignTextToFramePadding`; zero item size selects intrinsic defaults, negative size consumes
  remaining space ([layout API][n5-imgui-layout], [size resolution][n5-imgui-size]).
  Combo placement uses expected popup size; popup fitting tries the previous/preferred direction
  against allowed and avoid rectangles before fallback ([combo][n5-imgui-combo], [fit][n5-imgui-fit]).
- Godot combines computed minimum size with custom minimum componentwise. `BoxContainer`
  sums visible children's primary-axis minima plus themed separation, then distributes extra
  space by `SIZE_EXPAND` stretch ratios, clamping and redistributing at limits
  ([minimum][n5-godot-minimum], [box minimum][n5-godot-box-min], [stretch][n5-godot-stretch]).
  `SIZE_FILL` controls filling the allocation separately; Container owns child geometry and
  coalesces sort requests ([allocation][n5-godot-container]). Minimum-size invalidation propagates
  upwards and schedules an update rather than relying on repaint alone ([update][n5-godot-min-update]).
- GPUI anchors use laid-out child bounds before comparing/flipping attached placement and snapping into
  the viewport ([anchor][n5-zed-anchor]). Godot MenuButton starts from its screen rectangle,
  handles native/embedder transform and RTL alignment, and selects initial keyboard focus
  ([menu][n5-godot-menu]). A high z-index alone supplies none of this.

**Nara inference:** a component declares intrinsic/min/preferred/max size and baseline, shrink/
grow policy, padding/border accounting and wrap/clip/measured-ellipsis policy; display truncation
must retain full semantic text and source identity. Use child measurement already present to
prove toolbar overflow, aligned label/value rows, bounded scroll regions and narrow-window fallback.
Popup placement needs one explicit coordinate space, current anchor identity, available bounds,
measured content, preferred side/offset, flip/shift policy and bounded internal scrolling.
Trigger disappearance, scroll/resize/scale changes, dismissal and focus restoration must reconcile
through the original presented/input Host; a paint-only overlay cannot become independent authority.

### Typography, scale, locale and custom composition

**Source facts:** ImGui separates unscaled base font size, user font scale and DPI font scale;
`ScaleAllSizes` excludes fonts and warns against repeatedly rescaling rounded metrics
([fonts][n5-imgui-fonts], [scale][n5-imgui-scale]). GPUI converts measurement constraints across
device scale and snaps measured extents to device pixels ([scale][n5-zed-scale]).
Godot Button passes language/locale and inherited or explicit direction to text shaping;
translation/theme changes reshape and update minimum size ([shaping][n5-godot-shaping],
[notifications][n5-godot-text-update]). These facts do not qualify equivalent Nara behavior.

**Source facts:** ImGui's canvas example reserves a layout/interaction item with `InvisibleButton`
and draws through the window DrawList ([custom canvas][n5-imgui-canvas]). Godot's actual curve
plugin composes `HFlowContainer`, Button, numeric editor, MenuButton and custom `CurveEdit`,
refreshes themed metrics/icons, then installs the composite through `add_custom_control`
([composition][n5-godot-composite], [plugin][n5-godot-plugin]). These are concrete extension
ergonomics: ordinary controls and custom drawing participate in the same layout/style services.

**Nara inference:** expose equivalent composition outcomes through existing owners, not a copied
Godot node/resource graph, GPUI Entity model or ImGui global style stack. Keep density, user text
scale and native DPI independent; use admitted font identity/fallback, line height and baseline
for both measuring and painting. Locale/direction are explicit inputs even before full localization.
Missing glyphs, combining characters, CJK, mixed RTL/LTR labels, long paths and IME text need
declared behavior; a font atlas or a palette switch does not establish that behavior.

### Proposed typography and design-system contract inventory

**Proposed readiness requirements, not implemented features:** this evidence complements the
[style design inventory](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness).
It selects no public APIs or CSS/DOM engine. A recipe records **owner, input, units, precedence,
fallback, error, invalidation, lifetime and acceptance**. Cases below prompt qualification of the
selected slice; axes, emoji/color-font coverage and native forced-color integration require explicit
support/deferred states, not inferred support from a mature reference.

**Native mechanisms and their limits:**

- GPUI distinguishes family resolution from the actual fonts used in shaped runs. `Font` includes
  family, features, fallbacks, weight and style; `resolve_font` can use a default stack and panics
  if no candidate resolves. `baseline_offset` centers primary-font ascent/descent inside the
  supplied line height ([font system][n5r-zed-fonts]). On Windows, DirectWrite supplies line metrics
  and the glyph-run callback records the returned font face, including a face not originally
  created by GPUI ([platform implementation][n5r-zed-fallback]). Resolving a family successfully
  therefore does not prove cluster coverage, identical fallback metrics or portable line geometry.
- Godot's `Font` computes ascent/descent maxima across its fallback RIDs; changing fallback
  resources clears string caches and emits `changed` ([font resource][n5r-godot-font]).
  TextServerAdvanced retries failed shaped subruns with subsequent fonts, optionally system fonts,
  and accumulates actual font metrics and glyph offsets ([shaper][n5r-godot-fallback]).
  A conservative font-wide metric and a particular shaped line's metric are different contracts.
- ImGui's `MergeMode` combines glyph sources; `GlyphOffset` and advance limits align differing
  fonts/icons. `PixelSnapH` rounds advances and explicitly prevents fractional font sizes from
  working correctly; font data must remain alive for the atlas lifetime
  ([configuration][n5r-imgui-glyphs]). Glyph merging and raster sharpness alone do not establish
  contextual shaping or baseline compatibility.

**Small frontend comparison, as contract evidence:**

- Radix Themes separates text/code/emphasis/quote font families and exposes typography tokens
  ([typography][n5r-radix-type]). Its spacing scale is tokenized; theme scaling jointly changes
  spacing, font size and line height, whereas Nara's proposed independent user text scaling must
  remain a distinct input ([spacing/scaling][n5r-radix-space]). Buttons separate size, variant and
  `highContrast`; loading preserves normal size while disabling the button. Ghost buttons use
  negative margins for optical alignment while retaining interaction padding
  ([component contract][n5r-radix-button]). Copy the explicit decisions, not those layout tricks.
- Fluent distinguishes raw global tokens from semantic aliases, including compound typography
  and shadow values ([token layers][n5r-fluent-tokens]). W3C distinguishes forced user palettes from
  a preference for greater contrast, and reduced motion from an animation's visual style
  ([preferences][n5r-media]). Open-GPUI's resolver makes reduced motion a floor that an explicit
  component request cannot relax ([motion resolution][n5r-open-motion]).
- W3C sizing distinguishes min-content, max-content and available-space-dependent fit-content;
  they are not interchangeable meanings of "auto" ([intrinsic definitions][n5r-sizing]).
  WCAG 2.2 supplies comparison cases for text enlargement, spacing overrides, visible/unobscured
  focus and target size with exceptions. Its text-spacing criterion does not prescribe default
  spacing, and CSS-pixel target rules do not directly define native logical units
  ([criteria][n5r-wcag]). These are native design/acceptance prompts, not a conformance claim.

**Unit boundary:** font units-per-em resolve against authored logical font size, not glyph bitmap
height ([metric conversion][n5r-zed-fonts]). A relative design step must declare its base/mapping
before producing logical dimensions. Physical/device pixels here mean target raster pixels after
the native scale transform ([measurement conversion][n5-zed-scale]), not calibrated points or
millimeters. Zed density tuples and Radix scaling inform recipes; they establish no Nara browser
`px`/`rem` compatibility or inherited-root-font-size promise.

**Readiness and cost are observable dependencies:** GPUI font installation clears family
resolution and advances a generation; in-progress layouts may still finish with old fonts
([installation][n5r-zed-fonts]). Its line cache rejects old-generation reuse and rotates current/
previous frame entries; keys include text, size, actual font runs, wrap width and forced width
([cache][zed-lines], [keys and generation check][zed-line-key]). Godot exposes separate string/
paragraph cache capacities ([resource caches][n5r-godot-font]); no foreign cache proves Nara's cost.
GPUI's load cache records requesting views and notifies them on completion ([load lifetime][n5r-zed-assets]).
Its image element uses ready image dimensions for auto sizing and supports loading/error
replacements ([image layout][n5r-zed-images]). Thus resource completion can invalidate intrinsic
size, not just pixels; font admission, image decoding and GPU residency are separate readiness.

| Contract and concrete owner | Input, units and precedence | Fallback and error | Invalidation and lifetime | Proposed acceptance pressure |
| --- | --- | --- | --- | --- |
| Font identity/admission: `nara_text`; Host selects/discovers faces | Immutable face content and collection index, ordered fallbacks, features, locale/direction; font units resolve to authored logical size. Host supplies the finite ordered set; discovery stays outside text resolution. | Distinguish failed face admission, unresolved cluster and raster refusal; retain actual face provenance per glyph/run. Axes/synthesis require complete identity before support. | Font-set changes invalidate shaping, metrics and derived snapshots; published snapshots retain their admitted fonts. | Qualified Latin/CJK/combining cases use the same measured/raster face and bounded diagnostics; emoji/color fonts and axes expose actual coverage or deferral. |
| Line box and baseline: `nara_text` plus kernel measurement; component typography recipe | Resolve role/inherited/local size and line height to logical units; distinguish multiplier, ascent/descent, leading and ink bounds. | Declare extra-leading distribution and too-small-line-height behavior; do not silently clip fallback ink or substitute primary-font bounds for shaped metrics. | Font, text, line height and spacing invalidate intrinsic size/baseline and hit geometry; retain only with those inputs. | Mixed-font rows, diacritics, icons and editable caret align at a real baseline without state-driven height jumps. |
| Density and scale: component/Editor recipes plus native Host | Relative recipe metrics resolve to logical dimensions; user font enlargement, view zoom and monitor logical-to-device scale are distinct inputs. Apply each declared transform once. | Reject nonfinite/nonpositive scales; declare minimum readable geometry and unavailable preference handling. | Density/font scaling affects layout; device scale affects raster/snapping and presented geometry. Window/target changes cannot reuse stale transforms. | 100/125/150/200% monitor scale and independent text enlargement, including 200% as a comparison case; no cumulative rounding or double scaling. |
| Overflow and complete values: widgets/Editor plus semantic Host | Full source text, width and line budget; explicit wrap/clip/measured-ellipsis policy after typography resolution. | Missing ellipsis glyph and over-budget text need declared outcomes; complete values remain available through semantics and keyboard-accessible reveal/copy. | Text/font/width changes invalidate display measurement; truncation never rewrites semantic value, draft or source identity. | Long paths, RTL and narrow label/value rows remain identifiable and editable; a tooltip alone is insufficient. |
| Tokens, variants and interaction: component library/Editor; Host owns focus/preferences | Semantic roles, scoped theme, size/variant and combined state; use declared scope/state precedence and a separately qualified forced-palette policy. | Builtin fallback or explicit unsupported role/preference; disabled/read-only/loading differ behaviorally. Keep non-color focus/selection/error cues. | Color-only change invalidates paint; metric changes invalidate layout. Scope removal restores defaults; detached menus choose live or captured context. | Visible/unobscured keyboard focus across combined states; record which contrast themes and native preference paths were tested or deferred. |
| Icons, optics and motion: recipes plus text/render owners and Host | Logical icon envelope, optical offset, hit target and stroke; device snapping is distinct from logical layout. Reduced-motion preference is a floor. | Missing icon retains a meaningful label/semantic name; decorative animation may stop without losing pending/result meaning. | Asset/scale changes invalidate relevant geometry/raster; animation schedules end with visibility/owner lifetime and preference changes. | Fractional DPI, icon/text balance, stable hit boxes and static reduced-motion feedback; snapping does not round every text advance. |
| Intrinsic sizing and popup: kernel, component layout and original presented/input Host | Known versus available size, min/preferred/max, baseline/aspect ratio; explicit logical/screen/device anchor conversion and fit/flip/shift policy. | Unbounded probes cannot reuse truncated measurements; undersized viewport uses declared scroll/overflow/refusal, not invented geometry. | Content/font/image/anchor/scroll/viewport changes remeasure or reposition. Trigger removal closes attachment; focus restoration uses current identity. | Narrow toolbar, growing field and popup at every edge; indefinite constraints and native monitor changes preserve accepted input geometry. |
| Async readiness: Host asset owner, `nara_text`, presentation planner/backend | Pending/ready/failed font or image generation; placeholder extent is logical, decoded dimensions and GPU readiness are separate inputs. | Declare retain-old versus placeholder/reflow policy; rejected replacement keeps a coherent admitted state and exposes failure. | Completion invalidates dependent measures/layout/paint; obsolete results cannot replace a newer request. Consumer subscriptions detach; shared tasks/cache retention and GPU retirement follow their owners. | Delayed/failed font/image, completion after popup closure, changed fallback and device loss; no stale metrics, lost drafts or false ready state. |
| Work and caches: existing component/kernel/text/render owners under R11a/R11e | Attribute resolution, probes, shaping, raster and upload separately; keys include effective inputs, not revision alone. Measure counts/time/retained bytes. | Bound entries/bytes and negative-result policy; eviction/cache loss rebuilds equivalent output within existing admission limits. | Record hit/miss/eviction and invalidation reason; layout, glyph, image and target caches have separate lifetimes. | Cold/warm idle, hover, edit, resize and color/font/density changes; no effects during measurement and no claim of constant work from visible-node count. |

These evidence/limit mappings refine R11b's visual consumer; R11a retains baseline ownership and R11e
retains measured optimization. [ADR 0031][n5r-nara-fonts] still bounds font admission and editing;
shared text/kernel/render contracts do not move Editor recipes into the game's `nara_ui` adapter.
Select support per consumer and record deferred cells before expanding the catalogue.

### Complete visual consumer and readiness gates

**Nara proposal, not implemented:** use the independently drawn Editor's initial source workspace
to finish one coherent visual slice: document tabs and a compact toolbar,
selectable source/hierarchy rows, one editable property group with label/value alignment,
inline invalid/disabled/pending/result feedback, and one anchored actions menu. Use existing
selection/edit/Undo/Save/Reopen owners and exact results. Include one independently authored
composite row with custom decoration and scoped styling. A grid of disconnected widgets cannot
establish that these parts work together or that the Editor looks coherent.

| Readiness gate | Required evidence before claiming a reusable polished slice |
| --- | --- |
| Constraints and interaction | Widen/narrow through a documented minimum; long labels, missing values, all combined states, keyboard traversal, clipping/scroll boundaries and popup edges; repeated composition cannot duplicate an edit. |
| Visual quality | Review actual before/after captures at fixed content/size/fonts: surface hierarchy, alignment/baselines, density, icon/text balance, contrast, focus/error visibility and stable geometry; approve a coherent page before generalizing recipes. |
| Scope and extension | Same composite under default and local overrides; siblings remain unchanged, removal restores defaults, theme switch preserves drafts, and an independent consumer needs no engine-private styling or layout hooks. |
| Performance | Attribute composition/style resolution, intrinsic measurement/layout, shaping/raster, packet/upload and retained bytes separately at idle, hover, one edit, resize and theme/density change. Check bounded caches, cache loss and popup closure; report measured results, not foreign-framework speed claims. |
| Native and international text | Actual drawn executable at 100/125/150/200% scale, monitor-scale/font changes and compact/default density; CJK/fallback/RTL/long-path cases, accepted hit geometry, keyboard/focus/IME/clipboard and popup dismissal. Apply the existing [native qualification procedure](../verification/native-editor-qualification.md); state each tested OS and missing coverage. |

The resulting targets belong to the [UI design](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
and [R11b visual prerequisite](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite).
This study is evidence for those targets rather than production code or a qualification receipt.

### November 5 implemented-recipe audit and catalogue follow-up

**Evidence update:** the [styled workspace unit](../plans/2026-11-05-styled-source-workspace-plan.md)
has since implemented the initial slice. Its qualification and limits supersede the earlier
"missing"/"proposed" descriptions for that slice; they do not establish a complete catalogue.
This audit reads the November 5 working tree, including concurrent changes, and existing regression
definitions; it runs no product/native checks. All four local reference HEADs, origins and clean
worktrees were rechecked against the full pins above. No new frontend research was needed.

The [implemented recipe](../../crates/nara_ui_widgets/src/appearance.rs#L204-L274) already makes
concrete numerical choices. These are authored logical units, not physical pixels or a `rem` API:

| Recipe input | Implemented default |
| --- | --- |
| Body/control, Caption, Title | Font size/absolute line height: **14/20**, **12/18**, **16/24**; both multiply by `text_scale` in `0.5..=4`. Title changes size, not weight. |
| Small/Regular/Large control height at 1x text | Normal: **24/28/34**; Compact: **22/24/30**. Every size retains Body typography. Height is `max(minimum, scaled_line_height + 2 * vertical_padding)`; Regular at 2x therefore becomes **48/44** for Normal/Compact. |
| Spacing, Normal/Compact | Horizontal control padding **10/8**, inter-control gap **6/4**, panel inset **12/8**. Final vertical padding centers the explicit line box within the computed height. |
| Outline geometry | Radius **4**, stroke **1** in both densities and all recipe states. The plan's **16** icon envelope is not an implemented icon recipe. |

**Assessment and bounded follow-up:** reference comparisons below concern concrete contracts,
not a requirement to reproduce another framework's entire theme system.

| Boundary | Implemented evidence | Decision or support still needed before catalogue expansion |
| --- | --- | --- |
| Typography roles and units | [`UiAppearance`](../../crates/nara_ui_widgets/src/appearance.rs#L139-L144) scopes palette/density/text scale; [`TextTypography`](../../crates/nara_text/src/layout.rs#L13-L66) carries size and optional absolute line height. The native [font policy](../../editor/src/platform/drawn.rs) selects Fira Sans Regular plus an optional explicit CJK face. Zed's actual [text-size mapping][n5a-zed-type] also uses 14/12/16 steps. | Keep these as the selected desktop baseline. Small/Large currently change geometry only, unlike Open-GPUI's [per-size typography scales][n5-open-recipes]; record that choice. Role-specific family/weight/features/letter spacing and locale are not appearance inputs. Introduce one only for a named consumer, with actual admitted-face identity rather than synthetic weight or an implied CSS inheritance contract. |
| Leading, baseline and hit target | Fields use actual line-block extents, an adjusted text origin and the original baseline for paint/Point/IME ([geometry](../../crates/nara_ui_widgets/src/text_field.rs#L426-L472)); [`UiControlGrid`](../../crates/nara_ui_widgets/src/control_grid.rs) aligns measured child baselines. [Regressions](../../crates/nara_ui_widgets/tests/text_field.rs#L119-L169) cover fallback and a 20-size/6-height line. | `20 - 14` is not measured leading or a baseline. Retain the text owner's geometry; GPUI's [ascent/descent centering][n5r-zed-fonts] is a comparison, not a replacement formula. Recipe height is an allocation target: fields retain their measured minimum, while [buttons](../../crates/nara_ui_widgets/src/appearance.rs#L486-L510) only supply height when Auto and explicitly Clip. Declare overflow for shorter explicit button allocations; do not advertise the line box or icon envelope as the interaction target. |
| Combined states and behavior | [Button resolution](../../crates/nara_ui_widgets/src/appearance.rs#L310-L375) applies variant, selection, disabled neutralization, then enabled hover/press; selected interaction blends retain a distinct fill and enabled focus owns the border. [Fields](../../crates/nara_ui_widgets/src/appearance.rs#L276-L307) retain invalid tint under focus. Existing [combined-state regressions](../../crates/nara_ui_widgets/tests/appearance.rs#L107-L246) cover these combinations. | Resolve a concrete disagreement: the recipe paints pressed whenever `pressed`, including outside hover, while [`visual_state`](../../crates/nara_ui_widgets/src/button.rs#L121-L135) requires `pressed && hovered`; ImGui's [ButtonEx][n5-imgui-button] likewise uses held-and-hovered. Specify drag-out/reentry and primary/destructive hover emphasis, since unselected variants share interaction fills. Keyboard focus-visible, read-only and loading are not separate recipe inputs; define behavior and stable geometry before exposing them as variants. |
| Stroke, icons and optical alignment | State changes keep the same radius/stroke; explicit [`with_metrics`/`with_visual`](../../crates/nara_ui_widgets/src/appearance.rs#L462-L477) allow local alternatives. The shared button recipe emits text, not an icon/label composition. Zed separately [measures icon envelope and surrounding padding][n5a-zed-icons]; Godot [draws focus independently of the current state surface][n5a-godot-focus]. | Add one real icon-plus-label consumer before claiming icon support: choose envelope, gap, optical offset, missing-asset label and minimum hit allocation. Declare whether stroke thickness remains logical or uses a device hairline; review 1.25x/1.5x output without rounding text advances or moving the hit box for optical correction. A one-unit focus-colored border is the current cue, not a separately implemented outer focus ring. |
| Scope, overrides and detached content | Callers pass an effective immutable value; [independent row regressions](../../tests/fixtures/ui-module-consumer/tests/styled_property_row.rs) cover sibling isolation, override removal and retained drafts. Local style builders apply after recipe resolution. The current [Shell](../../editor/src/frontend/drawn/shell.rs) includes appearance in frame reuse and cancels its input/menu when appearance changes. | This is explicit value passing, not an automatic app/window/subtree cascade. Document builder order when overriding metrics and visuals; final local overrides can replace state cues. Shell currently closes the menu on appearance change, unlike Open-GPUI's [captured opening theme][n5-open-overlay]. Keep that concrete policy until a consumer requires live inheritance or captured styling across a change. |
| Text scale versus DPI | Density changes spacing; user text scale changes typography and required height, leaving horizontal padding/radius/stroke unchanged. [Shared geometry tests](../../crates/nara_ui_widgets/tests/appearance.rs) exercise 1/1.25/1.5/2 native scales; the unit records actual Windows/DX12 at 1.5 with both densities and 1x/2x text. | Preserve independent inputs and derive from original logical values; ImGui explicitly [separates font scales][n5-imgui-fonts] and warns against [repeated rounded style scaling][n5-imgui-scale]. Native monitor transitions, persistent preference adoption and the full admitted text-scale range remain unqualified. Do not infer a UI zoom or calibrated-point contract from these numbers. |
| Contrast and forced palettes | A [default-role test](../../crates/nara_ui_widgets/tests/appearance.rs#L82-L104) checks six opaque foreground/background pairs at ratio 4.5 using weighted raw RGB channels. [Palette admission](../../crates/nara_ui_widgets/src/appearance.rs#L63-L113) validates finite `0..=1` channels; the kernel warning color remains fixed outside the customizable palette. | Document the channel transfer/compositing assumptions before extending contrast calculations to supplied colors. Current tests do not cover arbitrary palettes, translucent combinations, focus/selection/border contrast or native forced colors. Select tested semantic role pairs and non-color error/selection cues; explicitly defer forced-palette integration rather than equating it with a high-contrast palette. The existing frontend/W3C inventory already supplies that distinction. |
| Resource readiness and cache effects | Font sets are explicit and immutable; [field reconfiguration](../guides/editable-text.md#local-editing) atomically admits display, preedit origin and bounded history. The [compiler cache](../../crates/nara_ui_kernel/src/compiler/text.rs#L16-L112) bounds successful layout/width observations to at most `min(max_nodes, 256)` plus source/glyph limits. Color-compatible requests reuse shaping; layout and width observations remain distinct. | Live native font-family switching and asynchronous font/icon readiness are not supplied by `UiAppearance`. A future resource consumer must choose retain-old/placeholder/reflow and coordinate text/compiler/planner provenance; GPUI's [completion notification][n5r-zed-assets] shows why readiness may invalidate measurement. Preserve bounded admission on hits. The unit's measured color change still recompiles nodes and measures children: warm shaping reuse establishes neither paint-only updates nor an end-to-end frame budget. |

**Recommended order:** reconcile drag-out/state precedence first; then qualify one mixed-font,
icon-plus-label row with actual baselines, optical alignment and contrast in the existing workspace.
Keep the shipped values while recording intentional size/overflow/scope choices. Font preference
and asynchronous resource policies should follow a concrete consumer under their existing owners;
they do not justify a universal style registry or another layout/text engine. Performance follow-up
remains with the unit's owner-local observations and R11a/R11e, not this source audit.

The subsequent correction `04da077` resolves the observed drag-out disagreement: the recipe now
requires hover for the pressed fill, while the original reducer retains the held press and focus.
The existing live-journal regression now covers drag-out/reentry, stable geometry/focus and no
activation during motion. The shared kernel/widgets suite passes 218/218
(`6ec4e35a-36dc-45c2-8d2b-84de2a7e41ae`). The same correction withholds disabled/inert semantic
actions without removing their descriptive state. Numeric recipes and unsupported typography/
icon/preference cells above retain their stated scope; this correction makes no new visual,
native-platform or performance claim. The [recipe guide](../guides/editable-text.md#scoped-appearance-and-measured-controls)
records actual values and the [operations unit](../plans/2026-11-05-presented-ui-operations-plan.md)
owns the affected complete native successor evidence.

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
[open-measured-tests]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/virtualizer.rs#L1788-L1880
[n5-zed-colors]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/theme/src/styles/colors.rs#L15-L115
[n5-zed-spacing]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/styles/spacing.rs#L29-L54
[n5-zed-sizes]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/components/button/button_like.rs#L451-L473
[n5-zed-variants]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/components/button/button_like.rs#L125-L153
[n5-zed-button]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/components/button/button_like.rs#L756-L850
[n5-zed-states]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/div.rs#L3467-L3585
[n5-zed-text-scope]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L4359-L4372
[n5-zed-typography]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/style.rs#L435-L482
[n5-zed-box]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/style.rs#L180-L276
[n5-zed-measure]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/text.rs#L654-L753
[n5-zed-anchor]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/anchored.rs#L161-L291
[n5-zed-scale]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/taffy.rs#L235-L273
[n5-open-scales]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/tokens.rs#L308-L390
[n5-open-recipes]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/theme/recipes/design.rs#L9-L40
[n5-open-variants]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/button.rs#L25-L37
[n5-open-button]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/button.rs#L438-L520
[n5-open-resolver]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/theme/resolver.rs#L12-L24
[n5-open-scope]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/theme/scope.rs#L64-L134
[n5-open-snapshot]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/theme/snapshot.rs#L117-L144
[n5-open-overlay]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_components/src/overlay/adapter.rs#L220-L284
[n5-imgui-style]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L1564-L1592
[n5-imgui-button]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui_widgets.cpp#L788-L843
[n5-imgui-color-stack]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L3747-L3783
[n5-imgui-var-stack]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L3841-L3898
[n5-imgui-disabled]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L8531-L8556
[n5-imgui-layout]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.h#L552-L603
[n5-imgui-size]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L11962-L11978
[n5-imgui-combo]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui_widgets.cpp#L2035-L2061
[n5-imgui-fit]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L13006-L13077
[n5-imgui-fonts]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.h#L519-L537
[n5-imgui-scale]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.cpp#L1662-L1682
[n5-imgui-canvas]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui_demo.cpp#L10670-L10700
[n5-godot-generation]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/themes/editor_theme_manager.cpp#L210-L230
[n5-godot-merge]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/themes/editor_theme_manager.cpp#L689-L709
[n5-godot-spacing]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/themes/editor_theme_manager.cpp#L392-L438
[n5-godot-button]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/themes/theme_modern.cpp#L467-L499
[n5-godot-margins]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/button.cpp#L59-L96
[n5-godot-variation]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/resources/theme.cpp#L1405-L1421
[n5-godot-theme-assignment]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L3842-L3888
[n5-godot-types]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/resources/theme.h#L49-L65
[n5-godot-lookup]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L3993-L4014
[n5-godot-owners]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/theme/theme_owner.cpp#L178-L261
[n5-godot-overrides]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L4271-L4312
[n5-godot-invalidation]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L4926-L4935
[n5-godot-minimum]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L2231-L2247
[n5-godot-min-update]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/control.cpp#L1971-L2005
[n5-godot-box-min]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/box_container.cpp#L319-L356
[n5-godot-stretch]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/box_container.cpp#L162-L204
[n5-godot-container]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/container.cpp#L133-L191
[n5-godot-menu]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/menu_button.cpp#L81-L117
[n5-godot-shaping]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/button.cpp#L568-L579
[n5-godot-text-update]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/button.cpp#L185-L208
[n5-godot-composite]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/scene/curve_editor_plugin.cpp#L961-L1022
[n5-godot-plugin]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/editor/scene/curve_editor_plugin.cpp#L1031-L1048
[n5r-zed-fonts]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/text_system.rs
[n5r-zed-fallback]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_windows/src/direct_write.rs
[n5r-godot-font]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/resources/font.cpp#L111-L375
[n5r-godot-fallback]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/modules/text_server_adv/text_server_adv.cpp#L7101-L7520
[n5r-imgui-glyphs]: https://github.com/ocornut/imgui/blob/9d0b72573a27d860280811d58ef9ed62f4d46c05/imgui.h#L3708-L3741
[n5r-radix-type]: https://www.radix-ui.com/themes/docs/theme/typography
[n5r-radix-space]: https://www.radix-ui.com/themes/docs/theme/spacing#scaling
[n5r-radix-button]: https://www.radix-ui.com/themes/docs/components/button
[n5r-fluent-tokens]: https://fluent2.microsoft.design/design-tokens
[n5r-media]: https://www.w3.org/TR/mediaqueries-5/#forced-colors
[n5r-open-motion]: https://github.com/Latias94/open-gpui/blob/689db5b49b33a5b2e64ba332bff72c98e70092bb/crates/ui_core/src/tokens.rs#L385-L402
[n5r-sizing]: https://www.w3.org/TR/css-sizing-3/#intrinsic
[n5r-wcag]: https://www.w3.org/TR/WCAG22/
[n5r-zed-assets]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/asset_cache.rs#L14-L66
[n5r-zed-images]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/img.rs#L302-L431
[n5r-nara-fonts]: ../architecture/adr/0031-text-and-font-strategy.md
[n5a-zed-type]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/styles/typography.rs#L132-L160
[n5a-zed-icons]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/src/components/icon.rs#L72-L109
[n5a-godot-focus]: https://github.com/godotengine/godot/blob/fefec82d74b46f5e6cc31f373071058f80a5cbee/scene/gui/button.cpp#L221-L230

## November 5: Supplied v0.4 Design Kit Assessment

**Evidence and scope:** local source/visual review only, against the November 5 working tree.
The supplied root is `D:\Frankorz\Downloads\nara-design-kit-v0.4-fixed\nara-design-kit-v0.4`.
Inspected text sources are `README.txt`; `nara-editor-design-spec-v0.4.md`, chapters 02-13,
16-17, 21-29, 32, the chapter 36 image-boundary subsection and 37-38;
`nara-design-tokens.json`, including proposed-contract metadata, appearance, component,
document/context/docking and task mappings; `design-process/image-index-v0.4.json`; and
`design-process/image-prompts-v0.4.json`, its provenance notes and the C01/C02/T01 prompts and
corrections. The latter declares built-in image generation, not an implemented application capture.
No external-product claims or links in the supplied specification were independently revalidated.

Images actually opened with `view_image`, relative to that root:

- `images/nara-components-properties-v0.4.png` (C01).
- `images/nara-components-navigation-feedback-v0.4.png` (C02).
- `images/nara-scene-inspector-v0.4.png` (T01).
- `images/nara-components-v0.2.png` (inherited component comparison).
- `images/nara-docking-scene-shader-v0.3.png` (adjacent document Views).
- `images/nara-docking-scene-game-float-v0.3.png` (multi-view/native-window direction).
- `images/nara-profiler-console-v0.4.png` (diagnostic context and dense tables).

At the user's subsequent direction, the [complete local reference](../design/editor-reference/README.md)
preserves **all 32 supplied files, 29,897,315 bytes (28.5 MiB)** before the English adoption README:
all sixteen UI PNGs, the original Chinese specification, tokens, `README.txt`, assets, figures and
design-process records. Every supplied file was byte-compared with its local counterpart; none
was missing or changed, and all six hashes in the adoption README match. Original relative links
retain their directory structure. The preserved source remains reference material; the English
README maps current adoption and exceptions. This sidecar edits only this research assessment.
Visual judgments cover the seven opened images above, not all sixteen merely because they exist.

**Visual assessment / inference:** the continuous work surfaces, narrow separators, compact
toolbars and aligned property columns suit a dense mouse/keyboard Editor. C01 makes mixed values,
source overrides, invalid input and read-only/disabled controls legible as different facts; C02
adds stable selection counts, typed picking and recoverable feedback. T01 provides useful page
hierarchy. The atlas's surrounding specimen boxes are comparison framing, not a recommendation
to wrap every production property group in a large card. Generated highlights, gradients, icon
shapes, tiny text and pixel dimensions are not production recipes. The 3D scene, Mesh/Light
examples and specialist tools do not expand the current 2D product scope or authorize a rebrand.

### Adoptable Rules And Concrete Differences

This compares existing code with recommendations; the After column does not report implemented
changes. It complements the [existing visual contract inventory](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
and [actual recipe guide](../guides/editable-text.md#scoped-appearance-and-measured-controls).

| Before: current implementation | After: recommended integration | Why / exact kit chapters |
| --- | --- | --- |
| `UiAppearance` uses 14/20 Body, 12/18 Caption and 16/24 Title; the Host explicitly admits Fira Sans Regular and a configured fallback. | Keep these qualified defaults while comparing the proposed 13/20 Body, 12/18 numeric and 600-weight headings on real admitted Latin/CJK faces. Record actual font identity/features; do not synthesize weight or assume system-family discovery. | 04, 11-12, 26: an English raster atlas proves neither CJK metrics, tabular figures, IME nor accessible text enlargement. |
| Regular controls already resolve to 28/24 Normal/Compact minima, radius 4 and stroke 1; Compact Small can be 22 high. Normal gap is 6, Compact gap 4. | Reuse measured minima and baseline alignment. Treat the kit's 4/8 spacing rhythm, 28/24 rows and 24-unit hit minimum as explicit Editor recipe choices; reconcile the 22-unit case rather than claiming all existing controls meet 24. Reserve units/reset/status slots and wrap narrow rows without reducing text. | 02, 04, 17, 28.1, 29: stable value columns matter more than tracing raster coordinates. Panel widths and the 1600-by-1000 reference canvas are examples. |
| The library palette has one border role, shared hover/pressed fills and a fixed kernel warning color; it is not the supplied graphite/teal palette. | Map sRGB HEX through the declared render conversion/alpha/output contract. Keep structural dividers distinct from necessary control borders, and error text distinct from destructive fill. Add library-local warning/info or variant interaction roles only when a real consumer needs them. | 03, 12, 16-17: primary/secondary text, on-accent text and state cues need contrast checks on actual composited surfaces. The kit's opaque-pair calculations do not certify Nara output. |
| `field(focused, enabled, invalid)` keeps invalid tint under a focused border; radius/stroke stay fixed. There is no separate 2-unit outer focus ring with a 1-unit offset, nor complete read-only/loading recipe inputs. | Preserve independent focus, selection, source, validity and availability. Qualify an outer focus cue with clipping and unchanged hit geometry if adopted. Keep read-only focus/copy distinct from disabled submission; retain invalid drafts and expose pending/missing/empty references separately. | 05-11, 28.1-7: these are combined behavior/visual contracts. The atlas shows independent specimens, not several simultaneously focused popups. |
| Density, user text scale and Host device scale have separate owners; appearance does not supply full UI zoom, icons, motion or persisted preferences. | Keep those inputs independent and derive from original logical values. Use actual line/ink/baseline measurements at fractional DPI and enlarged text. Introduce icon-plus-label composition and optional motion through their existing owners; pressed/focus/input feedback remains immediate. | 04, 12, 26: the kit's generic user UI scale must not silently replace the implemented text-scale contract. OS IME, monitor transitions, reduced motion and accessibility require their own evidence. |

**Ownership and reuse:** chapters 10-13, 21-26 and 28 agree with preserving stable identities,
local drafts, explicit command targets and owner/version checks. Their names such as
`DocumentStore`, `ViewInstance` and `ActionContext` are conceptual design vocabulary, not new
Rust API requirements. Under ADR0107/0118, document/history owners retain mutation, frontends
retain drafts/control state, the kernel retains measurement and presented geometry, and the
original Host retains native input, IME, GPU resources and presentation. Automation uses the
original presented-operation ports; a shared command concept does not require a generic bus.
Reuse recipes selectively for game UI without importing Editor docking, document state or its
desktop theme into `nara_ui`. Independent libraries may keep their own roles and controls.

**Plugins and custom drawing:** chapters 13, 25 and 28 support common property containers and
domain validation with specialized canvases. Extend the existing scoped state, child measurement,
captured geometry, semantics and original input/render seams through a concrete consumer.
The pictured curve, gradient, timeline and profiler are design pressure; finite mesh drawing
does not establish their gestures, keyboard support or plugin lifecycle. The kit does not prove
arbitrary native-plugin unloading, a complete widget catalogue or a new renderer/toolkit.

**Diagnostics and future docking:** carry task phase, actual target/input revision, durable
failure details, copy/locate actions and truthful cancellation into existing owner projections
(25, 28.6-7, 32, 37). A toast must not be the sole failure record; profiler timings are examples.
For future docking (02, 21-23, 26), keep group-local tabs distinct from local mode switches,
moving a view distinct from duplicating it, and view closure distinct from document/Runtime
termination. Preserve personal layout separately from content history, with reachable recovery
for missing plugins/displays and cancelled migration. Respect the existing explicit close
protocol rather than implementing the kit's last-primary-view rule from a screenshot. Fit/1:1
output mapping, capture ownership and native multi-window readiness remain separate capabilities.

### Lifecycle, Save And Input Conflicts To Keep Explicit

The supplied specification is a proposed design, including its behaviors. These differences
must be reconciled before implementation rather than treated as missing palette tokens:

| Kit rule | Current owning contract / inspected evidence | Adoption boundary |
| --- | --- | --- |
| Chapter 21 closes a document when its last primary editing View closes; closing a floating window coordinates those View closures. | [ADR0118 panel lifetime](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#panels-layouts-and-render-views-are-distinct) says panel close does not close its document. [Document close](../guides/editor-documents.md#save-reopen-and-close) uses explicit `EditorWorkspaceIntent::CloseDocument` and the original Save/Discard/Cancel protocol. | Retain Document/View separation and group-local tabs, but do not silently add last-view document closure or infer semantics from a native close button. A different product policy needs an explicit owning-contract decision. |
| Chapters 21 and 24 coordinate unfinished buffers during layout replacement, close and Save; Save admits valid inputs and blocks the affected document for invalid/composing/conflicting input. | The [panel lifecycle](../guides/editor-panels.md#instance-target-and-presentation-lifetimes) retires local state on actual hide/close, target/resolved-document change and layout reset/restore. [Inspector editing](../guides/editable-text.md#native-editor-integration) retains unapplied drafts across blur, field filtering/collapse and Inspector-tab changes, but publishes through explicit Apply/non-composing Enter; Save captures accepted document content. | Distinguish hidden controls from a retired panel. Do not claim Save persists local drafts, or that layout reset preserves them. A future cross-panel pending-input census/recovery policy belongs to original frontend/document/close owners, not the token loader or a second draft store. |
| Chapter 06 validates on blur and describes Escape restoring the edit-start value; chapters 28.1-3 later qualify cancellation after external change. | [Field lifetime](../guides/editable-text.md#field-input-lifetimes) forbids Apply on typing, composition completion or focus loss. Blur revokes the input token and cancels preedit; dirty/conflicting input remains recoverable. | Blur validation may explain a local draft but must not publish it. Escape removes only the current composition/draft/preview; it cannot write an obsolete edit-start snapshot over newer document state. |
| Chapters 05, 11 and 28.1 require read-only text to be focusable/copyable, while disabled actions skip ordinary navigation; focus/selection/error coexist. | `UiTextField::set_enabled(false)` revokes text input; `UiAppearance::field` has enabled/focused/invalid inputs, not a separate read-only interaction mode. The [current Inspector](../guides/editable-text.md#native-editor-integration) provides `Copy retained input` for complete drafts, including incompatible or unprojected fields, excluding preedit. | Disabling editing is not a selectable read-only implementation. Retained-draft Copy does not establish generic copy/select for every read-only committed value. Add such a consumer through existing text/clipboard/semantic owners; preserve clipboard-failure feedback and exact source identity. |
| Chapters 21-22 and 26 preserve View identity/binding across moves, with per-window focus, popup and IME state. | Existing panel state survives compatible relocation, but [native input scopes](../guides/editor-panels.md#native-image-panel-attachment) and presented observations retain their original endpoint, mapping and text lifetime; actual retirement invalidates them. | Preserve the View's document binding, not old native capture or event authority. Reconcile focus, release capture and remap popup/IME/input on migration; multiple native windows and monitor transitions still need their own qualification. |

### Complete-Package README Review And Rule Map

The imported README correctly records the 13/20-versus-14/20 difference, sRGB boundary, unchanged
2D scope, proposed schema and unsupported appearance inputs. Three small documentation additions
are recommended for its owner; this assessment does not edit that directory:

1. Record generated-image provenance from `design-process/image-prompts-v0.4.json` and license
   status: no explicit kit license, copyright owner or redistribution terms were found in its
   file inventory or inspected source metadata. User supply is provenance, not a declaration
   that the repository's software license covers these references. No public source URL or
   model/version is provided; do not invent one.
2. Adjust the implementation-authority sentence: Accepted ADRs own architecture contracts;
   current source/guides and qualification records establish supported behavior; plans organize
   work. A design package or plan does not independently approve a capability.
3. Extend its English mapping with the lifecycle/Save/read-only exceptions above and these
   precise chapter/subsection references. The complete original Markdown is now retained with
   its image links; the English map should summarize selected rules without translating all
   38 chapters or presenting their future scope as implemented.

| English rule to retain or extract | Original specification chapters/subsections |
| --- | --- |
| Continuous surfaces, stable columns, nominal density, measured wrapping and narrow-layout recovery | 02 space shortage; 04 metrics; 17 density/alignment and surfaces/boundaries; 28.1 property rows; 29 workspace layout |
| Semantic colors, non-color cues and explicit sRGB/compositing with actual contrast checks | 03; 12 color/compositing; 16 opaque contrast pairs; 17 color/dynamic states |
| Invalid/local drafts, mixed-axis scope, source override versus dirty state, read-only/copy, reference states and focus restoration | 05-11; 28.1-5 and 28.7; 29 visible and missing states |
| Separate DPI/text scale/density, actual fallback metrics, grapheme/IME behavior and original input targets | 04; 11-12; 22 command resolution; 26 keyboard/semantics/localization |
| Library composition/custom drawing without copied document, task or platform authorities | 13; 25 plugin contracts and reload boundaries; 27 implementation responsibilities; 28.8 reuse |
| Durable diagnostic context, truthful progress/cancellation, stale-result refusal and clear empty/error states | 25 task/diagnostic model; 28.6-7; 32 capture selection and log/error recovery |
| Future group-local tabs, explicit binding/close/move/duplicate policies, layout recovery and Game output/input mapping | 21-23; 26 window-level state; 27 T13-T17 |
| Mockup and acceptance limits, selected-image provenance and remaining capability gaps | Introduction; 37 image/spec coverage and proposed acceptance; 38 versioned image index |

**Recommended integration:** use the complete imported kit to review one existing complete 2D
workspace and one independently composed property row with icons, long Latin/CJK text, units,
validation and recovery. Compare fixed-content captures at both densities and independent
text/DPI scales before adopting palette or metric changes. Preserve existing field capacity,
draft/preedit/history and exact presented-operation results. This review adds no native,
functional, accessibility or performance certification and does not rerun Cargo or formatting.

## November 5: Allocation And Encoded Design Colors

The next [measured-property unit](../plans/2026-11-05-measured-property-composition-plan.md)
addresses a concrete consumer gap: the field previously captured viewport/scroll/selection/caret
geometry before its parent chose the final allocation. A reusable property row must measure its
actual children and place the original editable owner at the resulting size. Retaining an editing
buffer does not make an earlier width a valid paint or input dependency. Compare GPUI's
[`Element` phases](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/element.rs)
and [text element](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/text.rs):
layout requests determine size, while prepaint receives final bounds. Nara's existing exact child
placement resolves the subtree again; immutable field and viewport descriptions can use that
owning path without introducing a second paint phase, tree or input authority.

The pinned [input example](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/examples/input.rs#L493)
requests a full-width line, computes text/caret/selection in prepaint, and records layout/bounds
for later point lookup. It supplies no complete long-field scrolling implementation. The actual
[Editor prepaint](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/editor/src/element.rs#L8681)
derives its text viewport after gutter/margin/scrollbar allocation. Its `PositionMap` keeps a
snapshot and hit geometry; gutter hit testing intentionally differs from the text drawing origin.
[Editor IME range lookup](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/editor/src/input.rs#L3088)
instead recomputes from current editor facts and captured element bounds. Those mechanisms do not
prove exact displayed-snapshot input admission. Nara keeps its original accepted-frame projection,
paint-origin point mapping and native text-token checks, including focus changes before repaint.

GPUI's line-layout cache keys text, font size/runs, wrap and forced width and resets on font
generation change. Its view replay also restores dependency/interaction membership, not just paint
records. Reuse these distinctions in Nara's measured-work qualification; do not collapse shaping,
placement, description identity, displayed-input authority and GPU output into one cache. The
previous [phase-validity review](#november-5-phase-validity-and-measurement-retention) retains the
late-opacity replay caveat. Repeated compatible field composition should preserve its immutable
layout descriptions while complete changed text remains visible in the authored child snapshot.

The accompanying color review traces the original [`UiColor`](../../crates/nara_ui_kernel/src/style.rs),
[packet lowering](../../crates/nara_ui_render/src/planner/lowering.rs),
[quad shader](../../crates/nara_render_wgpu/src/sprite.wgsl),
[mesh shader](../../crates/nara_render_wgpu/src/ui/mesh.wgsl) and
[drawn native composition](../../editor/src/platform/drawn/graphics.rs).
RGB travels unchanged as linear sRGB; source colors carry straight alpha. Glyph coverage changes
alpha, and an sRGB image texture decodes its own source RGB when sampled. Straight-source
source-over into a transparent panel produces associated output; the drawn compositor consumes
that output with premultiplied blending and an sRGB window attachment encodes the result.
[ADR0092](../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md) owns this contract.
An explicit sRGB8-to-linear constructor belongs at color authoring, before packet construction;
conversion in brushes, shaders or every paint would duplicate it. Existing linear palette values
keep their meaning, and importing the design JSON is not selected.

Two limits prevent a palette-fidelity claim. The current panel is linear `Rgba8Unorm`: quantization
can change dark encoded colors appreciably. Formula-only examples are sRGB8 gray 8, decoded to
about 0.002428 and quantized to linear byte 1, then encoded near 13; gray 32 encodes near 34 after
its intermediate quantization. These are deductions, not GPU measurements. The Dear reference
adapter also uses the locked `dear-imgui-wgpu` 0.21.0 `GammaMode::Auto` sampling/blending path,
which is not the independently drawn compositor's linear associated-image path. Existing opaque
owned-panel backgrounds do not establish gamma parity or generic transparent embedding.

Before adopting the graphite/teal defaults, qualify actual swatches through both the original
panel and the final native composite, including attachment quantization, half-transparent layers,
glyph/rounded coverage, arbitrary mesh and tinted linear/sRGB images. Existing
[panel readback captures](../verification/native-editor-qualification.md) explicitly exclude final
swapchain/OS capture. GPUI's
[`rgb`/`rgba`](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/color.rs)
divide encoded bytes by 255, and its
[WGPU format selection](https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_wgpu/src/wgpu_renderer.rs)
prefers a non-sRGB surface. That implementation is useful comparative evidence; its constructor
semantics cannot be copied into Nara's declared linear color type.

An additional local source check pins Bevy `4c344352e063` and Godot `fefec82d74b4`, with no changes
in the inspected reference files. Bevy separates `Srgba`/`LinearRgba`, preserves alpha through
conversion, and supplies linear values to PBR and UI extraction. Godot exposes an author-facing
sRGB `Color` and explicit `srgb_to_linear`; its Canvas renderer chooses conversion according to
the target's HDR policy. Bevy also exposes a compositing-space choice. These engines therefore
support explicit pipeline semantics, not a claim that every mature 2D/UI path has one universal
blend space. Nara keeps ADR0092's selected linear SDR path. Source locations are
`bevy_color/src/srgba.rs`, `bevy_pbr/src/pbr_material.rs`, `bevy_ui_render/src/lib.rs`,
`bevy_camera/src/components.rs`, Godot `core/math/color.h` and
`servers/rendering/renderer_rd/renderer_canvas_render_rd.cpp` under their respective `repo-ref/`
trees. Normal/data textures retain explicit non-sRGB formats; future HDR output and intermediate
precision require their own complete render consumer.

### GPUI Reuse Versus A Local Fork

For the current complete consumer, prefer demonstrable reuse of GPUI mechanisms and independently
portable implementations over a wholesale local copy. The pinned GPUI crate is an integrated
App/entity/window/dispatch/text/render framework, not a component catalogue that can be copied
without its execution assumptions. Its `request_layout`/prepaint/paint interfaces, line caches,
view replay and input handlers carry those dependencies. Copying the complete source creates a
maintained fork and an upstream reconciliation cost; it does not supply Nara documents, plugin
activation, docking or graph-domain authoring.

If a complete frontend needs that execution model, compare a direct GPUI dependency as one
coherent frontend against Nara's own composition through the existing public panel/Host contracts.
Use complete editing/extension workflows and matched work/memory/native evidence to choose;
existing investment is not a reason to reject the better total-cost result. Independently portable
algorithms may be adopted with pinned provenance, applicable notices and focused equivalence
tests. Do not port Zed's entire editor/domain model merely to acquire its text cache or controls.
This assessment is comparative design context, not adoption of another native owner or a license
review of arbitrary Zed files.

## November 5: GPUI Source Feasibility And Reopened Candidates

**User-supplied context:** resuming maintenance of the user's Open-GPUI extraction is the first candidate.
Its dependency/license processing is already done; maintaining GPUI is acceptable. Pinned upstream
adoption remains a comparison candidate; this research selects neither route. The historical
["Embed a complete reference toolkit" exclusion](#alternatives-under-the-nara-owned-ui-target)
and independent-runtime preferences describe their original scope, not this reopened choice.

**Evidence scope:** `repo-ref/zed` HEAD was `dd3437011469d5c2ec8c821a84cc999979625697`, clean. Facts describe
**Zed upstream only**, not Open-GPUI's execution/backend or dependency/license processing. No Cargo,
build, native input, benchmark or reload experiment was run. Platform/render interoperability is a
separate investigation; this section neither resolves it nor establishes cost or impossibility.

### Custom Elements And State Are Real Extension Mechanisms

**Source facts:** `Render::render(&mut self, &mut Window, &mut Context<Self>) -> impl IntoElement`
builds an entity-backed view; `RenderOnce::render(self, ...)` consumes a recipe. `Element` exposes
`request_layout`, `prepaint` with final bounds, and `paint`, phase-local state and scoped identity;
the default accessibility role is `None` ([contracts][zed-element]). These are source interfaces, not a dynamic ABI.

`canvas` supplies two `FnOnce` callbacks with final bounds and a phase payload; it requests styled
layout, has no ID, and implements no child composition ([canvas][z5-canvas]). Custom elements can
request measured layout, insert hitboxes and draw paths ([layout/hitboxes][z5-layout-hit],
[paint][z5-path-paint], [PathBuilder][z5-path-builder]). **Inference:** a graph/node tool can combine
ordinary children with custom drawing; node/edge semantics, selection, commands and Undo remain its work.

`Entity<T>` retains state independently of rebuilt descriptions. `with_element_state` keys by
scoped ID and `TypeId`; `use_keyed_state` retains consecutive-frame state, and `use_state` defaults
to caller location ([entity][zed-entity], [state][zed-element-state], [transfer][zed-state-transfer]).
**Inference:** use domain keys and a draft owner across filtering; element identity/`TypeId` is not a migration schema.

`Entity::update`/`cx.listener` do not automatically notify; `Entity::write` does. Opt-in cached views
need definite outer size; notifications and bounds/clip/text-style/opacity affect validity. Replay
preserves interaction handlers and accessed-entity membership ([update][zed-entity],
[listener][zed-listener], [API][zed-view-identity], [cache][zed-view-cache], [replay][zed-frame-replay]).
**Inference:** specify notification edges for separate models. The earlier
[late-opacity caveat](#november-5-phase-validity-and-measurement-retention) still applies;
retention/replay does not establish exact presented-input authority.

### Native Text Plumbing Is Not A Finished Product Field

**Source facts:** `EntityInputHandler` adapts a view to UTF-16 selection/marked ranges,
replacement/unmarking and candidate geometry ([adapter][z5-entity-input], [protocol][z5-input-contract]).
Windows places composition/candidate windows from range bounds, forwards result strings to replacement
and preedit to replacement-plus-marking, and conditionally completes composition when disabling IME
([native text handling][z5-windows-ime]). This establishes plumbing, not native language qualification.

`examples/input.rs` owns content, selection, marked range and prior geometry; committed and preedit
replacement both mutate its local content and notify. It binds editing actions itself
([state][z5-example-state], [editing][z5-example-edit], [composition][z5-example-render]).
**Inference:** the adapter supplies no automatic production field, submit/cancel, validation,
project Undo or refresh policy. Edit-buffer preedit is not document acceptance.

`Window::handle_input` registers during paint for the focused handle and upcoming frame;
text/selection changes separately notify native services ([registration][z5-input-registration]).
`ElementInputHandler` holds a strong `Entity<V>` and element bounds; cloned focus handles retain
their entry, with last-drop cleanup queued ([adapter][z5-input-owner], [focus][z5-focus]).
**Inference:** test retirement, focus transfer before repaint, row reuse and delayed composition against field incarnation.

### Testing Hooks And Automation Have Different Authority

**Source facts:** `test-support`/test builds expose `TestAppContext` input/action simulation.
`dispatch_keystroke` creates an internal event with simulated IME, then dispatches character input;
it does not invoke OS composition ([gating][z5-test-gates], [tests][z5-test-input], [dispatch][z5-synthetic]).
`VisualTestContext::debug_bounds` needs selectors recorded under `debug_assertions`
([limit][z5-debug-bounds]). The distinct `VisualTestAppContext` is macOS-gated and captures through
`render_to_image` ([gating][z5-test-gates], [capture][z5-visual-capture]), not Windows qualification.

**Inference:** these deterministic in-process hooks do not define agent transport, authentication,
domain targets, presented-generation checks or Nara's exact operation/results. An agent adapter
must use the product's admitted operations/results and distinguish semantic invocation,
presented-control interaction and actual OS input.

### Exact Upstream Dependency And Toolchain Constraints

**Source facts:** `gpui = 0.2.2` declares `publish = true`; `gpui_platform = 0.1.0` inherits
`publish = false`. Both inherit edition 2024 and connect through workspace paths
([core][z5-core-manifest], [platform][z5-platform-manifest], [root][z5-root-package],
[paths][z5-workspace-paths]). Different version numbers alone are not incompatibility;
no registry release was checked for equivalence to this source pin.

Core local dependencies include `collections`, `gpui_macros`, `gpui_shared_string`, `http_client`,
`refineable`, `scheduler`, `sum_tree` and `gpui_util`; Windows adds `gpui_windows`
([core][z5-core-manifest], [platform][z5-platform-manifest]), plus their derive/tracing dependencies
([refineable][z5-refineable], [sum_tree][z5-sum-tree]). Copying only two directories loses this closure.
A same-repository, same-revision Git pair can structurally address workspace packages without
publishing ([Cargo discovery][z5-cargo-deps]); compilation is unqualified. Record source identity/features.

Root patches pin `async-task` to `015845a19fc31752c02367567d3b8900a406f9da` and Linux `calloop` to
`3759371fee14c40066d64777e2a36b6ffcc22590` ([patches][z5-patches]). Cargo ignores dependency patch
tables: consumers must explicitly retain relevant root patches or qualify removal
([Cargo scope][z5-cargo-patches]). `gpui`/`scheduler` directly declare `async-task`; `gpui_linux`
declares `calloop` ([core][z5-core-manifest], [scheduler][z5-scheduler], [Linux][z5-linux-manifest]).
This source selection difference is not a demonstrated compile failure or an Open-GPUI blocker.

The toolchain selects Rust `1.98.1`; neither root crate declares `rust-version`
([toolchain][z5-toolchain], [core][z5-core-manifest], [platform][z5-platform-manifest]).
`current_platform` uses `cfg_select!`, stable since `1.95.0` ([code][z5-platform-select],
[versioned std][z5-cfg-select]); that macro alone does not exceed [Nara's declared `1.95`](../../Cargo.toml).
The closure's MSRV remains unverified; the dependency toolchain is not a consumer compiler guarantee.

The publisher rewrites names, versions and `publish`; its hard-coded list omits `gpui_platform`,
`gpui_windows` and `scheduler` ([publisher][z5-publisher]). A separate graph planner maps names such
as `gpui_collections`/`gpui-macros` ([mapping][z5-publish-names]). **Inference:** unchanged manifests
and a version label do not establish registry readiness; qualify source consumption and packaging separately.

### License Declarations And Actual Reload Scope

**Source declarations, not legal advice:** upstream `gpui`, `gpui_platform` and `gpui_windows`
declare `Apache-2.0` ([core][z5-core-manifest], [platform][z5-platform-manifest], [Windows][z5-windows-manifest]).
Zed's higher-level `ui`, `workspace`, `component`, `editor` and `ui_input` declare
`GPL-3.0-or-later` ([ui][z5-license-ui], [workspace][z5-license-workspace],
[component][z5-license-component], [editor][z5-license-editor], [ui_input][z5-license-ui-input]).
These upstream declarations do not reopen the user's already processed Open-GPUI distribution.

**Source facts:** optional `hot-reload` requires debug assertions and an enumerated platform.
It connects to `dx serve --hot-patch`, filters by build/process ID, applies a Subsecond jump table
between foreground updates and refreshes windows; `FullReloadCommand` requires restart
([gating][z5-hot-build], [implementation][z5-hot-loader]). Render routes through `HotFn`
([boundary][z5-hot-render]). **Inference:** meaningful development support does not establish
a stable extension ABI, state migration, dirty-document recovery or safe unload. `Render`/`Element`
alone provides none; distinguish supported patches from explicit build/reconstruct/restart.

### Candidate Acceptance Still Required

**Proposed requirements, not executed results:** qualify the maintained candidate, not assumed upstream parity.

| Area | Exact required evidence |
| --- | --- |
| Source consumption | Build an independent consumer from the recorded candidate revision, features, lockfile and compiler; exercise the chosen native platform and custom element. Record root patch policy and actual dependency closure. Qualify registry packaging separately if promised. |
| Product text field | Use the candidate's real field with Windows Microsoft Pinyin and a Japanese IME; capture preedit/commit/cancel, UTF-16/grapheme selection, clipboard, long-field scrolling and candidate geometry across DPI/resize. Keep drafts separate from document submission; validate exact Undo and external-model refresh outcomes. |
| Input lifetime | Transfer focus before repaint, filter/reorder/delete the edited row, close/recreate its panel, and use a second window during composition. Preserve or cancel by declared policy; reject predecessor input/actions and prevent Editor shortcuts from consuming text-editing gestures. |
| Custom tool and operation | Present a node/edge tool with embedded ordinary text controls, clipping, hit testing and model-key selection. Verify the same admitted document operation and exact result through user and agent adapters; synthetic tests do not satisfy OS IME acceptance. |
| Extension iteration | Change independently authored panel behavior and state shape; prove supported migration or explicit refusal, dirty content/history/draft recovery, compile/startup/restore failures and last-good recovery. Retire old callbacks/tasks/input routes and reject delayed results. Hot patch claims name their supported change classes; unload needs separate physical-quiescence evidence. |

**Current limit:** source comparison only; no adoption, native, widget, performance, ABI or unload qualification.

[z5-canvas]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/elements/canvas.rs#L8-L88
[z5-layout-hit]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5459-L5585
[z5-path-paint]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5005-L5028
[z5-path-builder]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/path_builder.rs#L25-L109
[z5-entity-input]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/input.rs#L7-L148
[z5-input-contract]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/platform.rs#L2333-L2425
[z5-windows-ime]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_windows/src/events.rs#L645-L764
[z5-example-state]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/examples/input.rs#L39-L49
[z5-example-edit]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/examples/input.rs#L349-L407
[z5-example-render]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/examples/input.rs#L647-L681
[z5-input-registration]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5633-L5676
[z5-input-owner]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/input.rs#L150-L169
[z5-focus]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L536-L673
[z5-test-gates]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app.rs#L69-L76
[z5-test-input]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/test_context.rs#L605-L656
[z5-synthetic]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/window.rs#L5821-L5845
[z5-debug-bounds]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/test_context.rs#L1027-L1032
[z5-visual-capture]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/app/visual_test_context.rs#L380-L386
[z5-core-manifest]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/Cargo.toml
[z5-platform-manifest]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_platform/Cargo.toml
[z5-root-package]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/Cargo.toml#L277-L279
[z5-workspace-paths]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/Cargo.toml#L372-L384
[z5-refineable]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/refineable/Cargo.toml
[z5-sum-tree]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/sum_tree/Cargo.toml
[z5-cargo-deps]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories
[z5-patches]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/Cargo.toml#L1037-L1068
[z5-cargo-patches]: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html#the-patch-section
[z5-scheduler]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/scheduler/Cargo.toml
[z5-linux-manifest]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_linux/Cargo.toml
[z5-toolchain]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/rust-toolchain.toml
[z5-platform-select]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_platform/src/gpui_platform.rs#L61-L81
[z5-cfg-select]: https://doc.rust-lang.org/1.95.0/std/macro.cfg_select.html
[z5-publisher]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/tooling/xtask/src/tasks/gpui.rs#L128-L303
[z5-publish-names]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/tooling/xtask/src/tasks/gpui/publish_plan.rs#L74-L85
[z5-windows-manifest]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui_windows/Cargo.toml#L1-L6
[z5-license-ui]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui/Cargo.toml#L1-L6
[z5-license-workspace]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/workspace/Cargo.toml#L1-L6
[z5-license-component]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/component/Cargo.toml#L1-L6
[z5-license-editor]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/editor/Cargo.toml#L1-L6
[z5-license-ui-input]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/ui_input/Cargo.toml#L1-L6
[z5-hot-build]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/build.rs#L19-L29
[z5-hot-loader]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/hot_reload.rs#L15-L108
[z5-hot-render]: https://github.com/zed-industries/zed/blob/dd3437011469d5c2ec8c821a84cc999979625697/crates/gpui/src/view.rs#L339-L355

## November 5: Maintained Open-GPUI And Engine Fit

**Evidence:** source inspection against Nara `e65cbc39ab60959518ca3ceb03cf2373effcf354` and the
authoritative Open-GPUI development workspace, branch `optimize/docking-graph-transaction`, at
`89cb76ba1d59904d5c670f770aa8e30fe0346178`. The reference checkout remains a different snapshot,
`689db5b49b33a5b2e64ba332bff72c98e70092bb`. The inspected component exports, text controller,
Tree, VirtualizedList and ui-core match between those snapshots; docking internals and some
native-event interfaces differ. Open-GPUI citations below pin the development base, not later
working changes. This section assesses engine fit; dependency/build and GPU results belong to
separate evidence. The user authorizes continued maintenance of this framework.

**Recommendation — Trial:** build a complete independent GPUI Editor composition while retaining
Nara's game UI runtime as the initial control. Complete the fork's Zed synchronization before
engine adaptation. Then compare separate game/Editor implementations, a shared kernel and unified
execution; the initial control does not freeze the runtime choice. Prefer existing components and
docking over extending another Editor widget mechanism. This qualitative maintenance judgment
is conditional on retiring displaced
first-party UI mechanisms and counting Open-GPUI maintenance. It does not establish native readiness
or measured savings. Universal GPUI for games remains a separate candidate requiring its own proof.

| Existing owner and concrete interface | Reuse or replace in a GPUI Editor | Responsibility retained by Nara |
| --- | --- | --- |
| [Tooling models and actions](../../crates/nara_tooling/src/lib.rs#L21-L80) | Reuse immutable observations, queries and typed commands. | Document identity, selection, validation, history and persistence semantics. |
| [Project Session](../../src/project_host/runtime/editor.rs#L875-L949) and [action dispatch](../../src/project_host/runtime/editor/actions.rs#L23-L76) | Reuse through EditorApplication; GPUI supplies the frontend and native host composition. | Project/document execution, revision admission, pending work, Play and close custody. |
| [PanelHost](../../editor/src/panels/host.rs#L27-L83) and [PanelContext](../../editor/src/panels/context.rs#L24-L67) | Reuse state custody, instance tokens and exact submission results. | Panel incarnation, target binding, retirement and action/result ownership. |
| [FieldDraft](../../editor/src/frontend/controls/field_drafts.rs#L156-L195) and [ValueDraft](../../editor/src/frontend/controls/value_draft.rs#L13-L65) | Reuse domain draft values, conflict/recovery and Apply semantics. | Invalid input, original field/schema basis and pending PanelSubmissionId; no second caret/composition owner. |
| [OwnedInspector](../../editor/src/frontend/controls/owned_inspector.rs#L327-L349) and owned Hierarchy controls | Replace control construction, layout, scrolling, focus and text machinery with GPUI components. | Keep source projections, field classification, structural permissions and command preparation. |
| [Workspace layout](../../editor/src/workspace/layout.rs#L71-L83) and panel instances | Keep product identities/targets; use DockSurface for detailed placement and its snapshot for the toolkit blob. | Contribution catalog, instance lifecycle, default placement intent and preference persistence. |
| [Runtime UI adapter](../../crates/nara_ui/src/lib.rs#L21-L43), Kernel and Widgets | Retain for game consumers; GPUI Editor controls need no permanent Nara widget facade. | Scene/ECS authoring, stable source identity, game intents and admitted runtime frame/input contracts. |
| [UI presentation](../../crates/nara_ui_render/src/lib.rs#L1-L37) and [authoring preview](../../src/project_host/runtime/editor/ui_preview.rs#L19-L52) | Retain the game/UI Canvas content pipeline when displayed inside the Editor. | The edited game's UI representation and preview validity; changing Editor chrome does not replace them. |

The concrete Editor dependency set is `open-gpui`, `open-gpui-platform`, `open-gpui-ui-components`
and `open-gpui-docking`. [Components][og5-components] already depend on command, form, resource,
motion and ui-core; their exported [controls][og5-controls] cover more than primitive drawing.
For selective mechanism reuse, [ui-core][og5-core] depends only on motion and exposes virtualizer,
split, focus and [overlay helpers][og5-core-api]. Renderer-neutral state inside ui-components still brings
that package's GPUI dependency; it is not an independent headless dependency closure.

Use **one detailed docking placement owner**: DockSurface in the selected Editor shell, with
[lazy panel factories][og5-dock-factories] and [layout/viewport snapshots][og5-dock-snapshot].
Nara's stable panel IDs, targets and incarnation rules remain product facts. Default placement
is an input, not a second synchronized split/tab graph. Compatible docking moves preserve the
applicable panel state; explicit hiding/closing and target replacement follow PanelHost retirement.
At this pin, the new [canonical runtime][og5-canonical] exists, but production
[DockWorkspace][og5-workspace] still owns DockGraph. The canonical migration is not complete merely
because U4 operations have landed; consume the surface facade without coupling Nara to that kernel.

**Concrete integration gaps, observed in the maintained candidate:**

- **Controlled selection:** [Tree rendering][og5-tree-state] prefers runtime selection over the
  builder default, and [click handling][og5-tree-click] updates that selection before calling back.
  [VirtualizedList][og5-list-state] also resolves from runtime selection and
  [updates before notification][og5-list-input]. Default setters do not provide authoritative
  reconciliation after Session rejection, external selection, Undo or document replacement.
  Add a narrow controlled-selection contract to the framework; keep domain admission in Nara.
- **Text state:** use the actual [TextInputController][og5-text-state] for content, caret, selection,
  composition and local editing. Its [action/key table][og5-text-actions] has no local Undo/Redo;
  Nara's [TextEditor](../../crates/nara_text/src/editing.rs#L181-L216) already has bounded history
  and original-composition custody. [Preedit invokes on_change][og5-preedit], and a differing
  [controlled value clears marked text][og5-text-sync]. Fill those required behaviors in GPUI;
  retain Nara's domain draft/Apply boundary rather than synchronizing two editing engines.
- **Callback and drive order:** GPUI callbacks cannot retain a borrowed PanelContext. A concrete
  root owner can retain Application/PanelHost, queue requests with original instance/field basis,
  run prepare/submit-and-drive, then publish results to GPUI. Preserve
  [exact original callbacks](../../editor/src/panels/host.rs#L275-L365) and
  [maintenance without idle Play advancement](../../src/project_host/runtime/editor/model.rs#L21-L87).
  Rendering each GPUI view must not independently drive the Session or fabricate a new Runtime frame.

**Game UI is a separate decision:** ECS-to-GPUI is not inherently a duplicated Entity graph.
[Render/RenderOnce][og5-render] allow data-derived Element trees; one root or stateful control can
be an Entity without turning every game node into one. Nara already
[projects ECS children](../../crates/nara_ui/src/runtime/projection.rs#L313-L387) into UI descriptions.
A coherent GPUI game backend is therefore possible, but must demonstrate the existing headless
consumer, source identity, surface lifetime and
[presented-input admission](../../crates/nara_ui/src/interaction/timeline.rs#L22-L103) contracts.
GPUI's Window/App-oriented rendering is not already the
[window-free immutable Kernel](../../crates/nara_ui_kernel/src/lib.rs#L1-L5) contract.

**Maintenance evidence:** Nara's Hierarchy already
[materializes visible rows](../../editor/src/frontend/controls/owned_hierarchy/presentation.rs#L561-L611).
GPUI adds reusable [virtual Tree rendering][og5-tree-virtual] and [shared list descriptors][og5-list-data].
That supports replacing mechanisms, not claiming all virtualization is missing today or that
descriptor construction, filtering and document queries become proportional to visible rows.
Retain bounded domain queries and compare equivalent document/interaction workloads.

**Bounded acceptance proposal:** start with one independent GPUI root hosting Project Open/Close,
Hierarchy, Inspector and in-window DockSurface through the existing owners. Require exact
Apply/rejection and Undo/Save/Reopen, externally changed selection, invalid-draft recovery,
source/schema replacement and delayed-event rejection after panel retirement. Qualify IME
preedit/commit/cancel and local text history without document mutation during preedit.
Then extend complete Editor workflows and native docking coverage before switching defaults and
removing displaced implementations. These are proposed gates; this source study ran no Cargo,
formatting, native interaction or performance experiment.

[og5-components]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/Cargo.toml#L17-L29
[og5-controls]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/lib.rs#L9-L78
[og5-core]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_core/Cargo.toml#L17-L18
[og5-core-api]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_core/src/lib.rs#L37-L95
[og5-dock-factories]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_docking/src/surface/builder.rs#L92-L100
[og5-dock-snapshot]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_docking/src/surface/state.rs#L61-L74
[og5-canonical]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_docking/src/canonical_runtime.rs#L15-L24
[og5-workspace]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_docking/src/workspace.rs#L14-L33
[og5-tree-state]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/tree/mod.rs#L239-L270
[og5-tree-click]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/tree/mod.rs#L741-L755
[og5-list-state]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/virtualized_list/runtime.rs#L1142-L1183
[og5-list-input]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/virtualized_list/runtime.rs#L1618-L1635
[og5-text-state]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/text_input.rs#L253-L275
[og5-text-actions]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/text_input.rs#L190-L250
[og5-preedit]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/text_input.rs#L906-L923
[og5-text-sync]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/text_input.rs#L524-L535
[og5-render]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui/src/element.rs#L153-L175
[og5-tree-virtual]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/tree/mod.rs#L114-L129
[og5-list-data]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/ui_components/src/virtualized_list/data.rs#L5-L39

## November 5: Rendering Seam And Executed Maintenance Baseline

The maintained framework successor is
[`8c58cca8`](https://github.com/Latias94/open-gpui/commit/8c58cca8537a8d1278e9da5cdd945e353d8b35dc),
followed by the [upstream continuation](https://github.com/Latias94/open-gpui/commit/bce3a902e0a325035718f5f200cd9cba072fdf7b).
Both were pushed to `maintenance/nara-editor-baseline`; the local development branch and its
existing docking work were preserved. Nara has not installed this as its production frontend.

### Actual Rendering Difference

**Source facts:** the [platform selector][og5-platform] constructs the Win32 backend on Windows.
Its [device implementation][og5-d3d11] creates D3D11 devices. Nara's current [backend dependency](../../Cargo.toml)
is WGPU 30.0.1 and the qualified Windows Editor uses DX12. A shared version number or adapter
name does not make the devices, textures or queues interchangeable.

The public [surface element][og5-surface] and `paint_surface` use a macOS pixel buffer under
target configuration. Windows [draw_surfaces][og5-windows-surface] performs no surface drawing;
the [WGPU renderer][og5-wgpu-renderer] also skips that primitive. [RenderImage][og5-render-image]
stores CPU image frames; `ImageSource::Custom` still returns that type. These inspected paths do
not supply arbitrary retained Nara GPU-texture import. This is a concrete integration gap, not
proof that framework reuse is impossible or a reason to preserve D3D11 permanently.

**Preferred experiment, not a selected interface:** let an Open-GPUI rendering adapter consume
Nara's existing render services and retained engine output, or adapt the WGPU backend to the
same captured device/queue. Keep GPUI's element/layout/input execution independent of Nara
documents and World types. Backend-specific acquisition, scene lowering, atlas resources,
submission and shutdown belong behind that concrete adapter; do not add a parallel GPU graph or
device owner merely to join the frameworks. The current image-backed panel attachment is not a
mandatory route for the complete candidate.

Use a real changing Scene/Game View below clipped UI and an overlapping popup. Prove exact
presentation/input mapping, resize/DPI/color/alpha, refusal and physical retirement. Attribute
every GPU copy and synchronization step; steady-state CPU readback is not an acceptable bridge.
Window submission and accepted UI semantics remain different facts. The fork's current
renderer-neutral outcome and shutdown contracts must survive a backend replacement.

### Executed Source-Consumption Evidence

The original locked framework example compiled, but a fresh consumer first refused withdrawn
`spin` 0.10.0. After that correction, exact Taffy and Core Foundation patch requirements exposed
independent-consumer conflicts. The maintenance commit permits compatible Taffy/Core Foundation/
WGPU versions and updates `spin` to 0.12.3. It leaves other root lock records unchanged. The
[independent application](https://github.com/Latias94/open-gpui/tree/8c58cca8537a8d1278e9da5cdd945e353d8b35dc/tests/fixtures/dependency-consumer)
owns a separate lockfile, real component/canvas/DockSurface composition and optional actual WGPU
renderer dependency. This is source-consumption qualification, not an engine View placeholder.

All local Cargo work was serial, with one build job, Rust 1.95.0 and Nara's shared target on
Windows 11. The [framework verification record](https://github.com/Latias94/open-gpui/blob/8c58cca8537a8d1278e9da5cdd945e353d8b35dc/docs/verification.md#independent-dependency-consumer)
contains the reproducible durable consumer gate and limits.

| Check | Actual result and scope |
| --- | --- |
| Development minimal docking example | Locked compilation passed; no native window was launched. |
| Core, components and docking library cohort | 2,590/2,590 passed, no skipped selected cases; nextest `447739ea-48cb-4e78-a85f-5cb05769a64f`. Integration/native suites outside `--lib` were not selected. |
| Independent consumer, all application targets/features | Passed with 728 resolved packages, Taffy 0.10.0, WGPU 30.0.1 and the actual WGPU renderer. The root workspace retained Taffy 0.10.1 and WGPU 30.0.0. macOS-only Core Foundation 0.10.1 was resolved, not compiled on Windows. |
| Nara plus framework consumption | A disposable separately locked consumer compiled `nara-editor` without defaults and the four Open-GPUI entry packages, including a type implementing both EditorPanel and Render. Its 766-package graph and public API composition passed; no Editor project workflow or native integration was executed. |
| Attribution/import boundary | Existing `xtask scan-import-boundary` passed. The fork already carries Apache-2.0 and NOTICE/source attribution. This scanner checks its declared import policy, not every distribution obligation. |
| Formatting and review | Root/independent-consumer format checks and public-document checks passed; independent dependency-change review found no outstanding correctness finding. Linux/macOS CI and actual Rust 1.92 compilation remain unqualified locally. |

The [updated upstream queue](https://github.com/Latias94/open-gpui/blob/bce3a902e0a325035718f5f200cd9cba072fdf7b/docs/imports/zed-gpui-upstream-sync-candidates.md#november-5-maintained-framework-successor)
records current Zed `635026fabc38440fce5e1767d125026a17b12012`, five confirmed pending correctness
units, already-covered behavior and a later coherent text refresh. Begin with arena allocation/
nested-clear authority, then TextSystem lifetime, cached dependencies, text layout and pending
input. These are planned owning fixes, not results of the library cohort above. Compiler, Taffy,
font-generation and line-breaking changes belong to that refresh comparison.

**Remaining decision:** source and framework capability evidence favor a maintained GPUI Editor
trial over extending a second full Editor control stack. They establish no measured total-cost
win, GPU sharing, OS IME, complete docking replacement or unified game runtime. Retain the actual
game UI as the first control, then compare separate execution, shared mechanisms and unified
execution through the complete R11a consumers before selecting the lasting boundary.

[og5-platform]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_platform/src/gpui_platform.rs
[og5-d3d11]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_windows/src/directx_devices.rs
[og5-surface]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui/src/elements/surface.rs
[og5-windows-surface]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_windows/src/directx_renderer.rs#L850-L855
[og5-wgpu-renderer]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui_wgpu/src/wgpu_renderer.rs#L1577-L1582
[og5-render-image]: https://github.com/Latias94/open-gpui/blob/89cb76ba1d59904d5c670f770aa8e30fe0346178/crates/gpui/src/assets.rs#L42-L88

## November 5: Arena And Native Frame Maintenance

The published maintenance successor is
[`15cc4a7d85`](https://github.com/Latias94/open-gpui/commit/15cc4a7d8597fb9935b1210a1be8d8b6fc81293d),
following the separate local-worktree scan correction
[`a4c5b61548`](https://github.com/Latias94/open-gpui/commit/a4c5b61548171d832e555cfc7b8b93c914d58953).
Remote `maintenance/nara-editor-baseline` was verified at that exact successor. The
[framework verification record](https://github.com/Latias94/open-gpui/blob/15cc4a7d8597fb9935b1210a1be8d8b6fc81293d/docs/verification.md#element-arena-and-native-frame-maintenance)
owns its reproducible commands and limits.

The first correctness unit from the maintained-framework queue now implements upstream
`ec3d887507` and `2529740084` as one owning correction. App holds the element arena in a separate
Rc allocation. Draw scopes retain it, restore TLS on unwind and defer nested cleanup. A clear
token keeps weak allocation identity and requires the original App. All 1,076 migrated draw/clear
calls use that same App explicitly. The allocator additionally preserves exact allocation
layouts, checks bounds before pointer adjustment and supports over-aligned chunk reuse; Miri
exposed the baseline's assumption that alignment-1 chunks always fit their requested values.

The fork's existing AppCell event queue already prevents nested scene draws, so this port keeps
that owner. Its remaining Windows paint tail used to query App-owned text handlers synchronously
while another draw could hold the App borrow. Paint now validates its native update region and
retains the request; `CompleteNativeFrame` uses the existing post-borrow command queue for
DirectManipulation/IME maintenance, with exact native-registration revalidation. Forced cache
refresh also survives window throttling without overriding an already accepted initial scene's
presentation retry. No engine document, ECS, history or GPU authority was added to GPUI.

The Windows/Rust 1.95 results distinguish their scopes: 42 focused framework tests pass; the
core/components/docking/Canvas/DevTools library cohort passes 2,984/2,984; 598 component integration
tests pass across 18 binaries; and the Windows backend passes 162/162 with four existing
interactive cases skipped. Workspace all-target and independently locked all-feature consumer
compilation also pass. A new real dual-HWND test proves
region validation, coalesced-but-retained requests, delayed IME query, absence of caught invariant
panic, and later `WM_CHAR` input. The direct-production-file allocator fixture separately passes
10/10 tests in each Miri borrowing model. Independent review reports no outstanding P1/P2.
The all-feature DevTools framework adapters pass 10/10, and the test-platform Gallery convergence
workflow passes. Core/Windows correctness Clippy, formatting, import/public-API/document/release
checks pass. The existing UI contract gate passes its 16 registered scenarios after excluding
local `.context` worktrees from discovery; a filesystem regression preserves those local copies
and the focused validator cohort passes 16/16. The tooling correction stays in the existing
discovery filter.

These are bounded framework results. Miri does not compile the complete App/TLS/Window graph.
The Windows test does not qualify full OS composition or native game View integration.
DirectManipulation now samples after drawing, so changes can appear in a later frame; physical
touchpad scroll, inertia, pinch and response latency remain an explicit comparison. Completing
this correction does not establish complete Zed parity, canonical docking cutover, a measured
Editor cost win or the lasting Editor/game-UI execution boundary. Continue with TextSystem pool
lifetime, cached dependencies, text correctness and pending-input/blur before broad adaptation.

## November 5: TextSystem Lifetime And Native Reconstruction

The published successor is
[`229723e620`](https://github.com/Latias94/open-gpui/commit/229723e6205e8fc88da50ffabeb721c482d100e4),
preceded by the independent native-query correction
[`432d079675`](https://github.com/Latias94/open-gpui/commit/432d079675056cb559d0681a25298fa7fb24fc6e).
Remote `maintenance/nara-editor-baseline` was verified at that exact successor. The
[framework lifetime record](https://github.com/Latias94/open-gpui/blob/229723e6205e8fc88da50ffabeb721c482d100e4/docs/verification.md#textsystem-pool-lifetime)
owns the reproducible commands, failed reproductions, successful cohorts and limits.

The second upstream correctness unit ports `e4a3732b94` within Open-GPUI's existing TextSystem.
Its pool previously held complete LineWrapper values which strongly retained that same
TextSystem. A direct lifecycle regression reproduced the reference cycle before the fix. The
pool now retains only character-width caches; checked-out handles keep the TextSystem alive,
and returning the final handle releases it when no external owner remains. Font resolution and
wrapper construction stay outside the pool mutex. Public text and truncation behavior is
preserved, with counting-platform tests for ASCII/Unicode reuse, resolved-font/size separation,
overlapping handles and unwind.

A public headless App renders text controls, closes and releases its TextSystem/backend across
three reconstructions. The corresponding real Windows consumer opens and paints mixed Unicode
text, reuses the DirectWrite-backed wrapper, retires the exact HWND registration and drops the
App/platform before rebuilding three times. This complete consumer exposed another baseline
failure: the second native window creation accessed an invalid process-cached WinRT UISettings
factory after the previous OLE apartment had been released. A debugger stack and an independent
minimal apartment/theme-query loop reproduced it without the text system or GPU. Both appearance
and scrollbar queries now acquire the activation factory within the current query lifetime,
preserving balanced OLE ownership. The minimal and complete native regressions pass together.

The focused lifetime/wrapping/truncation cohort passes 20/20. Complete Core coverage passes
841/841, component integration passes 598/598 across 18 binaries, and the final Windows library
cohort passes 164/164 with four existing interactive cases skipped. The first combined run had
one unchanged activation-policy test time out at its 512-message-pump bound; two unchanged
focused reruns and the complete Windows rerun passed. The framework verification record retains
all those outcomes without assigning an unproven cause. Independent source consumption passes
the separately locked all-target/all-feature check, including its actual WGPU renderer.
Locked workspace all-target compilation, Core/Windows correctness Clippy, formatting and the
existing import/document/release checks also pass. Independent review reports no outstanding
P1/P2.

This evidence concerns exact object ownership and retained work. It does not establish process
memory bounds, a generally leak-free platform, native code hot reload, complete OS composition,
font-generation invalidation, touchpad latency or an integrated engine View. The remaining
cached-dependency unit must isolate nested draw read scopes and retain the fork's candidate,
presentation, geometry and reveal authority; simple upstream source replacement would lose
those contracts. The full foundation and the Editor/game-UI execution decision remain open.

## November 5: Diff-Based Upstream Baseline And Maintenance Skill

The published framework record
[`2d66e3ff88f`](https://github.com/Latias94/open-gpui/commit/2d66e3ff88f3acf6368f150b1f6ade99519c5a72)
fixes the current target at Zed `635026fabc38440fce5e1767d125026a17b12012`. A later remote HEAD
observation, `654efae441bc8eb117d0042bb28cd7cf3570bdc9`, remains separate. The
[framework candidate queue](https://github.com/Latias94/open-gpui/blob/2d66e3ff88f3acf6368f150b1f6ade99519c5a72/docs/imports/zed-gpui-upstream-sync-candidates.md#fixed-revisions-and-provenance)
owns the exact revisions, source evidence, dispositions and verification requirements. Finish
this bounded refresh before expanding the engine rendering adapter or Editor component migration;
the five initial correctness units do not substitute for the coherent text/layout/API and platform
comparison.

The initial extraction did not record an exact upstream checkout SHA. Zed
`6e9465a4288c332208643892e23b9d35d7be5c79` is a content-verified reconstruction anchor: the
331 imported entries have 326 identical blobs and five documented dependency-policy adaptations,
with license-file mode differences recorded separately. Its parent has identical imported
contents, so the exact original monorepo SHA remains unknown. July's target was a selected-port
comparison, not a fully synchronized base. Diff inspection corrected its earlier Middle-truncation
equivalence claim and identified further font-generation/fallback, Unicode wrapping, text drawing,
layout measurement, GPU representation/color and external-run-loop work. Those are source findings
and queued mechanisms, not executed feature qualification.

The repository
[upstream-sync skill](https://github.com/Latias94/open-gpui/blob/2d66e3ff88f3acf6368f150b1f6ade99519c5a72/.agents/skills/open-gpui-upstream-sync/SKILL.md)
records a repeatable three-tree comparison, relocation/package mapping, follow-up/revert checks,
complete prerequisites and fork-contract adaptation. Its schema/references and an independent
read-only animated-WebP scenario were checked. The changing baseline and task status stay in the
framework's existing queue and plan. No synchronization engine, Nara-specific scanner or second
framework copy was added.

## November 6: Cached Dependencies And Deferred Opacity

The published, remote-verified framework successor is
[`7a1bf6d73d4`](https://github.com/Latias94/open-gpui/commit/7a1bf6d73d46fbc8b251929cc4d8cfb53a9a5fc1),
following the separate native-fixture correction
[`015749945c5`](https://github.com/Latias94/open-gpui/commit/015749945c5df9c6c1a7e27e4404e1c09bac577e).
The cycle's upstream target remains Zed `635026fabc38440fce5e1767d125026a17b12012`.
The [framework qualification](https://github.com/Latias94/open-gpui/blob/7a1bf6d73d46fbc8b251929cc4d8cfb53a9a5fc1/docs/verification.md#cached-dependencies-deferred-replay-and-opacity)
owns the executed commands, initial failures, final results and limits.

This third priority unit adapts upstream `5e982c6bdc`, `decd2399b1` and `8fa085e397` within the
existing EntityMap, AnyView and frame journals. Callback scopes capture repeated reads; draw
guards isolate nested windows and restore outer reads on success, refusal and unwind. Retained
paint/deferred reads keep window subscriptions, released generations are filtered through live
reference-count membership, and nested deferred menus retain stable replay indices. Subscription
publication remains after candidate acceptance.

Ordinary deferred content and window portals now inherit logical primitive opacity. Private
opacity prediction reconciles stationary-pointer hover after the final admitted hit test without
changing input-latched layout refinements. Follow-up view notifications coalesce through the
existing Window animation owner. Regressions cover fresh cached parents, refused Inert candidates,
retry without MouseMove, group hover, unchanged reuse and exact rounded clips. Non-view model
notification still redraws subscribed windows without automatically dirtying a clean cached view;
notify the owning view or refresh when rebuilding is required. External group-context changes
also require owning-view notification. Arbitrary custom paint-only opacity is outside this
prediction guarantee.

Final Windows/Rust 1.95 execution passed Core 873/873, affected UI/component/docking/Canvas
2,964/2,964 across 25 binaries, and 165/165 selected Windows cases. Four existing interactive
Windows cases remain skipped. The native cache matrix checks real HWNDs, newer accepted and
nonempty renderer-Submitted generations, preserved cached render/paint counts, explicit child
rebuild and exact native/platform retirement. These receipts measure neither screen pixels nor
frame-time/retained-byte budgets. Native activation fixtures now establish their exact foreground
prerequisites and retain raw message parameters for failures; the whole-span keyboard assertion
and exact terminal/restoration checks remain. The verification record preserves earlier failures
without assigning an unproven keyboard-message source.

Locked workspace all-target and independently locked consumer all-target/all-feature compilation,
Core/Windows correctness Clippy, formatting and import/document/release/public-API checks passed.
Independent review found no remaining P1/P2. The declared Rust 1.92 floor and dependency graph did
not change; that compiler floor was not executed in this unit. Text layout and pending-input/blur
are the two remaining initial correctness units, followed by the coherent font/text/layout/API and
platform/render refresh. Full R11/R12, integrated engine View, Editor/game execution selection,
complete native input and measured performance remain open.

## November 6: Native Text Layout Successor

The verified local Open-GPUI successor is `0861f8d52b175e5c41b4d0081e121ed957e7f8ec` on
`optimize/docking-graph-transaction`. It is not published by this continuation; the recorded
maintenance branch remains at the previously verified `7a1bf6d73d4`. The cycle's fixed upstream
target is still Zed `635026fabc38440fce5e1767d125026a17b12012`. The framework's existing
verification guide, candidate queue and plan own the detailed results and next input unit.

The fourth priority unit adapts upstream `0294cfb58e`, `979cd3fdf0`, `ac6818fb2d` and
`e71963a599` within DirectWrite and the existing wrapped LineLayout. Native zero-glyph tab runs
retain their actual advance and source-byte caret entries; U+2028/U+2029 segments keep one
logical baseline without deleting source characters. Empty typography configuration applies
the standard liga/clig/calt defaults, preserving explicit disabling. Wrapped exact hit testing
returns the current row's end rather than panicking in the floating-point gap outside the
unwrapped width. Closest-index and caret affinity retain their existing behavior.

Core regressions reproduced two unwrap panics, and the corrected Windows baseline reproduced
four missing-feature failures while its explicit-disable control passed. Final execution passes
Core 877/877, affected UI/component/docking/Canvas 2,964/2,964, eight focused native/text cases
and selected Windows 173/173. Four existing interactive Windows cases remain skipped. Native
font/layout oracles cover real distinct FontRun identities, widths and hit positions. A public
Render/canvas/WindowTextSystem trajectory retains complete source through actual paint, a
notified newer accepted/nonempty renderer-Submitted generation and exact window/platform/OLE/
text-system retirement. The counter does not isolate notification causality from VSync, and
submission is not screen/glyph pixel readback or native keyboard/IME qualification.

Locked workspace and independent-consumer compilation, correctness Clippy, formatting and
import/document/release/public-API checks pass. Independent review found no remaining P1/P2.
The Rust 1.92 floor and dependency graph remain unchanged; execution used Rust 1.95.0.

Source reconciliation also identified `89e8a4b9ec` callback safety prerequisites. This unit
adopts mutable callback provenance and nullable-array admission. The remaining mapped color
staging release and atlas upload validation are queued separately: a malformed source must
return an error without publishing a tile or consuming its allocation. Copying upstream's
void-return upload guard would still let its caller publish success and would not satisfy the
fork's lease/candidate contract. Pending-input/blur is the one remaining initial correctness
unit, with IME preference, modifier recognition, dropped-focus and accepted/refused transition
prerequisites recorded in the framework plan. The wider refresh, Nara integration and measured
performance remain open.

## November 6: Native Upload And Color Readback Successor

The verified local Open-GPUI successor is `f1850835fbba0255f41703b529136d45a45d6eb6` on
`optimize/docking-graph-transaction`. It is not published by this continuation; the recorded
published maintenance baseline remains `7a1bf6d73d4`. The cycle target remains Zed
`635026fabc38440fce5e1767d125026a17b12012`. The framework's existing verification guide,
candidate queue and plan own detailed commands, regression results and remaining work.

This unit reconciles the remaining native resource parts of `89e8a4b9ec`. Both Windows atlas
insertion APIs now share source admission before allocation and checked destination admission
before UpdateSubresource. Malformed sources return an error without key publication or
allocation/generation changes; a later refusal cancels only its unpublished allocation. Existing
published regions and exact-instance visual leases keep their lifetime rules. The fixed target's
void short-buffer return was deliberately adapted: its caller still publishes a successful tile,
which would violate the fork's existing admission contract.

Color glyph readback now validates BGRA dimensions, checked byte spans and native RowPitch,
allocates output before mapping, and releases each successful map through RAII on return, error
and Rust unwind. The existing straight-alpha conversion remains after release. Independent
review also found an undefined initial target used for blending; the color-raster owner now
clears it to transparent. A real Segoe UI Emoji trajectory checks visible content, repeated
identical bytes and every pixel of an expanded transparent border. The border is an output
control, not a deterministic missing-clear red baseline without poisoning the target.

The safe mapped-readback baseline reproduced an already-mapped resource debug error on the
first repeated read. Final focused resource tests pass 13/13, including both insertion APIs,
three texture kinds, retry/cache-hit/lease preservation and multi-row readback at nonzero atlas
origins. The affected Core retained-resource/cache/deferred cohort passes 54/54. The final
complete Windows library passes 186/186 with four existing interactive cases skipped, retaining
the real HWND text/cache paint, Submitted-frame and exact retirement trajectories. WARP and
debug-layer initialization failures fail the resource tests instead of silently skipping them.

Locked workspace and independently locked consumer compilation, Core/Windows correctness
Clippy, root/consumer formatting and import/document/release/public-API checks pass. Independent
review found no remaining P1/P2. The declared Rust 1.92 floor and dependency graph remain
unchanged; execution used Rust 1.95.0. The earlier text-layout unit's 2,964 consumer tests were
not rerun for this Windows-only resource correction.

An issued UpdateSubresource call provides no asynchronous GPU completion result. Bucket
cancellation does not guarantee immediate recovery of all reserved bucket space. Atlas/color
glyph readback does not establish screen compositing, full native input, frame-time budgets,
non-Windows execution or Nara renderer integration. Pending-input/blur remains the last initial
correctness unit before the wider framework refresh. Complete R11/R12 and Editor/game-runtime
selection remain open.

## November 6: Pending Input And Native Handler Successor

The verified local Open-GPUI successor is `cde23d44adae92b8ad55f760a120865be23e34aa`, after
the separate native-cache fixture correction `fb196f8a9c876c2981c18d58b54c59966dce4084`.
Both remain local on `optimize/docking-graph-transaction`; this continuation publishes neither.
The recorded published maintenance baseline remains `7a1bf6d73d4`, and this cycle's fixed Zed
target remains `635026fabc38440fce5e1767d125026a17b12012`. The framework's existing verification
guide, candidate queue and plan own the detailed commands, failures and dispositions.

The last initial correctness unit adapts `bc7041c600`, `ba42ab9d98` and `f66ed399cd`, with
`dc1e815e47`, `a8cae3bd77` and `e463c5ab74` prerequisites. Recognized standalone modifier
releases now share keystroke interception/observation. Multi-modifier gestures do not generate
phantom standalone releases, and only an entire singleton-modifier prefix introduces the
modifier timeout. An intercepted modifier preserves the preceding chord's original deadline.

The existing focus owner settles pending input once after accepted focus/blur/drop transitions,
with deferred observation. Refused candidates retain their original input, timer and notification
state. Active pending and native IME preference validate focus plus dispatch context. Replay
keeps that original recipient across observers and input listeners, stopping old action/text
delivery after a target change or window retirement. Completed actions retain their keystroke
report while interaction remains admitted, including actions that move focus. No additional
focus, pending-status or input execution owner was introduced.

The original registered baseline ran 20 cases: eight passed and twelve failed, including one
fixture that attempted to open a replacement after last-window App shutdown. Subsequent review
and red controls exposed callback retarget, same-focus context invalidation, an overly broad
modifier timeout and a lost completed-action report. Final Core passes 901/901, including all
24 input scenarios. UI core/components/docking/Canvas passes 2,964/2,964 across 25 binaries.
Complete Windows passes 187/187 with four existing interactive cases skipped. The new native
consumer uses a public Render view, two actual text handlers, production PlatformInput
callbacks, leased IME-preference queries and HWND WM_CHAR delivery. It checks current accepted,
nonempty renderer-Submitted generations, handler replacement, correct text recipient and exact
window/platform/OLE/text-system teardown. Synthetic keys and WM_CHAR do not establish live OS
keyboard translation or IME composition; deterministic timeout evidence comes from Core.

The broader run also reproduced an existing native cache-fixture failure. Low-overhead tracing
identified a legitimate native-hover Window::refresh while every cache key/resource still
matched. The fixture now requests pointer-transparent, nonactivating creation, checks native
facts and finishes one explicit newer initialization submission before freezing its single
baseline. Original replay/model/child and teardown assertions remain. Temporary probes were
removed; the production cache and Window refresh owners retain their original code. The
verification guide preserves the failed runs and the separate intrusive diagnostic limit.

Locked workspace and independent-consumer compilation, correctness Clippy, root/consumer
formatting and import/document/release/public-API checks pass. Independent source, scenario and
native-fixture reviews find no remaining P1/P2 within this unit. The declared Rust 1.92 floor
and dependency graph remain unchanged; execution used Rust 1.95.0.

All five initial correctness units now have implemented, executed scope. They do not complete
the wider framework refresh or select Nara's frontend/runtime UI. The existing plan queues the
next GPU record/color-transfer unit with complete Rust/shader consumers, initialized flag-width
ABI checks and actual gradient/sprite readback. That source preparation is not implementation
or pixel qualification. Wider font/text/layout/API and platform reconciliation, engine rendering
integration, measured performance and complete R11/R12 remain open.

## November 6: GPU Records And Windows Color Transfer

The verified local Open-GPUI successor is `871c012c96c671f7e543175f7af8e1ee0d2ea29e`, following
`cde23d44ada` on `optimize/docking-graph-transaction`. It remains unpublished by this
continuation. The recorded published baseline remains
`7a1bf6d73d4`; the fixed Zed comparison target remains
`635026fabc38440fce5e1767d125026a17b12012`. The framework's existing verification guide,
candidate queue, migration guide and plan own the commands, ABI contract and remaining work.

The unit adapts `f28177003499ec9c5052d1883fee62b148451c59` and
`b9419ae72814157164b09592ab2466f9f4709f09`. GPU-facing sprite and underline flags now use
private, fully initialized zero/one PaddedBool32 storage. Author-facing bool inputs remain;
custom renderer-record consumers migrate with bool `.into()` values. Existing clip, exact
atlas-generation and transform layout remain. DirectX uploads the shared sprite slice,
removing its duplicate record and per-frame conversion vector. This removes source-level work
without establishing a measured performance gain.

The safe old-layout regression fails on the sprite field's one-byte width without reading
uninitialized padding. The native old-shader regression fails on an Oklab interior pixel:
RGBA `[1,1,1,255]` instead of expected RGB `[99,99,99]`, with channel margin 6. Corrected
Windows transfer powers preserve the existing approximate gamma-2.2 contract. They do not
select Nara's complete color-management policy or impose byte parity on WGPU's piecewise sRGB.

Complete Core/WGPU passes 941/941, affected UI/components/docking/Canvas 2,964/2,964 and
Windows 188/188, with four existing interactive Windows cases skipped. The expanded native
trajectory checks independent gradient/solid/alpha oracles, alternating grayscale image
instances, opacity, three text decorations, translated rounded clips, complete cached/deferred
RGBA equality and exact native/platform retirement. Actual draw-time cache mode is published
through the existing accepted-frame transaction and matched to the Submitted generation,
preserving legitimate accessibility/inspector bypass. Debug runtime shaders and embedded FXC
artifacts each pass the native trajectory. The latter disables Rust optimization/debug
assertions while retaining production FXC `/O3`; it is not optimized Rust performance evidence.

Locked workspace and independent-consumer compilation, Core/Windows/WGPU correctness Clippy,
root/consumer formatting and import/document/release/public-API checks pass. Independent review
finds no outstanding P1/P2 after correcting the fixture's draw-time accessibility-mode custody.

Observed captures use scale 1.5 and cached replay, retaining `(3,3,3,3)` consumer counts on both
root notifications. Native accessibility activation, other DPI scales/migration, generated
macOS scene headers, Metal execution, WGPU scene pixels, Linux/browser execution and OS
compositor output remain unqualified. The declared Rust 1.92 floor and dependencies remain
unchanged; execution uses Rust 1.95.0. Wider font/text/layout/API and platform reconciliation,
canonical docking U4-U9, Nara rendering integration, numeric cost budgets and full R11/R12
remain open. This framework prerequisite selects no new Editor or game-runtime default.

## November 6: Runtime Font Installation And Missing Glyphs

The verified local Open-GPUI successor is `ed2a55383f411fec6eb6a87b410c49fbeb83aadb`, following
`871c012c96c` on `optimize/docking-graph-transaction`. The fixed Zed target remains
`635026fabc38440fce5e1767d125026a17b12012`, and no publication follows from this continuation.
The framework's existing queue, plan, migration guide and verification guide own the detailed
commands, source adaptations and execution limits.

The unit reconciles `9e6e14160c`, `3389d87577`, `b7d50b95e8` and `b68add5b37`. Descriptor
resolution now retains Direct/Fallback/Missing outcomes. Successful font installation clears
descriptor selection and advances the generation observed by ordinary, wrapped and hash
layouts. Current/previous cache state and generation share one private lock; stale range
markers and delayed wrapped results cannot alter a newer generation. Window entry points
capture generation before resolving FontRuns. Width pools and borrowed wrappers separately
track selection and width-cache generations, so a same-ID fallback change or a new cache
cannot preserve an old selection. Existing loaded font identities remain usable.
Public headless/test contexts now expose the App's exact TextSystem rather than independent
cache/generation/reporting wrappers over the same platform backend. Installation through
their public accessors reaches the actual window owner.

Missing-glyph reporting preserves complete graphemes and proportional/monospace source class
through a bounded queue and receiver deduplication. One replaceable App callback runs after
shaping locks are released. Its task belongs to the App; ready batches wait for App borrow
release and revalidate generation and quitting before delivery. Replacing or dropping
subscriptions, destructor reentry and App release retain explicit ownership. Slot restoration
on panic does not promise automatic receiver-task restart.

The source review exposed cache-generation races, delayed wrap publication, width-pool and
wrapper-construction gaps, hash retry discovery, foreground App reentry and sink destruction
under a backend lock. These were corrected through existing owners and concrete regressions.
The original four-case baseline reproduced three failures while its rejected-install control
passed. The focused font cohort passes 51/51; the public-context correction passes its two
accessor cases and complete all-feature Core/WGPU passes 999/999,
including actual WGSL pipeline and atlas-lifetime controls. Real Window candidates cover
ordinary/wrapped/hash refusal and retry with exact layout Arc and committed revision checks.
The affected UI/components/docking/Canvas cohort passes 2,964/2,964 across 25 binaries after
the public-context correction.

Cosmic retains loaded face/configuration identities while refreshing family/fallback lookup.
The unit explicitly includes `d0f797a38c`'s retained font-source storage and `d6358b2d3c`'s
fallback-only face preservation. Tracked IBM Plex/Lilex/Noto fixtures preserve licenses and
fixed source hashes. Noto remains usable as actual fallback after a primary request rejects
its missing Latin measurement glyph; exact source indices, selected face/glyph and software
raster at scales 1.0/1.5 are verified. Pointer retention and software raster establish neither
a measured memory/timing gain nor native screen pixels.

The native Windows public Render/shape/paint trajectory passes 1/1. Installation uses the
App's actual TextSystem and private DirectWrite collection; explicit text-child notify yields
a newer accepted/nonempty renderer-Submitted frame with the new direct face. The original
layout paints again, old glyph metrics remain usable, and exact HWND/platform/OLE/font-owner
retirement completes. Lilex must initially be absent from the OS font collection. FontId
changes also change the cache key; same-ID invalidation is proved separately by Core/Cosmic.
Complete Windows passes 189/189 with four existing interactive cases skipped.

Locked workspace and independently locked consumer compilation, Core/Windows/WGPU correctness
Clippy, root/consumer formatting and existing import/document/release/public-API checks pass.
Independent Core/App, Cosmic/native and public-context owner reviews leave no outstanding
P1/P2 within this unit. Execution used Rust 1.95.0; the declared Rust 1.92 floor and dependency
versions remain unchanged, without Rust 1.92 execution evidence.

Applications retain font fetching and explicit redraw authority. Windows/CoreText do not gain
missing-glyph reporting from the optional default hook. Current Web uses Cosmic directly;
the target's later Canvas fallback layer remains a separate platform migration. Font
reselection after adding weights/styles to an already matched native family is queued
separately; the missing-family installation test does not qualify it. Font prewarming/name
policy, Unicode wrapping, layered text, window-local Rc state, Taffy and other
API/platform reconciliation, engine View integration, measured budgets and full R11/R12 remain
open. This prerequisite selects no new Nara Editor or game-runtime execution default.

## November 6: Native Font Reselection And Retained Faces

The verified local Open-GPUI successor is `b803d542b674af34feea0ca277b2ac6721de9a0e`, following
`ed2a55383f4` on `optimize/docking-graph-transaction`, without publication. The fixed Zed target
remains `635026fabc38440fce5e1767d125026a17b12012`. Its DirectWrite implementation
retains the same already-matched descriptor and system-only fallback gaps. These are owning
corrections found during comparison; a newer source tree is not evidence of complete behavior.
The framework's verification guide, candidate queue and existing plan own the detailed runs.

DirectWrite now revalidates selections after successful private or system collection publication.
Unchanged actual faces retain their FontIds and refresh collection/fallback metadata; changed
faces append records while old glyph identities remain usable. Complete requested descriptors
partition features and fallback configuration. Named private fallback precedes system fallback.
Native identity-cache entries retain the face whose IUnknown address they index. Unknown callback
faces retain their actual identity and simulation instead of resolving family/weight/style again.
Identity reuse requires exact COM or native FontFace5 equality, with conservative allocation
when equivalence is unsupported or unconfirmed.

The first baseline exposed a fixture assumption as well as the defects: DirectWrite synthesizes
Bold from Regular and already reports weight 700. Corrected controls use simulation flags,
actual fixture outlines and captured metrics. The corrected five-case baseline passes the
original missing-family native trajectory and fails the four new cases. An isolated cold-cache
callback also fails after confusing a retained synthetic face with a newer real Bold selection.

The first focused correction passes 6/6, but a stronger complete run passes only 194/195 with
four existing interactive cases skipped. Shaping again through the observed synthetic FontId
loses its 620-unit advance. The collection construction now retains the actual face for glyph
work and uses its unmodified backing file/index for format selection, so DirectWrite reapplies
the effective weight/style. Both second-shaping assertions and a previously unregistered native
synthetic-face case remain. The corrected focused cohort passes 9/9, including real `fi`
ligature configuration, stable IDs across multiple installs and successful system refresh.
Complete Windows then passes 197/197 with four existing interactive cases skipped, retaining
native rendering/input, cached/deferred pixels, candidate refusal/retry and exact release.
The earlier shared Core/WGPU 999 and component/consumer 2,964 results keep their original
scope; they were not rerun for this private native-platform correction.

Locked workspace and independently locked consumer compilation, Windows correctness Clippy,
root/consumer formatting and existing import/document/release/API-tier checks pass on Windows
11/Rust 1.95.0 with one build job. Existing nonblocking warnings remain. The declared Rust 1.92
floor and dependency versions remain unchanged; that compiler floor is unexecuted. Independent
Windows review, Mac source review and main integration resolve the confirmed findings while
retaining the unexecuted platform and variable-axis limits below.

The two native installation scenarios share the complete public Render/shape/paint/Submitted
and exact retirement fixture. They verify the original App/Window TextSystem, explicit child
notification, new accepted/nonempty submitted generations and old-layout repaint. Private Noto
fallback checks actual face/glyph/source indices and visible repeated raster at scales 1.0/1.5.
Windows failed-batch recovery retains the prior published collection until the next successful
publication; the builder may retain earlier valid files. This is not batch-atomic rollback.

CoreText source and five deterministic tests prepare matching selection invalidation, private
cascade construction and complete native feature/cascade identity. The obsolete OpenType module
is removed. Independent source review identifies locked CoreFoundation typed-dictionary equality
and an unchecked fallback traits path. The owner now compares complete attributes through CFType
and shares non-panicking traits validation before primary/fallback property matching, retaining
valid no-`m` fallback faces. Prepared mixed-script tests span 12/16/32px and old-layout raster.
macOS compilation, native execution and cross-size identity remain unqualified on this Windows
host. Windows submission is not glyph-pixel/compositor evidence. OS font-change watching,
custom variation-axis APIs, measured costs, full framework refresh, Nara engine View integration
and complete R11/R12 retain their separate scope.

## November 6: Cosmic Paragraph Shaping And Public Paint

The verified local Open-GPUI successor is `f48d20d27862e45ff27fcd7ba4f261fd9119d0f7`, following
the independent Core correction `3d0714f44e206a14fda7d6d830c7a58ce06bc94a` after `b803d542b67`
on `optimize/docking-graph-transaction`, without publication. It adapts
`c214057e086517920b214800725c5d16294ddf0d` against fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. The existing Cosmic 0.19 adapter passes multiple
BiDi paragraphs into one ShapeLine, which asserts when their directions differ. The original
four-case baseline reproduces three direction assertions and a width-oracle failure later
traced to the fixture's zero-width separator assumption.

The adapter now shapes B-class separators and adjacent paragraphs independently, then restores
their original UTF-8 glyph indices and horizontal offsets on one logical baseline. Widths sum
and ascent/descent take maxima; source bytes and public newline/wrapping policy remain. A private
monotone FontRun cursor keeps configuration and zero-length boundaries without repeated full
run scans per segment. This removes source-level repeated traversal without a measured timing
claim. Direct `unicode-bidi` uses the already resolved 0.3.18; both lockfiles change only that
package edge, without dependency/compiler version updates.

Eight expanded cases pass after correcting a fixture's zero-width separator assumption.
An additional regression then finds that segment-local missing reporting splits the source
CRLF grapheme. Raw missing indices now aggregate before glyph-output conversion, restore source
offsets and report once against the complete original text and FontRuns. Complete graphemes and
source proportional/monospace class therefore retain their original owner contract.

The first public run passes nine unit cases but exposes a pre-existing Core/fixed-target
closest-hit defect at the last LTR glyph: a position in its left quarter jumps to the line end.
The public assertion is retained and the owning final-cluster comparison has a separate Core
regression. This bounded LTR correction does not establish general visual RTL caret mapping.
The isolated Core regression fails before correction, then all ten selected unit cases pass.
The public trajectory exposes an invalid glyph-atlas diagnostic assumption; a public text-system
delegate now observes successful paint-raster calls while direct software-raster oracle calls
remain separate. The corrected complete public trajectory passes 1/1 without relaxing its
source, geometry, notification, reconstruction or lifetime assertions.
Final all-feature Core/WGPU/Windows plus public paragraph consumer passes 1,207/1,207:
Core 943, WGPU 66, Windows 197 and public consumer 1, with four existing interactive Windows
cases skipped. Existing real Windows text/input/rendering and exact release remain qualified
for their own backend; the Cosmic paragraph consumer keeps its headless/software scope.
Affected UI core/components/docking/Canvas passes 2,964/2,964 across 25 binaries with no skipped
cases, including existing text-field, virtualized collection, overlay and docking consumers.
Locked workspace and independently locked consumer compilation, Core/WGPU/Windows correctness
Clippy, root/consumer formatting and existing import/document/release/API-tier checks pass on
Windows 11/Rust 1.95.0 with one build job. Existing nonblocking warnings remain. Independent
production and observation reviews leave no outstanding P1/P2. Rust 1.92 remains declared and
unexecuted; other native platforms, browser execution and measured costs remain unqualified.

Tracked unmodified Noto Sans Hebrew Regular 3.000 from official notofonts revision
`ffebf8c1ee449e544955a7e813c54f9b73848eac` retains its OFL, exact blob and SHA-256. Real bases
and zero-advance marks shape and raster at scales 1.0/1.5 with original source indices. This
fixture has no Latin `m`; the existing primary filter can omit it from an explicit fallback
chain, while the retained database face serves Cosmic's automatic fallback. Actual selected
glyph evidence is separate from general configured-fallback priority.

The public consumer uses HeadlessAppContext, Render, canvas, shape and paint with independent
segment recipes. Its trajectory includes notification, changed text size/canvas layout bounds,
repeated frames, different-size window reconstruction and exact App TextSystem/one shutdown.
TestPlatform accepted paint and actual paint-raster calls plus Swash raster do not establish
GPU/native submission, backend resize acknowledgement, compositor output or engine View integration.
Unicode wrapping, full script/caret support, fallback-policy reconciliation, measured budgets,
remaining platform execution and complete R11/R12 retain their separate scope.

## November 6: Unicode Wrapping, Indentation And Truncation

The verified local Open-GPUI successor is `b466037e87e17f11e7acce70706b7866edda3ca1`, following
the separate compiler-compatibility correction `09fe9d2ec46f013030852683e1adfe8d00b56ccd` and
TypeId correction `8d35827da9a0bd335c3e85201ee82dbd77135224`, without publication. It
continues the same fixed Zed target `635026fabc38440fce5e1767d125026a17b12012`,
reconciling `f0210f1621c6be9df4510a4356fa5049680e1c01`,
`4f70d91bda7ec5f5600a000a0dc34391d9f1e96f` and inherited omission
`03a8544040ff95ec2e7921ca71c83799ad7f17cd`. The earlier Middle-truncation equivalence claim
was incorrect. This confirms why the content-verified import anchor remains necessary alongside
the July-to-target diff: the earlier selected ports did not establish complete source parity.

The existing wrapper replaces its script/punctuation heuristic with the target's non-complex-scripts
ICU line opportunities across fragments and atomic inline elements. Explicit `IndentAdjustment`
is a breaking third `wrap_line` argument. Middle truncation flows through public TextOverflow,
fluent style and element measurement with redistributed source TextRuns. Emergency wrapping and
all truncation directions preserve complete source graphemes. Shaped wrapping groups actual
adjacent visual glyphs by their original source grapheme, retaining its first actual visual glyph
as the boundary, including an RTL mark preceding its base across font runs.

Consumer and independent-review regressions expose additional defects beyond the upstream port:
carried End content, Start's real row budget and zero-deletion candidate, one-row explicit
paragraphs, retained CR in public shaping, and trimming a Prepend/ASCII-punctuation grapheme.
Start and End now share exact wrapper/paragraph probes and require source deletion before adding
an affix. Empty trailing paragraphs may be omitted without an affix. Wrapped Middle keeps complete
first/last paragraph ends together. This consolidates one wrapping authority while retaining font
resolution/width generations, independent width pools, admitted/aborted cache epochs and exact
TextSystem lifetime. Overflowing multiline probes perform bounded logarithmic source searches;
this correctness tradeoff has no measured speedup or allocation/retention claim.

Root ICU segmenter/data resolves to 2.3 with the required zero-copy closure; the independent
consumer keeps compatible ICU 2.4 and its existing Taffy/WGPU pins. Unrelated resolution churn was
removed. Actual Rust 1.92.0 execution exposes existing unsupported TypeId slice, profiler hint and
Windows match-guard constructs. Stable replacements preserve hash bytes, profiler state and the
same eight mouse-button decoder/handler paths. Framework Core/WGPU/Windows and independently
locked consumer all-target/all-feature checks pass on that compiler. The target's Rust 1.98.1
workspace pin does not define the extracted framework's floor. This is Windows source-compatibility
evidence; the new narrow minimum-compiler CI job remains locally prepared and unexecuted remotely.

Final all-feature Core/WGPU/Windows libraries plus both public text consumers pass 1,250/1,250:
Core 985, WGPU 66, Windows 197 and public consumers 2, with four existing interactive Windows
cases skipped. Affected UI core/components/docking/Canvas pass 2,964/2,964 across 25 binaries
without skips. The independently locked public API consumer passes 3/3; existing dependency-policy
tests pass 2/2. Locked workspace and independent-consumer compilation, correctness Clippy and
explicitly rerun root/consumer formatting pass on Windows 11/Rust 1.95.0 with one build job.
The framework verification record retains the reproduced source/review failures and disk-space
recoveries, including an initial MSVC PDB link failure and a later compile-only space failure.
Independent text and compiler/native-dispatch review leave no outstanding P1/P2 in this unit.

The real-Cosmic public trajectory uses actual fonts and style-driven measurement/paint, notified
reflow, narrower allocation, reconstruction and final TextSystem release. It checks unclamped
public shaping as an independent row-count oracle and retains headless/software scope. The
expanded DirectWrite trajectory paints full finite-width Unicode and multistyle Middle text in
actual accepted, nonempty Submitted frames; a notified narrower width increases rows and shortens
the retained Middle output while preserving original source, tab, separator and teardown checks.
Submission is not compositor glyph-pixel evidence. These scopes do not qualify general RTL caret,
full complex-script segmentation, fallback priority, other platforms, measured costs, Nara engine
View integration or the complete framework refresh. Font prewarm/name/raster reuse, layered text,
window-local ownership and Taffy/API work remain in the fixed-target queue. Complete R11/R12 and
the Editor/game execution choice remain open.

## November 6: Font Preparation, Inventory And Raster Reuse

The verified local Open-GPUI successor is `1d148d66dfdf89115046e08d6e01923b4b2e2165`, following
Unicode revision `b466037e87e17f11e7acce70706b7866edda3ca1` and separate DirectWrite alias correction
`19a4ed3434a19e0cedbaadff5bbd63bac00e7441`, without publication. It continues fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`, reconciling
`f1d27d545e79f98ffde7ebf3229f8eb8bad791aa`,
`5b4a2153a087055a7f9cb3464f188f1cab9fd678` and
`1ff7cb669b6351eaf2a5b79ccb40f5cd1676b8f8`.

Public `TextSystem::prewarm_fonts(&[Font])` resolves complete descriptors through the existing
generation-aware cache, deduplicates resolved IDs and invokes an optional platform hook.
Cosmic primes the same primary/fallback match attributes used by normal shaping. Callers choose
when and where to prepare; the framework adds no thread, task or redraw owner. Successful font
installation retains existing invalidation, and failed installation preserves the prior cache.
Available-family inventory now contains only sorted unique backend-reported names, including
registered fonts; aliases continue to resolve independently of suggestions.

Cosmic retains a measured glyph image for the subsequent raster request under its complete
RenderGlyphParams key. The fork bounds this temporary handoff to 256 images and 8 MiB of retained
image-buffer capacity. Pressure discards pending images and normal rasterization regenerates
them, including oversized images; it introduces no glyph rejection or text-capacity reduction.
This bound covers the new pending pixels, not the bounds cache, atlas, complete font memory or
process retention. Independent cold raster oracles check size, placement and bytes at scales
1.0/1.5, including grayscale, subpixel and actual Windows color emoji. Actual Swash attempt
counts distinguish the removed second generation from existing atlas reuse. They do not establish
frame-time, allocation-distribution or retained-process improvements.

Preparing another alias in the complete native installation consumer exposes an existing
DirectWrite defect: the native callback cache overwrites the first physical face's output FontId.
The owning correction preserves that first mapping while retaining separate descriptor IDs,
features, fallbacks, simulations and COM leases. A deterministic alias regression reproduces the
failure independently. The original installation consumer keeps the added preparation and its
full source/metrics/outlines, retained old glyph repaint, newer accepted nonempty Submitted frame
and exact HWND/platform/OLE retirement assertions. DirectWrite uses the optional default prewarm
hook; this proves identity/lifecycle correctness rather than a new native raster benefit.

The final all-feature Core/WGPU/Windows libraries and both public text trajectories pass
1,268/1,268 with four existing interactive Windows cases skipped. Affected UI core/components,
docking and Canvas pass 2,964/2,964 across 25 binaries with no skips. Public Cosmic paint retains
headless/software scope; actual Windows submission qualifies its own backend. The independently
locked public fonts/wrapping consumer passes 5/5 without skips, preserving actual Cosmic aliases,
mixed source, installation, old glyph bytes and exact release. Locked workspace/independent
compilation, affected correctness Clippy, both formatting checks and existing import/document/
release/API scans pass on Windows 11/Rust 1.95.0 with one build job and the shared target.
Actual Rust 1.92.0 also passes affected Core/WGPU/Windows and independently locked consumer
all-target/all-feature checks. Existing nonblocking warnings remain. Independent implementation
and native-identity reviews leave no outstanding P1/P2 in this unit. The framework verification
record retains the original inventory/alias failures, initial Bounds annotation correction and
final reruns. Build-cache cleanup causes native dependency reconstruction during compilation;
these cold build results are separate from runtime performance.

Text split/layered paint/underlines, window-local text ownership, Taffy/intrinsic/root/grid/stack
policy and remaining platform differences stay in the fixed-target queue. Source comparison for
the next text unit adds independent visual-RTL split and transformed-underline exclusion oracles;
it adds no runtime qualification. Other native platforms, complete script/caret support, measured
budgets, Nara engine View integration, complete docking and R11/R12 remain open. This framework
prerequisite does not select an Editor/game execution default.

## November 6: Source Fragments, Text Layers And Underline Exclusions

The verified local Open-GPUI successor is `55ddbcde39527629a28555b5eadb678cddfccd71`, following
`1d148d66dfdf89115046e08d6e01923b4b2e2165`, without publication. It reconciles
`2936989f1b`, `f792c2d72d`, `bda9c0bd43` and `0dea8c63d9` against the same fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`.

LineLayout now owns raw source-index partition and foreground/background painting. ShapedLine
delegates split and adds a forward cursor: ordered input advances glyphs/decorations once,
while visually reordered input scans the remaining glyphs per split. Exact original glyphs,
source indices and coordinates survive; painting fragments at the previous recorded advance
reconstructs original geometry. This fixes a reproduced RTL index-underflow failure without
pretending fragments are independently reshaped bidi layouts, caret positions or reflow results.
Adjusted source lengths and invalid UTF-8 boundaries are refused under documented contracts.

Public TextLayout background/foreground passes let a custom Element reuse StyledText measurement
and prepaint, insert its own highlights and paint glyphs/decorations. Local Rc line snapshots
release the internal borrow before backend reentry. Explicit passes propagate errors; ordinary
drawing finishes backgrounds before foregrounds while retaining its logging behavior. Shared
aligned/wrapped footprints and actual continuation/final-row origins keep backgrounds and
decorations together. Zero-advance background widths include the existing half-cell adjustment.

Independent review reproduces an additional fragment paint-order defect: a retained glyph beyond
the fragment advance is absent from the batch's registered footprint, so a later opaque cover
can draw underneath it. Actual sprite/quad ordering tests fail before correction. Reordered,
non-increasing-position and outlying glyphs now use primitive bounds for ordering; normal
contained byte-ordered text retains batching. This is a correctness policy, not a measured
batching, allocation or frame-time improvement.

Underline callbacks expose byte ranges, resolved styles and logical geometry. Window bounds and
painting share the existing snapped/projected primitive. Absolute device-space exclusions add
exact visible-span clips without shortening full wave geometry, preserving phase, opacity,
transformed/rounded ancestor clips and replay. Deterministic primitive tests cover fractional
origins, scales 1/1.25/1.5/2/3, nested transforms, interval normalization and invalid-scope recovery.

The corrected focused suite passes 19/19, including real Cosmic mixed-direction fragments,
custom layered paint and the complete native Unicode/notification/Submitted/retirement flow.
Final Core/WGPU/Windows plus three public text trajectories pass 1,281/1,281 with four existing
interactive Windows cases skipped. Affected UI core/components/docking/Canvas pass 2,964/2,964
and independent fonts/wrapping/layer API tests pass 6/6, both without skips. The independent
layer test type-checks the custom Element and method signatures; actual drawing comes from the
separate complete consumers. The framework record retains both reproduced owner defects,
the half-open Bounds test correction and native floating-intersection containment correction.
Locked workspace/independent compilation, correctness Clippy, both formatting checks and existing
import/document/release/API scans pass on Windows 11/Rust 1.95.0 with one build job and the shared
target. Actual Rust 1.92.0 also passes affected Core/WGPU/Windows and independently locked
consumer all-target/all-feature checks. Nonblocking warnings remain. Independent split/cursor
review closes the ordering P2; the separate Window/TextLayout review finds no additional
confirmed P1/P2. Reviews and executed main-thread qualification retain their own scope.
Actual primitive order, software glyph raster and native submission remain separate evidence;
the new tests establish no compositor glyph/exclusion pixels, general RTL caret/reflow, complete
script/fallback support, full existing ink-overhang coverage, other native platforms or numeric
costs. Window-local ownership, Taffy/API/platform work, canonical docking, Nara View integration
and R11/R12 remain open.

## November 6: Window-Local Text Ownership And Source Cache

The verified local Open-GPUI successor is `1f01e811c101e5a81e52e955942f6a24a8974902`, following
`55ddbcde39527629a28555b5eadb678cddfccd71`, without publication. It uses the same fixed
Zed target `635026fabc38440fce5e1767d125026a17b12012`. Provenance has three distinct
parts. Official `772a8a91006db1c44171545cb7ba2d4c23d3e4cb` supplies Rc window-text ownership,
RefCell layout state and per-window font-run scratch. Official follow-up
`dde76a1dca5305607a940309ada0568350d1aac6` uses Cell take/set for that scratch in the final
target. Official `2a74c8dff6902b7d02038050f5672a0989b18f94` deletes the unused hash-only layout
APIs and cache. The fork adopts those final owners and removes its parallel hash keys,
maps, range indices and source-empty ShapedLine path without compatibility wrappers.

The fork separately retains generation-coherent current/previous caches and rejected-candidate
recovery, required by its existing runtime font installation and atlas-lease admission consumers.
Backend shaping now runs without a local cache borrow; publication revalidates font generation
and reuses a canonical result published by reentry. This reentry behavior is a fork adaptation:
the fixed official ordinary shaping path still holds RefMut across its backend call. Shared
global TextSystem and immutable LineLayout results retain Arc ownership. Window/cache access
stays on the window thread; actual shared font prewarming/installation can run on a worker.

Independent review also finds a defect in both the starting fork and official target: wrapped
layout keys omit max_lines. A new fresh-owner oracle reproduces unlimited wrap boundaries being
reused for a one-line request. The existing owned/borrowed content key now includes the actual
wrapping limit; plain layouts use None. Tests cover both request orders, exact source,
same-clamp identity and committed/aborted frame reuse. This correction is explicitly fork-owned,
separate from the official migration. Another review correction removes forced refresh from
notified width/font observations so those checks require the automatically accepted successor.

The public Cosmic trajectory uses two actual headless windows with distinct Rc caches over
one exact global TextSystem. It retains complete source, cross-frame plain/wrapped reuse,
notified narrower layout, font identity/cache invalidation, old glyph paint and reconstruction.
The independent lockfile directly consumes two Noop WindowTextSystem owners and the public
getter signature; it does not launch an App/Window. The native three-cycle reconstruction
trajectory preserves original wrapper/HWND/platform/OLE release assertions and adds actual
nonempty Submitted frames, rebuilt cache identity and retained Rc lifetimes. Immutable layouts
remain readable after all text-system owners drop; painting still requires the original live
font namespace.

Corrected all-feature Core/WGPU/Windows libraries plus the four public text trajectories pass
1,292/1,292, with four existing interactive Windows cases skipped. This includes the clamp
regression and notify-only observations. Both bounded independent reviews close their confirmed
P2 findings. Affected UI core/components/docking/Canvas libraries and integrations pass
2,964/2,964 across 25 binaries without skips; independently locked fonts/wrapping/layer/window
text tests pass 7/7. Locked workspace/independent compilation, affected correctness Clippy,
both formatting checks and existing import/document/release/API scans pass on Windows 11/
Rust 1.95.0 with one build job and the shared target. Actual Rust 1.92.0 affected-framework
and independently locked consumer all-target/all-feature checks also pass on Windows.
Existing warnings remain. The framework verification record retains the original 29/29 and
5/5 checkpoints, reproduced clamp failure, corrected notify consumer and exact final runs.
Native submission, headless accepted paint and direct Noop API execution remain separate
evidence. These ownership changes do not measure CPU, allocation, frame-tail or
process-retention improvement. Taffy/intrinsic/root/grid/stack and remaining platform work,
canonical docking, Nara engine View integration and complete R11/R12 remain open; no Editor/game
default changes.

## November 6: Taffy, Text Measurement And Root/Grid Sizing

The verified local layout successor is Open-GPUI `11d9a96033ea8425b07f64fdf2bad6503f1720dc`,
following `1f01e811c101e5a81e52e955942f6a24a8974902`, without publication. It uses the same
fixed Zed target `635026fabc38440fce5e1767d125026a17b12012`. Official sources are
`91fdd55889`, `b05f40c554`, `31ceaf7908`, `cdf33ac25e`, `b488c4e814`, `b0da438545`,
`5ccbbbd88f` and `41d72f6c1d`. Taffy 0.15.0 is selected in both qualification lockfiles,
with compatible patches allowed by the library and an explicit independently locked
application pin. Its original direct Taffy value supplies preview dimensions rather than
a Taffy-to-GPUI conversion API; this unit does not claim that interoperability or a distinct
Taffy patch-version comparison. WGPU retains the independent consumer's existing patch choice.

The leaf adapter adopts the official result callback and alignment types, retaining explicit
relative positioning and existing logical/device conversion, stroke rounding and bounds caches.
Measured Elements receive pass-dependent optional known dimensions and content available-space;
Taffy 0.15 final layout supplies None for known dimensions. The callback returns content size,
while Taffy applies padding, border, authored dimensions and box constraints. Tests use
scale 1/1.5/2 through the existing test-platform geometry/resize route.

Official text changes retain prior truncation width before serving unconstrained queries and
check actual shaped width before nonwrapped truncation. Unknown list widths use MaxContent.
The official final target still lacks a general word-based MinContent text algorithm; adopting
these corrections does not establish one. An additional review exposes an ordinary wrapped
cache result reused for a later MaxContent probe in both the starting fork and official target.
The regression reproduces 60x384 instead of a fresh 672x24. Exact wrap-width equality now
guards that same cache, retaining both official truncation checks. This is an explicit fork
correction, separate from the official migration.

Window and prompt roots fill only auto axes. Explicit/percentage sizes and min/max constraints
retain their authored behavior; independent measured roots retain shrink-wrap sizing. Row
min/max-content APIs use the renamed GridTemplateMinSize without an alias. Official stacker
is opt-in across Taffy callbacks and all three Div phases. Default call-stack execution and
stack-growth wrappers retain separate qualification and do not promise arbitrary tree depth.
No new layout, cache, renderer or engine document owner is introduced.

The corrected initial owner/style/root suite passes 18/18 after fixture corrections for actual
width constraints, leaf callback semantics and valid draw phases. The first selected real-Cosmic/
Windows suite passes 11/11 before the inherited ordinary-wrap correction. The native kerning
fixture is then strengthened to install the bundled font with explicit kerning and require the
per-character wrapper to alter all three exact-fit strings before actual StyledText may prove
full-source paint. Public/native consumers observe notified successors before any compensating
refresh, keep actual grid/root/text geometry and require original reconstruction/retirement.
The corrected complete Core/WGPU/Windows libraries plus five public text/layout consumers
pass 1,312/1,312 with four existing interactive Windows cases skipped. The stacker-disabled
owner/portal selection passes 33/33; affected UI core/components/docking/Canvas pass 2,964/2,964
and independently locked public text/grid tests pass 9/9 without skips. A root-positioning
portal expectation now includes its authored inset, and a four-Sidebar identity fixture uses
an explicit visible grid instead of stacking full-height sidebars outside the viewport.
Original transform/presentation/linkage and physical-input/identity assertions remain, with
added layout/viewport geometry assertions. Both fixture adjustments have bounded independent
review; no corresponding portal/Sidebar production changes occur. The framework verification
record retains earlier failures, corrected oracles and exact reruns. Locked workspace and
independent-consumer compilation, affected correctness Clippy, both formatting checks and
existing import/document/release/API scans pass on Windows 11/Rust 1.95.0 with serial one-job
builds and the shared target. Actual Rust 1.92.0 affected Core/WGPU/Windows and independently
locked consumer all-target/all-feature checks also pass on Windows. Nonblocking warnings
remain; these results do not qualify other platform targets.
Numeric CPU/allocation/P99/process retention, other platforms,
remaining framework differences, canonical docking, Nara View integration and full R11/R12
remain open. No Editor/game execution default changes.

## November 6: Shared Image Decoding And Animation

The verified local image successor is Open-GPUI `995ac8576362e43f36364616b33cc45c9560fab7`,
following `11d9a96033ea8425b07f64fdf2bad6503f1720dc`, against the same fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Official sources are EXIF correction
`9552acc2bc242d45342fa9b5a987d43868aee1ec` and that final target's shared GIF/WebP
decoders. Both retired decode blocks are inherited official code, unchanged from the content
import anchor; their duplication is not a fork-specific contract to preserve. The existing
platform owner now performs static orientation before BGRA conversion and shared animation
decoding. ImageAssetLoader keeps resource acquisition. Successful frames retain their delays,
WebP has a transparent canvas, bad frames follow the official skip/empty-output error policy,
and SVG keeps its existing renderer. Animated EXIF is outside that official mechanism.

Official behavior and local adaptations remain distinct:

- Shared helpers return contextual errors, so loader raster decoder failures now enter
  ImageCacheError::Other rather than ImageCacheError::Image. This observable upstream change
  is recorded in migration notes; no public decode signature changes.
- The fork declares image 0.25.4 after inspecting that exact archive's orientation/background
  APIs. The official EXIF message states that intent but its actual manifest and final target
  still say 0.25.1. Both qualification locks remain on 0.25.10. Source inspection does not
  establish an executed full-workspace 0.25.4 matrix, and the separate target AVIF feature
  policy remains queued.
- Img elapsed time now uses the existing App executor clock, matching its loading timer.
  A public 199/200 ms regression reproduces the strict loading deadline: notification at
  exactly 200 ms previously failed to paint and scheduled no successor. The fork includes
  that deadline with `>=`; it adds no new clock, image cache or renderer owner.

The official EXIF and one-frame WebP failures are reproduced before adoption. Added controls
cover asymmetric pixel-exact transforms with eight distinct outputs, static/animated channel
and alpha preservation, GIF delays, transparent pixels outside WebP frame bounds, contextual
errors and malformed/truncated input. Isolated Image-only and path-only headless roots each
notify and paint without another loader, clock advancement or a compensating draw. Real Img
timelines distinguish 99/100 ms and loop twice; static images have no animation callback or
idle revision change. These headless image tests use NoopTextSystem and qualify no real-font
or GPU behavior. Atlas hits remain separate from existing view-cache/refused-candidate gates.

Actual Windows encoded/embedded Img instances complete 0/1/0 through native frame requests.
Every observation binds nonempty Submitted, accepted/rendered/submitted-diagnostic generation
and GPU readback, then checks source replacement, EXIF geometry, atlas reuse and exact native
retirement/reconstruction. No injected frame indices, forced native draw or fabricated receipt
drives that trajectory. Offscreen framework capture does not prove compositor display.

Final Core/WGPU/Windows plus six public consumer binaries pass 1,328/1,328, with four existing
interactive Windows cases skipped. Affected UI core/components/docking/Canvas pass
2,964/2,964 and independently locked public consumers pass 12/12 without skips. Both bounded
independent rereviews close confirmed findings. Locked workspace/consumer checks, affected
correctness Clippy, root/consumer formatting and existing import/document/release/API scans
pass on Windows 11/Rust 1.95.0, with serial one-job builds and the shared target. Actual
Windows Rust 1.92.0 affected-framework and independently locked consumer all-target/all-feature
checks also pass. The framework verification record retains red results and fixture corrections.
Other platforms, numerical CPU/allocation/P99/process retention, remaining image-feature/API/
external-loop/renderer-lifetime reconciliation, canonical docking, Nara View integration and
complete R11/R12 remain open. This changes no Editor/game execution default.

## November 6: Externally Driven Application Lifetime

The verified local application successor is Open-GPUI
`e4364085634906472d23dab0efe0b9305fab7a04`, following
`995ac8576362e43f36364616b33cc45c9560fab7`, against the same fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Official source
`74798c68d5c63d31e2ccca5c8f5ec0a02c90679c` supplies Application::run_embedded and a
strong ApplicationHandle with update/to_async. The final target leaves the handle methods
unchanged; later launch additions initialize App-owned display observation and conditional
Subsecond hot reload. Those remain separate closure differences, rather than implicit features
of the handle. The fork reuses its existing shared launch path, direct display/snapshot queries,
AppCell and shutdown owner. It adds no renderer, engine attachment or universal Host.

The explicit fork adaptation wraps handle updates in the existing App transaction, matching
AsyncApp::update. Copying the official direct callback body reproduces defer-only effects
stranded after return and nested entity work escaping the outer host generation/batch. The
corrected entry flushes effects before return and retains borrow-release native convergence.
Reentrant same-App entry still panics; unwind restores bookkeeping without rolling back queued
work or user state. AsyncApp remains weak and does not automatically cancel every future.
Delayed platform launch can retain its own callback/App after handle drop, so returning a
handle is neither a ready signal nor a cancellation protocol. Public nonterminal shutdown
retains its existing preparation barrier and completed-window lifecycle.

TestPlatform and VisualTestPlatform invoke launch and return through their existing scheduler/
window owners. A host using VisualTestPlatform drives both deterministic tasks and real native
messages. Stock Windows Platform::run still blocks and owns process exit: the new API does not
provide production Windows externally driven-loop support or shared GPU ownership. Ordinary
Wasm run keeps its existing permanent retention; run_embedded supplies the separate explicit
owner. Wasm execution is not established by the native qualification.

Corrected core lifetime tests pass 10/10, covering ordinary/embedded ownership, delayed launch,
update batching, defer-only effects, reentry/unwind recovery, wake-after-borrow delivery, async
timers and shutdown/release. The selected owner/native consumer passes 11/11. The latter uses
public ApplicationHandle/AsyncApp for four stateful text/quad stages over real Windows windows,
requiring matching accepted/nonempty Submitted/rendered/diagnostic/capture generations. Effects
settle before polling; background await and a visual timer drive actual successors. Exact native
retirement, observed App/model/root/asset/global-text/platform release and one OLE uninitialization
complete the trajectory. Input includes direct callback and synthetic SendMessageW ingress;
offscreen capture and text-backend weak references do not prove OS input fidelity, compositor
display, every GPU allocation or native vsync-thread termination. Text pixels establish visible
text, with source correctness checked separately from pixel OCR.

Complete Core/WGPU/Windows plus six existing public text/layout/image consumers pass
1,339/1,339, with four existing interactive Windows cases skipped. Affected UI core/components/
docking/Canvas pass 2,964/2,964 and independently locked public consumers pass 14/14 without
skips. Two independent embedded consumers type-check the public API without starting an App
or enabling test-support. Bounded production/consumer review finds no confirmed P1/P2. Locked
workspace/consumer checks, affected correctness Clippy, both formatting checks and existing
import/document/release/API scans pass on Windows 11/Rust 1.95.0 with serial one-job builds and
the shared target. Actual Windows Rust 1.92.0 affected-framework and independently locked
consumer all-target/all-feature checks pass. Dependencies and both lockfiles remain unchanged.

The queue now separately records official owning asset loads (`5896274168`), display observation
and conditional hot reload alongside the remaining platform/API/renderer/image-feature closure.
These require complete consumers and their own dispositions. Numeric costs, production platform
embedding, canonical docking, Nara View/render integration and complete R11/R12 remain open.
No Editor/game execution default changes.

## November 6: Official Framework Baseline And Reusable Additions

The owner's clarification makes the fixed target's official framework mechanisms the default
maintenance baseline. Preserve their supported behavior and regression scenarios, then build
features and refactor reusable modules on that foundation. First adjust or replace fork-owned
mechanisms when reconciliation needs a change. A retained difference needs a concrete consumer
or Accepted contract, source evidence and an owning verification path. An unfinished official
mechanism remains queued rather than being silently treated as rejected. Ordinary public
consumers must remain usable without Nara-private dependencies.

Open-GPUI's repository maintenance skill records this rule at local commit `bf3e0d2f7c6`;
the existing plan and candidate queue reference that one source. This changes maintenance
disposition, not execution evidence, the fixed Zed comparison target or the Nara Editor/game
default. The current asset successor adopts official owning CachedLoad and public API evolution;
its borrowed-App publication adjustment belongs to the fork's existing transaction contract.

## November 6: Owning Asset Loads And View Completion

The verified local asset successor is Open-GPUI
`1855a94c5eeb7e0a97e69479232211827efc3942`, following the maintenance-rule clarification
`bf3e0d2f7c6` and App successor `e4364085634906472d23dab0efe0b9305fab7a04`. Official
`5896274168e06663568f42b5bae142162936e0a6` and final target
`635026fabc38440fce5e1767d125026a17b12012` supply owning CachedLoad entries, one background
load and one foreground publisher, deduplicated interested views and cached results. App fetch
now returns Option output without subscribing; Window use registers each interested view.
Opaque ImageCacheItem new/get/use_image replaces shared task handles and public loading/loaded
variants. App, Window, retain-all and the actual gallery LRU use that same mechanism.

The direct official publisher reproduces a fork-specific active-App-borrow panic. The local
adaptation waits through existing AppCell release notifications, then publishes and notifies
inside the complete App transaction. It retains only weak App/entry identities while waiting.
Entry eviction cancels pending publication, including a ready result waiting for a borrow;
old completion cannot update a replacement. Owner cases include repeated borrow waits,
unwind recovery, terminal shutdown, observer-driven replacement and exact App/result release.
This is a concrete fork integration adjustment, not a reason to replace official cache ownership.

Focused owner/image/LRU/candidate/native cases pass 22/22. Actual two-window consumers share
one load and receive one notification each, then automatically produce exact accepted/nonempty
Submitted image frames with matching diagnostics and GPU readback. Passive fetch/get cause
neither notifications nor extra render/paint work after initialized native frame opportunities.
The record retains its initial no-redraw failure and correction: isolate pointer/activation
changes and establish an explicit initial submitted frame while results are still held. No
compensating refresh/draw is added after completion. Atlas-key removal retains accepted scene
leases, and the image releases after native windows retire while App remains alive.

The previously dormant gallery now has an explicit platform-package example, preserving core
dependency direction. Its actual capacity-one/two LRU tests cover pending cancellation,
replacement and two-window animated-frame disposal. A cached view's refused candidate retains
the exact old scene/generation/dependencies; an explicit notified retry accepts the same result
identity. That retry is distinct from the native automatic completion trajectory.

Complete affected runtime/public/gallery passes 1,361/1,361, with four existing interactive
Windows cases skipped; components/docking/Canvas pass 2,964/2,964 and independently locked
public consumers pass 16/16 without skips. The latter type-check the new public asset APIs
without launching an App or enabling test-support. Locked workspace/consumer checks, affected
correctness Clippy, root/consumer formatting and existing import/document/release/API scans
pass on Windows 11/Rust 1.95.0. Actual Windows Rust 1.92.0 affected-framework and independent
consumer all-target/all-feature checks also pass. The root lockfile only declares existing
gallery dev dependencies; no dependency versions or independent lockfile change. Bounded
production/consumer review closes confirmed findings, and all six subthreads are closed.

Offscreen native capture is not compositor or OS-input qualification. Native replacement uses
a passive key; subscribed replacement and candidate refusal have their separate owner cases.
Browser contention, numeric CPU/allocation/P99/process retention, non-Windows execution,
production external Windows loops, remaining fixed-target differences, canonical docking and
Nara GPU/View adoption remain open. No Editor/game execution default or complete R11/R12 changes.

## November 6: Connected Displays And Window Association

The verified local display successor is Open-GPUI
`e5ad73a24a69274378264f792f4efb4d6d892ac6`, without remote publication. It follows
owning-assets `1855a94c5eeb7e0a97e69479232211827efc3942`
and adopts official `f495dd0f77b69e287ed72c1168dc02965f08be7d` as retained by fixed target
`635026fabc38440fce5e1767d125026a17b12012`. App owns connected-display bounds and refresh
interval observation; Window owns its association. Public App/Window/Context subscriptions read
ready cached state, deduplicate spurious hints and initialize after platform connection in both
launch paths. Ordinary consumers require no Nara-private types.

Fork integration reuses immutable display topology, coherent committed window facts and ordered
native ingress. Earlier bounds facts cannot swallow a later association event. Windows uses a
hidden top-level platform HWND to receive broadcasts without app windows. Refresh intervals
participate in the existing publication generation. macOS and Wayland source map the complete
mechanism to existing owners; X11/Web retain unknown refresh intervals, and Linux headless no
longer fabricates a monitor. Non-Windows source adaptation is separate from native execution.

The official Hz helper's tiny-positive reciprocal overflow reproduces before a checked Duration
conversion corrects it. Full consumers also expose a fork deferred-close reentry loop: an async
native close had returned its platform callback before the user handler sent a nested WM_CLOSE.
The existing Window close-handler slot now protects both paths, preserving later close requests
and unwind cleanup. The geometry-wake fixture settles actual startup messages before its
unchanged exact-wake/coalescing/committed-geometry assertions. These corrections preserve official
display ownership while repairing fork integration.

The corrected focused owner/native selection passes 22/22. Complete affected runtime/public/
gallery passes 1,388/1,388, with four existing interactive Windows cases skipped. The native
display trajectory proves zero-window WM ingress, current-monitor rereads, an actual nonempty
Submitted frame, busy-App ordering and exact HWND/window/App/backend/OLE release. Detached Rust
association and deterministic property fixtures retain their distinct scopes. No physical monitor
mode or topology was changed.

Components/docking/Canvas pass 2,964/2,964 and independently locked public consumers pass
18/18 without skips. The independent build catches bitflags still being optional under Wayland
in the fork. The required Core dependency now follows the official public display types;
ordinary consumers need no extra feature activation. Versions and both lockfiles stay unchanged.
Locked workspace/consumer checks, affected correctness Clippy and root/consumer formatting pass
on Rust 1.95.0. Actual Windows Rust 1.92.0 framework/consumer all-target/all-feature checks pass.
Bounded production/acceptance reviews leave no confirmed P1/P2; all eight subthreads are closed.

An additional wasm32-unknown-unknown production-library check passes through Core/WGPU/Web.
Web test-target compilation stops in unchanged wait-timeout 0.2.1 test-support dependency before
the backend; source comparison retains the fork's proptest 1.10.0 pin versus target 1.12.0 for
the remaining dependency audit. No version is changed, and neither that comparison nor the
successful production compile establishes browser/test-support qualification.

Refresh-aware telemetry `4f6d97b9de7d9f167da8b7832eeecea22c5a9efa` remains a separate
queued owner. Physical hotplug/refresh-mode changes, non-Windows native execution, numeric costs,
remaining fixed-target differences, conditional hot reload, canonical docking and Nara GPU/View
adoption remain open. This source work does not complete R11/R12.

## November 6: Unified Profiler Foundation And Accepted Frame Measurements

The qualified local implementation is Open-GPUI
`3e5a907775e9f22d0a092a0f9f27c6f1696ade0e`, with qualification and prerequisite records at
`a536b1d93380cc9247a7cc9576ac6b81cdb65489`. Both are local commits without publication.
The fixed Zed target remains `635026fabc38440fce5e1767d125026a17b12012`. Source inspection
after the display successor identifies missing inherited mechanisms beneath refresh-aware
telemetry. This unit adopts official profiler feature gating `48511e0b9c`, safe last-reference
thread collection `c16d19c94d` and the common WindowProfiler
`a21007b7a948e46afbe719150f5e9968bfcd1078`, plus scheduler-clock, draw-duration-return and
dirty-to-present follow-ups. Official mechanisms remain the baseline; later journal, hang,
visibility, signal and refresh-aware work stays queued.

The official `profiler` feature now governs task/action/window measurement. Runtime trace
settings and overlapping scopes control detailed retention while aggregates remain available
with the feature enabled. Collection releases the global registry lock before locking, copying
or dropping upgraded thread owners. Fork integration removes InputLatencyTracker and the old
`input-latency-histogram` feature, retaining actual submission outcomes, pending input and exact
refresh-callsite diagnostics through the single WindowProfiler. Ordinary consumers use public
interfaces without Nara document, ECS, history or private Host types.

Fork adaptations preserve accepted-candidate and exact-submission authority. Only Submitted
completes input covered by that accepted draw; late input remains pending during old-scene
retry. Keyboard dispatch can draw synchronously, so the owning profiler associates input with
that draw only when it covers its invalidations and later handler work has not invalidated
again. Reset remains authoritative through draw rollback. Existing Window rejection handles
Render/prepaint/paint unwind while preserving the last scene and rebuilding consumed by-value
element state. Accepted publication failures finish subsequent accepted outputs and Window
completion before propagating the first panic. Executed regressions retain these recovery
paths with profiling enabled and disabled.

Separate source-confirmed corrections serialize final trace cleanup against re-enable and fix
inherited action/task slow-history ranking. Actual red baselines establish reset, Window and
partial-action-history defects; the trace race has deterministic green coverage and source
evidence, without a claimed executed red result. These corrections are distinguished from
official adoption in Open-GPUI's existing verification record.

The corrected framework/public/gallery suite passes 1,426/1,426 with four preexisting interactive
Windows cases skipped. The all-feature UI core/components/docking/Canvas suite passes
2,966/2,966 without skips. Disabled Core passes 1,095/1,095; independent enabled/disabled
consumers pass 20/20 and 18/18, with final scoped 6/6, 2/2 and 1/1 checks after the last
feature-boundary changes. The native trajectory checks synthetic input through production
callbacks, exact nonempty Submitted receipt correlation, actual paint/input batches, a settled
no-input redraw, retirement, reconstruction and HWND/App/backend/OLE release. It does not
qualify OS input injection or scanout.

The actual 20,000-node/19,900-edge labeled Canvas Criterion smoke passes with profiling on and
off. Its inherited quadratic setup is replaced with complete public snapshot construction
outside the measured loop; data scale, records, labels and rendering remain intact. This is
neither a runtime mutation optimization nor numeric performance evidence. Locked workspace/
independent checks, correctness Clippy, formatting, existing import/doc/release/API scans and
actual Windows Rust 1.92.0 framework/Canvas/consumer checks pass. Production Core/WGPU/Web
libraries compile for wasm with profiling on and off. Browser execution and the unchanged Web
test-support dependency gap remain separate. All ten implementation/review subthreads are closed.

Continue with official multi-subscriber wake and sleep/window visibility before foreground
journal/hang detection, the GPUI-owned monitor, first frame-signal provenance and refresh-aware
measurement. Power/cadence, complete upstream benchmark/report/debug-overlay disposition,
conditional hot reload and remaining framework/platform differences are still open. This unit
does not select the Nara renderer/View adapter, complete canonical docking U4-U9, establish
numeric budgets or finish R11/R12, and it changes no Editor/game default.

## November 6: System Power And Presentation Visibility

The qualified local Open-GPUI implementation is
`8b283cb151b070d7d5851a531a86e6d6d67196cb`, with the qualification and queue record at
`8554f9e765b4fe3369480fcba2d0580ec008bd18`. Neither commit is published. The fixed Zed
target remains `635026fabc38440fce5e1767d125026a17b12012`. This unit adopts official
multi-subscriber wake `7a7c3e1d2f03195c5fa19bc890da330ad7f3abef` and sleep/window
visibility `3db02c2c77659ad29810594e6249287ea1e791f7`, using their final-target mechanisms
across the framework and backend closure.

App owns independent sleep/wake subscriptions; the former Application singleton wake setter
and slot are removed. Window caches presentation visibility before delivering distinct changes,
with Window and weak entity-bound Context observers. Native shown/mapped state remains a
separate query. Public TestAppContext and VisualTestContext methods let extension tests drive
the same supported visibility and subscription path. Ordinary consumers need no Nara document,
ECS, history or private Host types.

Fork adaptations preserve the existing thread-safe native ingress, ordered post-borrow delivery
and shutdown discard. Already emitted sleep/wake and visibility boundaries are not coalesced;
native deferred sampling can still collapse native-message bursts as the official mechanism
does. Windows additionally samples SetWindowPos show/hide commits because the fork's actual
initial/provisional reveal need not emit separate show/size messages. Exact callback retirement
and a strictly newer Submitted restore receipt remain part of the existing native owner.
Visibility observation adds no implicit frame wake, renderer submission or hide/reveal authority.

The complete enabled framework/public/gallery/native cohort passes 1,446/1,446, including both
public lifecycle integrations and 209 Windows cases. Four preexisting interactive Windows cases
remain skipped. UI core/components/docking/Canvas passes 2,966/2,966; disabled Core/public
passes 1,113/1,113. Independently locked enabled/disabled consumers pass 28/28 and 26/26.
Their lifecycle tests distinguish ordinary API compilation and standalone subscription controls
from the separate actual App/Window delivery tests. Native execution covers real initial reveal,
SetWindowPos hide/show, minimize/restore, busy-App ordering, unsubscribe, reconstruction and
exact HWND/platform release. Synthetic power messages enter the actual hidden platform HWND
without suspending the machine.

Locked workspace/consumer checks, affected correctness Clippy, both formatting checks, existing
import/doc/release/API scans and actual Windows Rust 1.92.0 framework/Canvas/consumer checks
pass. Production Core/WGPU/Web compile for wasm with profiling off and on; browser execution
and the existing Web test-support dependency gap remain separate. Foreign backend sources are
migrated and independently reviewed; native macOS/Linux execution remains unqualified. The
record retains early compile/fixture corrections and the corrected headless Hidden instruction.
All nine implementation/review subthreads are closed.

Continue at official foreground journal/hang detection, the GPUI monitor, frame-signal provenance,
refresh-aware measurement and power/cadence. Physical suspend, general Windows occlusion,
numeric costs, complete benchmark/report/debug-overlay reconciliation, hot reload, remaining
fixed-target differences, canonical docking U4-U9 and Nara GPU/View adoption remain open.
This prerequisite preserves official mechanisms and modular public use; it selects no Editor/game
default and does not complete R11/R12.

## November 7: Foreground Journal And Hang Monitor

The qualified local Open-GPUI implementation is
`8493e9eb1cad7c3336fa99b4b8749c5be1d6c7c5`, with qualification and the advanced queue at
`84407f27ffacac0f2f5945c61220eb7567e7ac54`. Neither is published. The fixed cycle target
remains Zed `635026fabc38440fce5e1767d125026a17b12012`. This unit adopts the complete
official journal/detector/monitor checkpoint `d284cbbc87b55d659303106ced627c8612369827`,
including introduction `1861e58f98`, journal consumer support `7316cf7745`, trigger rules
`97b1e64a17`, lifecycle filtering `f0fb48c71a`, unnotified suspend `7232ba04ef`, monitor
`0f9c923e67` and both monitor/test corrections. Its complete measurement-v2 mechanism precedes
the final target's queued frame-signal/completed-skip and refresh-aware measurement-v3 work.

Official ownership remains intact: the foreground thread writes a bounded ring with bounded
pending publication, independent collector cursors and discontinuity reporting. Short polls
aggregate without losing count or total duration. Presentation, idle and power transitions are
explicit interval boundaries; demand deadlines, visibility and suspend clocks control valid
measurement. The detector reports completed intervals. App optionally owns a dedicated monitor
thread and joins its final poll/callback with existing bounded shutdown work. Public consumers
use the same APIs without Nara documents, ECS, history or private Host types.

Fork integration distinguishes actual work from successful presentation. Accepted/refused and
unwinding candidates retain their existing authority; only the exact valid Submitted frame can
complete its covered input and presented interval. Failed submission work is retained without
claiming success. Demand cancellation and old-scene retry preserve the real pending work.
Task/action scopes balance nested work and unwind, restoring enclosing timing state. Official
foreground-idle APIs and Web callbacks are restored through existing executor/dispatcher owners.
Queued foreground polls and unqueued background/timer work executed on that thread have distinct
count custody. Existing dispatcher admission now reports refusal so late closed-queue wakes do
not strand a shared counter; Windows/Linux keep their native !Send rejection policy. Web later
registration rejection settles its admitted count on the originating foreground thread.

Independent review reproduces canceled-focus demand, failed-retry deadline renewal and both
ordinary/idle late-wake defects before their owning corrections. Original regressions, public
consumers and counter-controls remain; all 45 official module scenarios are retained. Monitor
admission also rejects zero or unrepresentable intervals without consuming its App slot. Public
controls execute observer panic/reentry, final callback completion and bounded waiting when that
callback blocks. These are concrete fork/library corrections, distinct from official adoption.

Complete framework/public/gallery/native passes 1,531/1,531, with four preexisting interactive
Windows cases skipped. UI core/components/docking/Canvas passes 2,966/2,966; disabled
Core/public passes 1,114/1,114. Independently locked enabled/disabled consumers pass 32/32 and
26/26, without skips. The native trajectory correlates two journal collectors, exact input kinds,
paint timestamps and actual Submitted receipts, then visible reconstructed redraw, off-thread
monitor Flush and callback/resource release. Its hidden initial reconstruction still has an exact
Submitted receipt; visibility filtering intentionally excludes that initial dirty timestamp, and
the extra visible redraw retains the original complete dirty/draw/present oracle.

Locked workspace/consumer checks, affected correctness Clippy, both formatting checks, existing
import/doc/release/API scans and actual Windows Rust 1.92.0 framework/Canvas/consumer checks
pass. Stable production wasm compiles with profiling off and on. Optional multithreaded wasm
requires the existing nightly/shared-memory configuration; its stable attempt stops in unchanged
wasm_thread 0.3.3, while the installed 1.98.0-nightly production check passes with that configuration.
This compiles the mailbox branch, not browser callback execution or the unchanged Web test-support
dependency gap. Both lockfiles retain every package version/source/checksum and add only the
existing Core libc and Web helper dependency edges. All ten implementation/review threads close.

Flush completes a poll of finalized entries and its callback; a reader-contended publisher tail
can wait for the next foreground publication. Dispatcher admission does not guarantee execution
of accepted-but-never-polled backlog. These boundaries remain explicit, with complete publisher
drain and platform queue retirement still unqualified. Continue with official frame signals,
completed skips, refresh-aware v3 and power/cadence. Complete benchmark/report/debug-overlay
comparison, hot reload, native macOS/Linux, browser execution, physical suspend, numerical costs,
remaining fixed-target differences, canonical docking U4-U9 and Nara GPU/View adoption stay open.
This unit changes no Editor/game default and does not complete the refresh or R11/R12.

## November 7: Platform Frame Signals And Completed Skips

The local Open-GPUI implementation is `5811543bebe10657b9f99b42bfadb214e7dad999`, with
qualification and the advanced queue at `c7fb5ccd8a81ee8221fb90711b02649516b4c4fa`.
Neither is published. The fixed cycle target remains Zed
`635026fabc38440fce5e1767d125026a17b12012`. This unit adopts official frame-signal provenance
`cd4fc8de4ca8548cca2567352b87bcaaec13328f` and nonempty completed-skip correction
`36b6d0951fdee409f0957294a69360ba2e8e980e` through existing platform, Window and journal owners.

Official `RequestFrameOptions`, `FrameRequestSource` and `PlatformFrameSignal` preserve one
captured timestamp/source pair across coalescing. Profiling-disabled capture does not evaluate its
closure and the accumulator has no timestamp storage or atomic work. The atomic retains official
offset saturation and equal-time native-source preference; it is not a lossless arbitrary-clock
ordering promise. Raw callback arrivals and reentrant deferrals are diagnostics, not foreground
work or completion. Completed no-render/inactive/thermal skips seal only retained work, leave
pending demand intact and do not flush folded polls on every idle refresh. Refresh-aware
measurement v3 and later power/cadence are separate mechanisms.

Fork adaptations retain concrete existing contracts. Arrival is recorded before App access,
while the native ingress queues reentrant requests, merges force/presentation requirements and
preserves the earliest known signal pair. Active arena scope identifies draw-time arrival without
another draw-state owner. Delayed delivery does not duplicate the raw marker; refused/unwound
draws are not completed skips. The throttle helper regains official required-presentation bypass
and high-rate-input exemption from inactive throttling, while retaining thermal limits and exact
recovery/Submitted authority. Known signal pairs survive local backend adapters intact.

Windows binds each signal to its exact HWND/WindowId/generation, captures after actual VSync
wait or fallback sleep and drains stale signals on hide/minimize/retirement. Native workflows
exercise real Submitted frames, reconstruction, visibility recovery, delayed retirement, final
signal release, stale registrations and an actual background VSync signal before delivery.
macOS source adopts the per-display registry prerequisite `96ce8f2a05`, host-clock conversion
and stop cleanup while retaining structured startup and callback retirement. Wayland's wrapping
millisecond conversion and plausibility fallback are source-preserved. X11's current Expose path
still joins its local timer. Web RAF keeps official unavailable captured time/native provenance;
explicit local requests capture before post-borrow delivery. Native foreign-platform and browser
execution are not qualified by that source comparison or portable compilation.

Final Core/Windows library/public tests pass 1,482/1,482 with four existing interactive skips.
The earlier WGPU/gallery selection contributes 228 passing cases and one fixture-regeneration
skip. Its unchanged clipboard test failed once, passed an unchanged isolated rerun and passed
the final complete Windows run; the cause of the first failure remains undiagnosed. An interrupted
first final run has no completion claim. UI core/components/docking/Canvas pass 2,966/2,966;
disabled Core/public pass 1,122/1,122 and independently locked enabled/disabled consumers pass
38/38 and 29/29 without skips. Public test-platform flows retain exact accepted/submitted
generations, pending callbacks, inactive presentation bypass, Deferred demand and close/release.
Typed but unexecuted independent closures remain signature evidence, distinct from runtime tests.

Locked workspace/consumer checks, correctness Clippy, formatting and existing import/doc/release/
API scans pass. Actual Windows Rust 1.92.0 framework/Canvas/consumer checks pass. Stable wasm
production compiles with profiling off/on, and the existing nightly shared-memory configuration
compiles its multithreaded branch. These establish neither browser execution nor numerical
costs. All eight implementation/review threads are closed. No engine default or compiler-floor
change follows.

The source audit identifies an inherited official prerequisite that must be completed next:
`eb354c8d504071bdb79110a7a5c9d374c2864113` owns Wayland's complete demand-driven
FrameLoop/ping/retry/deadline contract, related Core scheduling and Web completion-hook removal.
X11's direct Expose/recovery mechanisms `ae99a867d7` and `f25434f3c5` need the same owner
reconciliation. Current timestamp support does not approve permanent divergence or close those
loops. Continue through that complete platform prerequisite before cross-platform refresh-aware
v3, power/cadence and later macOS MetalFrameSource. Full fixed-target refresh, numeric budgets,
publisher-tail/backlog lifetime gaps, benchmark/report/debug-overlay comparison, hot reload,
canonical docking U4-U9 and Nara GPU/View adoption remain open. This unit does not complete R11/R12.

## November 7: Demand-Driven Platform Frame Loops

The local Open-GPUI implementation is `b6506b48efca022c13256ef17d5ecfa26787b2ae`, with
qualification and the next complete measurement owner at
`9901ed0bcc6272a647bccfb73016f3f22206a013`. Neither is published. The fixed cycle target
remains Zed `635026fabc38440fce5e1767d125026a17b12012`. This unit adopts official weak
invalidator wakeups and Web RAF parking `417d03305ac194470f5d85a4b584c34dcec8cf57`, complete
Wayland FrameLoop/ping/retry and Core scheduling `eb354c8d504071bdb79110a7a5c9d374c2864113`,
their captured signal custody, and X11 direct Expose/recovery `ae99a867d7`/`f25434f3c5`.

The official scheduling model is the baseline. `PlatformWindow::schedule_frame` and its optional
weak `frame_waker` connect existing invalidation, App effect flushing and next-frame callbacks
to the backend source. The completion-driven `completed_frame` hook is removed. Dirty work,
accepted scenes awaiting presentation and callbacks each retain demand; a callback queued during
a frame requests a successor. `AsyncApp::refresh` completes its update/effect flush so parked
sources wake. Wakeups release the invalidator borrow before delivery and must not retain App or
Window. Wayland retains the complete configure/ping/compositor/retry/park loop, Web coalesces RAF
demand and parks after delivery, and X11 dispatches Expose outside the client borrow after native
event draining while retaining forced recovery reconstruction.

Fork adaptations preserve named contracts rather than a parallel scheduler. Wayland completes
its frame through the existing native command after App borrow release; Windows keeps the same
post-borrow maintenance owner and continuous VSync. Exact candidate acceptance, unsuccessful
presentation, mandatory recovery and terminal retirement retain their authority. Independent
review finds an accepted-publication unwind gap: an accepted clean scene can still need
presentation after a publication callback panics. The callback owner now rearms pending work,
restores rejected dirty/force demand where necessary, attempts native completion and resumes the
original panic. Real scheduled-callback tests require the accepted generation to submit without
another draw and a rejected render to retry without replacing its old accepted scene.

Corrected focused Core/public tests pass 62/62; actual private Web/Wayland/pointer production-state
modules pass 27/27 on the host. The affected native selection passes 5/5. Its extended VSync
trajectory uses public AsyncApp refresh, callbacks queued mid-frame and actual newer nonempty
Submitted generations, then proves pending callback capture release during retirement. It keeps
the original profiler, journal, monitor and reconstruction assertions. Windows execution does
not qualify Wayland or browser parking. X11's six callback regressions remain source-only here.

The full framework/public/gallery/state/native run passes 1,748 of 1,749 cases; one unchanged
native image decode/animation trajectory times out waiting for Submitted plus RGBA capture. Its
unchanged isolated rerun passes 1/1. The cause remains undiagnosed, and the record does not claim
a clean single-run 1,749/1,749 result. Five preexisting skips remain four interactive Windows
cases and the gallery fixture-regeneration command. Profiling-enabled components/docking/Canvas
pass 2,966/2,966, disabled Core/public passes 1,134/1,134 and independently locked enabled/
disabled consumers pass 40/40 and 31/31 without skips. Their two new scheduling tests establish
public signature compilation; five separate crate-external TestPlatform workflows execute the
actual parking, refresh, mid-frame callback, refusal/retry and held-terminal-close behavior.

Locked workspace/consumer checks, correctness Clippy, formatting and existing import/document/
release/API scans pass. Actual Windows Rust 1.92.0 framework/Canvas/consumer all-target/all-feature
checks pass. Stable wasm production compiles with profiling off/on, and the existing nightly
shared-memory configuration compiles the multithreaded branch. These are separate from browser,
native foreign-platform and physical timing execution. No manifest or lockfile changes are needed.
All nine implementation/review subthreads close; initial fixture failures, interrupted compilation,
the image timeout/rerun and source-only scope remain recorded in the existing qualification guide.

Continue at complete official refresh-aware measurement v3 `4f6d97b9de`, including real signal/
dirty-to-submission work, per-refresh reports, view self-time and hang/active-use rules, through
existing framework owners and ordinary public consumers. Later power/cadence and MetalFrameSource,
benchmark/report/debug-overlay comparison, publisher-tail/backlog retirement, conditional hot reload,
canonical docking U4-U9, numeric costs and Nara GPU/View adoption remain open. This prerequisite
changes no Editor/game-runtime default and does not complete the fixed-target refresh or R11/R12.

## November 7: Refresh-Aware Frame And Hang Measurement V3

The qualified local Open-GPUI implementation is
`411895475e9e51a9f06954d5d676333d597b991f`, with qualification and the advanced queue at
`110f9c88084227eb526515167c204af3555f22a8`. Neither is published. The fixed cycle target
remains Zed `635026fabc38440fce5e1767d125026a17b12012`. This unit adopts complete official
measurement v3 `4f6d97b9de7d9f167da8b7832eeecea22c5a9efa` and relevant final cleanup through
the existing profiler, Window, view, journal, detector and monitor owners. Zed application-only
telemetry/report consumers remain comparison inputs rather than framework dependencies.

Frame work starts at the later of its captured signal and first invalidation and ends at platform
submission start. A missing signal leaves it unmeasured; unknown refresh retains the official
fallback budget and no invented missed-refresh count. Public snapshots group accepted draws and
successful submissions by the refresh interval captured for each draw, including on-time, late,
unmeasured, missed-refresh and input-response statistics. Draw request-layout, prepaint and paint
durations retain their actual work. View contributions name the underlying type through erased,
weak and failed-downcast paths and exclude nested view work from parent self-time. The v3 detector
retains threshold, late-frame and blocking-work triggers, contributor serialization, active-use
attribution and background-monitor active-time results.

Fork integration preserves exact accepted-frame/input custody. A callback signal cannot overwrite
an older scene awaiting submission or leak from a no-render callback into a later direct draw.
Each draw owns its signal, refresh and phases; refusal, reset, unwind and old-scene retry retain
their existing rules. Only actual Submitted completes the covered frame/input measurement.
View phase scopes balance during unwind, and valid draw work survives visibility filtering while
unproven signal/dirty latency remains unmeasured. Ordinary public consumers acquire no Nara types.

Independent review and executed red controls reproduce four defects inherited from the official
mechanism: poll-only input-history growth, time consumption erasing evidence for a long-open
incident, completion-order clipping of enclosing work, and interpolation across inactive input
gaps. The sealer now retains its prior-boundary floor until sealing and considers the earliest
retained work. Active attribution uses bounded state and boundary-order observation. Existing
input dispatch supplies chronological raw activity controls and an exact-journal weak guard,
including nested/unwinding dispatch, so active time measures the covered interval union rather
than interpolating first/last input. These controls add neither work nor a completion boundary.
The explicit example with inputs at seconds 1 and 200, consumed at second 201, retains its
61-second expected union; the baseline returns 200 seconds. The original ten-millisecond outer
work assertion also remains. Independent reference/property controls retain caps, discontinuities,
loss/suspend, initial/capped/repeated consumption, guard retirement and dispatch coverage.

The final focused Core/public cohort passes 208/208 and expanded Windows profiler/signal
trajectories pass 2/2. The corrected complete framework/public/Gallery/production-state/native
cohort passes 1,809/1,809 with five preexisting skips: four interactive Windows cases and the
Gallery fixture-regeneration command. Its full native image-animation trajectory passes without
an isolated retry. UI core/components/docking/Canvas pass 2,966/2,966, disabled Core/public passes
1,134/1,134 and independently locked enabled/disabled consumers pass 46/46 and 32/32 without
skips. Signature-only closures remain separate from actual TestPlatform and native execution.
Native evidence correlates captured signal/source, display interval, emitted view work, dual
collectors, actual Submitted, reconstruction and exact callback/resource retirement. Short
controlled delays make attribution observable and establish no numerical overhead or speed claim.

Locked workspace/consumer checks, correctness Clippy, formatting and existing import/document/
release/API scans pass. Actual Windows Rust 1.92.0 affected-framework/Canvas/consumer all-target/
all-feature checks pass. Stable wasm production compiles with profiling off/on, and the existing
nightly shared-memory configuration compiles its multithreaded branch. No manifest or lockfile
changes are required. All eight implementation/review subthreads close. The existing qualification
record retains the initial 184/187 run, owner red cases, native fixture correction, deliberately
interrupted first broad build and final results; foreign native/browser execution remains separate.

The interface audit also identifies inherited missing official generic View/ViewElement
`74b5207744ef95505fd75c56bf2c9d4736e931ee`. Current measurement covers the fork's actual
Entity/AnyView/PresentedAnyView path; that does not establish stateless/generic interface parity.
The complete interface, macro and reusable-component owner is explicitly queued before hot reload
and full-refresh closure. Continue next with complete official power/cadence and MetalFrameSource,
then remaining interface/report differences. Publisher-tail/backlog retirement, complete benchmark/
report/debug-overlay comparison, conditional hot reload, canonical docking U4-U9, numerical costs
and Nara GPU/View adoption remain open. This unit changes no Editor/game-runtime default and does
not complete the fixed-target refresh or R11/R12.

## November 7: Power Modes And Stable Frame Cadence

The next maintained-framework successor starts at Open-GPUI
`110f9c88084227eb526515167c204af3555f22a8` and is implemented locally as
`2ceb09aa4590388b8e2d73ac1406caddc5b6c470`, with the fixed Zed target unchanged at
`635026fabc38440fce5e1767d125026a17b12012`. It adopts complete official power observation
`fdcb198110`, stable cadence `2580a00264`, configurable limits `7c8dbadcb5` and macOS
MetalFrameSource `0373366f43`, including the later offscreen screenshot correction `f816e1f926`.
The owner's clarification keeps official mechanisms as the baseline, refactors fork adaptations
first and requires ordinary public modules to remain usable without Nara types.
Failure diagnostics are committed at `1a6d90bdaac5256c36064be9824459de74ea91bf`; qualification
record `a89a7200c80fc40eb073dbad37789f17153ec7b8` preserves commands, failures and limits.

Platform/App own power queries and independent observations. Window owns the longest enabled
inactive/low-power/thermal interval and stable cadence. Dirty work, pending scenes, callbacks
and sustained high-rate input share that decision; mandatory presentation and existing recovery
still bypass limits. Idle callbacks consume no cadence slot. Windows native messages, Linux
modern/legacy providers and macOS process-info/Metal/CV sources stay in their existing backend
owners. Unknown native state remains Unknown. No second render or scheduler owner is introduced.

Fork adaptations retain ordered native ingress, immutable reader snapshots, exact candidate/
Submitted authority and weak native-generation retirement. Native qualification exposes an
inherited fork defect: a detached retirement retry retains the platform after another maintenance
drain destroys its HWND transport. The correction uses HWND-bound retry tokens in the existing
coordinator, rejects cancelled/previous-platform delivery, cancels before final destruction and
uses the existing validated message as timer-admission fallback. Exact platform and callback weak
owners release before another timer turn. This defect and correction belong to the fork, not Zed.

Corrected focused Core/public qualification passes 76/76, native Windows passes 14/14 and actual
production Linux profile/Metal state passes 3/3 on the Windows host. Public two-window workflows
exercise independent options, observer cancellation, retained force/callback/input demand,
mode-change recovery and exact Submitted generations. The independently locked fixture separates
value/signature controls from actual App/window execution. Components/docking/Canvas pass
2,966/2,966, independent enabled/disabled consumers pass 51/51 and 36/36, and disabled Core/public
passes 1,162/1,162 without skips. The complete framework run executes 1,850 cases: 1,849 pass and
an IME fixture fails before its explicit unlimited-window correction. Subsequent Windows runs
retain the recorded capture/activation failures. The final Windows selection passes 220/221,
with four existing interactive skips and one unchanged foreground-preparation failure. All new
power/retirement and both diagnostic consumers pass. Its final three-case image/activation recheck
passes 3/3. Prior capture timeout and activation failures remain unattributed; no clean complete
Windows gate is inferred from separate passes or relaxed assertions.

Locked workspace/independent all-target checks, final correctness Clippy, formatting and existing
import/API/document/release scans pass. Actual Windows Rust 1.92.0 all-target/all-feature checks
pass for the affected framework/Canvas and independent consumer. Stable production wasm compiles
with profiling off/on; the existing nightly shared-memory/build-std configuration also compiles.
No package versions or lockfiles change; Linux adds only a test edge to its existing Core
dependency. All fourteen implementation/review threads close. Source and scoped verification
can advance without converting the remaining native qualification gaps into completion claims.

Linux D-Bus and macOS Foundation/Metal native compilation and execution remain unqualified by
Windows portable tests. Service-owner recovery follows official Linux behavior; full system-bus
reconnection is not claimed. Nonempty Apple/Intel screenshot pixels, display migration, physical
power changes, compositor timing and numerical energy/CPU costs require separate evidence.
Generic View/ViewElement, complete benchmark/report/debug-overlay comparison, conditional hot
reload, canonical docking U4-U9 and Nara GPU/View adoption remain open. This successor changes no
Editor/game-runtime default and does not complete the fixed-target refresh or R11/R12.

## November 7: Generic Views And Reusable Component Interfaces

The maintained-framework successor starts at Open-GPUI
`a89a7200c80fc40eb073dbad37789f17153ec7b8` and is implemented locally at
`652122a4e118ff2d213ae0d9392c71f49c44553b`, against the unchanged fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Qualification and candidate corrections are recorded
at `18e5f46e87f919049628cd9fd1be782c0fbaf49a`. Official generic View/ViewElement
`74b5207744ef95505fd75c56bf2c9d4736e931ee` and final-target follow-ups replace the fork's
inherited Component and direct AnyView Element paths. Render and RenderOnce remain public.
Typed entities, erased/weak handles, stateless type scopes, the derive macro and both entity
cache routes migrate as one interface owner; no compatibility owner remains alongside it.

An ordinary manual View can combine parent props with an existing non-Render entity's identity.
The public consumers execute props replacement, sibling reorder, pointer/keyboard input, custom
Canvas painting, local state, removal/reconstruction and exact release. Entity-backed cached
embedding keeps explicit definite-size style and existing invalidation rules. Shared non-inlined
phase helpers retain fork clip/transform/presentation/resource admission, accessed dependencies,
candidate recovery, accessibility/inspector bypass and scoped profiler unwind. Stateless/manual
Views gain no independent frozen-cache authority. These interfaces remain usable without Nara
or Zed product types and do not introduce a second UI/render/state owner.

The first focused owner selection passes 117/117 and new public consumers pass 5/5. After an
exact diagnostic-result type correction in the new fixture, Windows native composition/cache/
profiler/signal trajectories pass 4/4, including actual newer nonempty Submitted generations,
notification/refresh, retirement and reconstruction. Inputs enter production callbacks outside
App borrows; this does not qualify OS injection or compositor scanout. The locked whole-workspace
all-target check passes. Independently locked ordinary consumers pass 53/53 with profiling and
38/38 without default features, without skips. Their explicitly unexecuted composition/root
closures establish signatures only. The complete framework/public/Gallery/native cohort passes
1,856/1,856 with five existing skips, including all 222 Windows cases without an isolated retry.
This later passing observation retains the preceding power unit's recorded failures and does not
establish their cause. Components/docking/Canvas pass 2,966/2,966, profiler-disabled Core/public
passes 1,167/1,167, and locked checks, correctness Clippy, formatting, existing static scans,
actual Windows Rust 1.92.0 and stable/nightly production wasm compilation pass.

Final independent review also reproduces framework-internal caller metadata in explicit entity
conversions. A bounded fork diagnostic adaptation propagates source file/line through typed,
erased, cached and upgraded-weak conversions. The public props workflow directly invokes the
derive's erased conversion, correcting an earlier coverage overstatement. This changes diagnostic
metadata/test coverage, not the official render/cache owner. The five public workflows pass
again with profiling enabled and disabled, including exact file/line controls; the exact native
composition also passes 1/1. These affected checks follow separately from the earlier complete
cohorts and preserve their original scope. Actual Rust 1.92 affected-framework/Canvas checks,
Core correctness Clippy and root/independent formatting pass again after the correction. All seven
implementation/source-audit/review agents close. No remote publication follows.

Source reconciliation also corrects product-wide candidate rejections. Official `83d4847462`
contains a framework synthetic-child callback/builder/Div path, and `2268045a11` contains
accessibility provenance, tree capture, Window JSON and dump/copy consumers. Only provenance
is debug-only; capture/JSON also exist in release builds. The fork's accepted-publication,
rollback and ADR 0023 diagnostic-redaction contracts require a separate complete successor with
actual public consumers. An unused view-name map would not close it. A bounded audit identifies
further surviving framework slices for List autoscroll, measured TextLayout queries, Rems,
os-release parsing, existing shell/path/key regressions and the complete benchmark harness.
Those source findings are queued, not implemented or runtime-qualified by this View unit.

Conditional hot reload, benchmark/report/debug-overlay comparison, complete fixed-target refresh,
canonical docking U4-U9 and Nara GPU/View adoption remain open. Native foreign-platform/browser
execution and numerical cost limits remain separate. This successor changes no Editor/game-runtime
default or R11/R12 completion status.

## November 7: Measured Layout Queries And List Scrolling

The maintained-framework successor starts at Open-GPUI
`18e5f46e87f919049628cd9fd1be782c0fbaf49a`, against the unchanged fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Utility prerequisite
`787ab55a8f68daab1dc48e2c80d12f4109028bdb` and UI implementation
`e58b8b7687aadc91408657b4e550a1b7d723c0a9` are committed locally, with qualification record
`aef505a3ba9c1d2e6b4fcf079f2298aa1e239235`. It implements six surviving framework slices
from product-labeled commits: List normalization `10628c3d2c`, measured TextLayout queries
`c7ad65e468`, Rems/key regressions `ccf4058b7a`, public os-release parsing `479bce0995` and
shell/path regressions `a923597341`/`362035d52a`. Their product search, picker, telemetry,
terminal and file-existence consumers remain outside the extracted framework closure. The
parameterized benchmark/context/dispatcher owner keeps its separate complete prerequisite chain.

Official List normalization walks preceding rows, measures uncached rows and clamps at the start.
Three executed red controls first expose the old negative offsets, including -500px. The fork
keeps PendingAutoscroll and admitted-frame publication through existing source/resource validity;
it does not restore immediate prepaint mutation. Actual atlas rejection preserves offset, exact
private revision and accepted scene. Retry advances only the vertical revision once. The public
consumer distinguishes exact accepted G+2 from Submitted G+1, presents G+2 on the following
callback and observes normalized row bounds at -10/10/30px. Zero-height and variable-height
preceding rows are covered without a second scroll or history owner.

TextLayout returns shared immutable logical-line layouts in source order and allows indexed
queries after measurement, before prepaint. Public software consumers then prepaint/paint the
same StyledText, notify, change width/source, refresh and reconstruct the window. Geometry still
requires prepaint. Rems conversion uses the current owning Window rem size; numeric values and
two-window/layout consumers retain ordinary module use without Nara types. Utility behavior and
all imported regressions remain official. Independently locked closures establish signatures
only and are separate from executed public consumers.

Focused Core/public qualification passes 54/54, utility owner/public passes 4/4 and the measured
text trajectory passes 1/1 using real Cosmic/Swash rasterization. A new fixture's one-frame
recovery premise is corrected after an actual 51/52 run. The text trajectory's cross-frame raw
Arc premise is also corrected against the target's cache policy: retained snapshots preserve
complete shaping values, but a later width change need not reuse that allocation. Actual backend
observations record zero new shaping for unchanged redraw and three calls for this reflow.
Same-width wrapped identity and same-frame query/paint identity remain asserted. No cache policy
or library capacity is reduced, and no general performance-budget improvement is inferred.

Complete Core/public tests pass 1,390/1,390, utility 115/115, WGPU 89/89, components/docking/
Canvas 2,966/2,966, profiler-disabled Core/public 1,181/1,181 and independently locked enabled/
disabled consumers 56/56 and 41/41, without skips. The affected Windows native selection passes
5/5, filtering 221 unrelated cases. This preserves existing Unicode text/redraw, cached dependency,
signal/profiler and generic View exact-submission/retirement controls; it is no new native-query
or complete Windows-suite claim. Native scope stays separate from the new software query workflow.
Locked workspace/consumer all-target checks, correctness Clippy, actual Windows Rust 1.92.0,
stable/nightly production wasm, root/independent formatting and existing static scans pass. A first
public text link fails during low disk space; only that failed invocation's PDB is removed and the
original command later links unchanged. The failure and cache-oracle correction remain recorded.
No dependency/lockfile or compiler-floor change follows; all seven implementation/review agents
close, and no remote publication occurs. Nara changes here are documentation-only.

Accessibility synthetic children
and published diagnostics, benchmark/report/hot reload, platform gaps, numerical budgets,
canonical docking U4-U9 and Nara GPU/View adoption remain open. This successor changes no
Editor/game-runtime default and does not complete the fixed-target refresh or R11/R12.

## November 7: Synthetic Accessibility And Explicit Published-Tree Inspection

The next maintained-framework unit starts at Open-GPUI
`aef505a3ba9c1d2e6b4fcf079f2298aa1e239235` against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Implementation
`751e563eb89eeb373dac41cf734bfe372b81d0e4` and qualification record
`156d0b8c1835108223bcf9fdd1f7d7c39a396013` are committed locally; qualification finishes on
November 8. It adopts the complete synthetic-child mechanism
from `83d4847462` and raw tree inspection/provenance from `2268045a11`. Element hooks, stateful
Div callbacks and the public subtree builder let custom controls expose semantic children and
mutate their parent after prepaint without constructing corresponding visual elements. The
official complete JSON schema and content-bearing Window reader remain available; the source-built
Foundation Gallery provides an actual explicit copy action through the existing clipboard owner.

Fork adaptation uses the existing accepted accessibility publication, subtree validity, modal
filtering, reference repair and prepaint rollback. Capture records accepted frame and activation
generations separately from the official capture counter, without claiming native or assistive-
technology consumption. Complete parent snapshots restore arbitrary callback mutations; exact
deferred-child rollback, early/late clip invalidation, atlas rejection and irreversible shutdown
have concrete controls. Shared entity-backed View attribution covers typed/erased/manual backing
identities beyond the target's AnyView writer. No second semantic, focus or renderer owner follows.

Accepted Open-GPUI ADR0023 now distinguishes explicit raw inspection from automatic diagnostic
artifacts. Raw labels, values, window titles and published announcements remain available to the
developer reader/copy action. Automatic DevTools histories/exports and announcement diagnostics
retain redacted metadata. Password content absent from the published tree remains absent.
Reading is passive; activation replacement, deactivation, close and interaction quiescence clear
the snapshot, and later inert publication cannot recreate it.

The reader requires an active, published accessibility tree; reading alone is not semantic
observation demand. A future Nara agent adapter still needs explicit observation demand and
window/presentation-bound operations through the existing admission owners. The debug capture's
accepted generation alone does not qualify the complete presented-control automation contract.

Three concrete regressions fail before the owner corrections: deferred-parent mutation rollback,
own-clip synthetic content and post-quiescence recapture. Focused owner/public checks pass 88/88;
Gallery actual clicks and existing privacy/field/live controls pass 13/13. A later reserved-root-ID
control fails before its duplicate-set correction. Final Core/public passes 1,412/1,412 without
skips. Components/docking/Canvas passes 2,966/2,966, WGPU/affected native passes 96/96 and
profiler-disabled Core/public passes 1,203/1,203. The 96-case selection includes 89 complete
WGPU cases and seven actual native synthetic activation, exact Submitted generation,
notification/cache/View/text/color and owner retirement trajectories. It does not close the full
Windows or real assistive-technology gate. An executed non-debug serializer slice passes six
tests, without claiming a release-profile whole App/native result.

The independently locked ordinary consumer compiles all targets with all features and without
default features; the two new type-only checks pass in each mode. No test-support or direct
AccessKit dependency is added to that consumer. Locked workspace checks, correctness Clippy,
actual Windows Rust 1.92 framework/consumer checks, stable/nightly production wasm, formatting
and existing static scans pass. Only enumn 0.1.14 and its AccessKit edge are added to the two
lockfiles; existing package versions and compiler floor remain. All eight implementation/test/
source-audit/review threads close, with no remaining confirmed correctness finding. No remote
publication follows. Nara changes here remain documentation-only.

A separate source audit keeps the two official commits split by mechanism. Active-descendant
focus with real ancestor gating, keyboard press/release continuity across focus generations,
tab-group container ordering, missing-node diagnostics, shortcut/numeric-step properties,
initial activation-tree delivery, enabled/active configuration and author identifiers remain
explicit successors. The fork's existing grouped property owner is equivalent in responsibility;
similarly named focus APIs do not establish official focus equivalence. Complete benchmark/report,
hot reload, platform/browser execution, numerical budgets, canonical docking and Nara GPU/View
adoption remain open. No Editor/game-runtime default or R11/R12 status changes.

## November 8: Composite Focus And Keyboard Continuity

The next complete maintained-framework unit starts at Open-GPUI
`156d0b8c1835108223bcf9fdd1f7d7c39a396013`, against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Local implementation
`53decdc64a2d182fad1fbf00ababcd2b55a906e7` adopts the framework composite-focus, keyboard
activation, Tab-group and missing-focus-node mechanisms from `83d4847462` and `2268045a11`.
Qualification is committed locally at `37ceac021fec7efb5091334bc60877ba36ab75d6`.
Official mechanisms remain the default baseline; fork changes adapt them through existing
owners instead of silently rejecting behavior. No dependency or compiler-floor change follows.

Composite menus and listboxes retain actual keyboard focus on a container while the selected
descendant receives accessibility focus. The fork resolves strict ancestry from the final
surviving semantic tree, after deferred parentage, modal filtering and source/geometry validity.
Its explicit accessibility-only focus override remains a different capability and cannot grant
keyboard ancestry; a different explicit display focus retains priority. Accepted inspection
keeps the base focus separate from the selected active descendant.

Official Enter/Space keyboard activation uses a shared one-shot marker. Actual focus movement,
including departure and return between draws, invalidates it; same-focus reassertion does not.
The fork also checks its existing key-dispatch revision and retires the arm through accepted
prepaint publication. Rejected candidates preserve the arm owned by the visible frame.
Window-local generation metadata counts actual focus separately from provisional intent,
including App-deferred quiescence, without moving the established post-borrow cleanup schedule.
Missing-ID/role diagnostics deduplicate at accepted publication and cannot revive after
irreversible interaction retirement. No additional focus or semantic owner is introduced.

Two concrete tests fail before the owning fixes: local prepaint rollback resurrects quiesced
focus, and an atlas-refused Inert candidate prematurely clears the accepted arm. Corrected
focused owner/public workflows pass 104/104; the profiler-disabled affected cohort passes
111/111. Complete all-feature Core/public/WGPU/Windows passes 1,770/1,770, with four existing
interactive Windows cases skipped. Core/public contributes 1,457, WGPU 89 and Windows 224.
Complete UI core/components/docking/Canvas passes 2,966/2,966 without skips. The actual
Windows composite trajectory also passes separately, retaining production callback delivery,
nonempty Submitted generations, passive inspection and exact native/root/App/TextSystem/OLE
release. Synthetic keys/activation do not establish OS-generated input, real AT consumption
or compositor scanout.

Independent bounded review closes the accepted-arm and direct/deferred quiescence findings;
all seven implementation/test/audit/review threads are closed. Ordinary independently locked
type-only accessibility/composite targets pass 4/4 in each feature mode without test-support;
the actual Gallery selection passes 13/13. Locked workspace/consumer all-target checks,
affected correctness Clippy, actual Windows Rust 1.92 and stable production wasm in both
profiling modes pass. Root/independent formatting and existing static scans pass. No dependency,
lockfile, compiler-floor or remote publication changes follow. Nara changes here are
documentation-only. The next coherent framework owner completes shortcut/numeric-step/author-ID
properties, enabled-versus-active configuration and initial activation-tree behavior through
existing modules. Complete benchmark/report, hot reload, foreign native/browser execution,
numerical budgets, canonical docking U4-U9 and Nara GPU/View adoption remain open. No
Editor/game-runtime default or full R11/R12 status changes.

## November 8: Accessibility Properties, Configuration And Initial Trees

The next complete framework unit starts at Open-GPUI
`37ceac021fec7efb5091334bc60877ba36ab75d6`, against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Official sources are `83d4847462`
(numeric step and initial root), `2268045a11` (shortcuts),
`9183ba4c5579c083b005d7f4c248d90c914fb46f` (enabled/active queries and test configuration),
and `cc053a4a6fa2fd0e8793201ed9099466af1be0b1` (author identifiers). Implementation and
qualification use the existing property, A11y, Window and ordered native ingress owners.
Local implementation is `d9608b1a3174f17df270d6b92894539ec32654a8`; its qualification record
is `1670c373e34751ac520237b7d04a480b06ab7b4a`.

The official `aria_keyshortcuts`, `aria_numeric_value_step` and `accessibility_id` builders
extend the existing grouped property module. Its writer preserves the official composition
rule: absent properties leave a custom node's existing values intact; normal fresh-node
reconstruction removes omitted source properties. Author IDs remain distinct from framework
element and action identity. The fork additionally includes the published author ID in explicit
raw tree inspection, with an actual inspection consumer. Automatic diagnostic exports retain
their existing redaction contract.

Official `Window::is_a11y_enabled` describes configuration independently of attachment, while
`Window::is_a11y_active` describes current-frame activation. The fork's old active-query name
is removed without an alias. The TestApp disabling switch affects subsequent windows.
Public source-driven controls install accessibility-only subscriptions before attachment;
passive queries neither activate nor redraw. Ordinary external dependencies retain these APIs
without Nara or test-support dependencies.

Activation synchronously returns the official immutable one-node Window root with creation-time
title and root focus. Full trees continue through ordered ingress after App borrows release;
repeated bootstrap replies retain the creation title while later complete trees use the live
title. The Send callback captures the existing admission and native-close facts through atomic
bits, without retaining the Window or borrowing App. Exact unregistration, construction rollback,
quiescence and the existing shutdown fence in the shared sequencer refuse obsolete activation.
An initial reply grants no accepted component/action/inspection or Submitted-frame authority.
Calls overlapping revocation retain their actual admission point; no new atomic transaction
covers native teardown and the reply.

Complete Core/public/WGPU/Windows passes 1,787/1,787 with four existing interactive skips;
UI core/components/docking/Canvas passes 2,966/2,966 without skips. The original Windows
trajectory executes the installed Send callback on a real OS worker while App/Window are
borrowed, then observes complete live-title publication, property updates/removal, nonempty
Submitted frames and exact native/root/App/TextSystem/OLE release. CPU TestPlatform execution
retains its deterministic scheduler contract. Initial roots, accepted full trees, native
submission and real AT consumption remain distinct evidence scopes.

Executed failures precede their corrections. An early generic admission revocation at native
close changes a queued command's exact terminal; sharing the existing native-close fact restores
WindowClosed settlement without weakening the original assertion. Old tree-history fixtures
are corrected to account for bootstrap replies while retaining accepted empty-root reconstruction.
One unchanged mouse-leave test receives an extra input/hover pair in an earlier complete run;
its isolated and complete reruns pass, but the cause remains undiagnosed. No assertion or source
change suppresses that failure.

The profiler-disabled affected Core/public selection passes 182/182, including all configuration,
tree and composite public targets. Independently locked ordinary type-only controls pass 6/6 in
each feature mode without test-support, while the actual Gallery selection passes 13/13.
Locked workspace/consumer all-target checks, affected correctness Clippy, actual Windows
Rust 1.92 and stable production wasm with profiling off/on pass. Root/independent formatting,
existing import/API/document/release scans and changed-workflow YAML parsing pass. No dependency,
lockfile or compiler-floor change follows. All six implementation/test/review threads and one
read-only benchmark audit are closed. No remote publication occurs; Nara changes are documentary.

That audit maps the next complete official benchmark harness/report owner: independent
`bench-support`, queue/timer closure, concrete `ThreadedDispatcher`, native text-system reuse
with separate routine platforms, context/foreground teardown, scheduled frames and complete
measurement/parameter/seed interfaces. Existing virtual contexts and Noop text do not establish
equivalence. The follow-up uses `297c4a4d78`, `1722fe63bc`, `8886dcb0d4`, `3135cdf8dc`,
`1f7eae7fb7` and `2440236055` against the same target, through current owners and actual public
Canvas/independent consumers. These findings are source evidence only. Complete benchmark/report,
conditional hot reload, numerical budgets, canonical docking U4-U9, real AT and foreign native/
browser execution, and Nara GPU/View adoption remain open. No Editor/game-runtime default or
full R11/R12 status changes.

## November 8: Threaded Execution And Queue Closure

The complete benchmark harness/report successor begins with its concrete dispatcher and
queue-closure prerequisite at Open-GPUI `1670c373e34751ac520237b7d04a480b06ab7b4a`, against
unchanged fixed Zed `635026fabc38440fce5e1767d125026a17b12012`. Official `3135cdf8dc` supplies
final-sender wake/receiver recheck and Linux timer-loop shutdown. The dispatcher uses
`8886dcb0d4` and the target's timer cancellation, readiness and frame-service follow-ups,
including `1722fe63bc`, `38df25d54c`, `82878540b5` and `1a89a92e7e`.
Local implementation is `018fbdc5f708d91f0ae16a7fb43ac04280ebb680`; its qualification record
is `916c7cb2d2fffcaa95199598655b5cef97fb9bda`.

The original blocked-receiver regression fails on the old queue: after final-sender release,
waiting receivers do not exit within its five-second timeout. The fix uses the official queue
lock synchronization before notifying all waiters and rechecks closure after waking. Queued
values and priority policy remain available through the existing owner. The Linux timer event
loop stops when its channel closes; a platform-specific regression is added for pending capture
release and worker/timer exit. Linux active-source compilation and execution remain unqualified.

`ThreadedDispatcher` implements the existing PlatformDispatcher boundary with real worker
concurrency, real-time timers and explicit main-thread pumping. It retains full official
ready-count bounds, completion checks between runnable polls, lock ordering, cancellation
outside the timer lock and process-lifetime timer-thread policy. The fork adaptation returns
the existing admission result, preserves thread-affine refusal handling and uses existing
scoped profiler guards. Background/timer polls do not consume foreground queue counts.
No second scheduler or engine-specific execution owner is introduced.

Current exposure uses test-support, with actual crate-external executor consumers for handoff,
idle/external submission, timers, cancellation and Rc/Task release on the creating thread.
The old virtual benchmark context is not advertised as equivalent to this concrete dispatcher.
The independent benchmark-feature split, real font/render injection, per-routine platform
isolation, complete context teardown, parameter/seed measurement and report consumers remain
in the complete successor.

Independent review corrects a cancellation-fixture race before execution: the timer thread can
legitimately acquire the lock before a destructor's one-shot probe. The corrected fixture uses
bounded acquisition, reports failure outside the destructor and retains rearm behavior. Panic
coverage proves the runnable was polled; a queued sentinel detects premature foreground Idle
after nested unwind. The first test build's untyped-channel error is corrected without production
changes. All three implementation and two review threads are closed.

Corrected focused owners/public pass 182/182. Complete all-feature Core/public/WGPU/Windows
passes 1,810/1,810, with four existing interactive skips and no isolated runtime retry.
UI core/components/docking/Canvas passes 2,966/2,966. The disabled affected cohort passes
35/35, including real public handoff/timer/cancellation/state-release consumers and zero-sized,
unregistered instrumentation controls. Independently locked ordinary dependencies pass 62/62
enabled and 47/47 disabled without test-support; their value/layout and explicitly type-only
checks retain separate scope from the actual new threaded consumers.

Locked workspace/consumer all-target checks, affected correctness Clippy, actual Windows
Rust 1.92 and stable production wasm with profiling off/on pass. The existing nightly
shared-memory configuration passes using Rust 1.98.0-nightly (`57d06900f`, May 27),
build-std and existing atomics/imported-memory flags. Root/independent formatting, existing
import/API/document/release scans and workflow YAML parsing pass. No dependency, lockfile,
compiler-floor or remote-publication change follows. Nara changes are documentary.

These gates qualify the scoped queue/dispatcher prerequisite. Linux/macOS native and browser/
test-support execution retain their own evidence requirements. Full benchmark/context/report,
real text/render injection, numerical budgets, full fixed-target refresh, conditional hot reload,
canonical docking U4-U9 and Nara GPU/View adoption remain open. No Editor/game-runtime default
or full R11/R12 status changes.

## November 8: Complete Benchmark Context And Measurements

Open-GPUI implementation `5f3a99fad9284c17a8eaac508ab8b4f47018be44` continues from
`916c7cb2d2fffcaa95199598655b5cef97fb9bda` against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Qualification is recorded at
`7c7a1411009a07afbcbd70340256adf55161491b`. It completes the benchmark owner mapped by
`297c4a4d78`, `1722fe63bc`, `1f7eae7fb7`, `2440236055` and their final-target follow-ups.
The independent Apache-2.0 measurement package comes from exact source blob
`1ec628b0e943e34ca6c0f87e5def8f86a63e967f`, with its license and attribution retained.

The virtual context and Criterion-forwarding macros are replaced by the complete official
real-time threaded context, separate routine platforms, native text-system reuse, scheduled
frames, explicit teardown, parameterized/seeded macros and primary/secondary reports. The
measurement crate has no GPUI dependency. Independent `bench-support` excludes deterministic
test support and entity leak tracking; both executable consumer graphs prove that exclusion.
Criterion 0.8.2 and the necessary `perf-event2` 0.7.4 closure are retained. Ordinary consumers
do not gain a benchmark dependency or an engine-specific document, history or ECS owner.

Headless factories are fallible and optional. Rendering without CPU readback is separate from
screenshots, through existing renderer/context/atlas owners. Windows keeps the official default
unavailable factory. An independently injected WGPU renderer executes actual source-driven
pixels, target reuse, refusal/recovery, reconstruction and exact renderer/atlas release; simulated
benchmark-platform Submitted counts alone are not GPU evidence. Metal adopts official fallible
intermediate allocations and caller propagation, with source-only regression scope on this host.

Independent review reproduces five inherited official edge defects before their corrections:
maximum-seed overflow, lost macro-wrapper visibility, undrained App-destructor tasks, a false
failure on the 100th successful timer cancellation, and input destruction before trace closure
during unwind. The fork separately adapts scheduled-frame delivery to ordered post-borrow ingress,
preserves accepted versus submitted candidates, and completes existing terminal-quit custody
without another fence. Actual regressions retain original output/lifetime assertions. These are
identified corrections to complete official mechanisms, rather than substitutions for them.

Focused context/public/renderer execution passes 60/60. The independently locked executable runs
nine actual benchmark cases, including native shaping, parameterized rendering, seeded tasks and
cross-module public registration. The maintained Canvas benchmark passes five smoke cases with
the original 200-by-100 grid and 39,900 indexed records. Complete all-feature runtime/public/
Gallery/native/metrics/macros passes 2,040/2,040. Five tests are skipped: four pre-existing
interactive Windows cases and the Gallery fixture-regeneration maintenance case. The complete
native trajectory requires no isolated retry. UI core/components/docking/Canvas passes 2,964/2,964
plus both separately gated Docking controls, preserving the previous 2,966-test set. Disabled
affected Core/public passes 112/112. Independently locked ordinary-dependency controls pass
62/62 with all features and 47/47 with defaults disabled, both without skips. Their value/layout
and explicitly type-only scope remains separate from the executed consumers above.

Locked workspace/independent all-target checks, affected correctness Clippy and actual Windows
Rust 1.92 all-target/all-feature compilation pass. No compiler-floor increase follows. Stable
production wasm passes with instrumentation off/on; the existing nightly shared-memory
configuration passes with build-std and checked-in atomics/imported-memory flags. Root/independent
formatting, import/API/document/release scans, workflow YAML/manifest parsing and whitespace
checks pass. Browser/test-support and foreign-native execution remain unqualified.

These dev-profile executions qualify functional measurement and lifecycle behavior. They do not
establish optimized numeric budgets, real device-loss behavior, Linux/macOS counters or foreign
native/browser execution. All ten implementation/test/review threads are closed, and no remote
publication occurs. Nara changes are documentary; no new engine runtime acceptance follows.

The remaining official `95cd535a5fad96d649513f96c5784ceefd379e47` mechanism separates test
support from leak tracking and identifies deterministic execution at runtime. It is now an
explicit complete successor before conditional hot reload; benchmark feature isolation does
not implement it. The hot-reload successor must include actual independent stateful
edit/build/apply/accepted-and-submitted output, beyond official websocket filtering and refresh
tests. Numerical budgets, full fixed-target refresh, canonical docking U4-U9 and Nara GPU/View
adoption remain open. No Editor/game-runtime default or full R11/R12/R11f status changes.

## November 8: Additive Test Support And Explicit Leak Tracking

The next Open-GPUI successor continues from `7c7a1411009a07afbcbd70340256adf55161491b`
against unchanged fixed Zed `635026fabc38440fce5e1767d125026a17b12012`. Local implementation
is `c6ffb32bd1499c9e8fb46783a3d1a4de626707a0`, with qualification
`da1b1fdecbd5bb57a17e8898650cf5592958085b`. Official
`95cd535a5fad96d649513f96c5784ceefd379e47` supplies the complete separation of compiled
test facilities, deterministic runtime identity and entity-handle instrumentation. Existing
App/entity-map, scheduler/executor, util and native renderer owners implement that separation;
no new engine scheduler, test framework or lifetime owner is introduced.

`test-support` no longer implies per-handle tracking. Explicit `leak-detection` or nonempty
build-time `GPUI_LEAK_DETECTION` selects it; nonempty `0` follows the official enabling rule.
Core's own unit tests remain tracked. Public snapshots are empty/no-op when tracking is absent,
while weak release assertions still refuse live strong handles and resources awaiting effect
cleanup. App, AsyncApp and executor `is_test` queries use their actual scheduler. A process-wide
test-scheduler hint cannot change a real-time benchmark or native App into a deterministic one.
Util's own tests keep a fixed home path; downstream programs retain the real home directory.
Metal screenshot access follows the official debug-only policy, source-reviewed on Windows.

Actual regression evidence precedes corrections. The old util feature changes a downstream home
directory and fails its new external test; complete corrected util passes 116/116. Two inherited
release-diagnostic edges are separately reproduced: a fully released weak handle panics after
the last owner disappears, and a caught release assertion consumes metadata, hiding still-live
handles from snapshot checks. The owning fixes retain the existing count/slot authority, borrow
diagnostics without consuming evidence, and preserve panic-safe destructor behavior. Focused
Core/scheduler/public execution passes 130/130 with all original entity tests retained.

A new independently locked test-support consumer executes public Test windows and asynchronous
entity updates, a returning Application and real Bench routines. The existing ordinary consumer
keeps its own no-test-support contract. The complete feature/environment matrix passes:
ordinary 2/2, benchmark-only 3/3, untracked Test/Bench 4/4, environment-selected Test 3/3,
nonempty-zero Test 3/3, explicit-feature Test 3/3 and all features 4/4. The corresponding root
public workflows pass in tracked and untracked modes. Real-time consumers assert actual worker
threads, timers and foreground work; these Noop text cases do not qualify native text or GPU.

The original full native View trajectory retains input, notified source updates, caches,
reconstruction, stale-callback refusal and exact entity/HWND/platform/OLE retirement. It adds
mixed Test/native runtime identity and exact-generation DX11 scene pixels. A reproduced default
backend capture failure is corrected by enabling its existing readback helpers for its own
unit tests, without enabling production capture. The complete trajectory passes with tracking
both off and on. Submitted receipts and same-scene readback remain distinct from compositor
scanout or actual OS input injection.

The initial complete framework/public/renderer/native/scheduler run passes 1,917/1,918 and
exposes the previously recorded native mouse-leave extra-event symptom. A bounded fixture
correction separates its exact three deliberately sent transitions from later OS messages and
finishes Core ingress before reading diagnostics. The first attempted snapshot move reproduces
five rather than six events; the completed correction retains the exact six-event, position,
physical-frame and teardown assertions. Both selected native scenarios pass, followed by the
complete Windows cohort at 224/224 with four original interactive skips. Combined with the
unchanged 1,694 non-Windows cases, this qualifies the selected 1,918-case union; it is not a
single all-green 1,918-test run. The initial extra pair's source remains unproven.

Three independent reviews close without remaining P1/P2 in their scope; all seven implementation
and review threads are closed. Complete components/docking/Canvas passes 2,966/2,966 without
skips, with explicit handle tracking and both consumer test-support features. The mouse-leave
fixture correction is separately committed at `2daf8412170540f7e2e586687bc25d5eadf4f2fa`.
The ordinary independently locked consumer passes 62/62 enabled and 47/47 disabled. Its lockfile
adds only the required unconditional backtrace dependency edge, with no version updates.
Default test commands without an explicit expectation variable pass public 6, independent
untracked 4 and environment-tracked 3 controls.

Locked workspace/both-consumer all-target checks, correctness Clippy, actual Windows Rust 1.92
all-target/all-feature checks, and environment-only tracking production compilation pass.
Stable production wasm with profiling off/on and the existing nightly shared-memory configuration
pass; browser/test-support execution remains unqualified. All three workspace formatting checks,
existing import/API/document/release scans, workflow/manifests and whitespace checks pass. Feature
trees confirm ordinary and benchmark-only graphs exclude Core test support and leak detection,
with build-time tracking explicitly disabled. No Nara runtime or Editor default changes.
Measured build-time/per-frame savings, foreign native/browser execution, conditional hot reload,
canonical docking U4-U9 and Nara GPU/View adoption remain open. Full R11/R12/R11f is unchanged.

## November 8: Conditional Native Hot Reload And Exact Terminal Settlement

The next maintained-framework unit starts from Open-GPUI
`da1b1fdecbd5bb57a17e8898650cf5592958085b` against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Local hot-reload implementation is
`ad70ac9230892ca4e4d47840b93215596b8c60e0`, with qualification record
`480430f02aecc3bdcf78bed8126ef6b871974770`. Official
`670a779a5ed9e7161c97aa4f998d395c207d5631` supplies optional native debug build selection,
per-App foreground/background task custody, bounded transport, build/process filtering,
foreground executable patching, window refresh and stored AnyView/common View HotFn boundaries.
The fork waits through its existing AsyncApp update owner while native callbacks hold an App
borrow, and refuses application after quitting. No engine reload manager or second render
scheduler is introduced. Ordinary builds keep direct render calls; non-debug and WebAssembly
builds exclude the runtime hot-reload cfg.

The independently locked native consumer uses public GPUI/Windows APIs, a separate panel
workspace member, a cached typed Entity, an AnyView constructed and stored at startup, and a
RenderOnce component with keyed state. Before editing, actual owner operations change constructor
counters from 37 to root 41 and children 43/47/53. A copied panel's compatible render-body edit
changes recipe values, labels and colors. At scale 1.5, actual accepted/nonempty/rendered/Submitted
generation 3 advances to generation 4 with the changed DX11 scene pixels while retaining process
nonce, window/entity identities, mutated counters and one initialization each. Normal retirement,
entity release, public shutdown, quit callback and process exit zero precede success. The final
single edit-to-observed result is 3,419.7 ms, including compilation, transport, redraw and sampling;
it is not a latency distribution, budget or OS compositor scanout result.

Three distinct source owners matter. GPUI's render/transport mechanism follows official Zed.
The published Windows x64 dx 0.7.10 executable has a separately reproduced code-generation defect:
its unresolved-function stub overwrites RAX before calling `__chkstk`. Actual stack/disassembly
evidence matches Dioxus issue 5966 and upstream fix
`14b4b767c8b661dffabe648bd07994e88ef57000`. The fixture carries only that register-transparent
jump backport against published CLI source, retaining the original failed attempt. Successful
execution uses an isolated identified tool with SHA-256
`4fcb34ce830ffbb0c79556b846d873448664f260e56d8b01bd553d39172c2ae1`; the installed developer
tool is unchanged. This is not an unchanged released dx success or a reason to remove GPUI's
official patch boundaries.

The third owner is the fork's existing native retirement protocol. A repeated terminal report
from a successful destruction result followed by queued Closed previously produced a false
missing-authority diagnostic, including without a hot patch. A separate red/green unit and
commit `28eebf0e8dcd0c8a71f1064bf9f66052b5d3a55c` distinguish Observed from Settled in
existing AppCell bookkeeping. Only successful exact-ticket acknowledgement marks Settled;
unknown and pre-quiescence reports still diagnose violations. Capture release, dependency
retirement and shutdown progression keep their existing owners. Three regressions and ordinary
parallel libtest pass after independent review. This defect is fork-owned, not official GPUI.

Ten focused transport/owner/render regressions pass. The same actual native executable passes
wrong-build, wrong-PID, invalid-DLL, full-restart and disconnect controls, retaining its original
pixels, state and exact generation through live checkpoints. The invalid DLL reports failure;
restart/disconnect remain explicit. All five controls, retirement/quit and exit zero pass the
final unchanged rerun without the false terminal diagnostic. The preceding attempt observed an
unexpected generation 3-to-5 refresh with unchanged state/pixels; its cause remains unproven.
The passing rerun does not explain it or establish immunity to arbitrary desktop events.

The enabled framework/public/Gallery run passes 1,823/1,823 with four original interactive skips,
and components/docking/Canvas passes 2,966/2,966 before the small terminal correction. After that
correction, affected lifecycle/Windows runs 313 cases with 310 passing. Two of its three native
failures pass an unchanged selected rerun. Foreground-restoration still fails while read-only
inspection identifies the user's Steam game as foreground. That observation supports a desktop
constraint without proving the source correct; the test and activation semantics are unchanged,
and its qualification remains open. These results are not one final all-green complete native
run. Final hot-reload-disabled View/cache/lifecycle passes 68/68 and the ordinary independent
consumer passes 62/62 enabled and 47/47 disabled.

Locked workspace all-target checks, affected correctness Clippy, actual Windows Rust 1.92
framework all-target/all-feature and native-consumer all-target checks pass. The root lock adds
seventeen required packages with every existing package version retained; the native consumer
resolves its own compatible dependency choices. Actual non-debug compiler/profile/build-script
evidence confirms hot-reload cfg exclusion. Stable production Core/WGPU/Web checks pass with
the feature requested and profiling off/on; those are compiler gates, not browser execution.
Four workspace formatting checks, existing import/API/document/release scans, YAML/Python syntax
and whitespace gates pass. All thirteen implementation/review threads are closed. No remote
publication or Nara runtime/default switch is performed.

Compatible panel render bodies now have an actual framework-level edit/build/apply proof.
Arbitrary type-layout migration, custom Element phase patching, framework crates outside the
consumer workspace, foreign native/browser execution and Nara's full Editor reconstruction/
versioned-state workflow still need independent evidence. Continue atlas/renderer lifetime,
remaining API/image-feature/platform reconciliation, numerical budgets, canonical docking U4-U9
and Nara GPU/View adoption. The full fixed-target refresh and R11/R12/R11f remain open.

## November 8: Shared Atlas Ownership And Retained Renderer Bindings

The next maintained-framework unit starts from Open-GPUI
`480430f02aecc3bdcf78bed8126ef6b871974770` against unchanged fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Local implementation is
`67e1360709ca48be74bf986f116b9c004ff5bd59`, with November 9 qualification record
`c3d31566ffd6cc55c6f2b98e5e33810812e27262`. Official
`0ef92145ff87aab5f19f6b44c9c55a486f4f064f` supplies owned AtlasKey insertion, one
Entry-based key/publication owner and the shared HeadlessAtlas. DirectX, Metal, WGPU,
TestWindow and Linux headless adopt that ownership. Official WGPU
`be8c6f9fb356dcd40a7ff06149568753e64ee171`,
`a434bb7ee275cefa348a2bfaf27dc30280bf38f7`,
`96e95edac45b0e0696179b8139aa521cf48db392` and
`e91b82c106817f2419207ebf81f1da766698ac95` supply instance preupload, separate texture
bindings, generation-aware caching, non-panicking resource lookup and shared window/headless
rendering core. These mechanisms replace duplicated ownership and per-batch combined bindings.

The fork's exact texture-instance leases, diagnostic facts and complete-frame submission rule
remain concrete adaptations. Missing resources require fresh paint or refusal; a renderer
cannot skip unavailable batches and publish a successful partial frame. DirectX and Metal
preflight the complete set and draw from retained native handles. WGPU consumes frozen views
and the existing exact instance as cache identity, with no second generation clock. Published
regions remain protected while leased. An inherited WGPU source-admission gap, also present
at the official target, now rejects malformed uploads before allocation and publication.

Complete Scene recording is separately committed at
`186deca5a46f9e5ed48614b3e631a051b305e96a`. Copies include clip arenas and paint journals;
the spatial tree clears its pointer scratch. They retain the original geometry-validity scope
without acquiring atlas leases or transferring accepted-frame/submission authority. Fourteen
owning regressions, actual Windows Rust 1.92 Core compilation and bounded independent source
review qualify this seam. It is drawing data, not a new Nara render-packet or GPU owner.

Actual GPU regression exposes a local integration defect before closure: a prepared view-only
receipt survives key removal but permits the official allocator to reuse its old pixel region.
The submitted red scene reads back blue after reinsertion. The receipt now holds the existing
complete-set texture lease alongside frozen views. Same-Queue submission orders later uploads;
the cache retains handles without becoming region-lease authority. Focused owner/renderer
20/20 and complete WGPU library/public 110/110 pass after the production correction. Additional
atomic/unsubmitted-Drop and submit-before-reuse controls pass in the final focused 21/21.
Complete components/docking/Canvas passes 2,966/2,966, and independently locked ordinary
consumers pass 63/63 with all features and 48/48 without optional features.

Earlier complete Core/public passes 1,516/1,516; ordinary independently locked disabled
consumption passes 48/48; DirectX upload/admission passes 14/14 and the corrected native asset
trajectory plus three WARP controls pass 4/4. The resumed complete framework/public/Gallery
selection runs 1,935 cases with 1,934 passing and four original interactive skips. Its sole
failure remains the previously recorded foreground-restoration test. That run precedes the
receipt correction and cannot qualify its final source. It also does not establish the current
desktop cause or one complete all-green native result. The initial final build stops before
execution with disk/PDB failure; the user frees disk space before the resumed run.

Final locked workspace and ordinary/test-support independent compilation, affected correctness
Clippy, actual Windows Rust 1.92 framework/ordinary-consumer checks, stable production wasm
profiling off/on, and the existing nightly shared-memory wasm configuration pass. Four formatting
checks and existing import/API/document/release gates pass. The ordinary fixture's final edits
are three formatting-only wraps; runtime code stays unchanged after the recorded acceptance.
The review threads are closed, and publication remains local.

Metal's thirteen new native tests remain unexecuted on macOS. Source review, Window-generated
WGPU scale-two clips, actual DX/WGPU readback and CPU/backend consumers have distinct scopes.
Foreign compilation/native execution, fractional/dynamic DPI, subpixel dual-source blending,
WGPU replacement-Device recovery and numeric cost distributions need their owning evidence.
Continue the separate image-feature policy and remaining API/platform reconciliation. Full refresh,
canonical docking and Nara GPU/View adoption remain open. No runtime/default ADR changes.

## November 9: Image Dependency Feature Policy

Official `8e06c323ae063f3d245ea5c283d4bf373ad51db2`, retained by fixed target
`635026fabc38440fce5e1767d125026a17b12012`, is implemented in Open-GPUI at
`b4355a36824f0709f1b73f7480ec88a884a489bc`. Qualification is recorded at
`aeeef7bcd377bd99cc527fcb718d7a3298b4633e`. The complete official dependency policy
disables image defaults while explicitly retaining fourteen formats and Rayon. The fork
preserves its existing 0.25.4 API minimum and resolved image 0.25.10.

The excluded `avif` feature supplies an encoder and ravif/rav1e. AVIF decoding separately
requires `avif-native` and dav1d, absent before and after this change. The official extension
inventory remains intact; TGA resource-format guessing retains its inherited limitation.
An ordinary independently locked application restores encoding through its own optional
image dependency and `avif-encoding = ["dep:image"]`. No GPUI codec-selection feature,
wrapper or engine owner is introduced. Source review confirms this official/fork boundary.

Actual Windows x64, Linux x64 and macOS arm64 dependency graphs remove exactly `avif`,
`default` and `default-formats`, adding no other image feature. The after graph includes the
entire workspace with all features; normal/build graphs omit ravif, rav1e and dav1d. The
ordinary consumer's default graph excludes encoding, while explicit opt-in and all-feature
graphs include it on all three targets. These are resolved graphs on Windows, without foreign
compilation or execution claims. Optional packages may remain locked without active build edges.

Final affected Core/public/native image consumers pass 25/25: twenty Core cases, four actual
public headless resource/animation/loading consumers and the complete native encoded/embedded
image trajectory. The latter preserves nonempty Submitted frames, framework pixel readback,
animation/replacement/reconstruction, atlas reuse and exact owner release. New resource controls
decode DDS, OpenEXR, Farbfeld, HDR and QOI through the awaited resource loader and compare all
BGRA pixels. A distinctly named header-only AVIF control checks the missing-decoder error
branch, without claiming valid AVIF content or decoding.

Independent ordinary image consumers pass 3/3 with encoding disabled and 4/4 with the
application-owned opt-in. The additional case actually encodes public GPUI-decoded WebP pixels
using one encoder thread and checks nonempty AVIF output. It does not establish an AVIF decode
round trip. Independent review closes without P1/P2 findings; both workers and the reviewer close.

Locked Windows Rust 1.95 workspace/ordinary-consumer all-target compilation, affected
Core/WGPU/Windows correctness Clippy and actual Rust 1.92.0 framework/ordinary-consumer
all-target/all-feature checks pass. Stable production Core/WGPU/Web wasm compilation also
passes. The independently locked smoke_web fixture exposes inherited stale layout,
segmentation, accessibility and runtime dependencies. Its separate lock refresh
`015863c085dbcd845658cfb97c289c3ac4433aad` preserves unrelated compatible selections and
passes its own locked Rust 1.95 production wasm check. That fixture is not qualified on
Rust 1.92 or in a browser. Formatting, workflow YAML, whitespace and existing import/API/
document/release scans pass.

The earlier complete suites are not repeated for this dependency-only policy. Their existing
foreground-restoration failure remains open; this scoped image success does not resolve it.
AVIF native decoding, foreign/browser execution and numerical cost distributions remain
unqualified. Upstream build measurements are not fork performance results. Continue remaining
renderer feature-policy, API and platform reconciliation before engine adaptation. Complete
fixed-target refresh, canonical docking, Nara GPU/View adoption and full R11/R12/R11f remain
open; no Nara execution ADR or UI default changes.

## November 9: WGPU Dependency, Shaders And WebGL

The complete renderer successor is implemented in Open-GPUI at
`38d3041acab90cc01cbc9984a8b738a8b2c3a194`, with qualification at
`7cd732eda7f8c5c80e0d08b737504f720f6fa6f5`. It follows official WebGL `4601ead416`,
dependency-feature cleanup `d544470d1d`, WGPU correction `402675e805`, reconfiguration
follow-up `24a2823456` and native configure-once `e20d19d32f`, against the unchanged fixed
Zed target `635026fabc38440fce5e1767d125026a17b12012`.

WGPU and matching Naga use exact upstream gfx-rs revision
`cddb2f5d80dc0009b2ef6767635140cacf90a007`. This contains Vulkan acquired-texture teardown
`3b3506ea31154edc04de64debda0e780cba2639c` and the wasm wait-timeout correction. The dependency
declares Rust 1.93; maintained framework/consumer manifests, CI and migration guidance now
state that floor. Actual locked Rust 1.93.0 Windows checks pass for selected framework/native
packages and the ordinary, test-support and hot-reload consumers, including the separate
reload panel. Earlier Rust 1.92 qualification retains its original revisions. Nara already
declares Rust 1.95, while its registry WGPU 30.0.1 and the maintained Git revision remain
distinct source/type identities for the eventual shared-device consumer.

The complete official storage/WebGL shader split and packed `Rgba32Uint` transport preserve
the fork's exact clip ancestry, primitive transforms, atlas leases and whole-frame refusal.
Auto initialization tries WebGPU, discards a failed detached canvas and prepares a fresh GL
canvas. Forced backend selection tries only that backend; software-adapter policy is also
forwarded exactly. Ordinary GL success cases explicitly permit a normal adapter, with a
separate software-only refusal control. Public constructors expose matching prepared
graphics/surface resources, and native context selection configures through the existing
surface owner. No second engine GPU, scheduler or window owner is introduced.

Actual browser qualification exposes a fork integration defect that native packed rendering
alone did not reveal. The official 32-byte surface uniform became 40 bytes when the fork added
its 24-byte exact clip envelope, violating GL's complete 16-byte binding requirement. The
private surface parameter record now reserves 48 bytes and WGSL reserves 32 bytes for that
nested clip member. Shared clip and primitive storage strides remain intact. Matching-Naga
reflection/validation, cross-row decoding and actual browser pixels qualify the correction.
The Open-GPUI `docs/verification.md#wgpu-dependency-shaders-and-webgl` record preserves the
failed trailing-padding attempt, nested-uniform correction and exact runtime scopes.

The independently locked ordinary Vulkan controls reproduce both old registry 30.0.1
semaphore assertion/cleanup aborts, then pass unchanged on the restored exact source. A third
ordinary `WgpuRenderer` trajectory initializes a real HWND surface with zero requested
drawable size, resumes to a nonempty submitted Quad, shares its exact device/queue with a
second surface and quiesces both shutdown tickets before HWND destruction. The HWND client
area remains nonzero; this qualifies the renderer request's deferred state. All three pass
on RTX 4090/Vulkan, driver 621.14, with bounded polls and error observation.

Final affected qualification passes:

- Complete WGPU library and all public integrations: 123/123.
- UI core/components/docking/Canvas: 2,966/2,966.
- Ordinary independently locked optional features enabled/disabled: 65/65 and 48/48; the
  three explicitly ignored Vulkan cases run separately above.
- Rebuilt browser policy/coordinate/reference controls: 14/14, followed by complete actual
  Edge 157.0.4341.42 WebGPU, GL and Auto-fallback trajectories.

Each of four browser rendering cases passes 22 fixed PNG samples at 800x600 and 960x720,
including the last of 16,384 Quads, nested asymmetric clips, nonuniform transforms, alpha,
image and path, plus visible text coverage. Input/focus/capture, docking, activation and
resize assertions remain intact. Forced WebGPU unavailability uses an explicit
`navigator.gpu` test control; forced rejection excludes other-backend attempts and verifies
candidate removal. Application callback counters remain distinct from GPU submission or
native presentation receipts. This browser scope is Windows/Edge at DPR 1.

Locked Rust 1.95 workspace/ordinary-consumer checks, native and production wasm correctness
Clippy, stable wasm profiling off/on, existing nightly shared-memory compilation, formatting,
workflow YAML and existing static gates pass. Resolved four-target graphs retain official
backend features, one WGPU source and the optional direct image edge. They establish no
foreign native execution; shared-memory compilation establishes no multithreaded browser
execution. Bounded independent reviews close their findings and all six threads close.

Continue remaining framework/API/platform reconciliation before engine adoption. Physical
device/context loss, general DPI, numeric cost distributions, existing native foreground
restoration and canonical docking remain open. Nara's actual shared-GPU/View consumer must
resolve dependency identity and existing Host/error/submission ownership. Complete Editor
and game-runtime selection and full R11/R12/R11f remain open.

## November 9: Frame-Time Debug Overlay And Remaining Core APIs

The maintained fork continues from `7cd732eda7f8c5c80e0d08b737504f720f6fa6f5` against
unchanged Zed `635026fabc38440fce5e1767d125026a17b12012`. Implementation
`73472ca230bac117f8cb417d0cd6b0c1364c980e` and qualification
`968f836f48a097ef49e692c62f5d1734c657bb75` complete the official frame-time overlay introduced
by `07a0bd1220`. The maintained fork's `docs/verification.md#frame-time-debug-overlay` owns
the actual runs, initial failures, corrections and scope.

All final-target Hidden/Minimal/Full recipes, 1,000-sample history, current/P99/P90/max readouts,
lifetime count, reset behavior, bitmap glyphs and six original official regressions are
preserved. Direct Scene painting uses the existing exact viewport clips and candidate owner,
outside cached view ranges. Completed accepted draws supply the existing profiler duration;
refusal, unwind, idle callbacks and old-scene presentation do not create samples. Hidden mode
retains official history. Explicit mode/reset controls refresh, while overlay painting and
recording introduce no font, layout dependency or self-scheduling loop.

The ordinary close/reconstruct consumer exposes a fork retirement-adapter gap: historical
TestWindow diagnostic state retains configured renderer/atlas resources after exact retirement.
Separate correction `d928c5459a8ed174d1a6835abd096b3e7a996b5a` releases them outside the
state lock and before exact-ticket acknowledgement, preserving blocked retries. The owning
regression reenters public App shutdown from renderer destruction before quiescence completes.
Official shared TestWindow ownership predates the fork; the missing release belongs to the
fork's added retirement boundary.

Final affected qualification passes:

| Consumer | Actual result and scope |
| --- | --- |
| Official and owning controls | 20/20, including all six original regressions, exact clips, full history, cached replay, refusal, unwind, idle and resource reentry. |
| Core/WGPU/Windows library and public integrations | 1,970/1,970; four original interactive skips and the separately executed new ignored overlay case. |
| Native overlay/profiler/frame signals/power/visibility | 5/5, including actual nonempty Submitted generations, both public GPU readbacks, mode/reset pixels, typed resize, input, focus and retirement. |
| UI core/components/docking/Canvas | 2,966/2,966 with existing interaction and retained-state assertions. |
| Ordinary independently locked optional features on/off | 67/67 and 48/48; three existing explicitly ignored Vulkan controls remain separate. Overlay values execute; Window signature checks are explicitly unexecuted type consumers. |
| Profiling-disabled Core/public integrations | 1,350/1,350; no overlay module/API/per-window state. |
| Independently locked additive test-support consumer | 4/4, including actual App/window cleanup and real benchmark contexts in this feature selection. |

The new native trajectory's failed Minimal pixel rerun exposes a fixture observation gap:
settling can advance beyond its last captured generation. Each control/reset/placement now
snapshots the current accepted generation and requires a newer actual Submitted frame.
All pixel and lifecycle assertions remain. Strict generation/readback assertions can still
expose extra intervening draws as failures. The earlier foreground-restoration case passes
unchanged in the complete run, without proving its historical variability resolved.

Locked Rust 1.95 workspace and both independent-consumer checks, affected correctness Clippy,
actual Windows Rust 1.93 framework/both-consumer all-target/all-feature checks, formatting and
existing static/YAML gates pass. Stable production wasm passes with profiling off/on; the
existing nightly shared-memory production configuration also compiles. This unit does not
execute overlay pixels in a browser or on foreign native platforms. All six implementation,
review and bounded-inventory threads close.

The bounded source inventory identifies three further official Core mechanisms:

- Animation controls/time: `d06ebb7c03`, `4ed3738c40`, `dd04a229dd` and `99e70ee4a0` supply
  reduced motion, synchronized repeats, per-animation FPS limits and executor time. The fork's
  wall-clock animation and qualified global power/cadence are not equivalent to this path.
- Stateful spring integration: `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` supplies typed
  targets/playback and Element-ID state through `with_spring`. Existing fork
  `MotionSpringPhysics`/`MotionScalarRun` need reconciliation through reusable owners.
- System notifications: `de827bce2ff1cbb40040bea0ef57c8ac56afd726` supplies Core values,
  App/Platform identity/show/withdraw/response and TestPlatform observation/simulation.
  Native system-event observers are a different consumer; actual OS delivery needs its own gates.

These are explicit queued source findings, not implemented behavior or an exhaustive residual
inventory. Continue animation scheduling before spring, with notifications as an independent
unit. Draw-duration readouts establish neither compositor latency nor numerical overhead/budgets.
Full fixed-target refresh, remaining platform/API closure, canonical docking, actual Nara
GPU/Host/View adoption and complete Editor/game-runtime selection keep R11/R12/R11f open.

## November 9: Generic Animation And Framework Maintenance Boundary

The successor to Open-GPUI `968f836f48a097ef49e692c62f5d1734c657bb75` reconciles the complete
official non-Spring animation mechanism against fixed Zed
`635026fabc38440fce5e1767d125026a17b12012`. Official `d06ebb7c03`, `4ed3738c40`,
`dd04a229dd` and `99e70ee4a0` supply App reduced motion, shared-phase repeats, per-animation
frame limits and executor time. ParentElement composition, chained static endpoints, image
policy and long-uptime modulo semantics remain part of that mechanism. Implementation
`4ed461f2b591d6e44d84abc742094740fbd607af` and qualification
`af682e37f36f8fc5de575ef485c1d247509a9d22` are local commits in the maintained development
branch. Open-GPUI's existing `docs/verification.md#generic-animation-controls-and-clock` owns
the full commands, red runs and source provenance.

The retained fork adaptations have concrete owners. Timer notifications wait through App borrows
and clear their pending flag inside the admitted closure before notifying. Invalid or
unrepresentable FPS intervals fall back to normal frame scheduling. A reproduced first-mount
refusal exposes a detached timer retaining rejected state and notifying an accepted replacement;
the timer now holds only a weak reference to its exact animation state. This preserves accepted
state and its original deadline through refusal without adding another scheduler.

| Qualification | Actual result and scope |
| --- | --- |
| Official/focused animation, image and candidate owners plus public workflows | 26/26, including all five official non-Spring tests. A first-mount refusal regression fails before the weak-state correction. |
| Strengthened final image observers | 3/3; HTTP completion must independently redraw the Img before passive observation can see its loaded Arc. |
| Core/WGPU/Windows/Gallery library and public integration cohort | 2,138/2,138 with seven declared skips. The historical foreground case passes unchanged without resolving its earlier variability. |
| UI core/components/docking/Canvas | 2,966/2,966 without skips, preserving the current domain-motion consumers. |
| Actual Windows animation trajectory | 1/1; real nonempty Submitted generations and exact-generation GPU pixels, input/focus, reduced-motion endpoints/restoration and pending-close retirement. |
| Ordinary independently locked profiler enabled/disabled | 68/68 and 52/52 without skips. Builder/value/parent tests execute; App/Window signature closures explicitly remain unexecuted. |
| Profiling-disabled Core library and public animation | 1,310/1,310 without skips, including final strengthened image and refusal controls. |

Locked Rust 1.95 workspace all-target and independent all-target/all-feature checks, affected
correctness Clippy and actual Windows Rust 1.93 framework/consumer checks pass. Stable production
wasm profiling off/on and the existing nightly shared-memory production configuration compile.
Formatting, import/API/document/release scans and workflow YAML parsing pass. All six current
implementation/review/analysis lanes close without remaining scoped P1/P2. No remote publication,
foreign-platform runtime, numerical FPS/overhead budget or complete refresh is qualified.

The owner's maintenance clarification keeps this continuation responsible for the fixed-target
refresh and modular engine-integration assessment before a separate Codex session maintains
Open-GPUI from concrete engine feedback. The existing
[R11a comparison](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#maintained-open-gpui-comparison)
owns the product acceptance. Each integration checkpoint records an immutable framework revision
and dependency graph; later framework changes require affected Nara consumer qualification.

| Owner | Maintained responsibility and evidence boundary |
| --- | --- |
| Open-GPUI | Complete official UI behavior, reusable fork additions, platform/render seams, ordinary standalone consumers, upstream/fork provenance and focused regressions for reusable engine-driven fixes. |
| Nara | Engine adapter and existing GPU/Host custody, document/ECS/history, complete editing and native View workflows, and the actual Editor/game-runtime choice. |
| Integration checkpoint | Matched dependency source/type identity, changing engine output beneath clipped/overlapping UI, exact input/presentation, resize/refusal and resource retirement, plus recorded measured costs and limits. |

The existing standalone/WGPU tests do not qualify a Nara engine View. Nara still selects registry
WGPU while the maintained renderer selects the fixed upstream Git source; matching version numbers
do not make their Device/Queue types identical. Evaluate the real shared-device/render-service
adapter through the existing owners, without steady-state CPU readback or a second GPU authority.
An actual consumer in Nara and an isolated reusable regression in Open-GPUI form the feedback
boundary; no cross-repository synchronization framework is required.

A bounded November 9 source check at the unchanged Open-GPUI renderer revision
`968f836f48a097ef49e692c62f5d1734c657bb75` makes that assessment concrete:

| Inspected owner | Current source fact | Required integration experiment |
| --- | --- | --- |
| `crates/gpui_wgpu/src/wgpu_context.rs` | Public Arc Device/Queue fields do not supply external-device construction. `from_adapter_and_device` is private, wraps owned values and installs device-wide loss/error callbacks. | Admit the engine's captured device requirements and error/loss observation without creating another device or replacing its owner's callbacks. Public fields alone do not establish this contract. |
| `crates/gpui_wgpu/src/wgpu_renderer.rs` | Private `record_frame` creates an encoder and calls Queue submit; ordinary draw also owns surface/presentation work. | Separate reusable scene recording from native acquire/submit/present so the engine's existing Host can account for the complete native prefix and physical retirement. Keep standalone GPUI drawing usable. |
| `crates/gpui_windows/src/window.rs` | WindowsWindowInner owns a concrete `RefCell<DirectXRenderer>` and constructs it from DirectXDevices. | Qualify a reusable renderer attachment/factory boundary through the same window/input/retirement owner; selecting the WGPU package does not currently replace this native renderer. |
| [Nara native composition](../guides/render-image-outputs.md#native-window-composition) | `with_device` exposes the original borrowed device scope without independent submission; `present_window` owns acquisition, submission and receipts. Nara already installs its original device-loss callback. | Reuse those authorities and exact retained-image displays. A shared Rust WGPU source is necessary but does not by itself integrate encoding, loss, input geometry or completion. |

These are source-confirmed adapter gaps and proposed experiments, not a new universal renderer
interface or executed engine integration. The existing renderer skips Surface primitives on its
WGPU path; the real engine-View consumer must also prove retained GPU-image sampling through its
selected concrete seam, with clipping, alpha/color and exact image/resource custody.

The bounded Spring successor analysis preserves the complete official typed targets, playback
and Element-ID lifecycle in GPUI. The existing dependency on renderer-neutral motion permits one
official pure Spring solver in that crate, with the same types exposed through GPUI, replacing
the fork's duplicate formulas. Splitter, keyed list-indicator, docking projection and
bring-into-view retain their own domain state while sampling the common mathematics. One
property has one live animation owner and all UI frame demand enters the existing Window.

This direction requires reconciling Open-GPUI ADR0018 and the applicable ADR0016/0017 decisions
before implementation. Constructor parameter order, settling thresholds, freeze-at-cancel versus
official reset-on-cancel and domain overshoot clamps require explicit migration; fork review
metadata is not measured physics. Existing component retarget paths do not universally preserve
velocity. App reduced motion is an adapter policy floor. This is source-backed successor design,
not an implemented Spring API or a game-simulation time/determinism contract.

## November 9: Stateful Spring And Shared Domain Sampling

The successor to maintained Open-GPUI `af682e37f36f8fc5de575ef485c1d247509a9d22` implements
the complete Spring mechanism at fixed Zed `635026fabc38440fce5e1767d125026a17b12012`.
Official introduction `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` and executor correction
`99e70ee4a02d8dcf1b72f487f81aaf0d3be74f76` retain typed/custom targets, interpolation,
playback, first-mount defaults and Element-ID continuity. The local implementation is
`ecbbfa361ae4489b2de4121c5f0ca8b2a1d7aebb`, with qualification record
`b14967342f2665c1c23d1c6a41d24108cf0fc62d`. The maintained framework's
`docs/verification.md#official-spring-and-shared-domain-sampling` owns exact red runs, corrected
source checkpoints and final qualification.

Open-GPUI ADR0018 now refines ADR0016/0017: the existing renderer-neutral motion crate owns one
pure solver, and GPUI reexports the exact same configuration/state types while owning the full
element API. The duplicate domain damping formulas and independent rest-speed threshold are
removed. `MotionSpringOptions` separates the official configuration and positional epsilon from
policy hints. Domain cancellation freezing a sample and normalized progress clipping remain
distinct from official Cancelled restoring the initial value and raw overshooting phases.

Executed regressions justify concrete fork adaptations:

- Common scaling of finite valid physical parameters underflows/overflows the official f32
  canonical intermediate. Three red tests expose infinity, Duration::MAX and NaN. Canonical
  conversion now uses f64 intermediates without clamping parameters or changing the remaining
  official formulas. The corrected motion suite passes 92/92, including all eight official math
  cases. That checkpoint retains f32 subsequent arithmetic; the later position correction is
  recorded separately below. Arbitrary-range safety and bitwise parity are not claimed.
- The fork's rejected candidate could retain a newly requested target or playback in mutable
  element state. Four red owner cases expose retarget, Completed, Cancelled and paint-unwind
  trajectory pollution. A Spring-local candidate sample now publishes through the existing
  accepted-frame transaction; the accepted retry uses elapsed time under the last accepted
  state. Frame demand commits with the sample. A static direct-draw regression preserves use
  outside a rendered view without requiring an animation callback.
- The independently locked consumer exposes a conservative settling-time boundary at a
  translated target: a nominal tolerance of 0.001 at -4 rounds the sampled position outside
  that tolerance. An owning red regression precedes an inward representable-endpoint envelope
  correction. Independent rereview reproduces a second cancellation boundary: target translation
  rounds before two opposing position contributions cancel. Fixed-target position accumulation
  now uses f64 before final f32 conversion. Propagator, decay formulas, velocity, ramp and
  `is_settled` retain their official paths. The complete final motion cohort passes 94/94;
  subsequent public/domain/native verification records the final arithmetic separately from
  earlier complete cohorts. Very small tolerances relative to target spacing may conservatively
  wait for a zero envelope; no arbitrary-range numerical or performance guarantee follows.

The focused Core/public cohort passes 50/50, preserving all six official element cases plus
typed targets, cache/identity/playback/reconstruction and exact CPU Scene/resource release.
The corrected domain-focused cohort passes 31/31. New fixture corrections account for Dock
content borders, constrained Splitter fractions/device-scale rounding and real prepaint
scheduling; production geometry and frame admission remain unchanged. App reduced motion is
an adapter floor for Splitter/list/docking/bring-into-view and finishes active motion without
rewriting domain preferences. Their existing wall-clock implementation has not become a shared
deterministic component clock.

The final actual Windows Spring trajectory passes 1/1 with newer nonempty Submitted generations,
matching source/paint observations and exact-generation GPU pixels. It covers velocity carry
and reversal, paused versus stopped state, playback endpoints, reduced-motion/restoration,
logical focus and native input callbacks, and active-close exact retirement/release. Bounded
fresh-epoch retries apply only to missed transient phases; failed source/generation/pixel or
lifecycle assertions never count as a successful retry. This is native correctness evidence,
not physical foreground, display latency or numeric cost qualification.

| Qualification | Actual result and source scope |
| --- | --- |
| Motion solver/domain contract and doctests | Final 94/94 and 3/3, preserving all eight official pure-math cases and both translated/cancelling-position regressions. |
| Complete framework/public/Gallery and UI core/components/docking/Canvas | 2,171/2,171 with eight declared skips, and 2,974/2,974 without skips, before the final duration/position arithmetic corrections. |
| Final affected public/element/candidate/domain paths | 43/43 after both numeric corrections, preserving all six official element cases and actual rejected/unwinding candidates. |
| Final actual Windows Spring | 1/1 after numeric corrections, retaining source/generation/pixel/input/playback and exact retirement/release assertions. |
| Ordinary independently locked consumers | Broader enabled/disabled selections pass 74/58 before the final position correction; final affected Spring selections pass 6/6 each with profiler on/off. Actual value/builder tests remain distinct from explicitly unexecuted signature closures. |
| Final profiling-disabled complete Core library and public Spring | 1,340/1,340 without skips, including the final arithmetic, generic-animation owners and CPU Scene/lifetime workflows. |

Locked Rust 1.95 workspace all-target and independently locked all-target/all-feature checks
pass. Expanded correctness Clippy includes motion, UI core, components and docking; one
preexisting reversed-range test literal is made explicit without changing its data or assertion,
then its owning test passes. Actual Windows Rust 1.93 framework/consumer checks, stable
production wasm profiling off/on, existing nightly shared-memory production compilation,
formatting, doctests and existing static/YAML gates pass. All thirteen implementation/review/
inventory lanes close without remaining scoped P1/P2. This introduces no dependency or
compiler-floor change and performs no remote publication or CI execution.

A separate bounded fixed-target source pass confirms notifications as the next complete unit
and records five additional successors in Open-GPUI's existing owner plan: gesture recognition
and axis locking, randomized element-tree/full-render equivalence, sendable foreground
submission, asynchronous clipboard/errors, and atomic credential insertion/Apple error
semantics. They remain source-confirmed queued work, not completed API parity. UI animation
samples do not change Nara simulation, document/history, input admission or GPU Host ownership.
The scoped Spring unit is complete. Full refresh, notifications and the recorded residual
inventory, measured costs and actual engine integration remain open. Open-GPUI maintenance
handoff still requires the qualified fixed-target baseline and a real Nara GPU/Host/View
consumer; no Nara runtime/default selection or full R11/R12/R11f completion follows.

## November 9: System Notifications And Framework Response Ownership

The successor to maintained Open-GPUI `b14967342f2665c1c23d1c6a41d24108cf0fc62d` imports
the complete notification mechanism from fixed Zed `635026fabc38440fce5e1767d125026a17b12012`,
introduced by `de827bce2ff1cbb40040bea0ef57c8ac56afd726`. Core values, App/Platform identity,
show/dismiss/response APIs, TestPlatform observations and the ordinary example accompany all
three native transports. The native modules are unchanged between introduction and fixed
target; final wiring and test gating use the target source.
The implementation is local Open-GPUI `26639a9715aff09ffbc61b92af45014728f02f94`, with
qualification `7d2c071b9550b93654a7ac2cc19a76f49a5b7a31`. Open-GPUI's existing
`docs/verification.md#system-notifications-and-ordered-app-responses` owns the detailed record.

Responses use the fork's existing non-coalescing ordered native App ingress, complete update
transaction and generation-aware handler checkout. Weak platform wrappers install only when a
user registers a handler, preserving lazy macOS initialization and authorization. Windows
response tasks, queued messages and WinRT resources retire through existing native finalization.
No Nara document/history/ECS owner, notification broker or second scheduler enters the framework.

The initial focused cohort passes 18/19; its terminal test incorrectly uses recoverable shutdown
instead of native quit. Correcting that fixture preserves the production lifecycle contract.
Independent review reproduces a new simulation-helper borrow failure before correction: running
the whole foreground queue inside an active App update can execute another App-borrowing task.
The helper now queues without pumping while App is borrowed. Corrected Core/public workflows
pass 12/12, retaining all four official cases and exact order, effects, reconstruction and release.

All eleven Windows notification controls pass. An actual Application/WindowsWindow consumer
routes synthetic body/action responses through the App wrapper and weak entity, observes notify
and deferred effects, and requires newer matching nonempty Submitted generations and exact GPU
pixels. Retirement and weak App/callback/native resource release remain exact. This is native
UI integration evidence; the notification activation itself is synthetic and proves neither
actual OS delivery nor a physical click. Backend and consumer reviews find no remaining scoped
P1/P2. All seven implementation/review lanes and the additional next-unit analysis lane are closed.

Linux retains caller/transport action-ID separation, no same-tag replacement and no-op dismiss.
macOS requires a real bundle and authorization; Web inherits unsupported best-effort no-op
behavior. Those foreign native execution and actual OS notification scopes remain unqualified.
Required notification dependencies add ten packages per root/ordinary/smoke lock graph without
changing existing versions. The complete affected Core/WGPU/Windows/Gallery library and public
integrations pass 2,194/2,194 with eight existing declared skips. Ordinary independently locked
consumers pass 77/77 with profiler enabled; the notification target passes 3/3 without optional
features. Core/public notification workflows also pass 12/12 without profiler. These focused
disabled-feature results do not establish a full disabled-framework run. This unit does not
rerun the earlier 2,974 component/docking/Canvas cohort because their production code is unchanged.

Locked Rust 1.95 workspace all-target and independent all-target/all-feature checks pass.
Expanded correctness Clippy and actual Windows Rust 1.93 framework/consumer checks pass.
Stable production wasm profiler off/on, the existing nightly shared-memory configuration and
the independently locked wasm smoke example compile successfully. Compilation does not supply
browser notification support, and Windows-hosted checks do not compile the target-specific
Linux/macOS bodies. Root and independent formatting pass after one assertion's formatting
correction; existing import/API/document/release checks and workflow YAML parsing pass.
The historical foreground case passes unchanged without establishing a repair of its earlier
variability. No remote publication or CI execution occurs.

The next selected complete official unit is sendable foreground submission with its Web Fetch
consumer, retaining existing scheduler/admission/profiler ownership. The framework's existing
plan records worker-to-main routing, streaming/backpressure/cancellation and their required
native/browser regressions; signatures or wasm compilation do not qualify those workflows.
Notification qualification does not complete Nara engine adoption, numeric budgets, fixed-target
refresh or the framework-maintenance handoff. Those still precede the later separate Open-GPUI
maintenance session; no Nara runtime/default or full R11/R12/R11f selection changes.

## November 9: Send Foreground, Streaming Fetch And Browser Queue Ownership

The successor starts from maintained Open-GPUI `7d2c071b9550b93654a7ac2cc19a76f49a5b7a31`
against fixed Zed `635026fabc38440fce5e1767d125026a17b12012`. Implementation
`ff4b3f4b95d48410c7c1d3820e2b2a98444aacab` and qualification
`f6f3d98d156cd2ada88253ff4e4606a0ee50664d` close this scoped unit. The separate official
std queue prerequisite is `72676731163c1da1534f7b30b984444924a3353e`. Official
`2c2f737ab2f3db793b7a313bc8e5307f1708d2e8` supplies SendLocalExecutor,
SendForegroundExecutor and the complete worker-to-main Fetch consumer. Handles retain the
original scheduler/dispatch destination and accept Send futures and Send outputs, including
Cell without a Sync requirement. The fork preserves originating counter/admission ownership
and unchanged !Send local-task checks. Scheduler retention neither retains App/window nor
promises that a stopped event loop resumes. Refused erased native runnables may leave their
Task unsettled under the existing contract; no synthetic completion is introduced.

The complete Fetch chain includes `b2131e9df8`, `45ae0572c4` and `b0048cde74`: main-thread
browser calls, response streaming, bounded eight-chunk backpressure and body-drop cancellation.
Request bodies remain buffered. Explicit credentials and client installation use the same
originating executor. Obsolete AssertSend and full-response buffering are removed. Dispatcher
lookup no longer stores a JS Window behind unsafe Send/Sync, and waitAsync capability is checked
before starting workers. Ordinary standalone and existing image consumers retain public APIs.

Actual browser execution exposes a missing official prerequisite,
`5c9efb75cf73fc56f6316455983e4f04e9330860`. Two unchanged 120-second Fetch runs time out in
the eight-worker stream after successful capability fallback. Spinning on acquisition does not
prevent parking_lot mutex unlock/notification from waiting on the browser main thread. The
complete official std Mutex/Condvar path replaces those primitives in existing queue/mailbox
owners, preserving priority selection, FIFO, fork last-sender closure and queued-count custody.
The same full HTTP workflow then passes. This is a concrete owning correction, not a new task
queue, reduced test volume or increased timeout.

Core/scheduler/macOS share exact async-task `015845a19fc31752c02367567d3b8900a406f9da`
directly, so downstreams do not depend on a root-only Cargo patch. Its fix uses the task header's
stored waker vtable; Zed `67b41bf97607f2dd86319b385040a1310cf24f45` selects that dependency.
The old registry debug identity test passes locally, so it supplies no local red reproduction.
The final worker source `0cf96c7708dfb97ccf3da50347e25edcf75d6937` has the same Rust code
as wasm_thread 0.3.3 and current object-form JS initialization. That separate source alignment
is not credited for the earlier timeout correction. Both dependencies retain Apache-2.0 OR MIT.

The final-source actual Edge HTTP workflow passes eight requests with 32,768 ordered chunks
each, worker and configured single-threaded control modes, six actual stream-cancel cases and
missing-waitAsync fallback tasks. The official queue stress separately passes eight and sixteen
workers, 524,288 items and 128 rounds each, exact-once delivery, idle/busy switching, forbidden
main-thread wait and asynchronous joins of every worker. A reviewed driver-cleanup defect is
corrected with owned-process-tree termination and awaited exit; Windows cleanup executes,
while POSIX cleanup has source/in-memory-control evidence only. Separate single-MAIN Trunk
entries avoid concurrent Cargo pipelines and preserve the original example.

Before the shared queue correction, the complete scheduler suite passes 31, focused Core
executor/profiler 162, public TestApp workflows 3 and actual Windows Send workflows 2. The
native fixture preserves real App effects, two exact new Submitted pixel generations,
cancellation, retirement and reconstructed refusal/Idle accounting. Two wrong fixture API
queries are corrected before passing, without widening production APIs.

| Final-source qualification | Actual result and limits |
| --- | --- |
| Complete scheduler/Core/WGPU/Windows/Gallery library and public cohort | 2,233/2,235 pass, with nine declared skips. Both failures refuse native window creation during pending/changed display topology, before their target interaction. |
| Unchanged failed cases and both native Send workflows | 4/4, explicitly including the ignored GPU trajectory and exact Submitted pixels. This does not erase the original aggregate failure or establish that native topology/focus variability is repaired. |
| Complete ordinary independently locked consumers | 83/83 with profiler and 67/67 without; the six actual Send consumers pass in both selections without GPUI test-support. |
| Complete disabled Core/public Send and affected UI core/components/docking/Canvas | 1,351/1,351 and 2,974/2,974. |
| Existing dependency-checker tests | 2/2 after separate `3de0388762d` matches its constant to the already declared Rust 1.93 floor. |
| Locked compilation and lint | Rust 1.95 workspace all-target and ordinary independent all-target/all-feature checks, expanded correctness Clippy, and actual Windows Rust 1.93 framework plus ordinary/test-support/hot-reload consumer checks pass. Hot-reload checks establish no new runtime qualification. |
| Production wasm | Stable profiling off/on, wasm correctness Clippy, independently locked stable smoke compilation and profiler-enabled nightly/shared-memory production compilation pass. |
| Actual stable Edge smoke | WebGL, WebGPU and Auto execute input/capture/docking/activation, all 22 fixed PNG samples, high-index quads, alpha and text at 800x600 and 960x720, DPR 1. Explicit software-adapter refusal and controlled Auto fallback pass. App counters are not submission receipts. |

An initial independent compilation stops with disk-full error 112 before execution. After capacity
recovers the complete consumer/compiler matrix above passes. Later cache-path loss interrupts
Trunk packaging with Windows error 3 before stable-browser execution; rebuilding the existing
tool and rerunning the unchanged smoke passes. Existing lock packages keep their versions and
metadata; only the recorded task/worker sources and Web channel edge change. Root, independent
and Web-fixture formatting, both Node syntax checks, workflow YAML and existing import/API/doc/
release checks pass. No remote publication or CI execution occurs.

All ten implementation/review/analysis threads, both HTTP/queue fixture servers and the stable
smoke browser/server close. The HTTP single-threaded controls explicitly disable workers within
the shared-memory fixture, distinct from the stable smoke build. Foreign-native/POSIX execution,
physical input/scanout, actual device loss and numerical costs remain unqualified. Open-GPUI's
existing owners retain exact runs, source/fork differences and the next complete randomized-tree/
platform-tree workload. Full fixed-target closure and the real Nara GPU/Host/View consumer still
precede the separate framework-maintenance session. Nara runtime/defaults and full R11/R12/R11f
status remain unchanged.

## November 10: Randomized Tree And Standalone Workload Qualification

The implementation is Open-GPUI `b42295d0e535e4105470e80873a6ad7832e81863`, with final
qualification at `770463485ac9aaf4150475fa84fe90c33fbd4507`. Both are local commits; no remote
publication or CI execution occurs in this unit.

The successor starts from maintained Open-GPUI
`f6f3d98d156cd2ada88253ff4e4606a0ee50664d` against unchanged Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Official randomized-tree introduction
`3135cdf8dc8e6800d2b7ad89d62ea4994d892fb6` and complete cached/hover/animation successor
`26abd1c9590827ffc5cca3058ff2a94ea9a34715` supply all sixteen mutations, stable identities,
nearest-entity notification, weak ownership, six original regressions, seventeen platform
tree workload groups and five families. The missing official BoxShadow builders are restored.
The model's two visibility declarations bind to the existing Accepted SubtreePresentation
contract, retaining layout and cross-channel suppression instead of restoring the retired
late-paint-only API. This is an explicit fork adaptation with regression evidence.

Complete cached-mixed execution reproduces a fork scheduling feedback loop: synchronous App
borrow release has already drained frame ingress, but its obsolete foreground wake triggers
another benchmark frame and another wake. The independently reviewed correction is committed
separately at `1756e8f79aa298900acac5c9f4b67ce915e90a85`. Successful tracked App borrows and
active post-borrow convergence retain ordinary wake custody; dedicated unwind and bounded
continuation paths retain their owners. A finite eight-frame animation control fails before
the correction and passes afterward, without limiting polls or removing workload animation.

The corrected focused Core/benchmark/ingress/presentation cohort passes 181/181
(`3bad5b99-e03a-4112-9294-283ce6c1624b`). Complete Core/scheduler/WGPU/platform/Gallery library
and public consumers pass 2,014/2,014, with one declared skip
(`854d63ce-cea0-4231-b967-888e22b38ce6`). Dev-profile platform smoke with the existing stacker
feature passes 115/115 cases; the independently locked ordinary benchmark passes 64/64,
including both declared seeds, every ordinary/cached mutation, strict accepted-generation
advancement, complete Scene equality against plain/entity-preserving fresh references, restored
visibility and exact release. Reference windows must return activation to the persistent
consumer so legitimate inactive throttling does not leave its restoration pending.

The new rejection regression also reproduces shared mouse-state loss during a refused Hidden
candidate. Div interaction membership now uses the existing accepted-frame publication/discard
owner, preserving the previously presented press until an admitted frame withdraws it.
Independent public property tests exercise the official proptest 1.12 reexport and corrected
path expression; the prior 1.10 pin does not support that argument. Detailed initial failures,
fixture corrections and source differences remain in Open-GPUI's existing verification record.

The unchanged optimized release/non-stacker platform smoke also passes 115/115, including the
same seed-0 tall family that overflows dev without stacker. This qualifies those sampled inputs,
not arbitrary-depth capacity or a numerical budget. The final original eight-node/Wide native
tree and generic-animation trajectories pass 2/2 (`ed6710a1-5507-4b18-b5af-88c4ccb7d2ca`). The
root pixel point is selected from actual Scene geometry before readback, accounting for the
transparent root border and native raster snapping. Accepted/Submitted/nonempty/capture
generation, draw-time cache allowance, replay versus RootColor invalidation, public-tree-only
RootVisibility, sibling-button clicks and complete entity/native/App/platform retirement execute.
These are submitted-generation Scene GPU pixels, not physical input or swapchain scanout.

The first complete Windows cohort passes 241/242 with nine declared skips; an unchanged focused
rerun reproduces the Composite Focus passive-check failure. Refresh-callsite diagnostics identify
incidental native MouseMove changing input modality after synthetic keys. The keyboard-only
fixture now uses existing `accepts_pointer_input: false`, preserving all idle/semantic/key/focus/
lifetime assertions, and passes 1/1. That isolated correction is committed at `515e4d77c16`;
the failed aggregate retains its original result. A subsequent profiler aggregate exposes its
unisolated input-free redraw phase. After the existing input/trace assertions, public pointer
withdrawal now waits for exact observed facts before taking that baseline; this separate fixture
correction is `36e2567b16c`. The final complete Windows cohort passes 242/242 with nine declared
skips (`35169956-88b5-4dd5-8e9f-db02626e93b2`), retaining both original failed aggregates.
Components/docking/Canvas pass 2,974/2,974. Ordinary independently locked enabled/disabled
consumers pass 91/67, additive property/API consumers 6 and disabled Core/public 1,383 pass.

Locked Rust 1.95 workspace and ordinary independent all-target checks, expanded correctness
Clippy and actual Windows Rust 1.93 framework/ordinary/test-support/hot-reload checks pass.
The final independent benchmark also observes the exact Activated ticket after reference close,
passes all 64 cases again and passes the corresponding Rust 1.93 check. Production wasm
profiling off/on and existing profiler-enabled nightly/shared-memory compilation pass. The first
profiling-on command selects a nonexistent Web feature and is rejected before compilation;
the corrected command selects Core's profiler feature. This requires no source or lock change.
Bench-only feature graphs enable neither Core test-support nor leak-detection; only explicit
dev smoke enables stacker. Formatting, workflow YAML, whitespace and existing rebuilt xtask
import/API/document/release gates pass. All eighteen implementation/review/next-unit analysis
threads close, with no remaining scoped P1/P2 or owned helper process.

The next source-mapped unit is complete official asynchronous clipboard/Web paste, including
error classification, current input-handler admission and actual browser/Windows consumers.
Windows embedded-NUL handling and macOS pasteboard ownership retain separate missing
dispositions. This source map adds no implementation qualification. The full fixed-target refresh, remaining platform
workloads, canonical docking, numeric budgets and real Nara GPU/Host/View consumer remain open.
The separate maintenance session still follows qualified framework and engine-seam evidence;
Nara runtime/defaults and full R11/R12/R11f status do not change.

## November 10: Asynchronous Clipboard And Live Input Delivery

Implementation is Open-GPUI `5209f4e82c7cef3c095807fcf13cd16fe860b5c6`, with final
qualification at `7c6b74021cdebec7e03dd558240789797bfad344`. Both are local commits; no remote
publication or CI execution occurs.

This successor starts from maintained Open-GPUI
`770463485ac9aaf4150475fa84fe90c33fbd4507`, against unchanged Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Official
`d70c45e5bb1d8b7b3926fba5a6d1c69c23bbc405` supplies owned asynchronous reads, distinct errors
and complete-item paste. `084b5d21f67cdbe06edd2822ce34e58b3cd774f7` and
`5d447d4223f8455db4811ed332c4ad744bb13e40` supply browser text writing, DOM paste and
consumed-only key-default suppression. Existing listener retirement already has the required
callback-removal owner.

The official mechanism keeps synchronous Platform/App operations and adds an owned Task with a
ready desktop snapshot. AsyncApp obtains that Task through its existing update boundary, then
releases the App borrow before awaiting. The existing text input hooks receive the full
ClipboardItem; default handlers project its text, while custom handlers retain images, metadata
and entry order. No engine clipboard service, UI-specific document authority or additional
task scheduler is introduced.

Fork adaptation binds these hooks to the existing admitted handler slot, exact generation,
native input transaction and accepted-candidate lifecycle. Browser reads start synchronously in
the triggering stack. DOM text paste remains synchronous; image Files are captured before the
event returns, read asynchronously and delivered through the currently focused live handler.
Real component actions recheck editing, focus and rendered membership at completion. Default
Web paste bindings yield to the browser DOM event, keeping explicit menu actions available.

The focused Core/public/component cohort passes 34/34, and actual Windows clipboard plus
pending-input workflows pass 2/2. The native consumer checks a Unicode/metadata snapshot against
a later write, registered text delivery, accepted nonempty Submitted generations, reconstruction
and exact HWND/entity/TextSystem release. The complete browser gate passes real text/PNG reads,
trusted Ctrl-V, actual read-permission denial, text writing, cancellation and delivery-time
replacement/retirement. Fixed handler identities, accepted-frame fences and independently checked
PNG byte digests prevent stale or empty deliveries from passing. Explicit Promise/Blob controls
retain their narrower scope; GPUI task cancellation is not browser-request abortion.

Complete framework/public/Gallery passes 2,028/2,028 with one existing skip, affected
components/docking/Canvas 2,985/2,985, and complete Windows 243/243 with nine existing skips.
Ordinary independently locked enabled/disabled consumers pass 96/69; disabled Core/public
clipboard passes 1,379/1,379. Locked Rust 1.95 workspace/independent checks, correctness Clippy,
actual Windows Rust 1.93 framework/consumer checks, production wasm profiler off/on and existing
nightly shared-memory compilation pass. Root/ordinary/browser formatting and existing import,
API, document, release and YAML gates pass. All nine threads and owned helper processes close.
Open-GPUI's existing `docs/verification.md` section
`Asynchronous Clipboard And Web Paste` retains the actual fixture failures and corrections.
Windows embedded-NUL normalization and macOS typed Retained/owned-NSData pasteboard migration
remain separate source-mapped successors. Neither this prerequisite nor browser rendering
selects Nara's Editor/game UI, resolves shared-device source identity or qualifies the real
engine GPU/Host/View seam. The fixed-target closure and engine consumer still precede the
separate framework maintenance session; full R11/R12/R11f remains open.

## November 10: Desktop Clipboard Ownership And Platform Scope

Windows implementation is `a7e47b311aa6a721c5dd0991c36893694db13043`, Mac source preparation is
`fe5bb458f2ef23c0def7f8e19aa2327e404504b4`, and the Open-GPUI scope/verification record is
`41bb30ab4f915dc996b93ecb13a6a9cb1553ec68`. All three are local commits.

This successor starts from Open-GPUI `7c6b74021cdebec7e03dd558240789797bfad344`, against unchanged
Zed `635026fabc38440fce5e1767d125026a17b12012`. Windows
`decbf641b18f1982b3475c037e7c5c554471574f` normalizes embedded NULs before both UTF-16 encoding
and metadata hashing. macOS `e83f1c949512bab023f2f82446ffb4b0e4173fef` includes the earlier
`914e1c9873b6b85bfeedc5b58e8270885bdd532e` retention fix and supplies typed Retained pasteboard,
NSString/UTType custody, Rust-owned NSData bytes and validated property-list types. The current
Rust ClipboardItem boundary supports that complete backend migration independently of the
window/rendering adapter. Only pasteboard-exclusive raw compatibility APIs are removed; window
drag/drop retains its existing owner. Fork private format namespaces remain explicit.

The original Windows NUL/metadata case fails 0/1 before the fix: the text truncates and metadata
is lost. Corrected focused/native tests pass 3/3; complete Windows passes 244/244 with nine
existing skips, public clipboard 7/7 and the independently locked ordinary consumer 5/5 pass.
The actual Windows trajectory retains original and normalized items separately, then checks
async snapshot, registered text delivery, accepted nonempty Submitted generations, reconstruction
and exact retirement. External text replacement retains old private formats while stale metadata
is refused. No common text-model normalization or new engine clipboard service is introduced.

Mac source preserves all six official tests and adds six ownership/content controls, including
creation/read autorelease-pool exit, retained Rust bytes, embedded NUL/Unicode, all UTTypes and
invalid metadata/property lists. Independent source review finds no confirmed defect. Actual
Windows-to-aarch64-Darwin compilation stops before this module at the existing media backend's
missing generated bindings. Native Mac compilation and all twelve tests remain unexecuted;
Windows host checks do not qualify cfg-excluded code. Three maintained locks add only the
required existing bitflags edge, without dependency-version changes. Locked workspace/ordinary
consumer checks, Windows-host correctness Clippy, actual Windows Rust 1.93 backend/consumer,
independent browser-consumer wasm, formatting and existing import/API/doc/release checks pass.
These host results do not qualify Mac code. All three implementation/review/mapping threads close;
no owned Cargo/native helper remains. Nara changes only these evidence documents.

The next credential source map distinguishes macOS system add-only insertion from unsupported
framework platforms and the separate application-provider fallback. It does not add a universal
credential service. Full fixed-target closure, actual engine GPU/Host/View integration, numeric
budgets, canonical docking and R11/R12/R11f remain open before maintenance handoff.

## November 10: Credential Contracts And Maintenance Boundary

Credential implementation is Open-GPUI `3f943ea373f4da87a4c8b0f7d5a84e7101874824`, with
qualification and remaining-platform mapping at `6613f6b0075c7df818518687f06128c7b19b1619`.
The fork's clipboard fixture correction is separately committed at
`510cfc5613dd02d38d30f2497587cf6abb2995d9`. These are local commits; no remote publication or
CI execution occurs. Fixed Zed target remains `635026fabc38440fce5e1767d125026a17b12012`.

Official Apple extraction `95d35c6efddfa6dc34f99bb1873620bd74af806a`, cancellation
`abd9b3cbd2b11292358d740b9c6ab3eae7d1ab36` and add-only
`79fe6a645f68f8c9230578f154bf99852d9f716f` form the complete Keychain source owner. The public
Mac package exposes independently nameable operation/status errors and validated retained
responses. Only the macOS framework API offers add-only insertion: one SecItemAdd returns true
on creation or false for an existing server/account without replacing its secret. Other Mac
Platform implementations inherit an unsupported error. Cancellation is not absence, and the
application-level read-then-write fallback remains outside the extracted framework.

Windows follows `8720fd1d093963be0aeb36954c15fd1aca1b94d2` and the credential portion of
`a0f8fa7bdbbf0269aaf5d5ebfac3ad9adfb576dd`: existing background execution, optional native fields
and freeing native storage before error propagation. Fork namespaces, missing-item HRESULT
handling and secret-free size validation remain. Linux selects the exact MIT oo7 Git revision
`731c1215edb6905876c6137ab300428c7b8d7c13`, whose sole source correction maps a prompt stream
ending before Completed to UnexpectedEof instead of a panic. The three maintained lock graphs
change only this credential dependency closure; no unrelated package or compiler upgrade follows.

The safe native thread regression fails 0/1 before the Windows fix. Corrected focused tests
pass 10/10, final complete Windows passes 255/255 with nine existing skips, public enabled/
disabled credentials pass 2/2 each and the independently locked ordinary consumer passes 2/2.
Actual native storage, updates, missing reads/deletes and background thread IDs execute. Six
status publications reach exact accepted nonempty Submitted generations with GPU text readback,
followed by suspended-task cancellation and exact HWND/root/TextSystem/platform retirement.
Ready stub consumers qualify owned parameters and App lifetime, not native storage.

Two earlier complete runs fail in existing clipboard assertions after 252/253 passes. The
fixture correction retains the complete normalization matrix and every public/native value,
snapshot, restoration and Submitted assertion. A cfg(test)-only observer permits a bounded
wait only for actual OpenClipboard acquisition failure; successful-open data failures return
immediately to the assertions. Final focused clipboard passes 5/5, including real hidden-HWND
contention and an observed native acquisition refusal. The failed initial NULL-window control
and unknown external holder remain in Open-GPUI's existing verification record. Production
clipboard admission is unchanged.

Locked Rust 1.95 workspace/ordinary checks, correctness Clippy, actual Windows Rust 1.93
backend/ordinary checks, independently locked smoke-consumer wasm, formatting/YAML and existing
import/API/document/release gates pass. Eight research/implementation/review threads close.
Apple compilation, the eight Keychain tests, actual storage/cancellation/competing insertion
and Linux Secret Service termination remain unexecuted platform gates. Windows host checks
cannot qualify their excluded code; the prior foreign-host media-binding failure still applies.

The bounded public-platform audit adds five explicit missing dispositions: runtime OS identity/
secondary keybindings, live windowing-mode switching, scoped idle-sleep prevention, restart
arguments and App-wide native appearance override. Runtime identity is next because Web host
detection currently does not reach Core's compile-target-based secondary parsing. Port its
complete parsing/mapping/display/context/dispatch and fork command/component consumers; adding
an enum alone would preserve the broken path. This list is not an exhaustive backend audit.

The owner's November 10 clarification reaffirms the existing handoff order: this continuation
finishes the fixed refresh and qualifies the first real modular Nara GPU/Host/View consumer;
only then does a separate Codex session maintain Open-GPUI against concrete engine feedback.
Each engine adoption records an immutable framework revision and resolved dependency graph.
Reusable framework corrections need Open-GPUI regressions, while Nara owns its adapter,
documents, ECS, history and Editor/game-runtime choices. This evidence update selects no new
Nara runtime/default or second rendering/service owner. Full R11/R12/R11f remains open.

## November 10: Runtime Host Identity And Keybinding Consumers

Open-GPUI implementation is `70b993c9660772b157546f46c4a65c0ebd4375e6`, with qualification
at `7ead5105e56b29255ece592e62fd5f2250542131`, following credential checkpoint
`6613f6b0075c7df818518687f06128c7b19b1619`. The complete official mechanism comes from
[`389c22e1c747d1a433c3b51c176936166c9cc1cf`](https://github.com/zed-industries/zed/commit/389c22e1c747d1a433c3b51c176936166c9cc1cf)
under unchanged fixed target `635026fabc38440fce5e1767d125026a17b12012`. These are local
framework commits; no remote publication or hosted CI execution occurs.

PlatformInfo separates the host OperatingSystem from Native/Web delivery. Backend reports,
App access, default contexts, modifier interpretation, shortcut labels and actual binding
dispatch use those facts. Static Keystroke::parse and KeyBinding::new require explicit
modifiers; dynamic secondary input uses parse_with_os or KeyBinding::load with the owning
App and real keyboard mapper. Display/unparse take an explicit OS. Only Mac resolves
secondary to Command; Mac/iOS Option-input detection retains its distinct rule. WebPlatform
captures its host report once rather than changing conventions on each navigator query.

Fork command installation, patch preview, palette preflight, capture and tooltip consumers
migrate around that official mechanism. Binding-syntax string preflight uses the installation
App's mapper; already-normalized backend event buffers are not mapped again. DevTools prepares
binding projections on the App thread and publishes owned snapshot DTOs through its existing
thread-safe sampler. The same registered probe and long-lived Gallery workbench read updated
facts. Diagnostic samples explicitly distinguish projected bindings from installed keymaps.
No engine-private identity, command router, input queue or sampler owner is introduced.

Three inherited debug-source interaction problems retain separate fork provenance: callbacks
previously required secondary at paint time, modifier invalidation followed the focused path,
and movement across text bounds inside one hitbox left stale underline state. Actual failing
input/Scene oracles precede corrections through existing Window input normalization and View
invalidation. Exact incoming modifiers, projected label bounds and accepted hitbox admission
still control clicks. Initial invalid hitbox/scale oracles, ordinary asynchronous activation,
retired input-slot observation, stale DTO fixtures and a wrong CDP Meta modifier bit are
recorded as fixture/driver corrections rather than production-framework regressions.

Final focused Core/public passes 92/92. Complete Core/WGPU/Windows libraries and public
integrations pass 2,130/2,130 with ten declared aggregate skips; the new native trajectory
passes separately. Command/components/DevTools/Gallery pass 1,230/1,230 with only fixture
regeneration skipped, and UI core/docking/Canvas pass 2,077/2,077. Ordinary all-feature,
profiler-only and feature-free consumers pass 102/88/72, with three skips only in the
all-feature selection; the actual host App/activation/mapper selection passes 4/4. Disabled
profiler Core/public passes 1,393/1,393. Original failures and final run identities remain
in Open-GPUI's existing verification record.

The actual Windows workflow checks registered native input, WM_CHAR, shaped source and exact
window/handler/platform retirement. A real prefix timeout dispatches once; another timer
starts before retirement and cannot affect a successor beyond its deadline. The accepted scene
has a matching nonempty Submitted record. capture_frame re-renders that scene for GPU readback;
it does not sample an earlier swapchain buffer. Row ink and marker checks do not establish exact
glyph images, physical keyboard translation, live IME or compositor scanout.

The complete Edge browser matrix executes original Windows and ten controlled navigator
samples, trusted single/chord/explicit modifier input for original/Mac/Windows/Linux cases,
iPhone/iPad and five throwing-getter controls. Every sample forces a fresh App read after
navigator mutation and retains cached identity. Init scripts and descriptors restore before
existing clipboard permission/text/image/focus/retirement cases pass. Existing WebGL/auto/
WebGPU renderer/input/docking/activation/resize checks pass, including 22/22 screenshot samples
at both viewport sizes, explicit software refusal and controlled auto fallback. Injected browser
host reports do not qualify corresponding native OS backends.

Locked Rust 1.95 workspace/ordinary checks, expanded correctness Clippy, actual Windows Rust
1.93 framework and three independent-consumer checks, production wasm profiler off/on,
formatting/YAML and existing static gates pass. No production dependency or compiler requirement
changes; the ordinary consumer adds only the existing Windows command dev-dependency edge.
All thirteen implementation/review/successor-research threads close.

Continue official restart arguments with exact native argv and dedicated worker/recorder proof,
scoped idle-sleep guards, native appearance, live windowing and the remaining fixed-target
dispositions. Mac/Linux/FreeBSD native execution, numeric budgets and canonical-docking U4-U9
retain their own gates. This unit supplies no captured-device constructor, Host-owned renderer
submission, native renderer replacement or clipped engine texture View. The real Nara
GPU/Host/View consumer remains required before handing ongoing Open-GPUI maintenance to its
separate session. Nara keeps its document/ECS/history/GPU and Editor/game-runtime choices;
each later adoption qualifies an immutable framework revision through affected engine workflows.
No Nara runtime/default switch or full R11/R12/R11f completion follows.

## November 10: Modular GPU And Host Source Map

This bounded source map uses Nara `35badad64539ab4220b2a29d83c3b501e0e2f0b3` and Open-GPUI
`7ead5105e56b29255ece592e62fd5f2250542131`; the separately qualified restart successor leaves
the examined WGPU renderer owners unchanged. It identifies prerequisites for the first real engine
consumer after fixed-target closure. It is not an implementation, renderer choice or performance
qualification.

Nara already owns the required physical authority. In
[WgpuRenderHost](../../crates/nara_render_wgpu/src/host.rs), request_device_requirements runs
before device acquisition; with_device exposes an observed device scope without transferring
submission ownership. create_producer, reserve/activate targets and submit produce an immutable
[WgpuImageOutput](../../crates/nara_render_wgpu/src/output.rs). Retaining an output preserves
that exact submission's pixels across later renders, resize and producer retirement.
with_image borrows its actual native texture; submit_image_use keeps the output until the
original submission prefix completes. begin_window_frame/display_image/present_window and
WgpuWindowFrame::encode provide the existing window encoder/target and final submission owner.
Only actual unclipped sampling permits record_image_sample and a matching composite receipt.
retire_producer returns its frozen
[WgpuSubmissionPrefix](../../crates/nara_render_wgpu/src/submission.rs), allowing other producers
to continue. A generic device-drain helper does not replace that producer-local retirement.

The first consumer should use the real content and observations already demonstrated by
[content_window](../../tests/fixtures/render-composition-consumer/examples/content_window.rs),
[editor_scene](../../tests/fixtures/stock-composition-consumer/examples/editor_scene.rs) and
[streams_window](../../tests/fixtures/render-composition-consumer/examples/streams_window.rs).
The [output consumer](../../tests/fixtures/render-output-consumer/tests/offscreen.rs) covers
retained old pixels, resize, producer-local retirement and refusal/retry. Editor Scene preview
uses take_scene_preview_candidate and
[EditorPreviewPresentation::accept/finish](../../src/project_host/runtime/editor/presentation.rs),
binding actual presentation and DisplayMapping2d before picking. Game View retains its candidate
and one-shot UI sidecar through submit_for_display and the exact composite occurrence before
completing input/presentation. The underlying EditorProjectSession input method is public, but
EditorApplication's convenience wrapper remains pub(crate). An external frontend must use or
correct the owning public seam rather than acquire private Editor authority.

Current GPUI boundaries leave these gaps:

| Owner in Open-GPUI | Confirmed capability and missing engine seam |
| --- | --- |
| gpui_wgpu/src/wgpu_context.rs | Native constructors select/request their own device. from_adapter_and_device is private and installs device-wide loss/error callbacks; a shared-device constructor must respect Nara's existing callback owner and actual enabled features/limits. Public Arc fields alone do not supply that contract. |
| gpui_wgpu/src/wgpu_renderer.rs | Native WgpuRenderer owns its Surface. The private core's record_frame creates an encoder and submits it, while draw presents; the main pass fixes Clear. There is no public external-target/encoder, recording-only entry point. |
| gpui_wgpu/src/wgpu_atlas.rs | WgpuAtlas::new already accepts application-owned Arc Device/Queue. Presentation leases and view collections remain internal; this does not import an immutable Nara output or bind its retirement prefix. |
| gpui/src/elements/surface.rs and WGPU primitive dispatch | SurfaceSource currently carries Mac PixelBuffer, and the WGPU renderer skips PrimitiveBatch::Surfaces. A canvas or a same-named surface element does not supply ordered, clipped Nara texture sampling. |
| gpui/src/platform.rs and gpui_windows/src/window.rs | Public Platform/PlatformWindow::draw receives the actual accepted Scene. The Windows implementation fixes DirectXRenderer/DirectXDevices, with DirectWrite coupled to those devices; no native renderer factory avoids the second GPU today. |

Prefer a reusable GPUI renderer extension that records the accepted Scene against an external
device/target while Nara retains its adapter, texture leases, submission and presentation.
Ordinary GPUI platform ownership can remain available. An alternative Nara Platform/PlatformWindow
adapter would retain the existing Winit loop and panel attachment, but must implement the larger
native input/IME/scheduling/close boundary. run_embedded retains App lifetime; it does not turn
the blocking Windows loop into an externally driven host or inject a shared renderer.
The existing EditorPanelImageHost constructor takes Arc<winit::Window>, so it is not already
a toolkit-neutral GPUI native-window attachment. Resolve the actual window/loop arrangement
through the consumer before freezing a new public API.

Nara still selects registry WGPU 30.0.1, while this GPUI revision selects Git
`cddb2f5d80dc0009b2ef6767635140cacf90a007` with package version 30.0.0. Device, Queue and
TextureView have different Rust type identities. Source/version alignment or a concrete renderer
adapter is prerequisite work; this study changes neither lock nor graph. Both consumer paths
must preserve Scene order/clip, color/alpha, actual input geometry, resize, refusal and device-loss
retirement, and count copies/submissions/synchronization without steady-state CPU readback.
Headless test renderer injection, an application-owned atlas and CPU-painted Host conformance
remain useful controls, not the required engine View. Fixed-target closure and that real
consumer still precede the separate maintenance session; no second GPU, document/history/ECS
owner, engine UI default or R11/R12/R11f completion follows.

## November 10: Restart Arguments And Terminal Native Exit

Open-GPUI implements the scoped successor at `175a82b47f10b0bc2bb4b5920d54e1be5d70f977`,
qualified in `0b7024f86af0beed8fde92887eb1c1387997085c`, under unchanged fixed Zed target
`635026fabc38440fce5e1767d125026a17b12012`. Official provenance is
`715076597926d566d560b30b806749e1a2166080`, with Windows PowerShell corrections
`4c6c4750d3f87f0b93c937c9bda7b50639c0d07d` and
`bcf6582ce3500df93a8a39366640173e6786cea6`. Application configures native OsString arguments;
restart observers precede one-shot executable/argument delivery through the existing Platform.
Windows uses official UTF-16 quoting and static child-scoped environment transport; Mac/Linux
use positional forwarding. Generic command-shell fallback stays separate from restart's explicit
PowerShell requirement. The void API still distinguishes helper spawn from final-target success.

Actual workers reproduce two different defects. The inherited official script leaves empty
transport variables on PowerShell 7.6.9/.NET 10.0.14; deleting three exact Env entries after local
capture preserves native argv and strict absence. The fork's Windows loop calls ExitProcess
before queued App Quit delivery. Keep its loop alive until the existing terminal shutdown fence
acknowledges completion, retaining ordered ingress, asynchronous observers, retryable HWND
retirement and panic behavior. Remove the obsolete native-quit helper; tests that need terminal
fencing without a platform exit use their existing precise test entry. Direct platform callback
owners acknowledge terminal completion, with no new shutdown authority or App strong-reference
cycle. Official Restart Manager's synchronous return contract and foreign queued-quit lifecycle
remain separate successors.

The ordinary worker starts from its build output and restarts a distinct special-character
recorder, preserving empty/complex/unpaired-UTF-16 arguments. Exact selected-executable identity,
transport absence, completed asynchronous quit, old-worker termination and bounded Job cleanup
are asserted. Omitting only the fixture's selected target fails its exact identity oracle, then
the restored final source passes. Complete Core/public 1,765 and utility 127 pass; ordinary
all/off consumers pass 105/75, selected profiler-disabled cases 59 and final native follow-up
10 pass. These are distinct scopes, including actual process relaunches without a deterministic
executor. Prepared Mac/Linux command tests do not inherit Windows execution evidence.

The first full aggregate remains 2,160/2,161 with ten skips because unchanged clipboard metadata
is absent. Its unchanged clipboard follow-up passes 5/5. Complete Windows remains 268/269 with
ten skips because the unchanged activation oracle observes keyboard messages with unknown
physical-versus-injected origin. The unchanged activation/clipboard/returning follow-up passes
10/10. Preserve both failed aggregates and unknown causes; subsequent passes do not explain or
resolve them. No data assertion is weakened and no clipboard/input production policy changes.

Locked Rust 1.95 workspace/ordinary all-target checks, correctness Clippy, actual Windows Rust
1.93 affected-owner/ordinary all-feature/all-target checks, production wasm profiler off/on,
root/consumer formatting, workflow YAML and existing import/API/document/release scans pass.
No lock, framework production dependency or compiler-floor change follows. All nine lanes close
and no dedicated restart helper remains. Continue scoped idle-sleep guards, native appearance,
live windowing, native-quit completion and remaining fixed-target reconciliation. The source map
above supplies concrete engine-seam prerequisites, not external-target recording or a real engine
View. Framework closure and that qualified Nara GPU/Host/View consumer still precede maintenance
handoff. Nara keeps its document/ECS/history/GPU and UI choices; no runtime/default or full
R11/R12/R11f completion follows.

## November 10: Scoped Activity And Native Resource Ownership

Open-GPUI implements this successor at `2459aebecc90fb2bd2d5fefd04b7b4548f3652bc`, qualified
in `633b3fced1f442d021eec7591df6f20dca403ece`, after
`0b7024f86af0beed8fde92887eb1c1387997085c`. It adopts official
`450775244fade36db70af2447be24e74edb43368` and Windows reason-lifetime correction
`d2074f4e15d289345a9583ef973470b8bdfad74a` under unchanged fixed target
`635026fabc38440fce5e1767d125026a17b12012`. The separate Linux input migration is committed as
`467dc18a169`. These are local framework commits, changing no Nara runtime or dependency.

ActivityGuard retains the official Send once-release closure. App/Platform sleep acquisition
uses existing tasks, while BackgroundExecutor/PlatformDispatcher App Nap remains separate:
Apple can avoid throttling without inhibiting idle system sleep; other dispatchers inherit
no-op guards. Web/VisualTest and headless Linux reject unsupported sleep acquisition explicitly.
TestPlatform preserves the official delay/error/count controls. Actual guard ownership can
outlive an App or platform; it does not create another application shutdown or scheduler owner.

Windows owns a real power-request handle and its exact UTF-16 reason allocation. Pre-execution
review removes an invalid duplicated-handle test observer that would extend the native object
beyond its buffer. Normal and partial-acquisition close paths now share one private storage
owner; a true close refusal retains that allocation before logging. Controlled tests use real
HANDLE_FLAG_PROTECT_FROM_CLOSE, then restore and close only their own handle. The independent
release observation holds no native object or reason allocation. This bounded fork correction
retains the official kernel-request mechanism, without a manager or retry queue.

Linux ports complete portal acquisition, response/error, timeout and asynchronous Request::close.
Its typed private headless capability replaces display-string checks in acquisition and prior
power-listener setup. Private-session protocol tests exercise normal/late-cancel/late-timeout
release and guard-after-platform-drop through production helpers. Exact child completion and
bounded process cleanup are explicit; a 200ms no-duplicate window has finite scope. The only
new dependency edge is an already-locked zbus version for Linux tests. Mac owns the complete
typed activity in its current module; upstream's larger shared gpui_apple/Metal extraction
remains a separate queued topology unit. The local helper does not qualify that extraction or
Apple native execution.

Focused Core/public/Windows passes 31, ordinary activity 3 and complete ordinary all/off
consumers 108/78. Complete Core/public passes 1,789 within the 2,064/2,065 aggregate: an unchanged
generic-view test refuses its second window creation after display topology changes. Its
unchanged follow-up and all seven activity native controls pass 8/8, retaining the original
failure and unknown trigger. The ordinary Windows child observes eight real request handles,
independent group release, worker-thread release and actual quit/normal exit. Those resource
observations do not measure energy or prove OS sleep-policy enforcement.

Actual Linux qualification uses Ubuntu 26.04 WSL2, Rust 1.95, one Cargo job and Nara's target.
The first backend compilation exposes inherited X11/Wayland calls to the removed native input
outcome wrapper. Migrate those calls to the existing Core Option/value contract, preserving
quiescence and callback checkout. Both windowing features then pass all-target compilation.
Actual isolated portal/activity passes 11/11, final no-windowing unsupported 1/1, Core activity
and existing input regressions 22/22, and separate public consumers 9/9. An inherited
unconditional click-distance test receives its function's existing windowing cfg. The initial
all-feature Core attempt stops at missing PipeWire for screen capture; the executed scoped
Core selection uses default features plus test-support/profiler and does not qualify that
unrelated capture feature or live desktop IME. Download/environment details remain in the
framework's existing verification record.

Profiler-disabled Windows Core/public activity passes 24/24. Locked Windows Rust 1.95 workspace
and ordinary-consumer checks, host correctness Clippy, actual Windows Rust 1.93 affected-owner/
ordinary-consumer checks, production wasm profiler off/on, formatting/YAML and existing static
gates pass. The new Linux test dependency edge reuses an already-locked package; no package
version or compiler-floor change follows. All eight implementation/review lanes and the
additional bounded handoff review close. Mac compilation/native activity, desktop power-policy
effects and numeric costs remain open, separate from the executed resource/protocol evidence.

The handoff conditions now name the real engine consumer's device/submission/loop arrangement,
overlapping UI and clipped texture sampling, exact View input, producer retirement, refusal/
panic/close/device-loss controls and recorded copies/submissions/synchronization/resource costs.
After fixed-target closure and that qualified consumer, a dedicated Codex session maintains
Open-GPUI against engine feedback; Nara continues complete Editor/game-runtime acceptance.
Remaining native appearance, live windowing, quit/Restart Manager, Apple topology and gesture
dispositions still precede closure. No UI default, new engine owner or full R11/R12/R11f
completion follows from this prerequisite.

## November 10: Native Appearance And Remaining Windowing Lifecycle

Open-GPUI implements native appearance at `0b0ea5416656c2c43c242c01e2a665e21d3814ba`, with
qualification `07851e88984e8091ce9a79357d44ea5aafe5020d`, following scoped activity. Official
source is `8e4e5a39eed932d943cb7a3e7078885b75d1ea54`, typed setter successor
`d86aa6e6b9f52046fb299ab57ec884b300190813` and appearance conversion successor
`10cfb459449a65c8fee7d390794d6f3d5098e9f7`, under unchanged fixed target
`635026fabc38440fce5e1767d125026a17b12012`. The complete App/Platform setter forwards an
optional appearance; Mac maps all four variants to NSApplication appearance, while None
restores system following. Other platforms keep the official no-op. No shared theme state,
Nara token policy or engine runtime/default is added.

The fork bridges its existing raw Cocoa owners to typed retained NSAppearance values. The
Window getter retains that same native window under its state lock, then queries it unlocked;
this preserves object custody without holding native state across Objective-C callbacks.
Existing application class, ordered ingress and native leases remain their owners. A private
cfg(test) recorder qualifies exact App forwarding without creating public fake-platform policy.
The busy-App case queues actual appearance ingress, coalesces it after borrow release and lets
the observer issue another setter request. Public no-op controls preserve getters, observers
and rendered revision.

Windows focused and profiler-disabled pass 4/4 each, affected callbacks/query snapshots 23/23,
corrected App registry/lifecycle selection 43/43, and independently locked all/off consumers
3/3 and 2/2. The initial plural App test filter selected three unrelated basics; its recorded
pass does not substitute for the corrected anchored 43-case gate. Actual Linux Core/public
passes 4/4 under locked/offline Rust 1.95 with default features plus test-support/profiler.
Locked Windows workspace/consumer compilation, host correctness Clippy, actual Windows Rust
1.93 affected-owner/consumer checks, production wasm profiler off/on, formatting/YAML and
existing import/API/document/release scans pass. No package, lock or compiler-floor upgrade
follows; the existing AppKit feature closure enables NSAppearance.

The actual foreign Apple check still stops at inherited media binding generation before the
Mac owner compiles. Native conversion tests and a dedicated real-main-thread GUI/Metal probe
are source-prepared; ordinary libtest workers cannot establish AppKit main-thread admission.
The probe observes native override/clear, existing/new window inheritance, explicit window
override, callback reentry, close cleanup and original global appearance restoration. It does
not enter Core App admission or qualify normal GPUIApplication launch/delegate installation,
titlebar pixels or an actual system-theme change after clear. Its non-Mac entry fails with
exit 1, observed by actual Rust 1.93 Windows execution. Independent source review and the
initialization-order follow-up find no confirmed P1/P2; all four lanes close. Apple compilation
and execution retain their separate gates.

The next complete startup/live-windowing unit is source-prepared from
`f37989fbdf840b308892af66516c2b9c3ef3853a` and fixed-target display/activation follow-ups.
Linux must keep one App/runtime/executor/text-system owner while replacing the display
connection, validating allowed modes, preserving the old connection on failure and settling
requests. Environment/token/socket custody, clipboard connections and source/listener teardown
belong to that same lifecycle. Windows requires delayed GPU/session attach, DirectWrite state
and stale-recovery fencing; Mac requires application activation policy distinct from per-window
activation. The fork must revalidate retained native retirement/capture/presentation custody
beyond an empty App.windows collection and preserve ordered callbacks and terminal quit.
Existing fixtures that open before Platform::run need owning initialization rather than
weakened native rendering assertions. These are prepared requirements, not implemented switching.

Live windowing still does not supply external event pumping or shared-device/target recording.
The real Nara GPU/Host/View consumer, final fixed-target dispositions and framework maintenance
handoff remain open. Nara keeps its adapter/document/ECS/history/GPU responsibilities and full
Editor/game-runtime acceptance; no R11/R12/R11f completion follows.

## November 10: Live Windowing And Framework Maintenance Handoff

Open-GPUI implements the startup/live-windowing lifecycle at
`c1d8204f2da7069f40ea8e0420a37a5c0f210474`, with qualification
`e7b2d848f555ef13d5ff4c9d4b5141ae7710f8b9` and separate clipboard-fixture correction
`1c33677716c`. Official provenance is `f37989fbdf840b308892af66516c2b9c3ef3853a`, display
refresh `f495dd0f77b69e287ed72c1168dc02965f08be7d`, Mac activation
`d86aa6e6b9f52046fb299ab57ec884b300190813`, Wayland activation
`4d847859f5506471bd80ca78ee6ced161b19a4e8` and stale URI-read correction `87a1ea30e8`.
The fixed target stays `635026fabc38440fce5e1767d125026a17b12012`.

Official WindowingRequest, GraphicalEnvironment, application ActivationPolicy and Linux
WindowingModes use one persistent App/runtime/executor/text-system owner with replaceable native
connections. Windows defers GPU acquisition until admitted startup/live attach, fences old device
recovery generations and releases/reacquires optional DirectWrite GPU state. Mac stores typed
application policy, distinct from per-window activation. The obsolete Linux generic per-client
platform and headless-client owner are removed. The ordinary toggle_headless consumer uses the
public platform factory without test support.

Fork adaptations retain the existing exact native owners. AppCell validation stays attached
through deferred physical apply, rather than treating an empty logical window map as readiness.
Retirement takes ownership before presentation preparation, preserving the native box across
cleanup panic and retry, observer fanout, dependent retirement and both shutdown modes. Explicit
owned Wayland sockets avoid adopting a running Host's descriptor from an environment string or
mutating its process environment. The official calloop idle pin is retained with explicit owned
timer shutdown and run-boundary foreground cleanup. A reproduced premature timer cancellation
is corrected so the public standalone background executor remains usable after its temporary,
never-launched platform drops. These are framework/Host custody corrections, not Nara services.

Core/public controls pass 126/126 with profiler enabled and disabled. Final Windows library
passes 283/283 with ten explicit skips; same-App native controls include three attachments,
four real recoveries on already Submitted HWNDs, actual mouse/model updates, text and isolated
color-emoji readback. The original 2,221-case aggregate retains its sole clipboard restoration
failure; the bounded fixture correction and later owner pass do not establish the original
failure's trigger. Complete UI core/components/docking/Canvas passes 2,990/2,990. Independently
locked consumers pass 115/115 with all features and three skips, and 83/83 without optional
features or skips.

Actual Linux complete library passes 162/162 with four native cases separately qualified.
The final configured native selection passes 4/4 on WSLg using Mesa llvmpipe software Vulkan:
same-App X11/headless/Wayland, actual X11 F8/model/paint/Submitted alignment, initial-only owned
socket with unreachable address and actual exec isolation, and two private Xvfb clipboard
managers with selection partition and handover cleanup. Default WSLg rendering reaches the
completion markers but SIGSEGVs during pthread-specific-data destruction at thread exit; GDB
does not identify the responsible library. Keep that default profile unqualified. The configured
pass establishes neither Linux hardware performance nor compositor scanout/pixel-readback.

Linux headless/X11/Wayland/combined feature checks and the ordinary example compile. Locked
Windows workspace/consumer checks, host/Linux correctness Clippy, actual Windows Rust 1.93
framework/ordinary/test-support/hot-reload checks, production wasm profiler off/on and independent
smoke compilation, formatting/YAML and static import/API/document/release gates pass. The Apple
foreign check stops before the new owner when host cc rejects Apple target flags in alloca;
actual Mac compilation and GUI/Metal execution remain open. All sixteen work/review lanes close.

The agreed maintenance boundary is dependency ordered: finish the fixed-target framework
reconciliation, qualify the real Nara GPU/Host/View consumer, then hand framework maintenance
to a separate Codex session. That session starts from the existing sync skill, candidate queue,
plan, current-state and executed public/native gates. Later engine feedback becomes a bounded
framework change with an ordinary public regression and an affected Nara consumer check.
Open-GPUI owns general UI behavior; Nara owns document/ECS/history, GPU submission, Host/View
policy and final Editor/game-runtime acceptance. The actual adapter still needs source-aligned
GPU types, external target/encoder recording, clipped retained textures, one submission/loop
authority and exact input/retirement custody. Live windowing does not provide those seams.
Native quit/Restart Manager, shared Apple topology and final residual reconciliation remain
ahead of that adapter. No toolkit default or R11/R12/R11f completion follows.

## November 10: Targeted Upstream Verification And Native Quit Completion

The user clarified the maintenance cost model: trust unchanged official implementations and
their upstream coverage. Compile affected framework owners and public consumers; concentrate
regressions and independent review on fork changes and integration seams. A changed contract,
observed failure or release claim can justify broader coverage. Previous full-suite results
remain historical evidence rather than a mandatory cycle for every port. Open-GPUI records
this rule in its existing upstream-sync skill, plan and verification guide.

Open-GPUI implementation `10f14f08be8572eaa802e4fb1cfcaf2d05e268c3` ports official
`7f2a2c3c3ee2f23f28772dee7661fb98d3910990` at the unchanged fixed target
`635026fabc38440fce5e1767d125026a17b12012`. Platform::on_quit now returns synchronous
completion. Windows application-window session messages reach the validated platform owner;
completed shutdown flushes logs and uses its existing process-exit policy, while pending
shutdown posts the official best-effort quit request. Fork readiness reuses the ordered Quit
event, bounded post-borrow convergence and exact native-retirement predicate. Queuing a
request cannot acknowledge completion. The ordinary Windows loop retains observer-panic
containment and terminal progress; no second lifecycle owner is introduced.

Focused Core passes 7/7 and corrected Windows native passes 6/6, including busy App completion,
retained native terminal, actual HWND forwarding, asynchronous observers/retryable destruction
and isolated production exit/log flushing. The initial Windows 4/6 exposed a callback-unwind
regression and a new fixture's filtered-message assumption; the final corrections preserve the
original observer-panic assertions. The separately locked ordinary consumer passes all-target
compilation, root formatting and existing import/API/document/release checks pass, and bounded
review finds no outstanding P1/P2. All three lanes close. No unrelated component, docking,
font/render or compiler/platform matrix is repeated.

Linux/Mac/Web signature migration adds no foreign termination wait policy or execution claim.
Shared Apple topology and final fixed-target dispositions still precede the real Nara
GPU/Host/View consumer and separate framework-maintenance handoff. Nara changes only these
evidence documents; its dependencies, document/ECS/history, renderer, loop and UI default retain
their existing owners. Full R11/R12/R11f and Editor/game-runtime acceptance remain open.
