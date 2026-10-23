# Chat-Informed Engine Architecture Review

Date: 2026-10-23. Source baseline: `f91466b0626543688bd98923dbd04fb842028120`.
Status: Review complete; replacement contracts and implementation units remain Proposed.

## Scope And Evidence

The user supplied the Markdown export **Rust game engine architecture choices**
(`Rust游戏引擎架构选型 (2026-10-23).md`, 3,609 lines, SHA-256
`988057509bb3763ec5bdb9650631012e0afdbb1d6b1059509b4f59168d3010d2`).
The earlier unauthenticated request through the supplied local HTTP proxy returned HTTP 403.
This review uses the supplied export, not an assumed authenticated conversation response.
The private conversation is not copied into the repository. Its advice is research input, not
an Accepted architecture contract. Some example blocks are absent from the export; missing code
is not reconstructed or treated as an executable specification.

Four read-only Codex review lanes cover runtime/tasks, persistent content/schema, render/UI,
and module/extension/automation interfaces. The integration owner challenges their findings
against current owners, real callers, existing qualification and the conversation's later
corrections. Reviewers do not run Cargo or mutate source. `vendor/` is pre-existing and excluded;
`repo-ref/` remains read-only.
A fifth independent reviewer checked the integrated audit and plan against the targeted source
and Accepted ADRs and found no blocking factual or design issue. This is a documentation/design
review outcome, not qualification of the unimplemented refactors. All five review agents are closed.

References are pinned source checkouts, not latest-release claims:

- Bevy: `85503941e488152711a4695bbe248e9ed9f453fc`.
- Godot: `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

The [final U1-U8 qualification](../knowledge/engineering/verification/2026-10/2026-10-23T075653Z-post-boundary-u1-to-u8-final-integration-and-requirement-audit-c11cf626f4494238bf3d1a2532e40634.md)
is the completed starting point. Its results are historical evidence, not a newly executed test
run. Old repaired findings are not reopened without a distinct current counterexample.
This review does not change Accepted ADRs, implement a production refactor, establish platform
support, or create a new Goal.

## First-Principles Review Lens

The useful unit of architectural criticism is a missing fact at an owning operation, not a large
file, many crates, a familiar engine pattern, or the presence of a low-level Rust type.

For every finding, identify:

1. The source of truth and its lifetime, scope and identity.
2. Who can mutate it, and the point at which a candidate becomes authoritative.
3. What a caller must know about ordering, failure, cancellation and resource retirement.
4. A reachable consumer sequence that violates a stated invariant or repeats owner work.
5. The replacement contract, obsolete mechanisms to remove, and behavior-level acceptance.

The outcome can be a contract correction, an interface deepening, a measured investigation,
or a deliberate capability limit. These are different outcomes; only the first is necessarily
a current correctness defect. Fearless refactoring removes wrong contracts without preserving
unreleased aliases; it does not justify inventing unused universal interfaces.

## Conversation Recommendations Versus Current Nara

Conversation line ranges refer to the supplied export, not versioned repository files.

| Topic | Conversation evidence | Current disposition |
| --- | --- | --- |
| ECS suitability | Lines 1277-1402 distinguish storage/execution from editing semantics | Retain the original Bevy ECS substrate. Stable identity, document history and source preservation are separate concerns, not reasons to replace the kernel. ADRs 0002/0006/0117 own the current split. |
| Authoring authority | Lines 178-229 propose a document; lines 1315-1366 later recommend an authoritative EditorWorld to avoid two mutable authorities | Keep exactly one authoring authority, but do not adopt the later recommendation mechanically. ADR 0090 requires preservation of records without executable bindings; a typed preview cannot represent all authoritative source data. Challenge synchronization, not the mere existence of a document and a projection. |
| Reflection | Lines 119-132 and 689-753 separate stable schema from native type access | Retain catalog, stable identity, native binding and frozen execution distinctions under ADR 0081. A process-global mutable TypeRegistry would merge scopes that Nara intentionally separates. |
| Extension tiers | Lines 755-813 and 1406-1439 narrow arbitrary dynamic unloading | Source-built Rust extensions, data-only authoring and any future dynamic Adapter are distinct products. No stable native Rust ABI, transparent unloading, or universal Wasm seam is inferred from registration. |
| Gameplay languages | Lines 817-974 propose Lua/C#; lines 1523-1554 revise the priority | Language choice is a product decision, not a substrate bug. STRATEGY keeps a complete Rust path and an optional Adapter. OQ-007 owns the concrete gameplay-language investigation; this review does not select a universal ScriptHost. |
| Agent integration | Lines 1736-1754 and 2029-2063 keep model/protocol code outside the core | Reuse typed observations, domain commands and exact results. Evaluate a thin external Adapter only against a real automation workflow; do not add a second mutation authority or a model-specific core dependency. |
| Transactions | Lines 1861-1889 explicitly exclude irreversible external effects from atomic rollback | Document transactions and filesystem/process operations require distinct contracts. A batch wrapper is not proof of rollback or cross-owner atomicity. |
| Frame versus background work | Lines 2452-2500 and 2670-2691 distinguish scheduling domains | Preserve ECS access-aware frame execution and owned cross-frame task preparation. An owned closure queue is not automatically a Future executor. |
| Job ownership and cancellation | Lines 2586-2623 and 2796-2820 emphasize late-result admission and physical lifetime | Retain request-specific baselines, task custody and physical close. A cancellation request or generation number alone is not publication permission or physical completion. |
| Worker budgets | Lines 2564-2572 warn against independent machine-sized pools | Existing [task attribution](../research/2026-10-21-task-execution-baseline.md) already distinguishes a fixed authoring-pool correction from the remaining runtime policy. Do not report the old Editor allocation as current or assume duplicate Bevy workers. |
| Render organization | Lines 257-287 and 2695-2724 suggest extraction and a graph | Retain backend-neutral owned data and the WGPU Adapter. ADR 0105's closed plan and ADR 0111's fixed cross-target transaction already have explicit expansion triggers; a public graph is not required just because graphs are common. |
| UI | Lines 231-255 and 2325-2349 distinguish widget state from scene state | Toolkit choice does not define document ownership. Dear ImGui Editor presentation and runtime UI contracts are separate; replacing both with one tree is not justified by the generic retained-UI recommendation. |
| Assets and artifacts | Lines 3137-3183 separate source, derived artifact and runtime resource | Preserve distinct identity and ownership. ADRs 0103/0086/0088 and actual Check/Play/Export consumers determine the supported slice; content addressing, cooking and sub-assets need concrete input/platform requirements. |
| Identity and residency | Lines 2996-3064 and 3185-3233 separate persistent identity, runtime handle and GPU ownership | Retain world-scoped full Entity identity, stable source IDs and exact prepared resource captures. A Handle must not silently acquire a strong-residency contract. |
| Coordinates and time | Lines 3235-3292 distinguish world/UI coordinates and time domains | ADRs 0018/0039 already own explicit units, fixed simulation, paused service work and separate real/virtual/fixed time. Do not adopt meter, Reverse-Z or future 3D conventions as an unreviewed change to supported 2D semantics. |
| Physics, animation and networking | Lines 3394-3471 recommend explicit authority but defer broad implementations | Preserve the existing concrete-consumer triggers. Missing a solver, animation graph or rollback implementation is a capability gap, not proof of a broken current substrate. |
| Compatibility and unknown data | Lines 3473-3504 reject silent loss of unavailable components | ADR 0090 already owns bounded semantic preservation and strict Runtime rejection. Unreleased API replacement needs no compatibility scaffolding or new migration guide, but authoring must still preserve supported project data. |

## Strategic Tension

The conversation contains an explicit preference for Unity-like dynamic extension experience
(lines 481-506). The current Strategy prioritizes a complete Rust production path and rejects
a mandatory second language, stable native dynamic ABI or universal language-neutral host before
consumer evidence. That difference should be acknowledged, not silently converted into an ECS,
schema or service-kernel rewrite.

A bounded future extension experiment should name one complete user workflow, one execution
mechanism, its owned data and retirement policy, target platforms, deployment/debugging needs,
and restart-versus-reload behavior. Its results may justify a separate product/ADR decision.
An installable scripted editor tool, a source-built Rust gameplay package and a replaceable
native GPU plugin do not need the same authority or lifetime contract.

## Findings And Redesign Priorities

All four review lanes returned findings. Integration has reproduced F4/F5/F7 through temporary
public-interface probes; the other findings remain source-confirmed. The distinction between an
implementation defect, a costly Accepted contract and a budget investigation is intentional.

| ID | Priority and class | Observed fact | Recommended disposition |
| --- | --- | --- | --- |
| F5 | P1 design debt, current contract obeyed | Normal projection replacement consumes lifetime identity capacity | Design the lifetime/knowledge boundary before implementation; do not enlarge the cap as the solution. |
| F4 | P2 implementation defect | An Accepted idempotent Stop returns an unobservable ticket | Keep reissuable operation records outside evictable history. |
| F7/F8 | P2 UI identity-contract debt / admission inconsistency | Theme compatibility trusts a hand-authored revision; Runtime and Canvas disagree on conflict | One bounded theme admission/content identity correction. |
| F2 | P2 interface hazard | Delayed acknowledgment can clear a different terminal result | Adopt exact, owner-issued result identity; no data-loss claim. |
| F1 | P2 public interface debt | Application drops existing exact action feedback | Return the existing disposition at the public orchestration seam. |
| F3 | P2 information-loss defect | Compiled tool failures erase domain code and safe repair summary | Bounded typed failure projection, formatted only at the outer presentation step. |
| F6 | P2 measured-investigation candidate | Local persistent apply deep-copies the entire asset directory | Establish cost attribution; trial lazy isolation before a delta design. |
| F9 | P2 budget-design candidate | Glyph output extent is checked after raster allocation | Separate output rejection from execution admission; do not claim current OOM or a violated hard-memory guarantee. |

### F1: Application Submission Discards Exact Action Feedback (P2)

**Evidence:** `editor/src/app.rs:735` calls `dispatch_editor_actions`, then returns only the
frame-drive update. `EditorApplicationUpdate` at line 142 contains `actions_observed` and
`exit_ready`, whereas the private dispatch at lines 174-233 already retains a batch report,
batch rejection and original-slot mapping. The independent dialogue panel at
`tests/fixtures/module-consumer/editor-panel-consumer/tests/dialogue_panel.rs:104` uses this
public entry, then infers success from later document state. Direct session consumers can receive
the more informative `EditorActionBatchReport`.

An accepted action and a rejected stale-revision action can both produce
`actions_observed == true`. A later revision is not evidence of this particular command's
outcome, especially with several actions or callers. The ordinary native frontend has private
feedback access; this is an incomplete public Application interface, not evidence that all
frontend feedback is broken.

**Missing fact/owner:** the Application must return disposition of the submitted original action
slots without making a caller depend on frontend-private routing.

**Proposed replacement:** return an owned submission result alongside frame/exit observation.
Preserve the original slot map, including close-filtered actions, batch rejection, and per-action
domain results. Reuse the existing dispatch implementation. Submission acceptance must remain
distinct from later Save, import or Play completion. Delete the public lossy return shape rather
than adding a second dispatch channel or compatibility method.

**Rejected alternatives:** polling revision/diagnostics to reconstruct outcomes; importing the
ImGui frontend; a universal command bus or Automation crate.

**Contract/acceptance:** refine ADR 0118's Application interface without moving ADR 0026's
transaction owner. An independent consumer must distinguish accepted, stale, close-filtered,
no-project and consecutive-batch outcomes for every original slot.

### F2: Persistence Exposes Clear-Latest Acknowledgment (P2 Interface Debt)

**Evidence:** `crates/nara_tooling/src/frame.rs:547` has a payload-free
`EditorPersistenceAction::AcknowledgeResult`; line 616 exempts Persistence from current-frame
matching. `src/project_host/runtime/editor/actions.rs:37` only rejects old frames when an action
requires the current frame. The acknowledgment at
`src/project_host/runtime/editor/persistence.rs:595` takes whichever result is currently stored.

A caller can retain an acknowledgment while result A is visible. Another caller acknowledges A,
performs another Save and observes result B. The retained action then clears B because no result
identity is present. This is loss of terminal observation, not loss of saved source bytes. The
existing interface behaves like **clear current result**; this review does not pretend an
identity-specific acknowledgment guarantee was already implemented or that the stock frontend
has reproduced this multi-caller sequence. No general Persistence exact-ack guarantee was found
in the Accepted ADRs: the matching Browser Open acknowledgment and internal Save-receipt
consumption are separate contracts. The guide at `docs/guides/editor-documents.md:90` likewise
does not promise observer-bound result consumption.

**Missing fact/owner:** the persistence owner must identify the result being consumed. A document
revision cannot distinguish several attempts/failures at one content state. A displayed frame
revision is also not a result identity.

**Proposed replacement:** owner-issued, session-bound, non-reused result identity projected with
the terminal result; acknowledgment names that identity. Stale, duplicate and foreign-session
acknowledgments leave a different result unchanged. Reuse the existing local pattern at
`src/project_host/runtime/editor/document_open.rs:338`, which checks `EditorDocumentOpenId`
before clearing its result. Explicitly define association of newly rejected attempts as well as
admitted asynchronous attempts. Adjacent payload-free acknowledgments need a bounded inventory,
not an automatic conversion to one global protocol.

**Rejected alternatives:** checking the current frame instead of the result; adding a global
event log; retaining clear-latest as the ordinary acknowledgment interface.

**Contract/acceptance:** ADRs 0047/0118 should state the newly selected exact-acknowledgment
contract. Test delayed A-after-B, duplicates, foreign sessions, unrelated frame advance and
failure/retry. Preserve the useful ability to acknowledge from an old displayed frame when the
actual result identity still matches.

### F3: Compiled Tool Failures Erase Existing Domain Diagnostics (P2)

**Evidence:** the independently compiled Text domain emits distinct safe codes and summaries
for excessive text and invalid settings at
`tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs:61` and line 74.
`src/project_content.rs:552` retains the `DiagnosticReport`, but its Display at line 706 only
reports the broad asset-domain category and its `Error` implementation has no further source.
`src/project_tooling/mod.rs:152` formats Display/source strings; the protocol at
`src/project_tooling/protocol.rs:128` carries one diagnostic string, and bootstrap returns
`Project(String)` at `src/project_tooling/bootstrap.rs:92`.

Direct Check can distinguish the domain failures. The existing compiled-project tool path loses
both the actionable domain identity and its repair summary. This is an information-loss seam
in an existing workflow, not merely a request for a future JSON CLI.

**Missing fact/owner:** the tool Adapter needs a bounded transferable failure projection; the
domain remains the authority for the diagnostic's meaning.

**Proposed replacement:** a finite wire failure carries operation/stage, existing checked code,
severity, approved safe summary and truncation facts. Bootstrap preserves structured failure;
the CLI formats it at the final presentation step. Only admitted public fields cross the process
boundary: do not serialize arbitrary diagnostic context, Debug strings, paths, capability data
or publication receipts. Correct the current protocol/consumer together without a speculative
general RPC framework.

The existing `SafeSummary` constructor accepts static source text at
`crates/nara_diagnostic/src/identity.rs:421`; it is defense in depth, not a complete arbitrary-text
sanitizer. Project-relative fields need their existing explicit classification; sensitive/secret
values never cross. Use a smaller wire count/byte budget than the possible full report, with
explicit omission/truncation. Errors without an admitted typed diagnostic receive a static
operation/stage fallback, never a fallback to an arbitrary Error/Debug string.

**Rejected alternatives:** generic Error/Debug serialization; one CLI branch per domain; pushing
operation failures through a second Runtime diagnostics bus.

**Contract/acceptance:** refine ADR 0110's compiled tool contract under ADR 0068's privacy rules.
Run the real external Text domain through child-process Check for both errors. Assert preserved
code/summary distinction, response and field bounds, UTF-8 truncation, protocol-version rejection
and absence of secret-canary context.

### F4: Stop Idempotence Outlives Its Observable Record (P2)

**Evidence:** `crates/nara_app/src/runtime.rs:2025` stores a bounded control-result queue while
line 2027 separately retains `stop_ticket`. `request_control` at line 2343 may return that
original ticket during `Stopping`. `control_status` at line 2405 only searches the queue;
`push_control_record` at line 2666 evicts the oldest record after 32 entries.

A valid retryable close participant can keep the Runtime in `CloseIncomplete` through enough
`RetryClose` attempts to evict the original Stop record. During the next `Stopping` attempt,
another Stop is Accepted with the original ticket, but an immediate status query returns None.
The temporary public-interface regression reproduced this at the baseline: nextest run
`53031c35-16b6-4a3c-ad69-36811794be03`, one expected red assertion. It completed physical
retirement before checking the disputed observation, so the failure is not a deliberately leaked
participant or timeout artifact.

**Missing fact/owner:** reissuable protocol identity and disposable historical observation have
different lifetimes inside `RuntimeInstance`.

**Proposed replacement:** retain the original Stop operation record and the active close-attempt
record for as long as an entry can return their tickets. Keep ordinary completed controls in the
bounded history. The smallest implementation may pin these existing records privately; an
additional public operation/attempt hierarchy is not a prerequisite.

**Rejected alternatives:** a larger queue, infinite history, event sourcing, or a second Host
status cache. None corrects the lifetime mismatch at its owner.

**Contract/acceptance:** clarify ADRs 0084/0076. Cross the history cap with deterministic retryable
participants; assert every reissued Accepted ticket remains observable, attempt outcomes remain
distinct, final close settles correctly, and ordinary history remains bounded.

### F5: Preview Replacement Spends Lifetime Identity Capacity (P1 Design Debt)

**Evidence:** `src/project_host/runtime/editor/preview.rs:527` replaces the materializer's document
after each changed source revision. `crates/nara_scene/src/authoring.rs:1515` fully replaces an
existing live instance. `crates/nara_identity/src/domain.rs:1680` retains claims for the instance
and its entities; retirement around line 1805 removes live mappings, not lifetime claims.
Admission at line 1637 rejects after the configured lifetime budget. The default at line 26 is
1,048,576 claims; Scene file admission separately permits up to 100,000 entities at
`crates/nara_scene/src/format.rs:70`.

For a stable 10,000-entity projection, each publication consumes 10,001 claims. From a fresh
otherwise unused identity domain, at most 104 such publications fit; publication 105 cannot fit.
This is a source-derived capacity example, not a 10,000-entity native performance measurement.
The exact path was exercised at a smaller scale: one entity, a six-claim budget, three successful
`replace_document -> sync_world` publications and rejection of the fourth while one live entity
remains. The witness passed in nextest run `af2c854d-9227-4c61-b6a6-c38a0df4f486`.

This obeys ADR 0058's lifetime-claim contract. It is not a regression against that decision or
source corruption: the last-good projection remains. The design problem is making ordinary
editing count a consumable resource of one long-lived preview Runtime.

**Missing lifetime decision:** exact historical membership, bounded owner memory and unbounded
arbitrary identity churn cannot simply be promised together. A numeric high-water mark does not
prove membership of arbitrary string entity IDs, foreign Worlds or out-of-order restore claims.
Compression is only a candidate when its issuing and lookup rules establish that proof.

**Proposed design alternatives:**

- **Preview epoch rollover:** reconstruct the disposable preview Runtime before claim exhaustion,
  with explicit old-observation invalidation, candidate publication and physical retirement.
  This contains the first real consumer without weakening general Identity semantics. Prove
  startup/close cost and last-presented continuity before selecting it.
- **Bounded historical knowledge:** revise the Identity lookup contract to distinguish current and
  retained exact knowledge from explicitly expired/unverifiable history, while always rejecting
  stale authority. This is a broader ADR change, not a cache implementation tweak.
- **Proven compact issuance:** retain exact instance/claim facts only if a closed issuer and
  explicit restore path can preserve all required Missing/retired/incarnation distinctions.
  Do not select this merely because it sounds cheaper.

Increasing the default limit only postpones the same lifecycle failure. A universal incremental
projection framework is not required to decide this contract, and resetting claims inside a live
domain would violate it.

**Contract/acceptance:** ADR 0058 must state the selected lifetime/knowledge model; ADRs 0026/0047
must agree on preview observation and retirement. Test tiny budgets through the real Editor,
stale locators, empty scenes, partial retirement, restore ordering, independent current Worlds
and successful continued editing after the chosen epoch/history boundary.

### F6: Asset Isolation Cost Scales With The Whole Directory (P2 Investigation)

**Evidence:** `crates/nara_asset/src/server.rs:109` derives Clone for five owned index maps.
`ComponentApplyBatch::from_world` at `crates/nara_reflect/src/persistent_apply.rs:445` clones the
live server into a baseline and clones that value again into scratch, even before an asset-free
component is staged. Asset-related commit admission at line 407 compares the complete directory.
`src/project_host/runtime/content_residency/replacement.rs:53` adds another staged-server clone
in project Scene replacement.

These are deterministic whole-directory allocation/copy operations in real direct apply,
preview and travel paths. They are not measured evidence of a current editor stall, memory-limit
breach or frame P99 regression. The existing isolation and original pre-flush asset baseline are
correctness requirements, not costs to remove without replacement proof.

**Missing owner mechanism:** cheap immutable catalog identity and private transaction isolation
are not expressed by `AssetServer`; callers obtain them through full value copies/equality.

**Alternatives:** first measure fixed asset-free and asset-touching operations against growing
unrelated directory size. Trial lazy scratch construction for the asset-free case. Only if costs
justify it, compare an owner-issued immutable catalog snapshot/private reservation delta with a
copy-on-write branch. IDs, exact original-baseline rejection and failure publication remain owned
by Asset/Reflect, not by a generic transaction framework.

Do not replace equality with a counter that misses ABA or bypasses structural replacement, share
mutable scratch through `Arc<Mutex<_>>`, or confuse Bevy's shared AssetServer clone with an isolated
Nara transaction snapshot.

**Contract/acceptance:** ADRs 0037/0081; domain imports remain under ADR 0103. Attribute copies,
allocation and wall time separately, then test zero publication on failure, branch-ID separation,
original-baseline preservation across flush, and actual Image/TileSet/travel consumers. A complex
delta implementation is gated on measured benefit, not preapproved by this audit.

### F7: Theme Revision Does Not Identify Theme Content (P2 Contract / Ergonomics Debt)

**Evidence:** `crates/nara_ui_kernel/src/style.rs:884` allows any valid palette with the same
revision. `compiler.rs:66` checks only the revision number against the environment, and
`compiler.rs:143` / `compiler/cache.rs:70` admit fallback by root key and environment without
checking the actual theme. `crates/nara_ui/src/codec.rs:550` and line 1464 expose palette and
revision as independently editable persistent fields.

The public-interface witness first publishes a red Accent root with theme revision 1. It then
submits an invalid version of that root and a second valid root under a green palette, still at
revision 1. The compiler accepts red `RetainedVisual` together with green `Published` in one
candidate frame. The witness passed in run `af2c854d-9227-4c61-b6a6-c38a0df4f486`.
This is mixed visual content, not ghost interaction: retained visuals still lack hit/focus authority.
The trigger requires reuse of one compiler/cache and a local root failure after the theme content
changes. It is not a claim that every palette edit renders incorrectly. No inspected API/ADR
explicitly assigns unique-content revision discipline to the caller. The real palette descriptor
at `crates/nara_ui/src/authoring_model.rs:1479` emits RGBA writes without revision advancement;
the revision control is separate at line 1719. Source review confirms that authoring path, but
the complete Inspector-to-Preview/Play manifestation has not been executed here.

**Missing fact/owner:** cache compatibility must bind the actual immutable theme content, not
delegate its identity to a hand-edited number that the authoring path does not enforce.

**Alternatives:** include actual `UiTheme` content in cache compatibility as the smallest
correction; or keep palette as authored truth and let a configuration owner issue runtime
publication identity. If a public revision remains, define its scope and enforce its content
association through every Rust, serialized and editor entry. A local Inspector `revision += 1`
is not sufficient, and no global Theme Registry is needed.

**Contract/acceptance:** clarify ADRs 0108/0109; removing persistent revision requires the UI
schema owner and in-repository fixtures to change together. Test same revision/different palette,
ordinary field edits, modal fallback rejection and mixed-root candidates.

### F8: Runtime And Canvas Disagree On Global Theme Admission (P2)

**Evidence:** `crates/nara_ui/src/runtime/projection.rs:204` reports a conflict between different
global themes but still returns the first theme. Runtime continues building the candidate at
`crates/nara_ui/src/runtime.rs:225`. Canvas's selector at `crates/nara_ui/src/canvas.rs:147`
instead returns `ThemeConflict` at line 164.

The same two-theme authored content is rejected by Canvas but accepted under Runtime's structural
first-wins order. Reordering theme entities can change the selected global palette. An emitted
diagnostic is not the same as rejecting an ambiguous authority. This is source-confirmed; this
review has not executed the complete Canvas/Runtime pair or claimed a native presented failure.

**Proposed replacement:** share one pure typed global-theme admission operation inside `nara_ui`.
Canvas and Runtime project inputs into it and consume its result. Equal duplicate declarations
remain admissible; conflicting declarations reject the new candidate. Preserve the existing
last-presented session and its input authority instead of tearing it down on rejection.
Revoke any old unpresented candidate that could otherwise be submitted after the conflict.
Keep default theme behavior when none is authored, avoid creating a surface/target when no UI
surface is active, and still allow an empty candidate to clear removed roots on an active surface.
Share configuration admission, not the distinct Canvas and Runtime presentation owners.

**Rejected alternatives:** undocumented first-wins, palette merging, per-root theme scope, or
another Theme service. These introduce unselected product semantics.

**Contract/acceptance:** ADR 0108's global baseline and ADR 0109's publication rules. One fixture
must receive the same conflict/equal-duplicate result in both adapters; structural reordering
must not change admission, and a rejected candidate must not replace presented authority.

### F9: Glyph Extent Rejection Is Not Preallocation Admission (P2 Investigation)

**Evidence:** `crates/nara_text/src/raster.rs:79` renders through Swash before checking
`max_glyph_extent` at line 97. `crates/nara_ui_render/src/atlas.rs:115`/125 checks its potentially
tighter limit after rasterization too. The locked Swash 0.2.6 implementation allocates the mask
with `image.data.resize(fmt.buffer_size(w, h), 0)` in `src/scale/mod.rs:871`.
The existing `crates/nara_text/tests/raster.rs:44` requests a 64px W with extent limit four and
expects rejection. That asserts no oversized publication, not absence of oversized intermediate
work or allocation. This review inspected that test and did not rerun it.

Current TextLimits documentation promises limits before publication, not an allocator-wide hard
memory guarantee. No OOM, malicious-font exploit or measured product memory regression is claimed.
The actionable design question is whether a real raster/atlas execution budget must also constrain
the preparation work.

**Alternatives:** explicitly retain an output-only limit if that matches the bounded product
requirement; or pass the actual raster extent/byte allowance down before rendering and use the
existing outline/bounds and mask-size stages for checked admission. Continue using the mature
rasterizer. Do not write a font parser/rasterizer or infer a hard bound from source font byte size.

**Contract/acceptance:** ADRs 0031/0068/0109 must distinguish result limits from execution admission.
Before adopting a hard-budget claim, measure allocations and prove hinted/subpixel bounds,
tighter planner allowance, checked sizes and no over-budget mask allocation. Failures must leave
the old atlas and packet unchanged.

## Pinned Reference Mechanisms

| Mechanism | Bevy/Godot source evidence | Nara consequence and limit |
| --- | --- | --- |
| Observable operation results | Bevy `bevy_ecs/src/world/mod.rs:3981` returns `Result`; Godot `editor/editor_undo_redo_manager.cpp:305` returns undo success and `editor/export/editor_export_platform.h:262` retains structured export messages separately from logging | Preserve facts already produced by a domain through the Adapter. This does not establish Nara-equivalent exact asynchronous acknowledgment. |
| Close and identity lifetime | Bevy `bevy_tasks/src/task_pool.rs:616` joins workers; Godot `core/object/worker_thread_pool.cpp:857` waits during finish and line 403 invalidates reclaimed task IDs | These are different shutdown contracts. Do not replace bounded Runtime close/custody merely because another engine's teardown is shorter. |
| Entity churn | Bevy `bevy_ecs/src/entity/mod.rs:74` documents generation-based reuse; Godot `core/object/object.cpp:2589` releases ObjectDB slots and `editor/inspector/editor_inspector.cpp:2117` edits properties without rematerializing an entire scene | Neither proves exact infinite historical membership for arbitrary Nara identities. Choose a sustainable preview epoch/knowledge contract. |
| AssetServer clone | Bevy `bevy_asset/src/server/mod.rs:61` shares an Arc; Nara clones owned maps | Shared execution state is not an isolated transaction branch. Compare cost only after preserving Nara's actual publication guarantees. |
| Theme change ownership | Bevy `bevy_ui_render/src/lib.rs:439`/456/720 observes actual color changes; Godot `scene/resources/theme.cpp:768` signals theme change and `scene/gui/control.cpp:3797`/3809 invalidates consumers | The source/configuration owner carries change truth. No hand-maintained decorative counter should stand in for unassociated content. |
| GPU retirement | Godot `servers/rendering/rendering_device.cpp:8150` owns delayed destruction | Keep Nara's exact prepared captures, resource-complete packets, device epoch and single execution owner; no new counterexample warrants a universal RenderGraph. |

## Architectural Verdict

The supported Rust-first architecture remains suitable: original Bevy ECS and schedules, separate
document truth and preview/Play projections, frozen schema/native execution, concrete Host
composition, owned background preparation and resource-complete render packets. The evidence does
not support replacing the ECS, collapsing authoring into a typed World, or creating a universal
service/job/script/transaction kernel.

The new findings do expose recurring architectural weaknesses worth fixing at their owners:

- **Operation identity, attempt identity and historical observation have different lifetimes.**
  F2/F4 must not be repaired with larger caches or caller polling.
- **Observation metadata is not content proof.** F7 requires an actual theme association; F8
  requires shared admission rather than parallel best-effort selection.
- **Exact lifetime facts need a sustainable lifetime model.** F5 is an Accepted design tradeoff
  that ordinary editing now gives a concrete reason to revisit.
- **Adapters must not erase useful domain facts.** F1/F3 are real interface-depth problems,
  independently of whether a future caller is a panel, CLI or Agent.
- **A publication limit does not automatically bound preparation cost.** F6/F9 need attribution
  and explicit execution requirements before selecting more machinery.

The [redesign plan](../plans/2026-10-23-chat-informed-foundation-redesign-plan.md) orders these
decisions and owner changes. It is Proposed work, not evidence of implementation or permission
to expand the product into all features discussed in the supplied conversation.

## Verification Limits

Three temporary public-interface probes were executed serially with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, the normal shared target and `--locked`. Stop has one red intended-invariant
assertion; Identity and Theme have two green witnesses of the undesirable current behavior.
Those green witnesses do not prove a fix. The initial combined probe compile used nonexistent
`UiLayer::Content`; it was corrected to `UiLayer::Base` before the two witnesses ran.
All temporary test files were removed from the repository after retaining their exact source for
the [probe and challenge record](../knowledge/engineering/verification/2026-10/2026-10-23T105719Z-chat-informed-architecture-review-probes-and-design-constraints-cc6bbbacdaac48eea3745bc7d36f8d9a.md).
Its embedded source hashes were checked against the executed files. Local documentation links,
whitespace and engineering-memory validation pass; existing historical memory warnings remain.
No production Rust source, Accepted ADR, dependency or lockfile changed.
No native presentation, performance or cross-platform acceptance was executed. The linked prior
qualification retains its original source/platform scope.
