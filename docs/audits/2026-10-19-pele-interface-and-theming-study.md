# Pele Interface And Theming Study

**Status: withdrawn as Nara design input.** The user clarified on 2026-10-19 that the URL was
sent in error and must not guide this engine. This retained research is historical only; its
recommendations and plan disposition below are superseded by that correction. It created no
Pele dependency or Pele-specific engine implementation. Current architecture navigation and
the correction plan no longer use it as evidence.

Date: 2026-10-19. This is comparative research for the
[foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md),
not an Accepted architecture decision or additional implementation scope.

Official documentation and upstream source were browsed directly. Source observations are pinned
to Pele commit `44da152bbb79b91f03e68016f42499bb25bf6e82`; its package declares version `1.6.0`.
Nara observations describe the active correction working tree based on `7bec88f`, including
uncommitted U12/U13 changes. No dependency installation, Rust changes, Cargo commands or Pele
runtime benchmarks were performed.

## Project And Attribution

Pele renders Mermaid diagram syntax to SVG and was developed for Obsidian. It is a focused
diagram-rendering library, rather than a general UI toolkit, ECS or Editor extension framework.
Its website's performance comparisons are upstream claims; this study does not reproduce them.
See the [official introduction](https://pele.run/) and
[compatibility scope](https://pele.run/compatibility).

The package lists `kepano` as author, and the site credits kepano. Steph Ango's own biography
identifies him as kepano and Obsidian's CEO. Those sources support that association; they do not
establish that he was Obsidian's original author or the sole contributor to every Pele module.
See [package metadata](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/package.json)
and [Steph Ango's biography](https://stephango.com/about).

## Observed Interface And Ownership

### One rendering task, several packaging choices

The public surface offers synchronous `parse`/`render` and a browser `mount` adapter. Results carry
SVG, dimensions, type and link facts. The default package registers all diagram types; `core`
permits selected registration, while `lazy` adds asynchronous type loading. These are packaging
choices for the same rendering task, not different application runtimes.
[Official API](https://pele.run/api),
[default entry](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/index.ts).

Each diagram module implements a small `Diagram<Model>` interface with parse and render methods.
Flowchart composition creates its parser database and delegates rendering; the shared output is
SVG plus dimensions and links. This is a concrete interface backed by many diagram adapters,
not evidence that a hypothetical common game-engine backend needs the same abstraction.
[Diagram interface](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/types.ts#L60),
[flowchart module](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/diagrams/flowchart/index.ts),
[registered implementations](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/diagrams/registry.ts).

The core keeps registration in a module-level Map; registering the same type replaces its entry.
Parsed object models are associated with their parser/configuration through a WeakMap, and render
rejects an unrelated model object. This establishes parser provenance within that module instance;
it is not an immutable, revisioned persistence candidate or a duplicate-rejecting plugin registry.
Core also checks source/edge/output limits and classifies excessive nesting.
[Core implementation](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/core.ts).

### Mounted state is local and explicitly retired

`mountWith` owns requested source, parsed/current model, last drawn model/result, errors, fonts,
width observations and zoom. Remounting an element destroys its prior mount. Failed updates can
leave the prior drawing visible with error feedback. A successful DOM publication precedes
`onRender`; a throwing notification does not roll publication back. Destroy disconnects observers
and zoom behavior and removes font callbacks, while leaving the displayed diagram in place.
Font completion uses weak references to avoid retaining abandoned mounts.
[Mount owner](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/mount.ts).

Inference for Nara: a small interface can hide substantial lifecycle work when one module owns
the whole operation. The useful analogy is Nara's compiler/panel owner, with requested, admitted,
presented and rejected state kept distinct. Browser garbage collection is not a substitute for
Nara's explicit task, device, session and resource retirement.

### Asynchronous loading does not make rendering nonblocking

`load` coalesces pending dynamic imports by diagram type, registers a successful load and permits
retry after import failure. `renderAsync` awaits that load and then calls synchronous render;
`mountAsync` follows the same pattern. It does not move layout computation into a worker.
[Lazy implementation](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/lazy.ts).

Inference for G9: label an asynchronous operation by what actually waits. An async return type
does not establish responsive parsing, cancellation or physical completion. Nara should measure
preparation phases, transport owned results when warranted, and keep owner-thread admission and
publication synchronous. Pele does not justify adding async methods to Plugin, codecs, Scene
resolvers or borrowed panel painting interfaces.

## Theme And Layout Separation

Pele exposes semantic CSS variables for surfaces, text, lines, borders, accents and errors, with
separate series colors. Classes and source `data-id` selectors provide local styling. Color
changes can affect existing SVG without regenerating it. Explicit source colors override theme
colors and therefore remain fixed across theme changes. Fonts participate in measurement:
`mount` reads font variables, pending font loading triggers redraw, and font changes require
refresh. Direct render callers must provide matching measurement and display fonts.
[Theming contract](https://pele.run/theming).

The implementation maps public tokens to private SVG aliases with fallbacks. Typography output
pins inherited spacing, transformation and direction properties instead of inheriting arbitrary
page text geometry. Flowchart rendering measures labels before computing shapes and layout.
[Theme implementation](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/svg/theme.ts),
[flowchart layout/rendering](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/diagrams/flowchart/render.ts).

The default text measurer can use Canvas, with a metrics-table estimate outside that environment.
Canvas width caches account for font availability; `forgetTextWidths` clears shared measurers.
This is a browser/library tradeoff, not evidence that approximate fallback widths satisfy Nara's
native shaping and exact FontFace contract.
[Text measurement implementation](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/text/measurer.ts).

The transferable design is a dependency classification, not a CSS implementation mandate:

| Change | Recommended Nara owner and invariant | Present scope |
| --- | --- | --- |
| Palette color | A typed theme snapshot resolves semantic roles; geometry and document history remain unaffected | Existing `UiColorRole`, `UiBrush::Role` and `UiThemePalette` already provide the essential vocabulary |
| Font face, size or other metric-affecting typography | Recompute measured geometry and shaped glyph/resource facts together; retain exact admitted face identity | U9 content geometry and U12 compile/render font pairing |
| Available extent | Compiler/layout owner receives admitted content space; rendering consumes published geometry | U9, plus existing environment/view ownership |
| Per-node explicit color | Treat it as an intentional override rather than silently claiming all content follows the theme | Existing `UiBrush::Solid` versus semantic roles |
| Theme or panel state change | Keep transient presentation state separate from saved Scene values, undo and source dirty state | Existing Editor semantic model/actions and U13 panel ownership |

These recommendations are inferences. In particular, typography must be classified by its effect
on metrics; not every property named a theme token is paint-only.

Nara source: [typed theme and brushes](../../crates/nara_ui_kernel/src/style.rs),
[compiler admission](../../crates/nara_ui_kernel/src/compiler.rs),
[resolved content geometry](../../crates/nara_ui_kernel/src/compiler/root.rs), and
[ordinary font configuration](../../src/product/default_settings.rs).
Nara resolves colors into immutable frame facts and validates theme revision agreement. CSS can
repaint retained SVG through browser style resolution. Consequently, copying Pele's "no re-render"
promise into Nara would require a different mechanism and measured need; current theme
recompilation is not shown to be a defect by this comparison.

## Editor Composition And Extension Lessons

The current Editor retains the semantic ownership selected by
[ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md):
documents own revisions/history/persistence, the frontend owns widget drafts, and the native Host
owns windows, input and GPU lifecycle. Its paired
[native panel composition](../../editor/src/platform/composition.rs) admits descriptors/factories,
and [instance presentation](../../editor/src/frontend/imgui/workspace.rs) owns panel objects.
These source-built contributions are a different extension domain from Pele diagram renderers.

Inference: preserve the small external interface, move lifecycle complexity into its existing
owner, and use concrete adapters as proof. U13's separate panel instances, reset/restore
retirement and retained GPU reconstruction follow that direction. A generic widget tree,
browser-style cascade or public theme plugin registry would add unrelated interface obligations.
Pele's replacing global registry is unsuitable for Nara's frozen composition identities,
duplicate checks and predictable reconstruction.

Pele also resolves style/resize information against an element's actual window rather than always
using the script's global window, and subtracts container padding when computing room.
[Window helper](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/src/window.ts).
Inference: the corresponding Nara invariant is per-view/per-window scale, input and content
geometry authority. DOM identity cannot replace panel/document/view identity, and a browser's
layout observer cannot replace an exact native presentation receipt.

## Limits And Differences To Preserve

Pele filters diagram-provided text/styles/URLs and applies size limits. The embedding app still
owns trusted icon markup, internal navigation, error display and render options. A hard time
limit requires a terminable worker according to its documentation.
[Security responsibilities](https://pele.run/security).
Inference: a renderer should return facts without acquiring host authority. Nara must retain its
stronger filesystem/source capabilities and admitted Editor actions; SVG strings, CSS classes and
link names do not authorize native source mutation or arbitrary project code.

Mermaid syntax compatibility does not imply layout equivalence: Pele documents its own layout,
unsupported external Mermaid plugins and no arbitrary HTML labels or JavaScript click callbacks.
[Compatibility differences](https://pele.run/compatibility).
Its parser verification compares accepted syntax and parsed data with Mermaid specifications and
uses differential fuzzing.
[Contribution guide](https://github.com/obsidianmd/pele/blob/44da152bbb79b91f03e68016f42499bb25bf6e82/CONTRIBUTING.md).
Inference: compare Nara adapters through semantic facts/actions and owner invariants, with
separate visual/platform acceptance, rather than demanding pixel identity with another toolkit.

## Disposition For The Current Plan

| Subject | Disposition |
| --- | --- |
| U9 text geometry | Useful corroborating precedent: measurement and final presentation need compatible font/content constraints. Existing correction remains the owning implementation. |
| U12 ordinary font configuration | Useful precedent for coherent measurement/display configuration. No new font asset pipeline, fallback or CSS bridge is authorized. |
| U13 panels | Useful owner/lifecycle analogy. Keep per-instance drafts and paired source-built composition; no dynamic registry or shared widget framework follows. |
| U1-U8, U10-U11, U14-U15 | No additional mandatory correction identified by this focused Pele study. Their existing acceptance remains unchanged. |
| G9 async/interface review | Retain explicit synchronous commit boundaries. Measure responsiveness before selecting execution changes; module loading and off-thread computation are distinct capabilities. |
| Future theme work | Enter only with a real Editor/runtime customization workflow or measured theme-update cost. Extend existing typed roles and adapters before considering selector/cascade machinery. |

This research adds comparative evidence. It neither installs Pele nor expands U1-U15, closes
unexecuted acceptance, qualifies another platform, or changes the existing G1-G9 triggers.
