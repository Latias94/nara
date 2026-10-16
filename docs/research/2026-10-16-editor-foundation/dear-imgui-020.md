# Dear ImGui 0.20 editor integration research

Date: 2026-10-16. Scope: a possible Nara editor adapter, not a migration receipt.

## Decision

`dear-imgui-rs` 0.20.0 is a credible candidate for a dockable Nara editor. Its WGPU and Winit dependency versions fit Nara. It already demonstrates engine-owned render textures inside detached UI windows. However, native multi-viewport is explicitly experimental, Linux/Wayland is unsupported, and its convenient `dear-app` runtime explicitly excludes multi-viewport. Adopt a bounded integration probe using the direct Winit/WGPU backends before replacing the ordinary editor. The research establishes source-level feasibility; no build or native execution was performed for this note.

The valuable durable boundary is the separation of authoring operations, document models, editor workspace state, and native/GPU lifecycle from toolkit-specific widgets. Future custom UI can reuse those responsibilities and engine rendering resources. It will still require new widget, layout, input, accessibility, and docking implementations; changing toolkits cannot promise zero UI work.

## Exact baseline

The local reference checkout describes itself as `v0.20.0-2-g391171d1`. The exact tag resolves to `af2942c78651b280d18af621de55b5462bb1e10a`. Research read tag files using `git show v0.20.0:<path>` without modifying the checkout. The two later commits concern Cargo 1.100 artifact layout; they do not change the core, Winit, WGPU, or `dear-app` files cited below. No post-tag capability is assumed.

The tag's workspace declares Rust 1.92, WGPU 30.0 and Winit 0.30.13. Its compatibility document identifies Dear ImGui v1.92.9b docking. Nara currently pins WGPU 30.0.0 and Winit 0.30.13 with Rust 1.95, so these boundaries do not require a dependency-generation upgrade. This does not prove feature unification or compilation. Use one release train and exactly one WGPU/platform feature route; do not apply `--all-features` to mutually exclusive backend routes. [S1, S3]

The optional Dear ImGui Bevy backend requires exactly Bevy 0.20.0 and Rust 1.97.1. Nara's use of Bevy ECS does not make that renderer integration appropriate; the direct WGPU/Winit route avoids introducing a second application framework. [S1]

## Three different meanings of “view”

| Concept | Responsibility | Evidence |
| --- | --- | --- |
| Docking | Tabs and split panes within a native host window | `DockLayout` and dockspace APIs; independent configuration |
| Native multi-viewport | Moving a UI window outside its host creates another OS window and presentation surface | Winit platform owner plus WGPU renderer route |
| Scene/Game view | An engine camera or game renders to an offscreen texture displayed in an editor pane | External texture registration and render-to-texture example |

Core tests specifically prove that enabling native multi-viewport does not enable docking. A Scene or Game view may remain docked or be detached; its engine camera, world, and input ownership remain engine responsibilities. The upstream multi-viewport documentation likewise defines the feature in terms of additional native windows, not additional game cameras. [S2, S5, S8]

## Platform readiness

| Target | Ordinary in-window docking | Native detached windows at the tag | Evidence limit |
| --- | --- | --- | --- |
| Windows | Separate from native multi-viewport | Implemented, experimental | Native policies exist; mixed-DPI/taskbar and repeated-click checklists are not claimed verified in this reference worktree |
| macOS | Separate from native multi-viewport | Implemented, experimental | Cocoa coordinate/focus policies exist; Retina, Spaces and Dock behavior still require actual platform acceptance |
| Linux/X11 | Separate from native multi-viewport | Implemented, experimental | Release gate uses Xvfb with Mesa/Lavapipe; work-area attribution conservatively falls back rather than claiming exact panel geometry |
| Linux/Wayland | Available independently of detached windows | Explicitly rejected | Global desktop window coordinates required by this route are unavailable |
| Web | Browser single-window integration | Unsupported | Native secondary windows are not this integration's browser model |

This matrix separates implemented routes from a tested Nara product. Upstream also warns that X11 behavior varies across window managers and documents Wayland's limitation. A Linux editor must retain single-window docking when native detachment is unavailable. Do not make detached windows a prerequisite for opening or editing a project. [S2, S6, S8]

Multi-viewport requires `HiDpiMode::Default`; rounded or locked primary-window coordinates are rejected. Window-manager policy also limits parity: some focus hints are best effort outside Windows, live X11 taskbar policy changes are rejected, and the Winit route cannot implement per-window opacity. Keep transparent docking payloads disabled rather than assuming every upstream flag is supported. [S2]

## Ownership and rendering integration

`dear-app` owns a complete Winit/WGPU runtime with main-window recovery. Its 0.20 configuration rejects `VIEWPORTS_ENABLE`. Wrapping Nara in that runtime would both miss the desired feature and create competing lifecycle authority. Use `dear-imgui-rs`, `dear-imgui-winit`, and `dear-imgui-wgpu` directly under Nara's existing application/event loop. [S4]

“Reuse the GPU” means sharing handles from one engine-created device lineage, not a backend API that only borrows `&Device`. `WgpuInitInfo` accepts `Device` and `Queue` handles and optional `Instance` and `Adapter`; clone the existing handles, do not request another device. Native secondary surfaces require the same instance and adapter. The current renderer writes sRGB UI output; wide-gamut/HDR output would require an application-owned conversion policy. [S3, S7]

The host remains responsible for event-loop entry, device creation/recovery, engine rendering, and main-surface acquire/submit/present. Ownership of secondary native windows is delegated to `WinitPlatform`, which retains their `Arc<Window>` allocations. The attached `WinitViewportRoute` consumes the renderer and owns secondary surface rendering/presentation. Thus “host owns lifecycle” should permit a controlled adapter sub-owner; a requirement that Nara independently create every secondary window would conflict with the published route. [S2, S3, S7]

The route performs one prepared-frame transaction: finalize UI, prepare secondary windows and texture work, then acquire and draw the main surface. Do not also invoke upstream platform-window phases manually. A skipped main-surface acquisition does not undo already completed secondary work. On device loss, replace the device-generation resources and reattach the renderer route; do not preserve stale external image mappings. [S3, S9]

Scene/Game textures have a practical existing path: register an application-owned `TextureView`, display its returned texture identity, and update the mapping on replacement. Keep the mapping until the final referring UI frame has been recorded; keep underlying resources alive through GPU completion. Nearest sampling is explicit, useful for pixel-art previews. The existing `wgpu_rtt_gameview` and native multi-viewport examples exercise these pieces without requiring the editor to own game objects. [S3, S5]

Keep frame execution synchronous for the first adapter. The safe API already represents unresolved versus drawable frames with `PendingFrame` and `ReconciledFrame`. Detached snapshots are for genuine thread separation and introduce texture completion/order obligations; they are unnecessary for proving this editor route. Consume the library's lifecycle contract rather than adding a parallel Nara proof framework. [S10]

## Input, accessibility, shutdown and tests

Winit supports committed Unicode text, caret-area forwarding and automatic IME activation while text widgets are active. Its event implementation deliberately ignores preedit text. Therefore Chinese input needs real candidate placement, composition/commit, focus-transfer and detached-window tests; “IME events exist” is insufficient acceptance. [S2, S11]

This investigation found no OS accessibility-tree bridge in the tag's core/Winit/WGPU implementation. This is a bounded source-search observation, not proof that every extension lacks accessibility. Keyboard navigation is not equivalent to screen-reader semantics. The adoption gate should record the actual accessibility capability and any regression from the current editor; a Cargo lockfile containing AccessKit through other workspace integrations is not evidence that these direct backends expose an accessible editor.

Shutdown must release renderer ownership before platform viewport ownership, then detach the primary window before destroying it if the Context remains alive. Retryable explicit shutdown reports failure; `Drop` is cleanup fallback, not the normal observable application contract. Editor tabs/documents must remain product state through these toolkit resource operations. [S2, S3]

The upstream route-evidence matrix gives Winit/WGPU level A because a native smoke observes detach, GPU submission/presentation, merge and cleanup. It does not certify every supported OS configuration or Nara's integration. Reuse a bounded native smoke plus Nara's existing authoring acceptance; do not reproduce upstream callback internals in Nara scripts. [S6, S9]

The Rust wrapper uses MIT OR Apache-2.0; bundled third-party components retain their own terms. In particular, Dear ImGui Test Engine has a separate upstream license with free and paid conditions. Treat it as an optional development/test dependency, not an implied permissive runtime requirement or a prerequisite for Nara's ordinary semantic command tests. [S1, S12]

## Bounded Nara probe and falsifiable gate

Build one real adapter path under Nara's existing host, initially pinned to 0.20.0. Include a docked hierarchy and inspector, one editable scalar using the ordinary revision-bound command/undo/save/reopen path, and a genuine Nara-rendered Scene/Game texture. Detach that same view into a native secondary window and merge it back while rendering continues. A colored standalone demo or an upstream example alone does not satisfy this gate.

Require evidence for: a single event loop/device lineage; correct input capture between fields and game view; resize/minimize/restore; repeated detach/merge and closure; mixed-DPI movement; Chinese text and IME candidate placement; layout save/reset/reopen; texture replacement without stale images; and renderer-before-platform shutdown. Keep failure/unsupported results explicit. Device-loss recovery must either pass the host's existing policy or be a named blocking gap before adoption.

Run Windows acceptance first on available hardware, then Linux/X11 and macOS on real release targets. Linux/Wayland must open the project with in-window docking and visibly unavailable native detachment, without failing the editing workflow. Record the accessibility comparison rather than asserting parity. Measure idle CPU and active frame time against the same current-editor project and retain the measurements; the probe has no invented performance target.

Promote the adapter only if those product workflows and lifecycle cases pass, with a documented platform capability matrix and no second authoring/history implementation. Then migrate the remaining UI panels against a finite feature inventory and remove the superseded production adapter. If a hard lifecycle or authoring regression remains, retain the current editor while addressing the specific finding. Future custom UI should implement the same semantic contracts and render-resource handoff, without a universal widget DSL or permanent duplication of all toolkits.

## Primary sources

All `repo-ref/dear-imgui-rs/` references below are at tag commit `af2942c78651b280d18af621de55b5462bb1e10a`; line numbers are one-based source-file lines.

- S1: `Cargo.toml:44–48,104–105`; `docs/COMPATIBILITY.md:27–45`. [Exact release compatibility](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/docs/COMPATIBILITY.md#L27).
- S2: `backends/dear-imgui-winit/README.md:79–85,111–143,183–278`. [Exact platform integration](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-winit/README.md#L183).
- S3: `backends/dear-imgui-wgpu/README.md:27–77,79–177,197–227`. [Exact renderer integration](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/README.md#L29).
- S4: `dear-app/README.md:6,149–174`. [Single-window runtime boundary](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-app/README.md#L174).
- S5: `dear-imgui/src/context/tests.rs:590–604`; `dear-imgui/src/dock_layout/model.rs:1–69`; `examples/04-integration/wgpu_rtt_gameview.rs:1–95`; `examples/02-docking/multi_viewport_wgpu.rs:1–47`.
- S6: `docs/COMPATIBILITY.md:211–222`; `docs/renderer-route-evidence.md:9–29,31–47`.
- S7: `backends/dear-imgui-wgpu/src/data.rs:169–239`.
- S8: upstream Dear ImGui [Multi-Viewports](https://github.com/ocornut/imgui/wiki/Multi-Viewports) and [Docking](https://github.com/ocornut/imgui/wiki/Docking), accessed 2026-10-16. These explain current upstream concepts/constraints; they do not expand the pinned Rust backend's support claim.
- S9: `examples/ci/wgpu_multi_viewport_smoke.rs`; `examples/support/wgpu_multi_viewport_runtime.rs`; `docs/renderer-route-evidence.md:27`.
- S10: `docs/CUSTOM_BACKENDS.md:17–50`; `docs/COMPATIBILITY.md:251–289,320–334`.
- S11: `backends/dear-imgui-winit/src/events.rs:142–162`; `backends/dear-imgui-winit/src/platform/window_state.rs:18–85,225–292`.
- S12: `extensions/dear-imgui-test-engine/README.md:169`; `extensions/dear-imgui-test-engine-sys/LICENSE.txt:1–58`; root `LICENSE-MIT` and `LICENSE-APACHE`.
