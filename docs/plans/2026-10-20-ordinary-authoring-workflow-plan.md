# Ordinary Authoring Workflow

Status: Completed for ordinary structural authoring. Starting source: `ff095c5`; implementation:
`e673693` on main. Presentation and later measurement/persistent-game stages remain separate.

## Outcome

Complete the first workflow recommended by the module/goal review: an ordinary external project
can create understood entities, arrange structure, remove eligible content, Undo/Redo, Save/Reopen
and proceed to strict Check/Play without loading game Rust into the Editor. Preserve unavailable
records, source authority, exact inverse history and rejection nonmutation.

Start with generic structural authoring. The approved follow-up sequence remains fixed-resolution
game presentation, decision-local interaction/iteration measurement, then one persistent two-level
game. Each stage needs its own concrete consumer and scope; do not implement speculative registries,
Tile tools, a new async framework or a second job kernel.

## Contract And Design Work

Read Strategy, ADRs 0090/0114/0117/0118 and the current authoring/patch/history owners. Reconcile
any affected Accepted contract before changing it. Existing research and pinned Bevy/Godot are
comparison evidence, not implementation authority.

For each structural operation, identify its actual reference and provenance effects. Admit safe
operations using document-owned data and exact transactions. Unknown reference shapes must never
be treated as proof that deleting a target is safe. Reparent/reorder should preserve data they do
not own; native world transforms or compiled bindings must not become prerequisites for document
editing. Keep the ordinary caller interface small and route action results through the existing
revision-bound protocol.

## Lane Map

Mode: execute_direct, beginning with bounded read-only design lanes in the shared checkout.
Root owns integration, contracts/plan/memory, all Cargo/fmt/Git and final verification. Reuse target
and CARGO_BUILD_JOBS=1; never run Cargo concurrently. Preserve untracked vendor and concurrent work.

| Lane | Initial responsibility | Writable files |
| --- | --- | --- |
| Root | Synthesize operation policy, inspect actual public workflow, integrate implementation and evidence | Plan, owning ADRs/guides, root implementation/tests after explicit assignment |
| A | Scene/document structural admission, unavailable references, exact inverses and history | None during design |
| B | Tooling/Editor action and native widget path, source/public consumer ergonomics | None during design |
| C | Independent challenge and concrete external/native acceptance design | None during design |

Implementation assignment after independent design challenge:

- A owns `crates/nara_scene/src/authoring_decode.rs`, cohesive private structure extraction under
  `authoring_decode/`, `crates/nara_scene/src/patch.rs` and Scene structural-authoring tests.
- B owns tooling composition/workspace projections and their tests, plus ordinary ImGui Scene-tree
  controls in `editor/src/frontend/imgui/authoring.rs`. No specialized UI Prefab gate changes.
- C owns external Editor structural workflow tests and native ordinary-widget trajectory additions;
  production implementation remains read-only for C until final independent review.
- Root owns ADR/guide/plan/memory integration and any explicitly coordinated integration corrections.

Selected policy: stable-ID Move/Reorder uses operation-relative placement preservation. Explicit
Delete requires a Complete document with no Prefab anchors; full candidate validation rejects
surviving effective inbound local references. Exact Restore is history-only; Degraded empty-insert
Undo retains its established narrow inverse path. Entity presence authority is separate from a
component field's Edit capability. ADRs 0090/0118 record the refinement before implementation.

An integration regression also requires Reopen to discard unsaved topology after a captured Save,
including uncertain publication. The concrete persistence Host supplies an existing observed
source digest and at most one validated uncertain-candidate digest. Exact source readback still
uses full current-basis classification and owner admission; external changes retain exact unknown
preservation. ADRs 0090/0047 record this finite source proof, without a second saved document.

Before implementation delegation, assign disjoint file ownership. Researchers/reviewers do not
run Cargo, format, stage, commit or edit high-context instructions. Use the same lanes for the
independent final review; a worker cannot be its own sole reviewer.

## Execution

1. Trace current generic operation restrictions and existing structural APIs/tests. Produce a
   concrete operation policy and identify the smallest complete external editing trajectory.
2. Implement the document-owned admission/transaction path, then tooling and native frontend
   routing. Remove obsolete blanket gates when their replacement contract is established.
3. Test meaningful positive/negative cases: unavailable records, references, provenance, stale
   actions, cycle/order rejection, exact Undo/Redo, Save/Reopen and Check/Play boundaries.
4. Run focused nextest suites serially and actual Windows native acceptance for changed UI paths.
   Use docs/verification/native-editor-qualification.md; do not infer other-platform qualification.
5. Independently review correctness/contracts/maintainability, fix blockers, update guides and the
   implementation ledger with actual evidence, commit precise files and push main normally.

## Completion And Follow-up

The ordinary generic structural workflow is implemented and qualified at its document, public
application and actual Windows native widget seams. Explicit refusal covers unavailable-record
deletion, unresolved Prefab instance closure and specialized UI structure outside its domain.
The generated desktop starter reaches strict Check and real external Play after structural
Save/Reopen. The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md)
records regressions, passing cohorts, final workspace/minimal feature checks and exact limits.

Source and owning contracts are committed together at `e673693`. Documentation closure is normally
pushed with that implementation; the delivery message verifies remote main's exact object ID.
Subsequent presentation/measurement/persistent-game stages retain their own acceptance and are
not claimed complete through this structural evidence. The user's selected next presentation
policy is recorded below. Tile tools remain deferred.

Source context: [module/goal review](../audits/2026-10-20-module-goal-alignment-review.md),
[authoring interface design](../architecture/authoring-interface-ergonomics-design.md),
[architecture map](../architecture/README.md).

## Next Presentation Consumer: Read-only Design

The bounded parallel source study identifies a small optional fixed world-canvas configuration,
lowered through the existing Render/plugin and project profile owners. Reuse `DisplayMapping2d`
and `IntegerFit`: whole physical scale when it fits, centered nearest-sampled bars, and the existing
fractional full-image downscale for windows smaller than the canvas. Keep real `WindowResolution`,
platform-logical input and fractional simulation truthful.

The public packet currently omits an internal image descriptor and ordinary WGPU execution expects
a window target. One closed runtime transaction must admit a frame once, render world content to
the canvas, composite into the actual window, then present. Do not call ordinary `render_packet`
twice for the same frame, expose Editor execution as game API or add a render graph. Publish input
mapping only after successful matching window presentation; invalidate it on resize/scale/device
changes. Pointer inversion composes panel-to-canvas with the extracted camera and rejects bars,
nonfinite coordinates and maximum edges.

The user selected a fixed low-resolution world with native-resolution UI drawn after world
composition. Keep the UI's real-window input/accessibility mapping unchanged. A future
low-resolution HUD additionally needs separate raster and presented/input/accessibility transforms
under existing surface generations and receipts. Faking a smaller window resolution is incorrect.
Neither presentation route is implemented or qualified by this structural stage.

Source anchors: [presentation](../../crates/nara_render/src/presentation.rs),
[frame](../../crates/nara_render/src/frame.rs), [runtime backend](../../crates/nara_render_wgpu/src/backend.rs),
[UI surface](../../crates/nara_ui/src/runtime/surface.rs),
[UI product mapping](../../src/ui_presentation/surfaces.rs). Pinned Bevy's
[pixel grid example](../../repo-ref/bevy/examples/2d/pixel_grid_snap.rs) and Godot's
[Window](../../repo-ref/godot/scene/main/window.cpp) support separate internal raster and displayed
input ownership; their policy is comparison evidence. Future acceptance needs real checkerboard/
bar readback, fractional HiDPI/undersized resize, picking/button input, stable allocation/retirement,
failed-present authority and an ordinary configure/Check/Play/resize/Stop/restart consumer.
