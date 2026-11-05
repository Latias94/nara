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
