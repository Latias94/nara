# Jellyflow Node Editing And Engine Adoption Audit

Reviewed November 12, 2026. This study evaluates whether node editing merits an independent
Jellyflow project and what must be reconciled before Nara adopts its mechanisms. It does not
select a new default Editor, replace a document owner, or qualify a complete Shader Graph.

## Recommendation

Continue maintaining Jellyflow as an independent node-editing library, with Nara as a demanding
consumer. Its reusable responsibility is graph authoring: real ports and connections, graph-space
interaction, view-local geometry, retained native node lifetimes, and optional graph layout.
Shader compilation, gameplay execution, asset resolution, project persistence policy and the
selected domain's document/history authority remain separate product responsibilities.
Authoritative content, semantic commands and Undo/Redo storage belong to that domain owner.
Nara's Workspace owns document identity, monotonic accepted revisions, saved content checkpoints
and dirty/history projections; `EditorProjectSession` coordinates dispatch and the existing
persistence Host.

Independent maintenance does not require preserving the current implementation. In particular,
the existing `GraphRuntime` is an authoritative document runtime, not a passive canvas widget.
Its command/history ownership must be reconciled with Nara before adopting the adapter. Keeping
two writable copies and synchronizing them after the fact would undermine the exact revision,
Undo and result contracts already exercised by Nara.

This recommendation concerns a useful module boundary. It is not a finding that Jellyflow is
complete, that it matches all xyflow behavior, or that its current framework dependency is the
maintained Open-GPUI source used by Nara.

## Reviewed Sources

| Source | Exact revision | Role |
|---|---|---|
| Jellyflow | `46a49c1db1eb4718d75bddf38803e1775d2e9fca` | Current clean release source, five crates and native product example. |
| Jellyflow's local xyflow reference | `1bc405cd113618c17fd181b931a9fe573bec9fee` | Behavioral reference; DOM/React architecture is not an adoption requirement. |
| Maintained Open-GPUI | `f75c10b2a9bf7b4dcb2c8c2aa446203997601369` | Current independent framework and Nara integration source. |
| Nara | `c7d1e10078b8430ccbc4bcaca86eba6dfaaee766` | Existing document, typed graph, independent Views and continuous-gesture control. |

The source audit separates graph transactions, the retained native adapter, geometry/layout work,
and engine ownership into four read-only lanes. Build and executable checks are serialized by the
integrating agent. `repo-ref/` remains read-only. Findings distinguish current defects, integration
prerequisites and unqualified product scope.

## Why Node Editing Earns A Separate Library

| Capability | Reusable complexity | Product responsibility |
|---|---|---|
| Ports and topology | Stable real-port identity, direction/capacity, connect/reconnect, dynamic-port changes and incident-edge handling must remain coherent. | Shader/value types, node-specific validation, compilation and error presentation. |
| Gestures | Drag previews, capture/cancellation, selection, snapping, pan/zoom and release-time mutation must agree on coordinate space and original basis. | Tool shortcuts, command labels and domain-specific manipulation rules. |
| Multiple views | Each occurrence needs its own transform, selection, measured geometry, hit targets and lifetime while observing shared graph changes. | Which documents/panels exist and how their previews are scheduled. |
| Native node content | Text fields, popups, scrolling and async providers must retain their framework state without accidentally starting graph gestures. | Node composition, appearance, resource pickers and custom controls. |
| Work at scale | Visible-node materialization, spatial queries, route dependencies and changed-node reconciliation need explicit work ownership. | Representative workload sizes and acceptable CPU/GPU/memory/latency budgets. |
| Persistence and editing | A standalone graph runtime needs atomic commands, stable identities, safe replacement and history. | Whether that runtime or the embedding product's existing domain owns the sole authoritative content and history. |

The existing Nara finite graph consumer intentionally proves a smaller set of these problems.
Its typed links, one-command node dragging, local transforms, previews and exact Host publication
are valuable controls. They do not replace a reusable production node editor. Conversely, a node
editor does not replace the UI framework's retained state, text, focus, layout or rendering.

## Existing Boundaries Worth Keeping

The published crate boundaries are substantial rather than renderer aliases:

- `jellyflow-core` owns graph values, stable IDs, validation and portable policy vocabulary.
- `jellyflow-runtime` owns opaque-version commands, graph history/persistence and disposable
  `ViewSession` capabilities. Frames, complete real-port measurements and identity-keyed feedback
  cross the adapter seam as owned values.
- `jellyflow-layout` is optional and returns graph positions; it does not own native component
  layout or window scheduling.
- `jellyflow-open-gpui` owns retained native entities, mounts, measurements, activity, drafts and
  input arbitration. Its production dependencies do not require the styled component package.
- `jellyflow-egui` is explicitly a narrow immediate-mode protocol witness, not a second flagship
  component library or proof of Open-GPUI native behavior.

The neutral crates declare no UI/window/GPU framework dependency. The retained adapter supplies
node/port/drag capabilities to ordinary GPUI components rather than defining a portable widget
tree. Those are useful directions for Nara extension authors. Exact public behavior, dependency
closure and current native qualification still need their own evidence. [J1] [J2] [J3]

## Findings And Evidence Levels

The following issues do not invalidate the independent-library boundary. They identify work in
the existing owners. Source-level findings require a focused regression before a fix can claim
execution or native qualification; their severity is the assessed impact, not a test result.

| ID | Priority | Finding | Evidence |
|---|---|---|---|
| JF1 | P1 | Runtime node-data/topology commands can violate reserved symbol/subgraph references, then save a file that canonical loading rejects. | Executed node-data/save/load regression; topology/subgraph variants are source-only. [J6] |
| JF2 | P1 | Port measurements survive a rolled-back native prepaint scope; ordinary deferred feedback is not an accepted-frame publication fence. | Source-only adapter measurement, completeness and deferred-publication review. [J8] |
| JF3 | P1 | Marquee uses a node-intersection query that excludes frame-culled nodes, so scrolling a selection region can remove nodes from its result. | Executed marquee/autopan/culling/release regression. [J11] |
| JF4 | P2 | Undo after a legal history-free derived size update can combine an old position with a new size into non-finite bounds. | Executed history replay regression. [J7] |
| JF5 | P2 | Horizontal tidy-tree uses height for level spacing before swapping axes, allowing ordinary wide nodes to overlap. | Executed default two-node left-to-right layout regression. [J12] |
| JF6 | P2 | A node-exported overlay boundary lacks its mount's revocation capability, so an externally retained old popup can still block input. | Source-only boundary creation, hitbox and listener review. [J9] |
| JF7 | P2 | Custom overview rendering ignores the disabled-interaction flag used for a transparent pinned-detail geometry copy. | Source-only application-overview, public port handle and activation review. [J10] |
| JF8 | P2 | Runtime drag constraints use authored node size, treating missing size as zero even when valid measured geometry is available. | Source-only extent policy and graph-size lookup review. [J13] |
| JF9 | P2 | Freeform layout repeatedly restarts collision scans; coincident equal-sized nodes produce cubic collision-check growth. | Source loop structure, not a measured runtime/latency result. [J14] |

JF1 belongs in final-candidate graph validation. Reserved `jellyflow.symbol_ref` and
`jellyflow.subgraph` constraints already belong to core; they are not arbitrary product payload
semantics. Checking only each adapter or only the file loader leaves the authoritative graph
invalid. An atomic command must validate its final candidate rather than reject a temporary
intermediate state that a later operation fixes.

JF2 belongs in the GPUI adapter. Complete expected-port coverage establishes semantic identity,
but does not establish that the native scope or final frame was accepted. Adopt the maintained
framework's existing transaction/publication mechanism, preserving prior geometry on refusal
and withdrawing it only through an accepted replacement. A runtime feedback receipt and
`on_next_frame` callback cannot substitute for the original Window's accepted-publication fact.
This audit has not executed the speculative-prepaint or native-refusal scenario. [J8]

JF3 needs separate query eligibility for current pointer hit targets and selection of a canvas
region. Rendering culling should not silently redefine the complete marquee result after
autopan. Reuse valid retained geometry and bounded query state rather than introduce a second
selection owner. The xyflow reference queries its full node lookup for marquee, independently
of the rendered collection. [J11]

JF4 needs failure-atomic history candidate validation; it uses extreme finite coordinates and
does not imply that ordinary Undo is generally broken. JF5 is a normal-dimension layout defect.
JF6/JF7 concern capability retirement and semantic interactivity, which opacity alone cannot
enforce. JF8 requires an explicit measured-versus-authored size contract, including the exact
view generation used by preview and commit. JF9 calls for a measured layout workload and an
appropriate spatial/interval structure if that workload matters; no frame-rate claim follows
from the source complexity analysis.

Several current limits are intentional rather than findings: a move cohort above 64 is rejected,
query/materialization budgets have explicit terminal outcomes, radial layout does not promise
non-overlap, and all layout engines do not promise pinned-node support. Preserve honest limits
until real consumers justify broader contracts.

## Engine Adoption Constraints

Jellyflow's Open-GPUI adapter pins registry `open-gpui = "=0.2.0"`. The maintained checkout also
uses package version `0.2.0`, but contains later source/API and accepted-publication changes.
Equal package versions do not establish equal Rust types or compatible lifecycle behavior.
Qualify the original public custom-node recipe against one exact framework source before
switching Nara's dependency closure. Do not combine registry and local framework identities in
one live node tree. [J2] [J4]

The existing native example's CI launches its Shader and Workflow smoke processes only on macOS;
the documented smoke proves reaching a valid first scene. Deterministic adapter tests own broader
interaction assertions. This is useful evidence with a defined scope, not Windows acceptance of
a complete engine-hosted editor. [J1] [J5]

The current adapter requires a concrete `ViewSession`, whose feedback dispatches directly to
`GraphRuntime`. The view and interaction constructors do not expose an external immutable
document source. `replace_document` is a real replacement: it clears history, advances the
document epoch and retires node incarnations. Repeating replacement for every Nara edit would
discard the retained identity this library is meant to preserve. [J15]

Nara's `EditorExtensionDocument` is `Send + Sync`; the current Jellyflow runtime contains an
`Rc<RuntimeShared>`. Putting that runtime inside `Arc<Mutex<_>>` does not satisfy the contract.
Passing an executable runtime/view through an ostensibly read-only projection would also expose
a path around Session admission. Nara's extension domain already owns its inverse/history,
while Session invokes it and tracks the saved content identity. Jellyflow's public history
depth does not provide a restorable, branch-safe content identity for dirty/checkpoint behavior.
These are integration prerequisites, not evidence that either standalone design is broken. [J15]

| Option | Single authority | Necessary work and scope |
|---|---|---|
| A: use Jellyflow's authoritative graph document | Jellyflow owns authored graph and its sole history; Nara owns document slots, admission and file publication. | Separate suitable domain storage from UI-thread view state, provide restorable content identity, route all native edits through Session admission, and add bounded byte codecs if choosing Jellyflow's format. |
| B: retain Nara's domain document | One Nara extension domain owns source and history; Jellyflow owns view/geometry/interaction over immutable facts. | Separate the current View's fact input from its submission authority; emit pending edit intents and consume exact authoritative completion without another writable graph copy. |
| C: consume geometry/layout first | The existing domain owner remains unchanged. | Useful bounded reuse, but it does not prove a complete native node editor or the external-document seam. |

For tools with existing Nara document semantics, prefer a Trial of B. Keep A as Jellyflow's
independent runtime and as an option for products deliberately choosing that document model.
Do not make Nara mirror Jellyflow mutations after they have happened, or add snapshot history
to compensate for a second runtime's missing checkpoint contract.

If A chooses Jellyflow persistence, its current public path-oriented canonical loader needs an
explicit bounded byte decode/encode counterpart for Nara's no-I/O codec contract. Reuse original
envelope/version/catalog migration and validation; do not round-trip through temporary files or
write a competing parser. This is not a prerequisite for B using its own domain format. [J16]

Content-derived height currently submits a history-free authored geometry command. Nara must
decide whether that size belongs to source or only a particular view/font/theme measurement.
Omitting Undo does not make a write non-authoritative. Likewise, graph geometry receipts cannot
replace the existing exact document/revision/request preview admission and original Render/Host
output publication. [J17]

Jellyflow should stay above the selected UI runtime and below product tools. It should not acquire
a second Window, pointer router, GPU device, scheduler, render graph, universal control DSL or
Shader compiler to simplify one integration. Nara's authored graph can produce a compact runtime
artifact without shipping the editor or Jellyflow's complete authoring runtime in the game.

## Maintenance And Adoption Sequence

1. Resolve reproducible graph integrity defects in their existing owners before using persistent
   files as adoption evidence. Preserve failure atomicity, result identity and history position.
2. Compile one real public custom native node against the maintained Open-GPUI source. Reconcile
   changed framework interfaces and qualify actual accepted/refused candidate publication rather
   than preserving obsolete adapter behavior behind compatibility forwarding.
3. Select one document/history authority through a concrete Nara graph document. Keep Jellyflow
   independently useful. For an existing Nara-owned document, preserve its original domain
   history and Session admission/persistence path; add no parallel Workspace history or writable
   graph mirror. Introduce the external-authority seam for that consumer rather than recreating
   the full runtime as a generic host trait.
4. Run one complete node-authoring trajectory with custom native controls, real dynamic ports,
   connect/reconnect, local gesture previews, Save/Reopen and two independent views. Each accepted
   atomic edit has one original domain-history entry, including a completed node drag; a canceled
   preview has none. Include cancellation and late work after replacement/close.
5. Attribute changed projection, materialization, layout/prepaint, routing, Scene construction,
   upload and GPU work separately. Select improvements from measured costs and the invalidated
   artifact, not from adopting Signals, persistent trees or a compositor by name.

A useful next consumer is an independently compiled Dialogue Graph: dialogue/choice/end nodes,
stable choice/real-port identities, custom text and popup content, variable height and an ordinary
property editor modifying the same domain. Prove atomic choice/port/link edits, cross-entry Undo,
return-to-saved clean state, Redo invalidation on a new branch, Save/Reopen and two independent
views. A small
conversation preview supplies an actual domain result without conflating node-editor adoption
with shader compilation. This is a proposed complete consumer, not an implemented successor;
the current list-of-lines Dialogue example does not already satisfy it. Shader/animation/behavior
tools remain later consumers with their own domain and execution owners. [J18]

Nara feedback should identify an operation, original document/view basis, expected result and
observed cost or failure. Jellyflow fixes belong in its owning module and public tests; Nara keeps
the consumer regression proving engine integration. Domain-specific node types remain Nara code.
This gives the independent projects a feedback loop without a second permanent implementation
or a demand to reproduce the entire xyflow feature set before the first useful consumer.

## Executed Audit Verification

All commands ran on Windows 11 with Rust `1.95.0`, `CARGO_BUILD_JOBS=1` and the existing Nara
`target`. Cargo invocations were serialized. Neither Jellyflow nor Open-GPUI production source,
manifest, lockfile or branch changed. Disposable external probes hold their own manifest/lock
outside all repositories. These are audit results, including expected regression failures;
they do not qualify fixes or a native Jellyflow/Nara integration.

| Check | Actual result | Scope |
|---|---|---|
| Existing core/runtime/layout/egui nextest suite | **711/711 passed**, no skips; run `2301ae76-7785-4935-8051-f9bafb046a15`. | Four crates, 12 binaries; excludes the GPUI adapter and real native example. |
| Runtime/layout normal/build dependency tree | No Open-GPUI, egui, WGPU, Winit, Bevy or Nara package in the selected Windows closure. | Confirms the current neutral production dependency direction; not a foreign-platform execution claim. |
| Original public custom-node recipe with all Open-GPUI packages patched to the maintained source | Compilation **failed** inside `jellyflow-open-gpui`: 42 error diagnostics. | Includes typed pointer-capture handles, targeted events, explicit scroll intent, measurement callbacks, clipping, focus and modifier-context changes; several diagnostics repeat an owning migration. The recipe itself did not reach successful compilation or native execution. |
| Four external regression tests using public core/runtime/layout APIs | **4/4 failed at the intended behavior assertions**; run `d0fe9944-322d-430f-8550-15256b0a82b4`. | Executable confirmation of JF1, JF3, JF4 and JF5 on unchanged library source. No mocks, private APIs or GPUI test-support. |

The external regressions establish the following specific outcomes:

- **JF1:** start from a legal `jellyflow.symbol_ref` and a canonically reloadable file; set its
  payload to an undeclared symbol using the current opaque version. The command returns
  `Applied`; `save_json` succeeds, but canonical reload returns `SymbolRefTargetNotDeclared`.
  The test expected rejection preserving the original snapshot/history/file. This execution
  confirms the node-data update scenario; the analogous topology-add/subgraph paths remain
  source findings, not additional executed variants.
- **JF3:** begin a real marquee, extend it to the right edge, then run two autopan ticks while
  publishing normal culled frame feedback. Both complete node rectangles remain inside the
  canvas-space marquee. On release, only node 2 remains selected; the expected result is nodes
  1 and 2. The first node is frame-culled and the second is currently visible. Query work is
  advanced through the public bounded interaction interface.
- **JF4:** move a node from `f32::MAX / 2` to zero, then apply a legal width of `f32::MAX` at
  zero without history. Undo restores the old position, returns `Applied` and moves history
  from `(undo=1, redo=0)` to `(undo=0, redo=1)`, while storage validation reports
  `NodeBoundsNotFinite`. The test expected failure-atomic rejection and an unchanged cursor.
- **JF5:** lay out a two-node chain with default `172 x 36` sizes and `LeftToRight`. The
  parent's right edge is 172 and the child's left edge is 86, proving overlap. The test
  expected disjoint rectangles. Other directions and mixed dimensions were not added to this
  audit execution.

The disposable sources are `%TEMP%/jellyflow-runtime-audit-probe/tests/{integrity,geometry_layout}.rs`
and the maintained-framework probe reuses Jellyflow's original `open-gpui-adoption/src/main.rs`.
The regression tests are named
`illegal_symbol_reference_update_is_rejected_without_breaking_saved_document`,
`undo_rejects_unrepresentable_bounds_after_derived_resize_without_changing_graph_or_history`,
`marquee_auto_pan_keeps_nodes_selected_after_frame_culling` and
`tidy_tree_left_to_right_separates_default_size_nodes`. Logs remain outside the repositories:
`jellyflow-audit-headless-baseline.log`, `jellyflow-audit-neutral-dependencies.log`,
`jellyflow-audit-maintained-gpui-compile.log` and `jellyflow-audit-public-integrity-red.log`.

JF2, JF6, JF7 and JF8 remain source-level findings. JF9 is a source operation-growth analysis.
No GPUI adapter/native/physical input/OS IME execution or numeric performance benchmark was
performed. The default-size overlap and actual save/load mismatch are current adoption risks;
the extreme-coordinate replay is a lower-priority robustness issue. None requires merging
node-editor responsibilities into the engine or keeping a known defect as compatibility.

## November 13: View-Local Intrinsic Geometry

The [intrinsic geometry successor](../plans/2026-11-12-dialogue-graph-integration-plan.md#implemented-view-local-intrinsic-geometry)
implements the size-ownership correction through local Jellyflow
`74f7f54ab1b6858ba177f0973aa05800ee72b906`. It follows Runtime `8d9c0a8`, layout `994cb17`
and the fixed-framework option-selector repair `93dd41c`.

Automatic standalone and external measurements now share one View-local path. The original
complete layout carries actual unscaled canonical size, root and real ports; the source graph,
revision, history and saved size remain unchanged. Explicit authored resize remains a domain
operation. Runtime geometry, semantic zoom, node/port placement, remount, Fit and current View
movement constraints consume this metadata without another accepted-size index. Layout Context
accepts resolved canonical sizes ahead of authored size and raw hints, then commits positions
through the original owner. Old stored sizes are not classified or deleted by inference.

Runtime/layout public cohorts pass 715/715 and Open-GPUI/egui consumers 585/585. The independent
Nara domain/Session/file cohort passes 110/110, with existing rebuilt DX12/Vulkan controls 3/3
each. Exact runs and corrected hidden/pending/sampling/panic boundaries belong to the linked
plan. The new public native-component workflow actually exercises two external Views with
different natural dimensions, non-unit zoom/origins, grow/shrink, eviction/remount, Fit, authored
width replacement and incomplete-port refusal/recovery.

The complete native Workflow sample remains partially qualified: 22/25 after selector repairs,
with two popup lifecycle failures and a zoomed port hit-boundary failure. An exact-parent
checkout plus only the same selector repairs reproduces those three failures. LLM conflict
growth/shrink and Condition growth/deletion pass. This supports the canonical mechanism while
preserving concrete product gaps; it does not establish complete node-editor maturity, fix
every original audit finding, qualify native external Dialogue or select a game UI stack.
Resolve the original overlay/presentation/input owners before closing the corresponding
product gate. These are fork/consumer changes, not additional Zed synchronization.

## Sources

- [J1] [Jellyflow context and current ownership](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/CONTEXT.md), [adapter authoring guide](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/docs/guides/native-adapter-authoring.md).
- [J2] [Retained adapter manifest](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/crates/jellyflow-open-gpui/Cargo.toml), [public module surface](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/crates/jellyflow-open-gpui/src/lib.rs).
- [J3] [Core manifest](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/crates/jellyflow-core/Cargo.toml), [runtime manifest](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/crates/jellyflow-runtime/Cargo.toml), [layout manifest](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/crates/jellyflow-layout/Cargo.toml).
- [J4] [Original public native-node adoption recipe](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/tools/fixtures/external-consumer/open-gpui-adoption/src/main.rs).
- [J5] [Current CI scope](https://github.com/Latias94/jellyflow/blob/46a49c1db1eb4718d75bddf38803e1775d2e9fca/.github/workflows/ci.yml).
- [J6] Jellyflow `crates/jellyflow-runtime/src/runtime/commands/planning/mod.rs:293`; `crates/jellyflow-core/src/core/validate/structural.rs:43`; `crates/jellyflow-runtime/src/schema/catalog/canonical.rs:374`; `crates/jellyflow-runtime/src/io/files/graph.rs:222` at the reviewed pin.
- [J7] Jellyflow `crates/jellyflow-runtime/src/runtime/graph_runtime/execution.rs:328`; `crates/jellyflow-runtime/src/runtime/commands/model.rs:276` at the reviewed pin.
- [J8] Jellyflow `crates/jellyflow-open-gpui/src/port_handle.rs:619,1193,1335`; `crates/jellyflow-open-gpui/src/host/scene_render.rs:1831,1873`; maintained Open-GPUI `crates/gpui/src/window.rs:12374` at their reviewed pins.
- [J9] Jellyflow `crates/jellyflow-open-gpui/src/node_drag_handle.rs:121`; `crates/jellyflow-open-gpui/src/interaction.rs:145,2166,2181`; existing `tests/mount_capability.rs:759` and `tests/overlay_ownership.rs:478` at the reviewed pin.
- [J10] Jellyflow `crates/jellyflow-open-gpui/src/host/scene_render.rs:1072`; `src/semantic_zoom.rs:279`; `src/registry.rs:118`; `src/host/connection.rs:496`; existing `tests/native_overview.rs:91` and `tests/measurement_feedback.rs:1692` at the reviewed pin.
- [J11] Jellyflow `crates/jellyflow-runtime/src/runtime/view_session/derived_geometry.rs:325`; `src/runtime/interaction/session.rs:6913`; existing `src/runtime/tests/view_session/geometry_publication.rs:2374`; xyflow `packages/react/src/container/Pane/index.tsx:235` at their reviewed pins.
- [J12] Jellyflow `crates/jellyflow-layout/src/tidy_tree.rs:141,231`; existing `src/tests/tidy_tree.rs:34` at the reviewed pin.
- [J13] Jellyflow `crates/jellyflow-runtime/src/runtime/interaction/session.rs:2430`; `src/runtime/interaction/intent.rs:883`; `crates/jellyflow-core/src/core/model/node.rs:49` at the reviewed pin.
- [J14] Jellyflow `crates/jellyflow-layout/src/freeform.rs:127` and `freeform_resolves_more_than_sixty_four_coincident_nodes` at the reviewed pin. This is operation-growth analysis, not elapsed-time measurement.
- [J15] Jellyflow `crates/jellyflow-open-gpui/src/host.rs:276`; `crates/jellyflow-runtime/src/runtime/view_session/protocol.rs:724`; `src/runtime/view_session/mod.rs:959`; `src/runtime/interaction/session.rs:2044`; `src/runtime/graph_runtime/mod.rs:572,790,1061`. Nara `crates/nara_tooling/src/workspace/extension.rs:10`; `src/workspace.rs:402,789`; Accepted [ADR0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md) at the reviewed pin.
- [J16] Jellyflow `crates/jellyflow-runtime/src/io/files/graph.rs:286` and Nara `crates/nara_tooling/src/workspace/extension.rs` at the reviewed pins.
- [J17] Jellyflow `crates/jellyflow-open-gpui/src/host/content_size_submission_owner.rs:929`; Nara `tests/fixtures/gpui-engine-consumer/src/graph/preview.rs:175` and `examples/gpui_view/preview_jobs.rs:67` at the reviewed pins.
- [J18] Nara `tests/fixtures/module-consumer/editor-panel-consumer/src/dialogue.rs:17`; existing [complex-editor study](2026-11-11-open-gpui-engine-integration-layers.md#typed-graph-extension-consumer); Accepted [ADR0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
