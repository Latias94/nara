# Allocation-Driven Fields And Measured Property Composition

**Status:** Complete within the qualification below; full foundation remains active
**Parent:** [R11b visual/component prerequisite](2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite)
**Owners:** Existing text field, immutable child layout, shared recipes and Editor controls

## Complete Consumer

Compose an icon/label, retained editable value, ordinary action and inline feedback under a
measured property row in the independently drawn Inspector and an independently authored
library consumer. Use actual admitted text baselines, finite/narrow allocation and both
densities with enlarged text. Focus/edit/IME, Apply, Undo/Redo and Save/Reopen retain their
original owners. Compatible appearance adoption must not move the draft or lose preedit. Native
resize preserves the draft while the existing mapping owner retires old native input; refocus
must use newly presented geometry. No input may use geometry that was never presented.

## Owning Correction

At the unit's start, `UiTextField::authored_node` captured viewport/scroll/caret/selection from a
width supplied before child layout. A measured parent could not allocate a different final width
without making captured paint and later Point/IME disagree. Replace that internal representation
with immutable allocation-aware field/viewport descriptions. The supplied width becomes the
preferred intrinsic width; a definite final allocation supplies the actual geometry. Keep the
same retained editor, complete buffer/history/provenance and original prepared-text pipeline.
Point/IME consumers read the actual accepted field rectangle and use its width.

Independent review also exposes a distinction in the shared container contract: ordinary
`measure` preserves authored widths, so a later exact narrower placement can wrap to a different
height. Add `measure_at_width` to the original child-measurement owner, using the compiler's exact
known width while height remains authored/intrinsic. Use it in property/icon rows and the existing
control grid/popup. Preserve ordinary available-space semantics, root-local poisoning, request
accounting and final placement. Fixed pixel/percentage child-width regressions must prove that
final text cannot overlap later siblings.

The reusable row measures actual label/editor/action/status children through the original
`UiLayoutChildren`, with explicit narrow stacking and real baseline alignment. A decorative
icon plus text uses captured original mesh geometry and explicit logical optical placement;
it creates no icon-font requirement, hit target or independent semantic action. The library
does not own document fields or a global component registry. Compatible descriptions retain
their immutable layout identity rather than allocating fresh callbacks on every composition.

The [Design Kit v0.4](../design/editor-reference/README.md) supplies graphite/teal direction,
28/24 control minima, optical-icon and combined-state pressure. Its sRGB HEX and proposed
13/20 typography require reconciliation with actual rendering and admitted faces. Preserve
qualified defaults until a complete comparison justifies changing them; no generated-image
pixel measurement or JSON loader can replace these owners.

## Acceptance

- Final allocation narrower/wider than the preferred field width preserves full values,
  actual glyph/caret/selection/Point/IME geometry, short-line ink and fractional native scale.
- Repeated intrinsic/final measurement is effect-free and honestly charged. Existing source,
  glyph, mesh, raster, layout-work and output limits remain unchanged; refused allocation
  uses original root-local failure/recovery without mutating local editing state.
- A shared measured property row supports long/mixed-font labels, complete 4,096-byte values,
  actions and status at wide/narrow widths without hand-maintained control rectangles.
- Independently consume custom icon/label/value composition with local appearance override;
  remove the override and preserve siblings, keys, buffers, preedit and local history.
- First-party Inspector retains every existing structured/reference/component/recovery/source-
  tool workflow and actual native source/history/persistence/retirement assertions.
- Review fixed-content native captures with the actual fonts/scale, and record performed
  versus unqualified platform/appearance cases. Functional correctness is not visual approval.
- Run appropriate focused shared/editor and independent nextest, actual affected native
  trajectories, correctness Clippy, locked builds, formatting and document checks serially.

General icon catalogues, asynchronous font/icon loading, selectable read-only fields, docking,
multiple windows, extension reconstruction and R11a/e cost budgets remain with their existing
complete consumers. This unit adds no renderer, mutation/history owner, CSS engine or public
cache arena. The full foundation Goal remains active.

## Implementation And Review Corrections

`UiTextField` now captures immutable outer-field and inner-viewport descriptions. Exact allocation
chooses scroll and caret/selection placement; the complete prepared source, original keys, field
token, local history and real line baseline stay with the original editor. The new child
`measure_at_width` operation uses existing exact known dimensions without changing ordinary
available-space hints or adding a layout phase. Its invalid geometry, post-placement requests and
repeated-cache work keep the original root poison and candidate limits.

`UiPropertyRow` and `UiIconLabel` are ordinary immutable child-layout recipes. The independent row
removes its separate text premeasurement and private placement algorithm. Its custom bookmark
mesh remains caller-authored. The first-party projected scalar row uses an original retained
field, one original Apply, a status caption and a decorative twelve-unit property mark with an
explicit baseline offset. Remaining actions use the measured control grid; structured/reference/
tool/boolean and recovery consumers retain their original reducers and domain authority.

The first full Editor pass exposed a real capacity regression: compacting unprojected recovery
drafts admitted more visible chrome before the original 4,096-glyph output ceiling. Such drafts
keep their prior full-width recovery presentation; compact measured rows apply to current scalar
projections. The complete 64-draft recovery test retains diagnostics, queued/pending submissions,
atomic Discard, Copy and fresh-control assertions. No source, field, history or glyph limit grows.
The wrapped footer also moved a structured Optional action below the CPU fixture's viewport.
Its helper now scrolls through the original owner, presents and locates the same direct semantic
field/action before continuing the unchanged conflict/Reapply/exact-result workflow. A measured
width derivative uses a small floating-point tolerance rather than exact equality.

Native Copy exposed a further interaction dependency: focus loss cancels preedit and changes
`Composing...` to `Modified`. Natural status widths could change a field's hit rectangle before
pointer release. The existing surface-geometry capture policy then correctly cancels the old
capture, even if Copy's own rectangle stays unchanged. The Inspector now measures the eight
finite status captions through its original TextEngine once per caption typography and retains
their maximum logical envelope under the owner's immutable font policy. Only the active caption
enters the snapshot; there are no hidden placeholder glyphs or new candidate/output budgets.
Palette changes retain compatible metrics. Threshold regressions require identical field/Copy
geometry through composition cancellation on both sides of inline/stacked layout. Keep the real
native press/paint/release Copy workflow and original capture policy; moving the test pointer
mid-gesture or weakening continuity would conceal the defect.

Two CPU font-reconstruction fixtures require simultaneous visibility of multiple field actions
before one ordered dispatch. They explicitly allocate a taller viewport rather than requesting
an action omitted from the current presented projection. They keep all font replacement, original
submission-ID order, conflict/refusal and retained-history assertions. Ordinary/narrow viewport
scrolling remains covered by the separate CPU and real native consumers.

The independent consumer's internal prepared-text node now has complete text geometry plus an
inherited viewport clip. Its containment regression checks that text's visible intersection,
while ordinary controls retain raw-rectangle containment and complete source/paint-clip assertions.
The kernel's semantic rectangle/clip contract is unchanged.

Pinned GPUI input/Editor/text/cache mechanisms and the complete design reference inform these
choices; see the existing [source study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-allocation-and-encoded-design-colors).
Bevy/Godot color comparisons preserve ADR0092's linear SDR contract and explicit encoded-byte
conversion. Current typography/palette defaults remain qualified separately from future kit
adoption. These changes do not establish a frame-time, heap-byte, HDR or complete visual-library
budget.

## Qualification

Shared kernel/widgets pass **231/231** (`97f05dcf-04f3-4158-a3f9-c88738ebe243`), including exact
pixel/percentage child allocation, repeat-probe poisoning/work, full buffers, line blocks and
mixed-font preedit. The independently locked UI consumer passes **39/39**
(`b21d779b-c9e0-4da3-b533-1adaaa0e97ac`), including default/local scope restoration, sibling
isolation, custom icon composition, real allocation and unchanged-description/warm-text reuse.
These are work counters and correctness observations, not time or heap-allocation measurements.

The final default Editor/public cohort passes **503/503**, with 61 excluded native/explicit probes
(`c6b07805-cbff-4da1-9f43-1eea0c290d80`). The independently drawn no-reference library passes
**251/251**, with three ignored native/explicit probes (`0567adff-a645-44ad-81bb-2b78aa330114`),
before the final status-envelope correction. Final-source native qualification follows below.
The targeted envelope/recovery run passes **33/33** (`eb9802bf-aa0b-4d04-a9b5-0c3beb681084`):
30 CPU correctness cases, two explicit debug CPU work probes and the full original native
Copy/64-draft capacity trajectory. Probe execution does not establish a matched performance gain.

The final-source complete drawn Windows/DX12 trajectory passes **1/1**
(`9ef69b5d-684b-49a4-9a92-2637347492e4`), after the status-envelope correction. It retains every
original ordinary/specialized source,
operation, history, persistence, recovery and physical-retirement assertion and adds actual
960/1360 logical-width draft/input/local-Undo checks. Before refocus, a delayed IME Commit and
Backspace after resizing a live preedit change no draft or document. Compatible appearance
adoption separately retains original preedit at normal/compact density and 1x/2x text scale.

Five fixed-content original GPU panel captures cover normal, narrow, wide, enlarged-text preedit
and the anchored menu. Actual native scale is 1.5; narrow/wide outputs are 1440x1650 and 2040x1650
physical pixels, using the admitted Fira Sans and SimHei faces recorded in capture metadata.
The initial 1400-logical-width capture exceeded the existing 2048-pixel artifact
ceiling; the same width-change workflow now uses 1360 without increasing that limit. Original
PPM/metadata and lossless PNG transcodes are local artifacts, not tracked generated assets.
Visual inspection confirms the property label/icon/text baseline, distinct actions and narrow
stacking under enlarged text; the broader engineering Shell remains information-dense and its
fixed-height large-text clipping needs separate workspace visual work. This is not full visual
approval. Captures read original panel output and compose texels at their actual mappings; final
OS swapchain, system IME/clipboard, font-family changes, HDR and other native platforms remain
unqualified. Test duration and screenshot work are excluded from performance claims.

The final reference-frontend native cohort passes **24/24**
(`3da2edb1-d08a-446d-9109-317a167a1f7d`), alongside the separately rerun complete Copy/capacity
trajectory above. It preserves ordinary Scene/Prefab, components, structured fields, references,
keyboard, scrolling, source tools, Canvas and original Save/Reopen assertions. These two runs
cover the same selected 25 native cases after the status correction; they do not qualify the
reference adapter's gamma parity or all native Editor features.

Locked offline builds pass for both default Editor binaries and the separately built
`--no-default-features` production drawn binary. Correctness Clippy passes for all targets/features
of kernel, widgets and Editor; existing non-correctness warnings remain. Independent final reviews
close exact-width height, capture geometry, shared field/recipe and Editor/adapter findings.

The independently locked reference panel consumer passes **46/46**, with four ignored native
cases (`bc433eb4-2979-4244-a399-d08e2d4ae774`). Its public source/import/instance and exact-result
assertions remain separate from first-party layout changes and native visual qualification.

The independently locked no-reference public UI Host consumer passes **16/16**, with three ignored
native cases (`b31cca45-0d91-4900-bc6f-0aecd2c82e9c`); all three public examples build. Its actual
Windows/DX12 Nara, independent foreign frontend and observed-operation trajectories pass **3/3**
(`0da77235-c9e5-4b12-a951-1fc38784e9f2`). They retain original source/history/persistence,
observation/input lifetime refusal and physical-retirement assertions. This demonstrates existing
public extension/agent-operation seams remain independently usable, not attachment compatibility
with GPUI itself or automatic code reload.

Root and all three affected independent-workspace formatting checks pass. All 1,389 changed-
document file targets resolve, final diff whitespace checks pass, and no new inline Rust code
comments or registry/memory shards are introduced. The full foundation Goal, complete Editor
visual/component catalogue and R11a/e performance targets remain active follow-ups.
