# Nara UI Integration And Product Development Handoff

Prepared November 12, 2026 for the Nara development session.
Updated November 13, 2026 with qualified native input entry and Editor quality priorities.
The Nara foundation rebaseline remains active. This document is a navigation and handoff record;
Accepted ADRs retain architecture authority, and executable results retain their stated scope.
The separate framework roadmap belongs to Open-GPUI's
`docs/plans/2026-11-12-framework-maintenance-handoff.md` in its own checkout.

## Scope And Next Deliverable

This session continues Nara's engine and product development. The Open-GPUI session maintains
the independent framework against concrete consumer feedback; Jellyflow's session owns its
library repairs and graph interfaces. Their source pins and qualified controls allow those
sessions to work independently without moving Nara's selected dependency on every library edit.

The next Nara deliverable is the complete
[Dialogue Graph consumer](2026-11-12-dialogue-graph-integration-plan.md): one original Session
document and history, two native Views, ordinary property editing and a conversation preview.
Its domain, public Session workflow and immutable facts are qualified at `ddcc880`. The later
external native transport and View-local intrinsic-size prerequisites are implemented through
local Jellyflow `74f7f54`; the subsequent `0a9453a` Workflow repair closes its three reproduced
sample failures. Exact qualification and remaining scope are recorded below.
The complete native Dialogue product remains open. Continue from those boundaries
without repeating the standalone connection or treating library readiness as a completed editor.

The November 13 user feedback prioritizes cross-monitor movement/DPI, unnecessary redraw work
and a coherent Editor visual workflow before further catalogue expansion. Follow
[UI quality and component development](#ui-quality-and-component-development) below. The full
Dialogue product remains the next integration deliverable; this quality work does not replace
its external-document, multiline, popup, Session/file or preview requirements.

## Start Here

| Project | Checkout | Maintenance responsibility |
|---|---|---|
| Nara | `F:/SourceCodes/Rust/nara` | Engine, project/document workflows, content, domain tools, Render/Host integration and complete product consumers. |
| Open-GPUI | `F:/SourceCodes/Rust/open-gpui` | Independently usable UI runtime, platform adapters, retained behavior, layout/text and rendering interfaces. |
| Jellyflow | `F:/SourceCodes/Rust/jellyflow` | Independently usable graph-editing mechanisms, optional layout, standalone graph runtime and native UI adapters. |

User authorization includes fearless refactoring, precise local commits, and the November 12
Open-GPUI merge and remote publication. It does not automatically authorize future pushes from
either maintenance session. Preserve concurrent work. `repo-ref/` remains read-only. Never run
concurrent Cargo commands in these coordinated checkouts; use `CARGO_BUILD_JOBS=1`, Rust
`1.95.0` and the existing Nara `target`. Do not clean that target.

Read each project's current instructions and status before editing. In Jellyflow, `CONTEXT.md`
and `docs/guides/native-adapter-authoring.md` describe its ownership; historical plans do not
prove completion. In Nara, begin with `STRATEGY.md`, `docs/architecture/README.md` and the
[active foundation plan](2026-10-23-foundation-architecture-rebaseline-plan.md).

## Published Framework Baseline

Open-GPUI's former maintenance branch `optimize/docking-graph-transaction` was published, then
fast-forwarded into local `main` and pushed to `origin/main` at the user's explicit request.
The remote `main` was independently queried after the push:

```text
origin: https://github.com/Latias94/open-gpui.git
published code baseline: f75c10b2a9bf7b4dcb2c8c2aa446203997601369
verified main including framework handoff updates: 625173c944fb32555293446405f346db7c8b17cf
previous remote main: 689db5b49b33a5b2e64ba332bff72c98e70092bb
```

The later `2cc8ff1` and `625173c` commits add and clarify the independent framework handoff;
the consumers intentionally retain code baseline `f75c10b` for those documentation changes.
The subsequent user-authorized November 12 push was verified against the remote `main`.
The merge has no conflict or merge-specific source change: the previous main is an ancestor.
Core/WGPU/Windows library checks pass on the published source. Earlier qualification remains
scoped to its documented consumers; this publication does not constitute a new full-platform
release qualification or completion of canonical docking U4–U9.

The fixed Zed comparison target remains `635026fabc38440fce5e1767d125026a17b12012`.
Source dispositions, complete official mechanisms, retained fork differences and executed
platform scope are recorded in Open-GPUI:

- `.agents/skills/open-gpui-upstream-sync/SKILL.md`
- `docs/imports/zed-gpui-import.md`
- `docs/imports/zed-gpui-upstream-sync-candidates.md`
- `docs/plans/2026-07-05-001-refactor-zed-gpui-upstream-sync-plan.md`
- `docs/verification.md`
- `docs/knowledge/engineering/current-state.md`

Continue from those exact sources. A later remote Zed HEAD is a separate observation; do not
silently redefine the completed comparison's target. Prefer complete official mechanisms as
baseline and refactor fork-owned changes around them. A retained divergence needs a concrete
consumer/contract, source evidence and focused verification. Do not rerun every unchanged
upstream mechanism merely to claim another synchronization milestone.

## Jellyflow Worktrees And Integration

The source audit baseline is Jellyflow `46a49c1db1eb4718d75bddf38803e1775d2e9fca`.
Its xyflow reference is `repo-ref/xyflow` at
`1bc405cd113618c17fd181b931a9fe573bec9fee`.

Two additional worktrees were created to keep distinct work separate:

- `F:/SourceCodes/Rust/jellyflow-gpui-adapter`, branch
  `refactor/maintained-gpui-adapter`: completed maintained-framework migration, locally committed
  and fast-forwarded into Jellyflow's original `main`. This worktree is clean and remains
  available for inspection; the Nara consumer uses the normal `jellyflow` checkout.
- `F:/SourceCodes/Rust/jellyflow-nara-integration`, branch
  `refactor/nara-node-integrity`: unfinished audit repairs. These changes are preserved for the
  next maintainer; they are not implicitly included in the adapter migration.

Inspect live status and the qualification section below before further integration. The user
redirected work toward compilation and practical integration before completing the wider graph
repair set. Do not discard the unfinished repair work or present its draft changelog as a green
release.

The adapter workspace and standalone product example now select
`https://github.com/Latias94/open-gpui` at the exact published `f75c10b...` revision. Nara's
independent GPUI consumer also selects that Git source. Equal package version `0.2.0` does not
permit mixing old registry, sibling path and Git instances in one native entity tree.

The adjacent Jellyflow manifests are needed even with `jellyflow-trial` disabled because Cargo
resolves optional path dependencies. This applies to the independent fixture, not the normal
Nara workspace or its default Editor dependency closure.

The library migration is `2deeeba79c538b47fcf45c1916bc886aa28a82dd`; the independent product
migration is `8a7c2df55acba7f00f5465d9c97ffd8387b791f0`. Both were subsequently
published on Jellyflow `main` with the user's November 12 authorization; the remote was queried
and matched `8a7c2df`. The local private read-side follow-up `d4125cc` is not yet published.
The server accepted the publication with a rules-bypass notice for three expected required
checks; their later CI outcome is not recorded here.
The unfinished integrity worktree still starts from the audit baseline and was not
merged. Read the maintained-source section of Jellyflow's
`docs/guides/native-adapter-authoring.md` for the new capture, focus, measurement and package
source contract.

The first connection is intentionally a **standalone Jellyflow-authoritative graph** rendered
through Nara's original HostedSurface/RenderHost/device/queue path. Two ViewSessions share one
graph runtime and history. This is not a second writable copy of the Nara ValueGraph document,
and does not yet establish Nara Session admission for Jellyflow commands.

The optional consumer entry point is:

```text
tests/fixtures/gpui-engine-consumer
feature: jellyflow-trial
example: gpui_view
interactive mode: --jellyflow
native qualification mode: --smoke-jellyflow
```

Use `--features jellyflow-trial,ui-work-observation` when building the qualification executable.
The normal Scene, typed graph, continuous gesture and independent View modes remain controls.
The final repository revisions, commands and executed results belong in the completed
qualification record below; a compiled example is not proof of GPU rendering or input.

## Ownership And Interface Findings

Keep the three projects independent by assigning one owner to each mutable fact:

| Fact or mechanism | Owner |
|---|---|
| UI entities, subscriptions, focus, text/IME, pointer capture and accepted native frame | Original Open-GPUI runtime and Window. |
| Engine device/queue, captured render programs, image Views, target/resource lifetime and actual submission | Original Nara Render/Host owners. |
| Generic node/real-port identities, connection interaction, graph-space geometry, local selection/viewport and optional graph layout | Jellyflow's existing owners, with native adapter obligations explicit. |
| Nara-authored graph content, semantic commands, inverse/history, saved content identity and compiled results | Selected domain owner behind original Workspace/Session admission. |
| Standalone Jellyflow-authored graph content and history | Jellyflow GraphRuntime when a product deliberately selects that model. |

The original ViewSession/adapter was coupled to writable `GraphRuntime`. The public immutable
Source prerequisite now lets existing Views read original external graph/incarnation facts
without constructing another writable runtime or history. Keep that Source local to the UI;
Nara's `Send + Sync` extension document retains authored commands, history and restorable
content identity.

The remaining native mutation interface must deliver typed canvas intents and exact original
Session outcomes. Native managed edits still expect synchronous Jellyflow commands. Do not
infer completion from publication, guess canonical domain IDs or forward opaque commands
wholesale. Canonical ID adoption must belong to the original operation. Preserve Jellyflow's
independent runtime; avoid writable mirrors, per-edit `replace_document`, universal forwarding
traits or Nara dependencies in Jellyflow. Choosing Jellyflow as the sole domain for a new tool
remains possible, with explicit storage/thread/checkpoint/admission work.

## Jellyflow Roadmap

| Order | Work | Trigger and acceptance |
|---|---|---|
| 1 | Maintain the completed framework migration and its native consumer. | Root/product targets compile against one published Git source; preserve accepted measurement, native controls, capture/cancellation, Undo and retirement in their original owners. |
| 2 | Fix confirmed graph correctness defects in their owners. | Preserve existing public regressions and add the smallest case for each newly confirmed failure; graph and history remain unchanged on rejection. |
| 3 | Open the external-document seam for one real Nara graph domain. | One authored source/history; node and ordinary property editor share Session admission, Undo/checkpoints/Save/Reopen; two Views retain separate local state. |
| 4 | Harden node/overlay/LOD capabilities and measurement publication. | Old mounts, popups and hidden geometry cannot regain input; speculative/refused native work never replaces accepted geometry. |
| 5 | Address measured scale limits and reusable layout. | Attribute visible/affected work; compare paging, routing, collision checks and native layout on named graphs before widening limits or adding indices. |

The [audit](../research/2026-11-12-jellyflow-node-editor-audit.md) records four executed defects,
source-only findings and exact reproduction names. The unfinished integrity worktree contains
marquee and tidy-tree repairs with a clean bounded source review, plus draft shared node
candidate validation. Its first combined test build failed because a new external test imported
a private Diagnostic path; that import was corrected but has not been requalified.

Two review findings in that draft remain unresolved:

1. Undoing an added node after a history-free topology update can leave newer owned ports
   dangling. Candidate validation must cover current affected dependencies, not skip all facts
   merely because the node is absent from the candidate.
2. New candidate validation reads are absent from `GraphCommandPlanningWork`. Keep the existing
   receipt honest rather than claiming locality from undercounted work.

Do not merge that draft until those findings and its actual tests are resolved. The corresponding
owner plan is `docs/plans/2026-11-12-node-integrity-prerequisite-plan.md` in that worktree.

## Nara Roadmap

Nara should continue product workflows while the other sessions maintain their libraries.
The graph integration consumer is useful evidence, not the default Editor or a completed
Shader Graph. Keep the existing finite graph as a control.

| Order | Work | Complete consumer and acceptance |
|---|---|---|
| 1 | Preserve the standalone Jellyflow integration control. | A standalone library graph has two live Views, real nodes/ports and a native operation; close retires library state while the independent engine producer remains valid. |
| 2 | Complete the external-domain graph consumer without another history. | Domain, public Session/file workflow, immutable Source, renderer-neutral requests and Open-GPUI journal transport now use one original owner and exact outcomes. Next deliver View-local intrinsic height, two native Dialogue Views and ordinary property editing. |
| 3 | Complete reusable panel/component and visible-UI operation seams. | External components and agent/test consumers use presented identity, focus/text/overlay services and exact outcomes without private hooks. |
| 4 | Advance the independent Editor composition through full workflows. | Qualified source/asset/property/custom tools, workspace/docking and original View lifetimes; the selected build has no Dear runtime dependency. Keep the reference selectable until the replacement is complete. |
| 5 | Qualify the authored game UI choice separately. | HUD and multiple logical roots, player/gamepad focus, time/pause, cooked resources and world-space/material/depth needs; share suitable mechanisms without importing the whole Editor. |
| 6 | Optimize actual engine composition and complete product delivery. | Attribute UI/Scene/Host costs, preserve color/alpha/resource authority and execute the remaining foundation plan, including source-built extension iteration and reference-game delivery. |

Nara owns its device/queue/targets and render program integration. If framework raster layers or
retained GPU geometry become useful, consume them through the existing finite Render/Host
contracts. Do not install a second render graph or expose raw cross-source WGPU handles to make
one toolkit fit. Keep linear-light working/blending, authored/display encoding, alpha convention
and output/tone-mapping policy explicit; an sRGB attachment alone is not the complete contract.
The current color study and actual source/processed-image consumers are the controls.

The selected external-domain successor is the
[Dialogue Graph plan](2026-11-12-dialogue-graph-integration-plan.md): stable choice/port IDs,
atomic choice/link changes, native text/popup/variable-height content, a separate property
editor and two Views, one domain history, return-to-saved clean state, branch-safe Undo/Redo,
Save/Reopen and a real conversation preview. Domain, file workflow and preview exist; native
controls and external edit intent/completion still need implementation. Let that consumer
drive Jellyflow's seam rather than first designing a universal graph/compiler/plugin protocol.

Editor and game UI can reuse suitable mechanisms and complete unstyled behavior, while keeping
product roots, state, skins and adapters distinct. A game choice still needs authored HUD,
split/world-plane roots, time/pause, player/gamepad focus, cooked resources, materials/depth,
lifecycle and cost evidence. Editor success does not establish those requirements.

## Research And Reference Map

- [Jellyflow audit and executable reproductions](../research/2026-11-12-jellyflow-node-editor-audit.md).
- [Dialogue Graph implementation, external-interface gaps and qualification](2026-11-12-dialogue-graph-integration-plan.md).
- [Engine integration layers, shared mechanisms and hosting](../research/2026-11-11-open-gpui-engine-integration-layers.md).
- [Compositor/cache source comparison](../research/2026-11-11-open-gpui-engine-integration-layers.md#compositor-and-cache-research-direction).
- [GPUI critique, Entity granularity and invalidation](../research/2026-11-11-open-gpui-engine-integration-layers.md#gpui-critique-and-state-invalidation-research).
- [Maintained framework and engine-fit study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-maintained-open-gpui-and-engine-fit).
- [Independent engine consumer and run modes](../../tests/fixtures/gpui-engine-consumer/README.md).
- [Workspace/frontend and complex-editor ownership, ADR0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
- [Shared UI/product layers, ADR0107](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md).
- [Original Render/View authority, ADR0111](../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md).
- [SDR color, alpha and output encoding, ADR0092](../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md).
- [Implementation evidence and remaining gaps](../architecture/adr/implementation-status.md).
- Nara local references: `repo-ref/zed`, `repo-ref/imgui`, `repo-ref/egui`, `repo-ref/godot`.
- Jellyflow local reference: `repo-ref/xyflow`.
- User background files: `D:/Frankorz/Downloads/gpui_research.txt` and
  `D:/Frankorz/Downloads/open-gpui-nara-compositor.md`. Source comparisons in the linked study
  supersede unverified claims in those inputs.

## Qualification And Final State

The compilation-and-first-native-connection prerequisite is complete. Independent Open-GPUI
and Jellyflow maintenance can proceed while Nara advances the external-domain product workflow.
The full foundation rebaseline, complete Editor and game UI selection remain active work.
The qualified Nara consumer implementation is
`99c2b26164fcc9929fc1e8224a0429cd380164c4`.

| Check | Actual result and scope |
|---|---|
| Published Open-GPUI | `main`/`origin/main` verified at documentation handoff update `625173c`; code consumers retain `f75c10b2`. The explicit main publication is complete. |
| Migrated Jellyflow adapter | 519/519 nextest tests passed, no skips; run `25b9407c-fcc2-4729-9b2b-8c8939119dd4`. |
| Separate Jellyflow native product | Locked Windows all-target/all-feature compilation passed, including both native binaries and real-window-smoke targets. Product runtime was not rerun. |
| Existing Nara graph/domain/public Host controls | Release nextest 46/46 passed, no skips; run `25c3e58e-503e-4e1c-842f-861308a607ce`. |
| Actual DX12 native cohort | 3/3 passed, four unselected tests; run `d06af434-2830-4bc5-b43a-97c4cabe72aa`. |
| Actual Vulkan native cohort | 3/3 passed, four unselected tests; run `fefe7d07-e079-40ce-9fbd-bc0513659039`. |
| Correctness and conditional compilation | Adapter correctness Clippy passed; Nara fixture all-target correctness Clippy and feature-off release example compilation passed, with existing Nara/consumer warnings. |
| Supporting checks | Root/product/fixture formatting passed. Six existing Python tests, three generated manifest parses and neutral dependency checks passed. These do not establish Cargo package publication. |

Both native cohorts ran serially on Windows 11 with an NVIDIA RTX 4090. Each includes the new
Jellyflow workflow and the original complete Scene and graph-gesture controls. The new workflow
proves real graph/control pixels in the original GPU submission, overlay input isolation,
independent local control state, captured node previews, exactly one completed drag/history
entry, Undo, independent pan/zoom and explicit original-host retirement. It observes accepted
Window geometry and injects framework input; it does not claim physical input or scanout.

The ordinary optional consumer command is:

```text
cargo +1.95.0 build --locked --release --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view --features jellyflow-trial,ui-work-observation
target/release/examples/gpui_view.exe --jellyflow
```

Set `WGPU_BACKEND=dx12` or `vulkan`, `NARA_WGPU_FORCE_FALLBACK=0` and the shared target
environment above. `--smoke-jellyflow` runs the finite native acceptance. Its ignored
`gpui_view` integration test uses the same prebuilt executable; set `NARA_GPUI_VIEW_EXE` to its
absolute path when the test profile differs.

Recoverable integration corrections:

- Port markers must contribute a finite layout size inside the semantic wrapper; absolute
  positioning belongs outside it. Missing real ports remain errors.
- Retained nodes observe their binding so accepted graph/measurement changes refresh their
  ordinary native contents.
- The full-screen Jellyflow layer occludes the hidden Scene. The native test clicks the actual
  hidden Move-button coordinate and checks both its command queue and original source.
- Press does not itself prove selected state, and a new frame does not prove command completion.
  Drag/Undo wait for the exact source/history result and matching accepted presentation under
  the original deadline.
- Retained library hosts close through their public `close_in_window` before the original
  logical surface closes. Repeated shutdown does not revisit a removed Window root.

The source audit's wider graph defects remain open. The dirty integrity-repair worktree is
preserved, including the two unresolved review findings listed above; neither its draft
changelog nor its corrected-but-unrerun test build is release evidence. No Shader Graph,
external Nara domain bridge, new Editor default, complete docking or numeric performance
qualification follows from these results. Source and exact run details are consolidated in the
[maintained consumer study](../research/2026-11-11-open-gpui-engine-integration-layers.md#maintained-jellyflow-consumer-qualification).

### Dialogue Domain Follow-Up

Nara `ddcc880893c0081e6ceb76be31d995e3ce7ee37b` adds the new
`consumer.dialogue-graph` extension document, atomic dynamic choices/links, bounded versioned
codec, immutable snapshots, live incarnation facts and one-transition conversation preview.
The original Session owns admission, history, saved-content identity, captured Save and Reopen.
The feature-gated Jellyflow projection is read-only; it creates no runtime or history.

Release nextest passes 75/75, no skips, run `ed15941f-9ba7-4568-8bdc-b58afcda3798`.
This combines 21 domain, four projection and four public Dialogue tests with 46 existing
controls. All-target correctness Clippy, feature-off all-target compilation and package format
checks pass. Independent domain and facts/public Session reviews found no concrete P1/P2.
The plan records the initial projection-test serialization failure and its correction.
No new native Dialogue qualification follows from those CPU/public results.

The local [private read-side prerequisite](2026-11-12-dialogue-graph-integration-plan.md#private-read-side-extraction)
at `d4125cc` separates graph facts, indices and query implementation from command state.
Runtime/public tests pass 599/599 and adapter tests 519/519; Clippy, format and independent
review pass. At that checkpoint, no public external-source or asynchronous command interface
was added.

### External Source Follow-Up

The subsequent [public immutable Source prerequisite](2026-11-12-dialogue-graph-integration-plan.md#public-immutable-external-source)
at local Jellyflow `046a660` implements shared read-side publication separately from standalone mutation rights. The Source
retains the original immutable graph allocation and complete incarnation observations. Existing
Views receive version-checked publications and an explicit reset on original document
replacement, keeping separate selection, navigation and accepted geometry.

Nara's four additional public Session/file tests cover two Views over one original document and
history, successful Noop, coalesced remove/Undo, equal-content Save/Reopen and View-local
geometry. Unsupported commands preserve the valid View; the returned original feedback can
have its commands moved out and its layouts resubmitted with that same token. Stale layouts
cannot revive restored node identities. Runtime/public release tests pass 620/620 and Nara's
combined domain/graph/public cohort 79/79. The retained/egui adapter controls pass 560/560.
Correctness Clippy, feature-off compilation, format, document checks and separate source
reviews pass within this scope. The library's published baseline remains `8a7c2df`;
`d4125cc` and `046a660` are local follow-ups.

Typed canvas intents and exact asynchronous completion remain the next library boundary.
Associate accepted canonical IDs with their original operation: the domain owns node/choice
allocation and derives link identities. Native intrinsic height must remain View-local.
Nara retains its domain commands and Session admission; the library retains standalone
`GraphRuntime`. The native two-View Dialogue consumer and its DX12/Vulkan acceptance remain
open. Keep the unmerged integrity repair worktree and the published library baseline distinct.

### Exact Domain Outcomes And Atomic Canvas Commands

The [next owner prerequisite](2026-11-12-dialogue-graph-integration-plan.md#domain-outcomes-and-atomic-canvas-operations)
replaces the unit-only extension apply result with associated `Outcome`. Existing owners that
need no additional result declare `Outcome = ()`. Accepted reports expose
`extension_outcome`, retaining the original command, accepted revision and a nominally typed
domain value. Rejection supplies no accepted outcome; successful Noop retains its precise
result while preserving unchanged content/history. Clones share the original observation.

Dialogue returns actual node/output and ordered choice allocations. Its `CreateFromWire`,
`Reconnect` and structured `Remove { nodes, links }` each validate and commit one candidate,
including all-or-nothing refusal and one Undo boundary. Creation helpers consume those results
rather than discovering IDs by a snapshot difference. Source schema/codec remain unchanged.

Tooling tests pass 179/179, the independent domain/graph/Session cohort 94/94 and independent
panel consumers 46/46. The complete public trajectory joins both wire directions, dynamic
choice IDs, reconnection, mixed removal, Undo/Redo, saved-state identity and Save/Reopen with
the original owner. Historical results remain readable after real Session close and Reopen.
Separate generic-result, atomic-domain and scoped-unwind reviews find no concrete P1/P2.
Affected root persistence, slot-invariant and existing DX12 extension-document controls pass
45/45, including one actual native trajectory. Correctness Clippy, behavior-language trial
all-target compilation, fixture feature-off compilation, format and document checks pass in
the stated scope. Existing unrelated warnings remain. This adds no new Dialogue native UI or
language/tool-install runtime qualification.

Continue with the original Jellyflow journal/interaction owners: typed external transport,
exact asynchronous completion and canonical ID adoption, followed by View-local intrinsic
height and the two native Dialogue Views. An immutable Nara domain outcome is execution
evidence, not the one-shot external completion capability or accepted native presentation.

### External Canvas Requests And Original Publications

The [request successor](2026-11-12-dialogue-graph-integration-plan.md#exact-external-canvas-requests-and-publication-receipts)
implements the renderer-neutral external mutation path and an independent Nara Session consumer.
Its local Jellyflow baseline is `4bcd0f2591d1f14146c56d924059a51f9aa89686`; the private
read/Source and request successors have not been remotely published. Nara still selects
Open-GPUI code `f75c10b2`, independently of the framework maintenance checkout.
`DialogueCommandReceipt` retains the exact post-command source alongside its effect.
`DialogueCanvasRequest` lowers captured semantic work into an original Session command;
its sealed publication pairs the exact accepted outcome with the original external receipt.
Canonical node/port/edge identities come from the domain result, with a complete creation-port
mapping. Delayed A results retain their own evidence after B; neither projection rollback nor
command reexecution is needed.

Both reconnection directions, creation during reconnection, movement and mixed deletion retain
one original candidate/history. External pending work survives its own publication and detached
old-owner notifications; reset and close retire it. Canonical success still waits for the
original View's accepted route. Unknown delivered work cannot infer success or offer speculative
retry. The owning plan records actual tests, review corrections and qualification limits.
Final Runtime/public controls pass 647/647, existing adapter controls 560/560 and the independent
Nara domain/graph/Session/file cohort 110/110, with scoped correctness, feature-off, formatting
and document checks. These are CPU/protocol/file results; they add no new native Dialogue claim.

Continue in Jellyflow's existing native journal, interaction and frame/close owners. Add external
asynchronous transport while retaining prefix order, one in-flight submission and exact admission
dispositions. Intrinsic height must be View-local throughout layout, routing and accepted geometry;
multiline controls must preserve the full source. These and the two native Dialogue Views remain
the next complete consumer, not a new default Editor or completed R11c.

### Native External Journal Follow-Up

Local Jellyflow implementation: `3a430b906d8b902bee6a682c436dcdede1de7cd5`. The source,
request and native-transport successors remain local; Open-GPUI stays pinned to `f75c10b2`.

The [native transport successor](2026-11-12-dialogue-graph-integration-plan.md#implemented-native-external-transport)
now extends Jellyflow's original native-adapter journal and interaction owner. An explicit
application handler receives each external request after its original frame feedback and outside the
Host borrow. Its completion port retains the exact original slot. Source retirement preserves
already received results; close never replays delivered work; pure completion waiting stops
interaction ticks. Standalone and external bindings refuse the wrong writer explicitly.

Runtime/public Source controls pass 647/647 and the journal cohort 45/45. The complete
Open-GPUI/egui adapter library/integration cohort passes 583/583, no skips, across 24 binaries,
including nine new public TestApp/Visual external-command trajectories. Two independent reviews
closed the corrected frame-association, panic-custody, source-retirement and idle-scheduling
findings. The linked plan records actual run identities, the real-clock VRR test correction
and the distinction between framework tests and desktop/GPU acceptance.

The independently locked Nara domain/graph/Session/file consumer passes 110/110. Its release
native example was rebuilt; the existing Scene, captured graph gestures and standalone
Jellyflow/two-View controls pass 3/3 on DX12 and 3/3 on Vulkan. These retain the original engine
renderer and ordinary-writer regression scope, with three unselected tests per native run;
they do not qualify a new external Dialogue UI.

Continue with View-local canonical size through the existing complete geometry publication,
then the two native Dialogue Views, ordinary multiline/property/popup controls and the original
Session/file/preview workflow. Canonical size is distinct from displayed canvas bounds at
non-unit zoom. The audit identified consumers in semantic zoom, placement, port/root measurement,
mount restoration, Fit, admission and routing; do not introduce a second geometry owner.
This remains a product integration prerequisite, with no new Editor default or complete R11c.

### View-Local Intrinsic Geometry Follow-Up

Local Jellyflow baseline: `74f7f54ab1b6858ba177f0973aa05800ee72b906`. Runtime geometry
`8d9c0a8`, optional layout `994cb17` and Model selector repair `93dd41c` are its predecessors.
These commits remain local; the Open-GPUI selection remains `f75c10b2`.

The [qualified intrinsic unit](2026-11-12-dialogue-graph-integration-plan.md#implemented-view-local-intrinsic-geometry)
keeps automatic node dimensions in each original View's complete accepted layout. Both source
kinds retain their original document/history/persistence; explicit authored resize remains
recorded and savable. The existing reducer now stages layouts instead of graph commands.
Renderer/lifetime/reset and authored-size changes retire invalid seeds. Semantic zoom, real
ports/routes, remount, offscreen Fit and movement constraints consume the correct canonical or
actual canvas geometry. Layout Context uses `node_sizes` / `with_node_sizes`; the obsolete
serialized Context field refuses explicitly, with no graph-file migration.

Runtime/layout pass 715/715, adapters 585/585 and the independent Nara consumer 110/110.
Rebuilt existing Scene, graph-gesture and standalone Jellyflow/two-View controls pass 3/3 on
DX12 and 3/3 on Vulkan. These do not qualify an actual external Dialogue desktop workflow.

At the intrinsic geometry checkpoint, the consumer gate retained three original Workflow
sample failures: Select overlay registration across semantic zoom, popup close after
culling/restoration, and the zoom=0.5
port hit boundary. That product checkpoint passed 22/25; exact parent plus selector-only
repairs reached 20/23 with the same failures. The repair below preserves those assertions and
resolves the original presentation/overlay/input owners without recreating graph size writes.
A public framework observation interface should eventually replace the sample's private
debug-selector encoding.

The library mechanism can be maintained independently from this fixed baseline. Full native
Dialogue multiline/property/popup editing, its two Views and original Session Save/Reopen/
preview still belong to Nara's next product deliverable. Canonical docking, full Editor/game
UI choice and the foundation Goal remain open.

### Workflow Native Control Repair

Local Jellyflow baseline: `0a9453a6df5483d30ad05195bd62fdbdab38a97e`. The source selection
remains Open-GPUI `f75c10b2`; this local successor has not been remotely published.

The [qualified Workflow successor](2026-11-12-dialogue-graph-integration-plan.md#qualified-workflow-repair)
closes all three original sample failures without removing their assertions. Complete Workflow
and model-provider component tests pass 25/25; focused admission/port/capture owners pass 24/24.
The complete Open-GPUI/egui adapter library and integration cohort passes 588/588.
Stable native component ancestry preserves Select state across semantic zoom. Accepted
scene admission restricts new graph actions without a full-canvas hit blocker; native controls
keep both event phases, existing capture settles normally and Focus-only pinning remains local.
The port diagnostic proves a fixture `WindowInactive` refusal, corrected by real window
activation rather than changing hit bounds or connection policy.

The rebuilt original Nara standalone Jellyflow/two-View native trajectory passes 1/1 on DX12
and 1/1 on Vulkan, six unselected per run. These actual GPU controls retain pixels, local
controls, captured movement, Undo, independent navigation and retirement. They do not qualify
the complete external-domain Dialogue desktop product. The linked plan owns exact runs,
review scope and the still-open multiline/property/popup, Session/file and preview acceptance.

### Ordinary Window Input Entry

Nara `1cea9f6` sends node/control clicks, captured movement and middle-button pan through the
ordinary Winit input converter, with explicit focus loss/regain and physical coordinates.
The complete original GPU trajectory passes on DX12 and Vulkan at actual scale 1.5. The
[qualification](2026-11-12-dialogue-graph-integration-plan.md#interactive-entry-follow-up)
retains exact runs, unchanged assertions and its direct-platform-input exceptions. The visible
title identifies its drag region. Physical desktop drag delivery remains an unresolved user
observation; these injected events do not close it.

## UI Quality And Component Development

The user's two-monitor Zed comparison is evidence against treating desktop arrangement as a
sufficient explanation of the Nara movement failure. The independently drawn source Editor
currently uses Winit and Nara's earlier controls, not the maintained GPUI Windows window or
`open-gpui-ui-components`. The standalone GPUI graph also uses Nara's Winit-hosted surface.
Distinguish those native entry points when reproducing the report. Do not imply that Zed,
the framework's native Windows adapter and Nara's external Host share one window implementation.

The read-only Windows comparison finds two concrete differences, not a reproduced cause.
Winit 0.30.13's `platform_impl/windows/event_loop.rs` preserves logical client size during
`WM_DPICHANGED`, adjusts the title-bar grab position and may search positions to reconcile the
selected monitor. The pinned GPUI Windows `events.rs` applies Windows' suggested rectangle.
Their move-loop scheduling also differs: GPUI has a native movement timer, while Winit can
synchronously enter Nara painting from `WM_PAINT`. No current drawn cursor-confinement or
forced-position loop was found. A real title-bar move/DPI trace must distinguish a rectangle
that fails to cross from a moved window whose content stops presenting. Injected normalized
events and programmatic placement cannot validate the native title-bar correction branch.

### Component Responsibilities

Continue the existing layers rather than starting another general UI framework:

| Layer | Responsibility and first consumers |
| --- | --- |
| Open-GPUI runtime and platform/render adapters | Element/Entity lifecycle, layout, text, input/focus/IME, custom drawing, admitted presentation and resource lifetime. Keep Nara's original device/queue/Host integration. |
| Existing `open-gpui-ui-core` and `open-gpui-ui-components` | Reusable control behavior and concrete GPUI controls: Button, TextInput/Textarea, NumberInput, Select, Tree/Table, menus, scrolling and virtualization. These component implementations are fork-owned code, not automatically validated by Zed's upstream runtime pedigree. |
| Nara Editor theme and components | Design Kit palette, measured density/typography/icon recipes, toolbar/document tabs, property rows and groups, resource-reference fields, hierarchy rows, tool feedback and graph property composition. Session/document/history and graph authority stay with their current owners. |

Use the [complete Design Kit reference](../design/editor-reference/README.md), especially its
Scene/Inspector and Properties sheets. Its graphite surfaces, restrained teal emphasis,
compact alignment and explicit state hierarchy are the visual direction. Preserve sRGB input
conversion through the existing color contract. Reconcile actual font faces, CJK fallback,
baseline metrics and enlarged text instead of shrinking text or synthesizing weight to match
the picture. Effects lacking a current drawing capability remain explicit gaps, with simpler
coherent rendering until their owner can support them.

Build Nara components through real pages before extracting another package. Generic controls
must work without Nara dependencies; Nara-specific controls may compose them or author ordinary
custom GPUI elements. There is no mandatory widget registry or second focus/input implementation.
Editor and game UI can reuse appropriate mechanisms without sharing one product skin.

Component state is View-local: drafts, expansion, scroll, focus and popup state must survive
compatible parent updates through stable identity and original subscriptions. Accepted values,
selection facts, dirty state and command completion come from Nara's existing owners. Choose
Entity boundaries around independently updating regions through the real page, and observe
notification/render/layout work for a single-field edit. Reusing control code or a Scene packet
does not by itself establish incremental layout or cheap whole-Editor refresh.

### Immediate Work And Acceptance

1. Reproduce movement on the actual native entry. Compare both directions across negative
   desktop coordinates and different DPI/work areas. Observe Moved, ScaleFactorChanged, Resized,
   client/outer bounds and event-loop responsiveness. Audit minimum-size constraints, resize
   reconciliation and synchronous rendering during the native move loop. Verify ordinary
   dragging separately from programmatic placement; keep the user's report open until the
   corresponding path works. Moving an existing native window is distinct from moving docked
   Views between windows and does not require a new docking owner.
2. Attribute the current performance complaint before claiming a release improvement. The
   source audit confirms that every converted event, including ordinary pointer movement,
   requests redraw. Ordinary Hierarchy also marks paint dirty unconditionally, and Inspector
   builds its description before immutable snapshot reuse. Panel images already reuse an
   unchanged original frame. Refine visible-state invalidation through original reducers;
   `consumed` alone is not a redraw decision. Preserve exposure, geometry, business updates,
   pending presentation and deadlines. Measure quiet idle, movement within a control and
   boundary crossings, with description/layout/text/upload/composite/present CPU work separated.
3. Make the current Source page usable: compact the command region, give editing content
   space, clarify selection/document/source context and align existing property rows. Keep
   this frontend's changes bounded; the reusable product component catalogue and complete
   docking/workspace development belong to the GPUI route.
4. Exercise the selected framework controls in the complete Dialogue consumer: multiline
   editing, number/text properties, Select/menu popups and two native Views. A named defect
   gets a focused regression at its original control or integration owner; report library
   defects to the independent maintenance session against the fixed source pin. A catalogue
   listing or old aggregate green tests do not prove these combined native interactions.
5. Qualify Nara's first coherent visual page with actual accepted-frame captures and real
   editing, Cancel/Apply, Undo/Redo and Save/Reopen. Check keyboard/focus, invalid/disabled/
   read-only/pending/result combinations, long paths/CJK and the actual available DPI/work-area
   transitions. Docking, foreign-platform execution, physical IME and numeric performance
   budgets retain their own unexecuted scope. Do not qualify the page through static mockups.

The November 13 visual review inspected the two reference PNGs and implementation source, not
the live application's pixels. The performance review was also source-only: the prior debug
process had already exited, so no current CPU/FPS sample exists. The release Editor build is a
comparison prerequisite, not evidence of a speedup. Full Editor visual acceptance and R11e
cost acceptance remain open.
