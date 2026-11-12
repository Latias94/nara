# Nara UI Integration And Product Development Handoff

Prepared November 12, 2026 for the Nara development session.
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
Its domain, public Session workflow and immutable facts are qualified at `ddcc880`; native
external-document hosting remains open. Continue from that boundary rather than repeating the
standalone connection or treating the missing interface as a completed graph editor.

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

The current ViewSession/adapter is coupled to a writable `GraphRuntime`, not an arbitrary
read-only external document. Nara extension documents require `Send + Sync`, while the runtime
uses `Rc`; its history depths also do not provide Nara's restorable content checkpoint identity.
These are actual integration gaps. Adding `Arc<Mutex<_>>`, mirroring writes after execution,
or calling `replace_document` after every edit does not resolve them.

For existing Nara domains, trial a fact/intent/completion seam driven by one complete external
consumer. Immutable graph facts enter a local View; accepted intents execute through the original
domain; exact outcomes reconcile the view. Preserve Jellyflow's independent runtime. Do not
copy every runtime method into a generic forwarding trait, add Nara dependencies to Jellyflow,
or introduce another universal UI graph. Choosing Jellyflow as the sole domain for a new tool
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
| 2 | Complete the external-domain graph consumer without another history. | Domain, public Session/file workflow and immutable facts are qualified at `ddcc880`; next implement the external-document interface, two native Views and ordinary property editor over that original owner. |
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
controls and the external-document interface still need implementation. Let that consumer
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

Jellyflow still binds `ViewSession` to the object that also executes commands and history.
The local [private read-side prerequisite](2026-11-12-dialogue-graph-integration-plan.md#private-read-side-extraction)
at `d4125cc` separates graph facts, indices and query implementation from command state.
Runtime/public tests pass 599/599 and adapter tests 519/519; Clippy, format and independent
review pass. No public external-source or asynchronous command interface is added yet.
The coordinated next library work separates shared read-side publication from standalone
mutation rights, then supplies typed canvas intents and exact completions.
Intrinsic height remains View-local. Nara retains its domain commands and Session admission;
the library retains standalone `GraphRuntime`. Do not bridge this gap with a writable mirror,
per-edit `replace_document`, a universal forwarding trait or a Nara dependency in Jellyflow.
