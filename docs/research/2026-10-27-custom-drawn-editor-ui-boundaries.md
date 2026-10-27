# Custom-Drawn Editor UI: Reference Boundaries

Date: 2026-10-27. Scope: supplied architecture conversation, lines 3611-4134, versus primary sources.
The user subsequently clarified the target: **Nara's own custom-drawn Editor UI**, not embedding
another toolkit. References supply concepts and counterexamples, not an adoption shortlist.
This is reference research, not runtime qualification. **Source fact** means inspected code;
**inference** means recommendation; **unverified** means no build/native/performance/unload experiment.
Sparse checkouts stayed unchanged; omitted files were read online at the same pin. Official docs checked today.

## Baselines

| Reference | Remote | Inspected commit |
| --- | --- | --- |
| Fyrox | `https://github.com/FyroxEngine/Fyrox.git` | `f008e4c2cdfdcda7b37bf8534d0df8de155748f6` |
| Xilem/Masonry | `https://github.com/linebender/xilem.git` | `271a27a6d4a930f7878d404f9014e3c50a3a9b88` |
| egui, integration alternative | `https://github.com/emilk/egui` | `86099aca1276fe786921992c8d5a68914ee79bec` |
| Open GPUI, limited alternative check | `https://github.com/Latias94/open-gpui` | `689db5b49b33a5b2e64ba332bff72c98e70092bb` |

Nara context is `refactor/scene-data-representation` at `65402b9206b26555787e4da5b7f7abd1a0c048fa`.
Only dependency pins are compared: `winit =0.30.13`, `wgpu =30.0.1` ([local manifest][nara-deps]).
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
