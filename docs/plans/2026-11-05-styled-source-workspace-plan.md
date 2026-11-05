# Styled Source Workspace and Reusable Control Recipes

**Status:** Implementation in progress; visual, native and cost qualification pending
**Parent:** [R11b visual prerequisite](2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite)
**Owners:** Shared text, controlled widgets, Editor presentation and the existing native Host

## Complete consumer

Extend the independently drawn source workspace with a coherent document shell, toolbar,
source/hierarchy rows, the existing complete property Inspector and an anchored action menu.
Preserve all original source preparation, exact result, history, persistence and field-capacity
behavior. A separately authored decorated row consumes the same style and child-measurement
interfaces without Editor, ECS or native dependencies. Keep the reference composition selectable.

The [pinned primary-source study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-style-layout-and-component-library-readiness)
and [design inventory](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
own the comparison and requirements. This unit selects concrete recipes through actual consumers;
it does not adopt another toolkit's state graph, a CSS parser or a universal style registry.

## Ownership and implementation

- `nara_text` owns validated typography, actual fallback faces, line geometry and atomic local
  editor reconfiguration. Size/line-height changes admit the current display, composition origin
  and bounded history before adoption. Published snapshots keep their original geometry/fonts.
- `nara_ui_kernel` accepts that typography and retains measurement, clipping, paint, hit and
  semantic geometry. It owns no Editor palette, user preferences or font discovery.
- `nara_ui_widgets` owns immutable control recipes over explicit appearance inputs. A caller
  passes the effective nearest scope; local overrides are ordinary owned values. Recipe
  resolution changes no state, installs no listeners and emits no domain effects.
- Controlled field appearance and padding are adopted together with their input geometry;
  Point, selection, caret and IME cannot use a second hard-coded inset. Existing text and
  focus owners survive compatible appearance changes.
- Extract the original complete Inspector/draft implementation from the Dear adapter. Both
  frontends share it; no reduced property editor, copied preparation path or extra history.
- The drawn composition owns page arrangement and component selection. Reuse the original
  semantic panel/native image/presenter/WGPU owners and exact presented routes.

## Initial numeric recipe

These are candidate values for fixed-content visual review, not proven accessibility or cost
budgets. All dimensions are authored logical units before the original Host scale transform.
Density changes spacing; text scale changes typography. Neither applies native DPI a second time.

| Role | Initial value and rule |
| --- | --- |
| Body/control text | 14 font size, 20 line height; multiply both by the explicit user text scale |
| Secondary/caption text | 12 font size, 18 line height; preserve full semantic values |
| Section/title text | 16 font size, 24 line height; no synthetic font weight |
| Normal control | At least 28 high; body line box plus 4 vertical units per side |
| Compact control | At least 24 high; body line box plus 2 vertical units per side |
| Small/large variants | Adjust minimum and vertical padding; never reduce the admitted text line box to fit |
| Inline padding | 10 normal, 8 compact per side; local component overrides are explicit |
| Inter-control gap | 6 normal, 4 compact; section spacing 12 normal, 8 compact |
| Panel inset | 12 normal, 8 compact; bounded content uses actual measured remaining space |
| Radius and stroke | 4 radius, 1 stroke; focused/invalid states reserve the same border geometry |
| Icon envelope | 16 logical units initially; optical alignment is separate from hit geometry |
| State layering | Disabled first for behavior; variant/selection, hover/press, invalid and focus have explicit paint precedence; focus retains an independent indicator |

Use neutral background/panel/raised surfaces, clear primary/muted text, a restrained accent,
and separate focus/selection/error cues. Exact colors follow the appearance value consumed by
the actual page. A library can supply another palette or its own additional roles without
changing a central widget enum. Pending/loading and read-only must remain behaviorally distinct
from disabled, even when they share a visual recipe.

## Font, metric and state readiness

Use the explicitly admitted Fira Sans Regular face for the drawn workspace's primary text;
keep the original bundled Bold choice available for its existing consumers. Record font bytes,
face index, license and content digest in the text assets' provenance. A title's larger size does
not authorize synthetic weight. The Host's finite ordered fallback policy remains explicit.
The real Noto CJK test subset checks fallback metrics without requiring a platform font; native
Chinese acceptance still records the actual configured Host font. Missing glyph diagnostics do
not silently authorize replacement source text or pretend to prove complete language coverage.

Treat the numeric control height as a target/minimum allocation. Text size is neither a measured
line height nor an ink bound. An automatic font layout must be measured under its actual faces;
explicit line spacing can be smaller than font ascent/descent. A field's original geometry owns
its full line block, draw offset, outer baseline, minimum box, scroll, Point, selection, caret and
IME projection together. Parent layouts consume that actual measurement instead of reconstructing
a baseline from font size or padding. Explicit authored clipping remains an allocation choice.
The pure `UiControlMetrics` constructor requires an absolute line height; it cannot infer automatic
font metrics without a concrete text owner.

Selected, focused, invalid and active pointer states may coexist. Preserve a selection fill across
hover/press and an independent focus border; invalid fields retain an error surface while focused,
with the original visible validation feedback. Reserve identical geometry across these states.
Disabled controls retain distinct behavior and text emphasis. A public semantic/accessibility
selected or invalid state remains an explicit follow-up; color tests alone are not its proof.

| Capability | Current implementation/qualification target | Later complete consumer |
| --- | --- | --- |
| Typography | Finite size and automatic/absolute line height, real admitted fallback metrics, atomic local reconfiguration and immutable prior paint | Multiple named family/weight/style choices, variable axes, color emoji and project font loading |
| Appearance scope | Caller-owned immutable effective palette/density/text scale; explicit local override and exact equality | Persistent/versioned Editor preferences, OS theme/contrast/forced palette integration |
| Geometry | Measured controls and groups, actual field baseline, clipping/scroll and available-bounds popup fitting | General min/preferred/max layout vocabulary, responsive docking and multiple windows |
| State cues | Concurrent focus/selection/validation with unchanged hit geometry and exact actions | Complete accessibility state vocabulary and native platform qualification |
| Reuse | Bounded compiler-owned compatible plain-text layouts; source/output admission applies on every candidate | Measured incremental layout/paint, retained GPU updates, process RSS/frame-tail budgets |
| Icons and effects | Existing explicit image/custom geometry seams; no mandatory icon font or synthetic weight | Qualified icon resources/optical alignment, animation interruption, reduced-motion and effects recipes |

This inventory sets the intended contracts early while keeping unsupported capabilities visible.
It does not require a central registry, CSS/DOM compatibility, a second font service or a new
interaction/history owner.

## Lifetime, invalidation and fallback

Effective appearance equality includes typography, metrics and colors, not only a revision.
Color-only adoption preserves compatible text/layout; typography or density changes invalidate
their dependent measurement and presented geometry. Repeat composition cannot recreate fields
or lose preedit/history. Native output replacement and cache reuse keep original route admission.

Fonts retain the finite ordered Host-selected policy under ADR0031. Variable axes, synthetic
weight/style, project font loading and color emoji remain unsupported until their actual identity,
raster and complete-consumer paths qualify. Empty/unavailable values, long international text
and invalid scale/configuration have explicit outcomes rather than silent source truncation.
The native accessibility and OS preference integrations remain separately qualified capabilities.

An action menu captures the exact action/query basis, scope and anchor. Its current measured
geometry fits available bounds, with defined fallback/scroll, dismissal and focus restoration.
An old anchor, queued result or presented frame cannot retarget a successor control.

## Verification

1. Shared tests qualify real line metrics/baselines, mixed fallback, atomic typography adoption,
   full editing history/preedit and immutable prior paint. Verify default automatic metrics and
   all existing default consumers as well as explicit line height.
2. Recipe/state tests exercise meaningful combined states, independent text-scale/density,
   scoped override removal and rejection. The independent decorated row uses public interfaces,
   real measurement and text editing; compile/paint is not native acceptance.
3. Preserve complete reference and drawn source/Inspector CPU workflows, all source kinds,
   nested values, tools, source preservation and 4,096-byte fields. Add style adoption during
   focused preedit and history, narrow/wide layout and anchored-menu boundary controls.
4. Run the actual drawn Windows/DX12 trajectory with complete old assertions, adding Inspector
   editing and menu actions. Exercise 100/125/150/200% scale and both densities where executable;
   state actual tested combinations and OS-input limits. Verify retirement/reconstruction.
5. Review actual fixed-content captures for alignment, hierarchy, clipping, density, state cues
   and long/CJK text. Record corrections; an engineering screenshot is not visual acceptance.
6. Attribute style/composition, measurement/layout, shaping/raster, packet/upload and retained
   work for idle, hover, one edit, resize and appearance changes under R11a/R11e. Run serial
   nextest, relevant independent consumers, correctness Clippy, builds, formatting and links.

Full Editor replacement, default switching, docking, multiple windows, broad effects and code
reload retain their existing complete-consumer requirements. This unit cannot close the full
foundation Goal or substitute design coverage for actual implementation evidence.

## Integration checkpoints

The shared prerequisite currently passes the complete 83 text tests. Kernel/widgets/render
execute 261 tests: 259 passed initially; two new render fixtures lacked mandatory text keys,
and both pass after supplying those identities (`4082f66f-6393-41a5-b66e-dc0ae4fb6cf7`).
The run includes the bounded plain-text cache, actual fallback/short-line field measurement,
concurrent state cues, popup fitting and default opaque role contrast. The independent styled
consumer passes all seven cases (`eab18162-e75e-4f96-a29f-02ff68f7a9ff`), including real CJK
fallback baselines and complete 4,096-byte composition/history across density/text scale.
These CPU observations do not yet qualify the current native workspace or visual appearance.

Independent review drove owning corrections rather than reduced field capacity: automatic
metrics cannot be guessed from font size; actual field measurement supplies the outer baseline;
prepared text's draw origin is independent of its content clip through packet lowering; color
changes reuse bounded compatible plain layouts; focus/selection/validation cues coexist. Menu
navigation must select from the full enabled action order, reveal before requesting focus in
the successor presented frame, and leave wheel offsets unchanged during hover/reconciliation.
A compatible appearance change cannot hide and blur a live small-viewport filter. Ordinary
Inspector input/clipboard/stale-action refusals need local feedback, and component terminal
results must retain their original submission ID. Editor/native corrections and their full
verification remain in progress.
