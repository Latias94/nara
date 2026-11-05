# Styled Source Workspace and Reusable Control Recipes

**Status:** Implemented; bounded shared, independent and Windows/DX12 qualification below.
The component catalogue, broader platform support and R11a/e cost budgets remain open.
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

Editor integration also exposed a dependency-identity error in the new field measurement:
constructing a fresh custom-layout description during every unchanged composition invalidated
the original Inspector/form snapshot comparison. The field now retains one geometry-keyed
description; compatible color/focus redraws reuse it, while changed measurement replaces it.
Old authored snapshots keep the original immutable description. The existing exact-frame reuse
and source/history pressure tests remain acceptance requirements.

Native image compatibility changes when a menu admits or releases window-modal scope. The old
image's `OutputReplaced` cancellation must retire its original input, not cancel a successor
already admitted by the presenter. Test this through the actual native Host with repeated paints
and focus restoration. Menu-outside dismissal also uses the actual popup viewport inside its
panel image: a click on a blocked Shell button, empty Shell space or another panel closes the
menu without applying the underlying action.

Final review also separates local Hierarchy editing from Inspector Apply. Enter keeps the
Hierarchy field's original local input lifetime; Inspector Enter still uses its original prepared
field action. When Escape finishes and blurs a local filter, reduce the original interaction
cancellation before clearing native text services. Otherwise the kernel can keep a focused key
whose field has no token, preventing a second click from admitting text. The native trajectory
must cover Enter, same-field refocus and further input in both ordinary and specialized fields,
plus Escape and same-filter refocus, without a fixture-injected focus reset.

The reference thousand-declaration trajectory exposed a separate information-retention issue:
moving every detail into the virtualized source prefix made the source count disappear during
scrolling. Pin the count when the viewport can accommodate it; retain a scrollable fallback in
tiny viewports. Preserve the complete original filtering, wheel, unknown-data, Inspector draft,
document-switch, history and persistence assertions rather than relaxing them for the new style.

The final shared review found that an unconditional min-content layout probe could reject a
paragraph whose final allocated width fits: the probe first broke at word width and exceeded the
output line ceiling. A concrete `min_content_width` query now observes the same subtree and shaper
without producing drawable text. Direct layout and final measurement/paint retain the original
line limits. Observation entries remain separate from drawable entries within the same bounded
cache, with full source/glyph/layout-work accounting and no exploratory geometry in the final
cache. Shared regressions cover a 257-word paragraph and one-line wide-versus-narrow controls.
The isolated 20-wrapper/one-glyph regression then reproduces excessive recursive work when
every width-probe dispatch clears its cache (`2124e270-f4db-4737-9355-ae6e84062f36`): 20,001
reached operations exceed the caller's 20,000 allowance. One private per-node cache now carries
its measurement mode and clears only when that mode changes. Existing custom-subtree final-layout
recomputation remains active, and compatible requests within a mode still charge work on hits.
The correction adds no second retained layout arena or public cache owner.

The first actual Windows/DX12 complete trajectory passes in 68.920 seconds
(`3b4d0c93-164e-45e7-b5b8-eafaa789e826`). It retains all original source/UI/retirement assertions
and adds the complete Inspector, active-preedit appearance adoption and native menu scope/focus
cases. GPU output readbacks at the actual 1.5 native scale expose coherent field geometry but
also unnecessary emphasis on disabled primary actions and clipping in enlarged tool groups.
Refinement neutralizes disabled fill, keeps ordinary destructive entry points quieter than their
confirmation, and makes groups choose columns from actual child min-content widths. Popup width
similarly grows to actual min-content before fitting available bounds. The refined native result
then passes in 69.573 seconds (`6e432cb5-01bf-4367-8800-ba70b746d849`); final review adds the
input and intrinsic-measurement corrections above. The final qualification below supersedes
these intermediate runs and retains their visual findings.

## Owner-local work observations

The existing Inspector probe and current Shell regressions pass 27/27 after the reference/public
cohort's final work assertion is reconciled with compatible text reuse. Width-dependent wrapped
text and target-scale changes reshape; color/hover and an unchanged immutable font policy do not.
Every candidate still performs source/output admission. The final shared widget cohort passes
39/39, including actual min-content group/popup fitting and the retained field description.

These are serial **debug** CPU observations on the current Windows machine with other workloads
uncontrolled. They are not release comparisons, GPU timings, input-to-presentation latency,
memory measurements or R11e budget acceptance. This recorded checkpoint predates the final
intrinsic-width observation correction; later correctness runs do not remeasure these timings.

| Probe | Samples | P50 / P95 / P99 | Observed work and remaining cost |
| --- | ---: | --- | --- |
| Unchanged complete Inspector | 512 after 16 warmups | 0.233 / 0.344 / 0.453 ms | Reuses all 512 original frames with zero new compiler work; still builds 512 descriptions / 23,552 authored-node occurrences |
| Inspector local edit plus composition | 128 | 7.082 / 7.744 / 8.318 ms | 128 new compiler frames; 6,144 layout-node occurrences; one new plain-text layout request; 2,960 compiler raster attempts and 3,220 full prepared-glyph scans |

The local edit probe's text-engine counters exclude Parley's internal editing work. Zero editor
`layout_attempts` is not zero editing cost. These results keep description construction, bounded
full-history/text work and prepared-window raster/scan costs visible for the later R11e comparison.

The separate three-document Shell probe records one sample per transition, not distributions:
cold 5.499 ms, unchanged 0.054 ms, palette 0.789 ms, width 0.771 ms and text-scale 3.814 ms.
Cold compilation creates 36 nodes, invokes 300 measurement callbacks and shapes 25 requests.
Unchanged composition enters no compiler work; palette and Clip-only width changes each rebuild
36 nodes / 300 callbacks but shape zero new requests. Text scale reshapes 25 requests. Thus the
bounded shaping cache does not establish incremental layout, cheap arbitrary composition or
retained GPU updates. Packet/planner, upload, GPU, allocation/RSS and end-to-end budgets remain
under the original R11a/R11e owners.

## Final qualification

The corrected shared text/kernel/widgets/presentation and game-UI cohort passes **509/509**
(`94afc12c-1e02-46f5-ab2b-92ced394b128`), including the deep auto-layout regression. Its isolated
red run above establishes the owning cache defect; no higher work ceiling was required for green.
Earlier shared checkpoints pass 378/378 before the mode-cache correction and 261 with two
fixture-identity corrections during initial implementation. These cohorts overlap.

The complete default Editor library and selected public integrations pass **494/494**, with
61 ignored cases (`eb720ef2-a853-47df-850a-007d83a94615`). The earlier 493-case run passed 492
and exposed the work-pressure assertion corrected above. The clean 494-case run includes the
new pinned source-count regression and precedes the final private mode-cache correction; the
affected frontend and native reruns qualify that final integration separately.
The final affected frontend cohort passes **269/269**
(`8043b62d-2e7f-49c1-88b4-614378667722`). The original reference native cohort initially passes
38 of 39 in `bbe91ac4-209e-42c9-a89f-2ebcd1404764`, exposing the source-count regression above.
After its owning correction and the final shared cache change, both complete native Hierarchy
trajectories pass **2/2** (`d66bfa9c-d86e-46b0-98a9-2cfb47b944a1`): ordinary Scene/Prefab
arrangement/removal/history/persistence and the 1,024-declaration filter/wheel/text-lifetime
workflow with unknown source records and retained Inspector input. The broader 38-case pass
and final two-case retry are separate runs; neither is represented as one clean 39-case run.
The final no-reference Editor library passes **245/245**, with three ignored native cases
(`e89d7cd4-bcac-4d70-9582-9a40087c7f3f`), under `--no-default-features`. This scope includes
the drawn Shell and adapter correctness tests without initializing the Dear reference.

The final actual Windows/DX12 drawn trajectory passes **1/1** in 88.396 seconds
(`f258a6bb-1cfe-4f76-b502-21717f0a2c30`). It preserves the full original source/UI/4096-byte/
history/persistence/retirement/reconstruction path and adds local Enter/refocus, filter
Escape/refocus and Inspector dirty-Discard/refocus without synthetic interaction resets.
The exact original Shell, Hierarchy and Inspector images all participate in real native
composites. Pointer, keyboard and IME events are injected through production adapters;
this does not qualify real OS IME delivery, system clipboard or native accessibility.

Final fixed-content artifacts are the original GPU panel-output readbacks and text metadata
in local `nara-styled-qualified-0114ed98942c4d3b9093ad8c449dc26c` under the platform temporary
directory: `drawn-normal-inspector`, `drawn-compact-large-text-preedit` and
`drawn-anchored-actions-menu` in PPM plus lossless PNG conversion. They use the actual 1.5 native
scale, 1800×1650 physical window, Fira Sans Regular and configured SimHei fallback. Review confirms
consistent field/control typography and alignment, quiet ordinary removal entry, neutral disabled
primary actions, wrapped enlarged toolbar groups, retained preedit and visible menu focus.
Source IDs, explicit workflow controls and diagnostic detail still make this an engineering
source workspace; this is not approval of a finished broad component catalogue.

The native path exercises both densities and 1×/2× user text scale at that actual OS scale.
Shared packet/geometry tests additionally exercise 1/1.25/1.5/2 scales; no native monitor/DPI
transition, Linux/macOS, color-emoji/font-family selection or forced-preference qualification is
claimed. Readbacks compose original panel texels at their actual bounds, not the final OS
swapchain. Capture and unoptimized trajectory duration are not performance measurements.

The independently locked UI-library consumer passes **37/37** with all optional features
(`0c3112e8-7bf6-4af9-a911-c5f1ea768481`). Its seven styled-property-row cases retain actual
fallback metrics, full text/edit history, scoped override removal, sibling isolation and real
child measurement. The same library adds no Editor/ECS/native owner; these CPU results remain
separate from first-party native visual review.

The independently locked reference panel consumer passes **46/46**, with four ignored native
cases (`346c63b8-cfb6-4f9b-8147-d39e4cefe1b5`), preserving its public source/import/instance and
exact-result assertions. Its separately selected real Windows/DX12 embedded Host startup,
paint and ordinary-close trajectory passes **1/1**
(`3d5e89f4-af67-4f4a-a92e-e556d6565d8c`).

The independently locked no-reference public UI Host consumer passes **14/14**, with two ignored
native cases (`89fc54bc-8629-470d-b11e-5e17ef67c92d`). All three examples build, including the
ordinary public `drawn_editor` entry. Its actual Windows/DX12 Nara and independently painted
frontend trajectories pass **2/2** (`0b205262-a728-4945-a8dd-86118b04559e`), retaining their
original complete source/history/persistence, stale-route/output refusal and physical-retirement
assertions. The latter frontend adopts no Nara widget tree; this proves the existing public
image/semantic Host remains consumable alongside the first-party recipe changes, not compatibility
with a named external toolkit or automatic extension reload.

Correctness Clippy passes for all targets/features of Text, kernel, widgets, UI presentation,
game UI and Editor. Existing non-correctness warnings remain; this unit does not claim a
warning-free repository. The shared typography/recipe prerequisite is `fc7824a`; its final
measurement/reuse correction is `c569c94`.

Both default Editor binaries and the separately built `--no-default-features` production
`nara-editor-drawn` binary build with the pinned toolchain and locked offline graph. Public-entry
normal/build/feature dependency inspection excludes Dear/ImGui packages, `imgui-reference` and
`editor-imgui`. These are build/dependency results; this unit's actual event-loop evidence is the
complete native trajectory above, while ordinary executable startup/close retains the preceding
independent-workspace unit's scope. Root and three affected independent-workspace formatting checks
and changed-document file targets pass.

Independent final reviews close the concrete input, stale-output, measurement and cache-work
findings described above. This unit supplies the first shared recipes and complete property
workspace, with original source and physical lifetime authority. It leaves broad component
catalogues, Editor information design, persistent preferences, font-family/style choice,
platform accessibility, docking/multiple windows, remaining Editor tools, extension code
reconstruction and R11a/e end-to-end budgets open under their existing owners. The full foundation
Goal remains active.
