# Architecture Document Map

This index identifies each document's owner and authority. It does not duplicate decisions,
implementation progress, or execution history.

## Document Responsibilities

User instructions define the authorized task. Within that scope:

- [Strategy](../../STRATEGY.md) defines product scope and non-goals.
- [Accepted ADRs](adr/README.md) define durable architecture contracts. Reconcile a conflicting
  contract before changing the affected boundary.
- [ADR implementation status](adr/implementation-status.md) summarizes implementation evidence and
  remaining gaps. It does not introduce new architecture decisions or task requirements.
- A selected plan organizes multi-stage work and its relevant verification. Local fixes do not
  require a plan, and plan or registration metadata does not grant execution authority.
- [Open questions](open-questions.md), draft designs and research describe unresolved choices or
  background evidence. Current guides explain supported workflows and yield to Accepted ADRs.

## Execution and Evidence Navigation

The [production cost measurements](../benchmarks/2026-10-20-production-costs.md) distinguish
ordinary editing, Browser transport, runtime work and Rust rebuild/restart. The private admitted
schema index removes quadratic model lookup while preserving the document owner. The subsequent
[scalar preparation correction](../benchmarks/2026-10-20-scalar-preparation.md) reduces repeated
format parsing and complete classification with conservative fallback and unchanged admission.
The [locality follow-up](../plans/2026-10-20-measured-work-locality-plan.md) keeps remaining
scalar, Browser, Tile and compiler work behind explicit attribution and complete
consumer acceptance. These observations do not select a universal async interface or job kernel.

The [Browser preparation slice](../plans/2026-10-20-browser-preparation-locality-plan.md) uses
existing task custody for finite scans and explicit request-specific Open preparation. ADRs
0047/0118 own pre-retirement revalidation and truthful pending/result observation; its
[report](../benchmarks/2026-10-20-browser-preparation.md) separates correctness, native scope and
maximum owner occupancy from total completion.

The [Tile locality report](../benchmarks/2026-10-20-tile-visibility.md) qualifies streaming
material candidates and conservative per-view queue clipping at `15e6564`, including an
unclipped real-GPU oracle. It preserves complete extraction and resource authorization without
retained-cache authority. The separate [CI portability repair](../plans/2026-10-20-ci-portability-repair-plan.md)
adds Linux handle enumeration and compile-only portable wasm checks; browser execution remains
outside that matrix.

The [CI feedback comparison](../research/2026-10-20-ci-feedback-and-bevy-comparison.md) attributes
hosted compiler/consumer costs and compares the pinned Bevy job/cache structure. A first correction
separates positive API probes from negative compile suites while retaining both feature combinations;
cache, profile and job-layout follow-ups need their own measured qualification.

The [Rust/Play attribution](../benchmarks/2026-10-20-rust-play.md) at `2c26c92` measures public
Play through accepted first presentation and retains separate Restart/headless controls. A
bounded Windows path-input correction proves same-parameter CLI artifact reuse without changing
profile/linker settings or immutable generation custody. Mixed uncontrolled timing changes do
not establish a general build-speed gain.

Use [engineering memory](../knowledge/engineering/index.md) to resume work or recover its reasoning,
verification and handoff context. Registration is continuity evidence, not a lock or an approval
gate. Read records relevant to the task rather than treating the whole bundle as required context.
Historical plans and verification retain their original source, toolkit and platform scope;
their completion conditions do not automatically apply to later tasks.

| Subject | Owning sources |
|---|---|
| Current architecture and implementation | [Foundation summary](nara-foundation.md), [ADR catalogue](adr/README.md), [implementation ledger](adr/implementation-status.md) |
| Managed runtime, immutable composition, assets, hierarchy, 2D order and views | [ADR 0117](adr/0117-2d-foundation-ownership-and-authoring-contracts.md) |
| Project creation, compiled schema, editing, Check and Export | [Product recipe guide](../guides/rust-product-recipes.md) |
| Persistent schema, codecs and custom scalar authoring | [Component guide](../guides/persistent-components.md) |
| Workspace/frontend boundaries, creation templates, panels and native views | [ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md); [native qualification procedure](../verification/native-editor-qualification.md) |
| Source-built stateful native panel composition | [Editor panel guide](../guides/editor-panels.md) |
| Image import and authoring publication | [ADR 0115](adr/0115-editor-image-import-metadata-and-authoring-publication.md) |
| Compiled source domains and Editor metadata import | [ADR 0103](adr/0103-concrete-domain-owned-asset-import-execution.md); [source-domain guide](../guides/project-asset-domains.md) |
| Public extension ergonomics and complete compiled Editor construction | [Bevy comparison](../research/2026-10-22-bevy-public-extension-ergonomics.md); [implementation plan](../plans/2026-10-22-public-extension-ergonomics-plan.md) |
| Supervised external Play and saved-source admission | [ADR 0116](adr/0116-standalone-supervised-editor-play.md) |
| Historical plan and verification lineage | [Engineering memory](../knowledge/engineering/index.md) and [UI plan history](../plans/ui-program-roadmap.md) |
| Foundation audit corrections and follow-up triggers | [Foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md) maps A01-A09, B01-B14 and M01-M04 to ordered units and consumer/measurement gates |

The [task execution baseline](../research/2026-10-21-task-execution-baseline.md) traces four actual
Bevy feature unions and observes 50 Nara workers plus three custody threads in one idle Windows
product. The [execution-budget plan](../plans/2026-10-21-task-execution-budget-plan.md) measures
Editor owner multiplicity and real work before selecting worker-policy or feature changes.
Dependency names alone do not establish duplicate execution, and this observation does not measure
CPU, memory, build size or frame P99.

## Background Designs

The [complete conversation reassessment](foundation-architecture-reassessment.md) revisits 36
foundation topics and [all 103 baseline ADRs](../audits/2026-10-23-foundation-architecture-coverage.md),
including execution, Schema/storage, Prefab, asset/build delivery, rendering, packages and
Editor/UI authority. It challenges current design choices rather than treating owner bug fixes as
a complete architecture verdict. The [rebaseline plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
orders concrete comparisons and fearless replacements. ADR 0119 adopts the Schema-first metadata
correction; 0120 and 0121 remain Proposed. Coverage does not mean the replacements are implemented
or every recommended ADR amendment has been applied.

The [per-document persistence-custody design](editor-document-persistence-custody-design.md)
specifies R5c's concrete replacement for the integrated Host's per-kind file lanes. It separates
source aliases, document sessions, preview selection and executable startup without an erased
document store. ADRs 0047/0118 adopt custody, common source admission and retained Scene selection;
0111 owns bound Scene presentation/input and 0034 the exact trusted Play source observation.
0113 owns the source-keyed refresh obligations and complete successor validation.
ADRs 0047/0118 also adopt additive Open independently of explicit Close;
0034/0047/0084 retain Play source association through cancellation and physical retirement and
distinguish associated-source/whole-session Close from unrelated document Close.
Prepared edits or per-document storage alone
do not qualify public same-kind multi-open.

The [Scene preview lifetime design](editor-preview-projection-lifetime-design.md) separates
disposable Scene identity epochs from document history and the unchanged authoring Runtime.
ADRs 0058/0026/0047/0111 adopt complete predecessor ownership and exact accepted-source/producer
association; ADR 0115 retains shared task/image custody. The
[bounded comparison](../benchmarks/2026-10-24-preview-projection-lifetime.md) records the frozen
incumbent and same-World candidate, without implying incremental projection or native qualification.
The separate [scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md)
binds the implemented epoch/producer/presentation contracts to public, held-work and actual native evidence.

The [durable property addressing design](durable-property-addressing-design.md) bounds the
R5a implementation to a Schema-relative selector and resolver. It keeps source-document/entity,
Prefab occurrence, operation basis and runtime incarnation as separate owner axes, and records the
scoped nested-ID lineage, keyed-item, migration and unresolved-target rules that must be settled
before R2/R3/R5b persistent contracts freeze.

The [source-owned creation and construction design](authoring-construction-design.md) removes
Workspace ordinal allocation and ID-reservation choreography from ordinary Scene/Prefab creation.
Native registry/database-aware and Published admission retain their distinct owners. Its retained
transaction-local draft and direct public adapters have separately scoped pinned qualification;
the bounded owner cost comparison does not complete R2's field-query, panel or full ergonomics work.

The [authoring read-projection experiment](authoring-read-projection-design.md) separates actual
component/root discovery from selected value copying. Source-owned borrowed views retain admission
and default interpretation; Tooling must provide bounded owned observations and exact target/basis
fencing. The [read-query qualification](../knowledge/engineering/verification/2026-10/2026-10-25T002933Z-bounded-authoring-query-complete-review-and-aggregate-budget-qualification-893b5a3ffa104ec8bc7c7413c12e0c5d.md)
records actual read/computed-edit workflows, affected native/static evidence, source simplification,
complete review and its multi-row budget proof correction. Prepared panel submission, broader
index-maintenance costs and full R2 remain separate from this bounded workflow and the
[timing baseline](../benchmarks/2026-10-25-authoring-root-read.md).
The [selector-directed successor](../knowledge/engineering/verification/2026-10/2026-10-25T051125Z-selector-directed-property-reads-and-indexed-root-retirement-qualification-bd267ec01e4d44d3ad5f2a2d4543514d.md)
replaces root-only read operations with the existing property selector and separate lookup/copy
budgets. Actual nested observations drive non-active Scene/Prefab panel edits; indexed root
retirement, provenance and failure distinctions have scoped owner/public evidence. It does not
materialize a defaulted root for mutation or qualify nested discovery/full R2.
The [first-write successor](../knowledge/engineering/verification/2026-10/2026-10-25T060130Z-source-owned-first-property-writes-and-bounded-selector-error-qualification-0dfaa18252de473b90ff57ef150e64b0.md)
adds Source-owned initialization of only the touched omitted root and an absence-restoring inverse.
Invalid selector errors retain no unadmitted IDs, and construction bounds component IDs at append.
Explicit values, storage-parent/list-slot limits and complete creation-template admission remain
distinct from decode defaults. These bounded corrections do not complete the foundation Goal.

The [prepared-panel design](editor-prepared-panel-design.md) selects finite read-only preparation
and consuming commands through existing panel submission, rather than mutable paint access or
a candidate registry. ADR 0118 adopts that boundary; the
[scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md)
records complete review, independently checked proof corrections, actual public workflows and
affected-existing-native/static evidence. The
[bounded panel comparison](../benchmarks/2026-10-25-prepared-panel.md) now records actual
preparation/enqueue/dispatch measurements. The subsequent
[owner phase attribution](../benchmarks/2026-10-25-reference-preparation-phases.md) records actual
reference-certificate fallback and local classification, locating populated instrumented wall time
in complete format admission. That diagnosis introduces no optimization or permanent profiling API.
The subsequent [finite reference certificate](../benchmarks/2026-10-25-reference-shape-certificate.md)
extends the existing private proof without changing admission, observes zero parser-preflight
calls and records two uninstrumented candidate comparisons. Control drift prevents a fixed causal
speedup claim; complete envelope traversal and both actual encodings remain.
This does not close full R2 or installable-tool workflows; the
[pinned editing/format study](../research/2026-10-25-editing-versus-format-admission.md) keeps
equivalent locality and a deliberate editing/Save contract redesign as separate alternatives.

The [source-built Snap command](../guides/editor-panels.md#a-source-local-snap-tool) is a concrete
R1 consumer of those existing read/preparation/result owners, not another engine abstraction.
Its [scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-25T112059Z-source-local-snap-public-consumer-and-installer-qualification-f89f21479f7d4c06a81ab5bc63b5ff87.md)
separates actual panel/Session and installer proof from native rendering and full tool lifecycle.
The next [finite child-proposal study](../research/2026-10-25-finite-snap-child-proposal.md) examines
explicit trust, bounded exchange, original-basis validation and physical custody after UI
retirement. It is a researched experiment design, not implemented process/tool support.

The [exact persistence-attempt design](editor-persistence-attempt-design.md) develops
R1/F2's missing observation correlation: one owner-issued attempt through dispatch, pending and
terminal state, with exact acknowledgment and explicit retry supersession. ADRs 0047/0118 adopt
the replacement of parallel private lifecycle slots, not a result bus or observer registry.
Its [scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-25T081443Z-exact-persistence-attempts-and-terminal-observation-qualification-332f36f3c0f94f93ae1eb2721e981359.md)
records exact owner/public correlation, unchanged independent file/worker custody and four selected
actual Windows/DX12 workflows. This does not complete R1 or qualify installable tools.

The [compiled-project failure design](compiled-project-failure-projection-design.md) replaces
R1/F3's string boundary with bounded domain code/severity/summary observations, explicit loss and
read-only admitted repair hints. ADR 0110 adopts canonical tool protocol 1 and truthful Schema/
Repair publication outcomes; compiler/child stderr remains outside this projection's privacy
claim. Its [verification record](../knowledge/engineering/verification/2026-10/2026-10-25T100841Z-compiled-domain-failure-projection-and-truthful-publication-observations-8b995496a82b411184e95f6f462b48f8.md)
distinguishes the actual compiled-domain regression, complete review corrections and their
qualification scope. This is not a general RPC or a completed semantic/installable-tool workflow.

The [source-context and logical-occurrence proposal](scene-source-and-occurrence-identity-design.md)
addresses the separately reproduced same-World/two-Scene UI key collision. It compares a required
source-bound envelope with embedded document identity, retains the independent runtime incarnation,
and specifies the direct/file-backed trial and ADR 0102 refreeze boundary. Source admission and
canonical Prefab binding, required containing-source materialization and Scene-owned active
occurrence have scoped qualification. [Ordinary unbound preparation](../knowledge/engineering/verification/2026-10/2026-10-24T164025Z-owned-ordinary-scene-preparation-and-complete-support-admission-qualification-e94d2a95cbf94dd7a0cbbd371aedf55c.md)
now has separate owner/public/independent and selected Windows/DX12 qualification. Complete
Runtime/Canvas key adoption has its own [successor conformance](../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md),
including exact public inventory, neutral examples and selected native evidence. None of these
bounded units completes the full foundation goal.

The [chat-informed foundation review](../audits/2026-10-23-chat-informed-engine-architecture-review.md)
compares the supplied engine-design conversation with the current owners and pinned Bevy/Godot
sources at `f91466b`. Four independent review lanes and three public-interface probes distinguish
an observable Stop-result defect, lifetime identity and theme-content contract debt, and concrete
Application/tool observation gaps from unmeasured preparation costs. The
[Proposed redesign plan](../plans/2026-10-23-chat-informed-foundation-redesign-plan.md) orders
bounded owner corrections and explicit lifetime decisions. Neither document changes Accepted ADRs
or claims a production refactor, native qualification, or a universal extension/execution kernel.

The [ADR boundary and frozen-execution plan](../plans/2026-10-23-adr-boundaries-and-frozen-execution-plan.md)
refines ADRs 0002/0044/0081 and narrows future spatial/asset representation commitments. Its
[qualification record](../knowledge/engineering/verification/2026-10/2026-10-23T022246Z-adr-boundary-and-frozen-codec-execution-verification-b210d1cf09c94203b04bffcfa1dbe236.md)
binds implementation `05531a0` to executed owner, public-consumer and selected native evidence.

The subsequent [post-boundary interface review](../audits/2026-10-23-post-boundary-architecture-and-interface-review.md)
records open driver-output, task-disposal, filesystem-cursor, post-flush admission and Image
association/recovery findings against `10ccc36`. It separates current-source and cached-artifact
probes from source-only paths, and proposes bounded owner refactors without changing Accepted ADRs
or claiming their implementation is complete.
Its [replacement design](post-boundary-contract-redesign.md) includes lossless owned driver calls,
task-disposal custody/outcome, capability-owned write position, post-flush admission, Image-owned
prepared/recovery facts, and separate Scene ergonomics/full Bevy entry naming. Its selected owner
boundaries have been adopted through the affected Accepted ADRs; the
[implementation ledger](adr/implementation-status.md) links each unit's actual qualification.
The [pinned Bevy/Godot comparison](../research/2026-10-23-post-boundary-bevy-godot-comparison.md)
checks these units against the local reference sources, separates stronger Nara guarantees from
upstream mechanisms, and retains bounded recovery continuity after challenging a diagnostic-only
capacity-rejection alternative against Loading supersession.
The [implementation plan](../plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md)
orders owner repairs, real consumer migration and fresh qualification; its presence does not mark
the proposed units implemented.

The [spatial affine representation design](spatial-affine-representation-design.md) retains
post-affine residuals, direct affine and authoring-field alternatives for a future concrete import
or world-preserving edit workflow. ADR 0097 accepts domain boundaries and the supported 2D model;
it no longer fixes future 3D schema fields, residual normalization or a no-format-change promise.

The [asset extension Host interface design](asset-extension-host-interface-design.md) compares
pinned Bevy/Godot and Unity resource-import seams, then proposes operation-specific typed domain
contributions coordinated by Nara's Check, Play, and source-carrying Export Hosts. ADR 0103 now
adopts the narrower compiled source-domain slice, documented in the
[public guide](../guides/project-asset-domains.md) and
[qualification plan](../plans/2026-10-21-external-asset-domain-qualification-plan.md). The remaining
design, ADRs 0087/0088 and package distribution questions remain open.

The [Rust authoring, reload, and ECS direction study](../research/2026-10-21-rust-authoring-reload-and-ecs-direction.md)
compares pinned Bevy/Godot mechanisms, other mature engines, and the supplied `hotcode` article.
It recommends a complete Rust path, the existing ECS substrate, measured rebuild/resume, and a
bounded function-patching experiment. It selects no new runtime contract or dependency.

The [module and product-goal alignment review](../audits/2026-10-20-module-goal-alignment-review.md)
covers all 35 workspace packages against Strategy, existing research and pinned Bevy/Godot source.
It records four concrete boundary repairs and their qualification, then orders the remaining work
around ordinary authoring, measured iteration and one persistent two-level game. Capability gaps
and unmeasured costs remain separate from confirmed defects; Tile tools remain deferred.

The [persistent two-level consumer](../plans/2026-10-20-persistent-two-level-game-plan.md) orders
selected Scene loading and finite declared delivery before current-level ownership, travel and
game-owned checkpoint persistence. ADR 0110 distinguishes delivery source facts from executable
startup agreement; Proposed ADR 0089 does not grant a general scene manager or native rollback.
Its [Windows consumer qualification](../knowledge/engineering/verification/2026-10/2026-10-21T025657Z-two-level-checkpoint-consumer-and-complete-frame-headless-qualification-72c5ef2705f0461e90916bb60e1bb345.md)
records full-frame headless execution, actual travel/save/fresh-process restore and release export,
with native presentation and other-platform limits kept explicit.

The [ordinary authoring workflow](../plans/2026-10-20-ordinary-authoring-workflow-plan.md)
implements the first structural consumer from that review: stable-ID reparent/reorder, closed
source-local subtree deletion and exact history through ordinary Scene/Prefab authoring. ADRs
0090/0118 own operation-relative preservation; ADR 0047 owns captured-source Reopen after later
unsaved moves. Its [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T051819Z-ordinary-structural-authoring-and-captured-source-reopen-qualification-f9b317e0ceda49a6b37f9000c61af8d7.md)
separates domain/public/native results from the still-separate presentation and measurement work.

The [workflow boundary refactor](../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md)
repairs ordinary input/render/diagnostic behavior and deepens runtime close, hosted native Scene
travel, local EntityRef editing, independent documents and ordinary product launch. Its
[bounded ownership and ergonomics review](../audits/2026-10-22-post-refactor-ownership-and-ergonomics-review.md)
retains those concrete owners and records six measured-work or consumer-driven follow-ups.
The [integration evidence](../knowledge/engineering/verification/2026-10/2026-10-22T130424Z-engine-workflow-refactor-final-integration-ac0a299fafd64a6dba6179531a1f5cff.md)
separates executed correctness/native scope from unmeasured performance and unsupported platforms.

The [2026-10-18 foundation audit](../audits/2026-10-18-foundation-contract-audit.md) records
confirmed ownership repairs and proposed refactor dependencies. Its standalone HTML deliverable
is kept outside the repository. The audit does not change Accepted ADRs.

The [low-level module and interface review](../audits/2026-10-22-low-level-module-and-interface-review.md)
examines App/ECS, task and asset custody, reflection, identity, hierarchy, Transform, Core and FS.
It separates reproduced ownership/admission/invalidation defects, source-confirmed reload/watcher
issues and caller-side interface debts. Its bounded cached-artifact probes are not fresh product
qualification, and its repair order does not change Accepted ADRs.

The [low-level architecture reassessment](../audits/2026-10-23-low-level-architecture-reassessment.md)
maps every low-level finding and the six earlier follow-ups to its missing fact and owner. The
[contract redesign](low-level-contract-redesign.md) proposes custody, spatial consumption, owned
locks, desired asset state, typed codecs and success-dependent finalization, with alternatives,
caller migrations and research gates. Its [pinned mechanism study](../research/2026-10-23-low-level-contract-mechanisms.md)
compares Bevy/Godot implementations without claiming equivalent guarantees. These are research
and Proposed design; they neither amend Accepted ADRs nor qualify implementation.

The [low-level contract implementation plan](../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
turns D1-D8 and the confirmed findings into owner slices with ADR corrections, real consumer
migrations and fresh verification. Execution evidence and Goal continuity belong in engineering
memory; the plan itself does not claim implementation completion.
The [implementation review follow-ups](../audits/2026-10-23-low-level-refactor-review-followups.md)
retain concrete new counterexamples and their owner corrections, including slot restoration,
metadata/removal observation ordering and retained native watcher custody. The
[spatial cost comparison](../benchmarks/2026-10-23-low-level-spatial-cuts.md) records residual
source-scan cost separately from native consumer qualification.

The [post-publication architecture review](../audits/2026-10-19-post-publication-architecture-review.md)
records the TileSet/Image corrections and the remaining concrete module-interface refactor order.
Its implementation and verification limits remain separate from future tile authoring tools.

The [Bevy/Godot architecture comparison](../audits/2026-10-19-nara-bevy-godot-architecture-review.md)
records current execution/Scene interface hazards and the workflow triggers for asset, Editor,
schema and residency refactors. It is source-review evidence, not a new architecture contract.

The [expanded foundation audit](../audits/2026-10-19-foundation-audit-expansion.md) adds twelve
focused research lanes across tasks, components, reflection, extensions, runtime, Editor, input,
UI/text, filesystem, assets, rendering and module integration, with independent challenge and
explicit correctness/capability distinctions.

The [interface and module review](../audits/2026-10-19-interface-and-module-boundaries.md)
separates synchronous owner contracts from triggered asynchronous preparation and private module
extraction.

The [post-correction foundation review](../audits/2026-10-19-foundation-next-stage-review.md)
records new raw-Prefab, Inspector and render error-path findings, and the next action-result,
component-validation, code-generation and document-ownership design triggers. It keeps current
defects separate from deliberate capability limits and the completed U1-U15 corrections.

The [authoring interface ergonomics design](authoring-interface-ergonomics-design.md) proposes
one complete field-edit entry, exact dispatch-result routing, a single component inventory and
finite shared constraints. It compares alternative interfaces and defines caller-level acceptance;
its implementation section groups corrections into complete editing and component-evolution
workflows, with qualification at each changed boundary. S1 owner corrections and S2 field editing/
exact results are implemented within the linked verification scope. S3 adds typed finite intervals,
safe component-domain diagnostics and one explicit component inventory, qualified through the
linked external, owner and selected native workflows. ADRs 0045/0081 and 0047/0118 own the adopted
contracts. A separate N05 App plugin-error follow-up under ADRs 0010/0068 is qualified at `40bcdc6`;
it preserves checked code/summary and framework-owned invocation facts through existing product
failure and cleanup reports. Direct App addition and `seal` errors retain the same invocation
envelope; Editor reports terminal shutdown facts once while physical close is still pending.
The design links its focused owner/product evidence separately from S3 and native release claims.
Triggered asynchronous preparation, inline tools and multiple-document designs remain future work.
This does not reopen the completed correction plan.

The [Editor responsiveness and task-retirement investigation](../benchmarks/2026-10-20-editor-responsiveness.md)
selects a bounded G1/G2/G9 follow-up: ordinary public Open/Save/Reopen phase measurements and
standalone pending-task cancellation before capture disposal. The starting-source regression fails;
the correction at `3b70e48` passes the final 120-test Tasks/Image library cohort and independent
review. Three optimized measurement processes pass; at 4,096 empty entities, Save-drive P95 spans
15.559-16.357 ms across processes. The evidence prioritizes targeted Save preparation design, then
Reopen/startup-closure preparation, using 8 ms only as a private single-call investigation trigger.
Sixteen samples per phase make P95 equal max; this is not a stable tail estimate or native SLA.
Browser Open, source receipts, no-op Cargo and native redraw remain unmeasured. Platform review
disproves an unbounded output-drain suspicion because each poll already consumes at most 64
records. Caller-thread disposal of rejected captures or untaken completed results can still run
user destructors and remains an open G9 latency concern. This does not complete D6, implement
asynchronous preparation, change owner-thread traits to async, or extend S3/N05/native evidence.

The subsequent [owned Save](../benchmarks/2026-10-20-owned-editor-save.md),
[owned Reopen](../benchmarks/2026-10-20-owned-editor-reopen.md) and
[owned startup-closure preparation](../benchmarks/2026-10-20-owned-editor-startup-closure.md)
follow-ups implement the measured phase corrections. The
[Host lifetime custody qualification](../plans/2026-10-20-editor-host-lifetime-custody-plan.md)
separately closes post-terminal Scene/Prefab failed-cleanup ownership. The subsequent
[Image metadata custody repair](../plans/2026-10-20-editor-image-publication-custody-plan.md)
at `bae0440` closes the separately confirmed Image publication path: focused engine/public 116,
Editor/application 186 and actual Windows/DX12 native 2 tests pass. These bounded changes preserve
owner publication, native callback affinity and existing task retirement; they do not make synchronous
Plugin, codec or panel traits asynchronous. Each report owns its actual scope and limits.

The [UI foundation design](ui-foundation-design.md),
[UI boundary study](ui-product-boundaries-editor-dogfood-and-porting-strategy.md), and
[render pressure matrix](render-capability-pressure-matrix.md) provide design context when a task
reaches those boundaries. They are not current implementation status or additional task gates.

Earlier composition and package-kernel sketches are covered by the
[runtime design history](runtime-composition-interface-design.md) and
[dormant kernel proposal](extension-contract-kernel-interface-design.md). Other package, import
and render-extension drafts remain background material for a triggered investigation; they are
outside the normal authoring and implementation reading path.

## Maintaining These Documents

- Before reusing a design workbench, bind it to landed owners and symbols, current ADR statuses,
  relevant open questions, and observed consumer pressure.
- Update the current contract and direct navigation affected by a change. Keep implementation
  status concise and link to verification for execution details.
- Retain historical reasoning and evidence within their original scope. Obsolete drafts and
  duplicated process artifacts may be retired after checking their consumers and references;
  unrelated history does not need to be rewritten for an implementation change.
- A draft may propose names and scenarios. A durable invariant needs an Accepted ADR; unresolved
  proposals and historical acceptance checklists do not expand the current task.
