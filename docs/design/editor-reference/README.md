# Editor Visual Reference: Design Kit v0.4

**Role:** User-supplied visual and interaction reference, reviewed November 5, 2026.
**Authority:** Accepted ADRs own architecture; source, guides and qualification establish current
supported behavior. Plans organize delivery. This reference proposes future product design.

This complete reference comes from the project owner's `nara-design-kit-v0.4-fixed` package,
containing the `nara-design-kit-v0.4` directory. All 32 supplied files are preserved byte for byte,
including sixteen UI PNGs, the original specification and tokens, brand assets and design-process
records. The original specification retains its supplied language; this adoption map is English.
Local Git attributes preserve the supplied reference bytes across platform line-ending settings.
These mockups express a target design; they are not screenshots of implemented Nara tools,
performance measurements or approval of every pictured capability.

## Included References

- [Scene and Inspector workspace](images/nara-scene-inspector-v0.4.png): surface hierarchy,
  dense tool organization, selected rows, property groups and explicit editing context.
- [Property component sheet](images/nara-components-properties-v0.4.png): field states, source
  versus unsaved state, references, numeric composition and combined text/icon controls.
- [Navigation and feedback sheet](images/nara-components-navigation-feedback-v0.4.png): collections,
  navigation, command discovery and explicit feedback.
- [Original reference tokens](nara-design-tokens.json): proposed logical sizes, encoded colors,
  typography and behavior vocabulary. This is design data, not a supported runtime schema.
- [Original full specification](nara-editor-design-spec-v0.4.md), [package notes](README.txt) and
  [image index](design-process/image-index-v0.4.json): the original component, tool, docking,
  document, View and workflow contracts with their intact relative references.

The original package is 29,897,315 bytes, about 28.5 MiB. Brand assets and design-process records
remain references. No production brand or application icon changes follow from this import.

| Additional UI reference | Focus |
| --- | --- |
| [Base components](images/nara-components-v0.2.png) | Earlier shared-control visual vocabulary |
| [Assets and import](images/nara-assets-importer-v0.4.png) | Browsing, properties and operation feedback |
| [Shader graph](images/nara-shader-graph-v0.2.png) | Custom canvas and graph controls |
| [Animation editor](images/nara-animation-editor-v0.4.png) | Tracks, properties and document context |
| [Animation controller](images/nara-animation-controller-v0.2.png) | State-machine canvas |
| [Animation timeline](images/nara-animation-timeline-v0.2.png) | Time navigation and editing density |
| [Profiler and console](images/nara-profiler-console-v0.4.png) | Diagnostic hierarchy and signal presentation |
| [Project and input settings](images/nara-project-input-settings-v0.4.png) | Settings navigation and complex forms |
| [Particle editor](images/nara-particle-editor-v0.4.png) | Module stacks, preview and properties |
| [Audio mixer](images/nara-audio-mixer-v0.4.png) | Channels, meters and parameter controls |
| [Build profiles](images/nara-build-profiles-v0.4.png) | Profile selection, tasks and exact results |
| [Docked Scene and Shader](images/nara-docking-scene-shader-v0.3.png) | Per-group tabs and adjacent document Views |
| [Docked Scene, Game and floating View](images/nara-docking-scene-game-float-v0.3.png) | Multiple Views, windows and context separation |

The package notes explicitly supersede the older images' global-tab arrangements with the v0.3+
per-DockGroup rule. Use newer behavior/specification text when interpreting inherited images.

The included design-process record identifies built-in image generation. It provides no exact
model/version or additional reference-asset license declaration; this import does not invent
either or reclassify the original files as production code.

## Decisions To Carry Forward

| Design guidance | Nara application and owning contract |
| --- | --- |
| Graphite surfaces, restrained teal emphasis and clear surface hierarchy | Use as the next visual direction. Map semantic roles through the component library's scoped palette; compare actual output before changing defaults. |
| Ordinary/compact controls at 28/24 logical units, 4-unit corner radius and 1-unit control stroke | Compatible with current Regular geometry defaults. These are minima: actual text measurement, padding and user text enlargement still determine required allocation. |
| Separate visual and interaction geometry | The reference's 16-unit icons, 24-unit hit minimum, 1/6-unit splitter visual/hit sizes and 6/12-unit scrollbar visual/hit sizes are proposed recipe inputs, not permission to change kernel hit authority or impose one target size on every component. |
| Read-only, disabled, mixed, invalid, pending, missing-reference and source-override states | Keep behavior and visible feedback distinct. Source override, dirty document, local draft and exact operation result remain separate facts from the original document and UI owners. Unimplemented states need their own consumer. |
| Composite property rows with icons, text, units, actions and validation | Use a complete mixed-font icon/label/value row as the next recipe consumer. Measure actual baselines and ink; specify optical offsets and combined states without deriving geometry from a mockup's pixels. |
| Document, View, selection and runtime context remain distinct | Preserve ADR0118 and existing document/Session ownership. Docking, multiple windows, pinned Inspectors and tool-specific contexts need their current-plan consumers; the pictured 3D world does not expand the reference game's 2D scope. |
| Layout adapts without silently overwriting saved user arrangement | Keep temporary maximize/collapse/scroll and persisted layout distinct when implementing docking. Reference panel widths and workspace dimensions are examples, not fixed rectangles for every panel. |
| Text/IME, actions and future agent access use original input authority | Use actual field tokens, accepted native mapping and the presented-operation ports. Design labels and JSON command names grant no mutation, input or presentation authority. |

## Reconcile Before Implementation

The proposed body is **13/20**, with 12/18 small/numeric text and 600-weight headings. The current
[implemented recipes](../../guides/editable-text.md#scoped-appearance-and-measured-controls)
use **14/20**, 12/18 and 16/24 roles, an explicitly admitted Regular face and configured fallback.
Evaluate the proposed density with actual Latin/CJK text, fields, long paths and user scale.
Do not silently synthesize weight, assume system font availability, shrink qualified capacity or
change current metrics simply to reproduce an image.

The JSON explicitly uses **sRGB-encoded HEX** colors. Specify the conversion into Nara's admitted
render-color representation, alpha/compositing and target encoding before adopting them. Check
actual text, border, focus, selected and error contrast on their surfaces; a valid color tuple
does not certify contrast. Raster gradients, glows and tiny labels remain illustrative.

The reference's 2-unit focus stroke and 1-unit offset require stable reserved geometry and clip
behavior. Its numeric tabular figures, system font selection, icon optical recipes, reduced
motion, preferences and resource readiness are design pressure, not current supported inputs of
`UiAppearance`. Introduce each through its existing text, component, Host or resource owner.

Do not generate public widget kinds, a command bus, docking state, task owners, render topology or
font/cache services from the JSON. Third-party libraries can keep their own roles and control
types and use the same layout/text/drawing/input ports. Common recipes are reusable defaults;
Editor and game-authored UI need not share one product theme.

## Product Behavior Exceptions

The original specification includes proposed behavior as well as visual guidance. These current
differences remain explicit until their owning contracts are deliberately changed:

| Proposed reference behavior | Current Nara behavior and adoption boundary |
| --- | --- |
| Closing the last primary editing View closes its document | ADR0118 separates panel/View lifetime from document lifetime. Retain explicit `CloseDocument` and the original Save/Discard/Cancel protocol; moving, closing or duplicating a View must not implicitly retire its document or runtime. |
| Save coordinates valid local input buffers and blocks on invalid/composing input | Current Inspector publishes through explicit Apply or non-composing Enter. Save captures accepted document content. A future cross-panel draft census/recovery policy needs an owning design and complete consumer; the reference import does not establish it. |
| Layout changes recover pending input and compatible state | Current field filtering/collapse and Inspector-tab changes can preserve drafts. Actual panel hide/close, target change and layout reset retire panel state. Distinguish those operations rather than claiming automatic draft restoration. |
| Blur validation and Escape restore an edit-start value | Blur may update local validation but does not Apply. Escape cancels current composition/draft/preview and cannot overwrite a newer document with an obsolete start snapshot. |
| Read-only values remain selectable/copyable | Editing-disabled text is not a general selectable read-only control. Current retained-draft Copy preserves its own qualified scope. Add committed-value selection/copy through the text, clipboard and semantic owners with error feedback. |
| Moving a View preserves its identity and context across windows | Preserve compatible document binding and local state, while releasing/reconciling native capture, focus, popup and IME authority against newly accepted mappings. Native multi-window migration still requires separate qualification. |

The [source assessment](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#lifecycle-save-and-input-conflicts-to-keep-explicit)
links the inspected current contracts and exact specification chapters for these exceptions.

## Source Map And Integrity

Relevant chapters in the included `nara-editor-design-spec-v0.4.md`:
03–05 color/metrics/states; 06–11 control, source/history and input behavior; 12–14 rendering,
extension and agent boundaries; 21–27 documents, docking, runtime, tasks and recovery; 28 and
37 expanded component contracts and acceptance. The complete original package is retained for
later component/tool/docking work; this English mapping records the current adoption boundary.

| Original file | SHA-256 |
| --- | --- |
| `nara-design-tokens.json` | `ea8d8194cb0fd6b4aa7c6c8bcebd889da05901e7c6880e65e802e1296f644249` |
| `images/nara-scene-inspector-v0.4.png` | `4d2eb202df84a14fdb5d6aa20ee9f0ab0dd25ebcf50dc85e4e6b978aee620157` |
| `images/nara-components-properties-v0.4.png` | `e8daa88f701ef48e4e816bcb253d690a8967a3edd3cc8ee2151f35d0ef7132e7` |
| `images/nara-components-navigation-feedback-v0.4.png` | `b30b83dd069f5c515b1d16fa5fedf3c7a72c3bbb96a999fee4516413d37feff4` |
| `nara-editor-design-spec-v0.4.md` | `c78ce3c1b789bec6db70040e493a5f412c9a408eb9b76e7502620783b3e8aee4` |
| `README.txt` | `e4f85f596c5eebc24f15adc11b1fceb9c688cd6357276ce4eca0f6c5407bf35a` |

The [source study](../../research/2026-10-27-custom-drawn-editor-ui-boundaries.md),
[UI design inventory](../../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
and [current foundation plan](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite)
record assessment, implementation evidence and remaining work separately. Revisit this mapping
when adopting a recipe; copying a reference does not complete its acceptance.
