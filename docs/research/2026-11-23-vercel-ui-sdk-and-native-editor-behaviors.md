# Vercel UI SDK and Nara’s Self-Drawn Editor

November 23, 2026. Use UI SDK as a reference for how an editor control responds to typing,
dragging, keyboard navigation and interruption. Keep Open-GPUI as the first native implementation
to try and Nara's Design Kit as the guide to appearance and editing semantics. coss ui helps
compose the field, label, focus ring and error message. UI SDK adds working examples where one
can inspect exactly when a gesture changes a value. This choice fits the accepted division of
responsibilities in ADRs 0107 and 0118 and needs no new architecture decision.
[Earlier coss comparison](2026-11-23-editor-direction-and-native-authoring.md#coss-ui-as-a-component-reference) · [Component ownership](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L141-L159) · [Native frontend direction](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L113-L138)

The local reference is `repo-ref/vercel-ui` at `a349d52a7c019260202dcb2139b28d073e218b28`,
with `@ui-sdk/core` at `0.1.0-beta.15` under Apache-2.0. Core supplies behaviors with React,
Vue, Svelte, Solid and Web Components adapters. Their shared target is the browser. For example,
number scrubbing uses a DOM element, document listeners and pointer capture. Nara can reuse the
interaction design while Open-GPUI supplies input, layout and drawing services and Nara's Host
keeps the engine device and final frame submission. The interactive observations below come
from the hosted React demos in headless Edge on Windows. The Nara comparison uses `8f6dbe6`
and the two Open-GPUI revisions identified below.
[Upstream README](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/README.md#L1-L24) · [Package](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/package.json#L1-L32) · [Pointer handling](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/foundation/drag-and-drop/pointer.ts#L183-L235)

## The Inspector number field

| Action in the hosted Number Input example | Observed result | How Nara should use it |
| --- | --- | --- |
| Drag the Quantity label 24 CSS pixels from an initial value of `1` | The field showed `7` while held. The demo logged `Quantity 7` once on release. | Let a field preview continuously. Let the document record one accepted change on release. |
| Type `20` in Guests, whose range is `1–8`, then press Enter | The draft showed `20`. Enter changed it to `8` and disabled Increase. Arrow Down produced `7`. | Nara's Inspector keeps an out-of-range entry editable with an error, so the author can correct it. |
| Repeat the Quantity drag and inject a DOM `pointercancel` for that pointer before release | The demo kept `7` and logged `Quantity 7` before mouse-up. | Handle interrupted capture as an explicit cancellation in Nara. Discard the preview and keep the original document history. |

The SDK keeps display text separately from the numeric value. Its ordinary commit suppresses an
unchanged result, and an untouched rounded display preserves the underlying value. Its number
scrubber calls `commit(end)` even when `onEnd` receives `canceled`. The third observation used a
synthetic browser cancellation. This distinction matters when a user switches tools or loses
pointer capture partway through an Inspector drag. Nara's continuous-edit contract already names
the target, base revision, preview and cancellation, and its drawn Inspector already retains an
invalid `-` after Enter so the author can correct it.
[Live Number Input](https://ui-sdk.dev/docs/react/components/number-input) · [Value and commit](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/components/number-input/number-input-api.ts#L176-L241) · [Typed submission](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/components/number-input/number-input-api.ts#L342-L386) · [Scrub ending](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/components/number-input/number-input-api.ts#L522-L539) · [Nara gesture contract](../architecture/adr/0026-editor-command-patch-and-undo-model.md#continuous-interaction-transactions) · [Drawn Inspector test](../../editor/src/frontend/drawn/inspector/tests.rs#L296-L330)

Nara's GPUI consumer pins `f75c10b2`. Its `NumberInput` supports stepping and an `on_change`
callback. The later Open-GPUI reference at `428b460e` has `on_preview`, `on_commit` and `on_cancel`,
with tests for release outside the handle and for capture loss. It also treats some invalid
submissions as cancellation. Its accepted scalar is `f32`. Passing the integer `16,777,217` through
that type gives `16,777,216`. Reuse the gesture where it fits, and preserve Nara's draft text and
typed `I64`, `U64` or `F64` conversion for reflected fields. `UiPropertyRow` can continue to
measure and position the control independently of these decisions.
[Nara fixture](../../tests/fixtures/gpui-engine-consumer/Cargo.toml) · [Pinned stepper](https://github.com/Latias94/open-gpui/blob/f75c10b2a9bf7b4dcb2c8c2aa446203997601369/crates/ui_components/src/number_input.rs#L238-L270) · [Later callbacks](https://github.com/Latias94/open-gpui/blob/428b460e6d33e246aeb2d740a0e9c6215381c658/crates/ui_components/src/number_input.rs#L475-L501) · [Invalid submission](https://github.com/Latias94/open-gpui/blob/428b460e6d33e246aeb2d740a0e9c6215381c658/crates/ui_components/tests/number_editing.rs#L228-L253) · [Captured scrub](https://github.com/Latias94/open-gpui/blob/428b460e6d33e246aeb2d740a0e9c6215381c658/crates/ui_components/tests/number_editing.rs#L688-L771) · [Commit type](https://github.com/Latias94/open-gpui/blob/428b460e6d33e246aeb2d740a0e9c6215381c658/crates/ui_components/src/number_input/editing.rs#L132-L160) · [Nara value parser](../../editor/src/frontend/controls/value_draft.rs#L426-L438) · [Row layout](../../crates/nara_ui_widgets/src/property_row.rs#L6-L15)

## Selection in the Hierarchy

The SDK's Tree lets clicking a folder select it while its disclosure arrow controls expansion.
Shift extends a selection, Ctrl/Cmd toggles an item, and Select All applies to visible items.
Those behaviors give Nara's Hierarchy a familiar keyboard and pointer vocabulary. Nara's design
also specifies what a scene operation will affect after filtering. A selection keeps its entity
identities, and a row count such as `3 selected · 1 hidden` makes the scope visible. Keep that
selection in the editing context and expansion and scroll in the view.
[Tree action and selection](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/apps/docs/content/docs/react/components/tree.mdx#L125-L149) · [Nara selection scope](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1279-L1291)

## The animation timeline and graph canvas

The component catalog's `Timeline` displays dated events with keyboard navigation. The **homepage
clip editor** is a separate `reel` built with `createSpans`. It has seven clips, a 24 fps timebase,
six-frame snapping and a per-track overlap rule. In the live demo, Arrow Right moved the start of
Title card from `00:01:00` to `00:01:06`. A pointer drag previewed `00:03:00`, then Escape returned
it to `00:01:06` while the playhead stayed at frame `120`. The span behavior keeps a draft during
the drag and calls the owner's setter on a successful drop. The demo keeps playback position in
its own state. Those two choices carry over to a dope sheet. Authored key identity, time units,
interpolation, curve tangents and Undo belong to Nara's animation tool and document.
[Live clip editor](https://ui-sdk.dev/#timeline) · [Catalog Timeline](https://ui-sdk.dev/docs/react/components/timeline) · [Reel configuration](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/apps/docs/components/home-builds.tsx#L493-L551) · [Span cancellation and drop](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/foundation/direct-manipulation/spans.ts#L420-L472) · [Playhead](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/apps/docs/components/home-builds.tsx#L614-L642) · [Nara animation design](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1439-L1453)

For a Dialogue graph, `createConnect` gives a concrete keyboard alternative to dragging a wire.
Arrows pick an eligible port in the requested direction, Enter connects, and Escape cancels.
Pointer targeting uses a radius measured in screen pixels so a port stays equally easy to hit
as the author zooms. Adopt these ideas where they help graph authors. Jellyflow can draw the
candidate wire and deliver the request to Nara's existing Dialogue document, which validates the
connection and owns history. The SDK's DOM bounds and CSS transforms answer questions about a 2D
canvas. A pick in Nara's Scene View uses the camera and mapping of the frame actually displayed.
[Canvas behavior](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/apps/docs/content/docs/core/behaviors/canvas.mdx#L176-L210) · [Connection handoff](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/foundation/canvas/connect.ts#L170-L199) · [Dialogue handoff](../guides/editor-documents.md#L107-L126) · [Existing graph gestures](../guides/editor-documents.md#L207-L213) · [Presented Scene mapping](../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md#L148-L176)

## Semantic actions and evidence

UI SDK lets an agent discover a registered control's name, state and available actions. Its
snapshot derives accessible names from connected DOM elements, and `press` clicks a DOM button.
A callback can decline a requested change while `act()` returns `ok: true`. The caller can read
the returned changes and values to see the outcome.
[DOM labels](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/page/observe/registry.ts#L227-L266) · [Bridge](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/page/agent/agent.ts#L2102-L2159) · [Button click](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/packages/core/src/page/agent/agent.ts#L1111-L1117) · [Callback contract](https://github.com/vercel-labs/ui/blob/a349d52a7c019260202dcb2139b28d073e218b28/apps/docs/content/docs/agents/connect-an-agent/reference.mdx#L125-L138)

Nara's drawn Inspector already exercises this through a presented `UiSemanticSnapshot`. Apply
advertises an action on a particular control. The image Host checks the observation, the original
panel handles the request, and the document reports the edit result. Confirming a save requires
`Saved` for the matching document and revision. Later presentation and OS accessibility each
have their own evidence. The same route can serve future editor automation.
[Apply from a presented control](../../editor/src/platform/drawn/tests/operations.rs#L59-L81) · [Image Host](../../editor/src/native/images/observation.rs#L224-L255) · [Save result](../../editor/src/platform/drawn/tests/operations.rs#L273-L292) · [Frontend contract](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L490-L499)

## First use in Nara

Start with one designed control, **Transform X in a two-object selection**. With X values `2`
and `10`, the row shows a mixed value. Drag by `+3` to preview `5` and `13`. Release and one Undo
should restore `2` and `10`. Escape during another drag should drop its preview. Typing `3`
instead should replace X on both objects. Y and Z stay as authored. This single workflow gives
the team a reason to adopt a native gesture, preserve its document semantics and decide what is
worth sharing with the next control.
[Nara numeric and vector rules](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1242-L1250) · [Multi-object edit and Undo](../design/editor-reference/nara-editor-design-spec-v0.4.md#L309-L322)
