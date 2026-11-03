---
title: Foundation Architecture Rebaseline And Fearless Refactor
date: 2026-10-23
status: Design and workflow-driven execution baseline
---

# Foundation Architecture Rebaseline And Fearless Refactor

## Objective

Reconsider the complete supplied foundation conversation and implement a Rust-native, modular
integrated engine whose public authoring and extension workflows do not depend on accidental
prototype constraints. Preserve proven ownership/safety invariants, not every current type,
crate boundary or Accepted decision. Nara is unreleased: remove replaced APIs and obsolete
mechanisms instead of adding aliases or permanent migration layers.

The [integrated design](../architecture/foundation-architecture-reassessment.md) contains D01-D18,
alternatives and target interfaces. The [coverage inventory](../audits/2026-10-23-foundation-architecture-coverage.md)
records all 36 conversation topics and all 103 baseline ADR dispositions. A design disposition
is not an implementation claim. The previous [nine-finding plan](2026-10-23-chat-informed-foundation-redesign-plan.md)
remains evidence and is subsumed as explicitly mapped work below, not silently marked complete.

This plan is a decision/execution artifact, not an approval service, a release gate or permission
to alter concurrent work. Execution automation state remains external to the architecture plan.

## November 2 Follow-up Review

The [follow-up review](../audits/2026-11-02-refactor-follow-up-review.md) examines the committed
`a2640a1` baseline, before the current closed compiled-storage integration. Its four behavioral
findings concern existing owner transitions. Correct them through their current modules before
extending the corresponding authoring, render or build workflow; they do not select a shared
transaction framework or reopen already qualified units.

| Finding | Owning correction and acceptance |
| --- | --- |
| F1: converted source loses contextual defaults | Preserve the selected record's lexical default contract separately from Native-flat values, remap nested/masked defaults at localization, and require ordinary JSON/RON admission plus exact conversion Undo/Redo. Include this in the current CP2 compiler prerequisite. |
| F2: cancelled recovery blocks an identical retry | Retain durable recovery custody while giving a new publication attempt its own destination. A cancellation after recovery publication but before Pending must allow a new adequate-limit operation to adopt the unchanged proposal. Same-operation retry retains its exact destination and guards. |
| F3: replaced window retains Runtime picking | One successful same-window presentation replacement revokes the previous Runtime interaction authority through either Host entry. Preserve other windows, immutable image outputs and the incumbent on failure before replacement. |
| F4: bootstrap hides compiler diagnostics | Preserve bounded diagnostics on child and decoder failure through the existing Cargo owner. A real failing headless bootstrap must expose its distinctive compiler error and never launch an executable. |

The [F4 correction](../knowledge/engineering/verification/2026-11/2026-11-02T051018Z-failed-bootstrap-diagnostics-retained-through-the-real-project-tool-caller-446882fff4584df1a23d3930a6fda2dc.md)
is qualified independently at349921f. Same-window revocation is committed at d6dccbc and
cancelled-upgrade retry at080852c; their actual transition controls pass in the
[focused integration replay](../knowledge/engineering/verification/2026-11/2026-11-02T053411Z-compiled-scene-owners-and-reviewed-transition-corrections-pass-current-public-cohort-e8f7d20e3e2b49d5b086ce2100f1b9b2.md).
F1 conversion and the current compiled prerequisite retain their separate integration evidence
and completion gates.

The review's classification-cost observation remains an owner-locality follow-up: Scene should
describe its own representation cost while Host owns the aggregate ceiling. Add explicit CPU
execution for the new independent render consumer workspaces to the existing CI matrix, with
native/GPU cases selected by their actual prerequisites. Do not multiply every feature combination
or introduce another source-analysis verifier. Historical qualification keeps its original scope;
full CP2, editable Nara-drawn UI, motion/execution and rebuild/resume remain open.

The review also identifies a test-configuration gap: runtime-2d/serde/tooling tests import a
project-tooling-only filesystem helper. Give their support code the actual feature ownership
before qualifying that library-test combination. Do not treat this test build failure as a
production-library failure or silently enable project-tooling in every ordinary consumer.

Applying F1's ordinary-reopen control also identifies a live-export gap: a successful flattened
snapshot omitted the new source's contextual declarations. ADR0006 selects effective-field
snapshots in the already lowered output namespace for that new source. Qualify recompile/reopen
under the same definitions and count both explicit/default data before copying. Keep this new-
snapshot policy separate from conversion's preservation of the original hidden default contract.

## November 2 Infrastructure Comparison

The supplied [comparison](../research/2026-11-02-infrastructure-comparison/README.md) and its
[receiving disposition](../research/2026-11-02-infrastructure-comparison-reconciliation.md)
retain their snapshot limits. Current closed compiled storage supersedes that study's source-backed
Native input description. Keep CP2's selected direction; add only the bounded R6 dependency-refresh
unit, R4c/R9/R10 temporal-history interleaving, R11 buffer/document history transitions, and R12's
exact restore oracle and observational trace consumer below. These changes extend existing units,
not the current compiled-unit acceptance or a parallel roadmap. Qualified mechanisms stay closed;
no performance or whole-platform support claim follows from this comparative source review.

## November 3 UE6 Direction Review

The [UE6 reconciliation](../research/2026-11-03-ue6-direction-and-nara-architecture.md) separates
Epic's announced direction from a shipped implementation and from Nara's product requirements.
Retain Schema-owned meaning, composed source documents, independent flat compiled output,
Rust-complete gameplay and the Nara-drawn Editor. No second scene hierarchy, mandatory Verse-like
language, universal rollback runtime or distributed-world implementation is selected.

Strengthen only the existing R4c/R10 execution comparison: define bounded speculative admission,
recoverable state, execution-attempt identity and effect publication/confirmation before freezing
the affected phase Interface. ADR 0024 states that qualification boundary; ordinary runtime faults
keep their current close/reconstruction policy. Complete prediction implementation remains R10
work, not a new prerequisite for every R4c comparison or the current CP2 extraction workflow.

## Decisions Made In This Round

- Strategy explicitly includes installable tools and optional-language/prebuilt-Host workflows
  alongside a complete independently usable Rust path.
- ADR 0119 replaces 0004's obsolete reflection authority: Nara Schema is semantic authority;
  native Bevy reflection remains useful adaptation, not a required parallel declaration.
- Research, concrete owner API, reusable compatibility freeze and actual support qualification
  are separate scopes. Historical plan IDs, full Nara UI or complete package governance are not
  automatic prerequisites to bounded semantic/representation experiments.
- Proposed 0120 develops complete non-native representations and immutable owner publications;
  current native freeze and exact-`T` proof remain until an equally complete replacement exists.
- Proposed 0121 develops typed render composition and Host-local execution; current captured
  input and single-submit authority remain, but permanent closed topology is challenged.
- The rest of the reviewed recommendations have named units below. They are not all adopted
  or implemented merely because the coverage table has no Pending entries.
- The engine has not shipped, so internal persistent-format history is not a compatibility
  contract. Once each owning replacement is final, converge its canonical schema/format to v1
  and remove superseded readers, converters and historical fixtures that exist only for that
  unreleased history. Keep the existing migration guide as requested; do not add a release
  migration guide. Do not mechanically rewrite runtime epochs, stale-result generations or package
  semver as file-format versions. Retain authored component-schema evolution only where a real
  project-data migration workflow needs it; otherwise consolidate it with the canonical schema.

## Execution Rules

1. Reconcile each affected owning ADR before changing its contract; update the ledger only with
   actual evidence and limits. Proposed successor qualification is a unit outcome, not a premise.
2. Use a complete public consumer with at least one real failure path. Compare competing designs
   on the same task; record author concepts/choreography, required engine edits, dependencies,
   lifecycle and measured costs. Do not compare an implemented design to an intentionally weak fake.
3. Choose numerical performance targets and stop conditions at experiment entry, based on the
   target workflow/hardware and current baseline. The audit has not measured those targets and
   must not invent them. Correctness invariants below are already non-negotiable.
4. Keep comparison prototypes disposable and outside ordinary public APIs. A selected concrete
   owner API may ship before a universal cross-adapter interface; do not build all alternatives
   into production merely to preserve optionality.
5. Compile/test serially in this checkout, `CARGO_BUILD_JOBS=1`, reuse `target`, use focused
   `cargo nextest run --locked`, then relevant consumer/native tests and `cargo fmt --all`.
   Agents can work on disjoint review/code scopes, never concurrent Cargo commands here.
6. Inspect diffs before formatting/staging; preserve `vendor/`, read-only `repo-ref/`, user work
   and unrelated changes. Commit coherent verified units with precise Conventional Commits.
7. Report failed experiments honestly. Failure may select a simpler design or narrow support;
   it does not justify silently weakening publication, identity or physical retirement guarantees.
8. Design shared contracts for named foreseeable engine workflows before freezing them. The user
   prioritizes avoiding another large architectural replacement over minimizing initial code.
   Stage implementation depth, not the identity, dependency and ownership model. Compare total
   integration/evolution cost; a smaller demo is not evidence for a deliberately restricted core.
   Record the future workflow and the contract it constrains, then challenge that contract with a
   bounded executable consumer. Defer unneeded optimizations or support breadth behind that same
   model. Do not call a throwaway architectural restriction an incremental implementation merely
   because its replacement is scheduled later.

## Dependency Shape

R numbers identify responsibilities, not a mandatory linear implementation order. The previous
`R2 + R3 -> all of R5` chain was too coarse: durable addressing must be decided before freezing
prepared-edit/Behavior/Prefab contracts, while projection/history lifetime is independently useful.
Likewise native completion custody precedes narrow executable replacement, not every experiment
in R3. No slice must implement a Cartesian product of languages, representations and Host modes.

```text
R0 design -> R1a exact direct submission -> R1 semantic tool / public workflow oracle
R5a source/property address semantics -> R2 prepared edit / query contract
R5a + R2 -> R5b declarative Prefab overlay -> durable Behavior/Prefab qualification
R3a representation trial -> R3b language workflow comparison -> selected Behavior owner API
R4a native close completion + selected R3 owner -> R3c narrow module replacement
R4b activity scopes / facility policy and R4c phase-plan comparison qualify independently
R1/R2 + selected Host custody -> R1b real tool placement / local install lifecycle
R6a continuity + typed Runtime decode + bounded immutable catalog -> R7 base delivery
R6b recipe-index / CAS is an independent cache work unit, not base delivery admission
R6/R7 validated-reference demand precedes travel/package contract freeze; backing residency is separate
R8 render composition + R9a View / color / captured-time facts qualify together
R6 shader inputs + selected R8 seam -> R9b durable shader/material resources
R9c retained geometry is a separate measured mechanism
R10 joint motion oracle -> R4c phase/executor selection -> selected R10 domain qualification
every changed seam -> matching R11 / R12 integration, deletion and current documentation
```

R1 can begin on current fields/patches. R3a may use disposable adapter-private records and a fresh
Runtime while addressing and native-close work proceed; it cannot thereby freeze the final Prefab
contract or claim narrow reload. R8 can begin without durable materials/CAS. R7 needs explicit
immutable source capture, not the complete Editor recovery journal. Independent source experiments
may run ahead; canonical integration and Cargo verification in this checkout remain serial.

## Seven Delivery Waves

The [target architecture](../architecture/foundation-architecture-reassessment.md#target-architecture-and-refactor-decision-rule)
defines five responsibility groups, not a mandated crate layout. These waves turn that direction
into complete outcomes; a small prerequisite repair does not count as finishing its whole wave.

| Wave | Complete public workflow | Displaced design to delete | Exit criterion |
| --- | --- | --- | --- |
| 1. Authoritative editing | Query a non-active document; construct an object with components/references; prepare/diff/commit; Undo/Redo and Save/Reopen through tool and window-free consumers | Tool-owned inverses, separate dry-run logic, action lists presented as atomic transactions | Final-operation failure publishes nothing; stale basis rejects; one accepted transaction is one Undo; query does not move selection |
| 2. Independent sessions and physical lifetime | Two preview/Play scopes use selected facilities; close one while the other continues; sustain projection editing | Replaced mandatory per-Runtime physical owners and release-by-call-order assumptions | Pending consumer blocks provider release; error/physical completion differ; epoch-stale references never alias; edit churn has an honest history policy |
| 3. Installable useful tool | The same command plus inline Inspector customization compares source-built, isolated semantic and trusted native placement; selected path installs/updates/disables/removes locally | Losing prototype protocols and first-party-only private tool hooks | Ordinary installed tools need no Shell rebuild; crash/disable leaves documents usable; late results fence; required dependents and modified files survive correctly |
| 4. Behavior and durable Prefab | Two runtime-defined Schemas coexist, edit/run/migrate independently; base Prefab changes; missing module saves/restores | Global each-Schema-needs-Rust-type assumption and operation patches as durable overlays | One selected representation/language path completes native coexistence and fresh-Runtime recovery; narrow reload separately proves physical code custody |
| 5. Products to independent delivery | Compound importer -> Check/Play -> verified Runtime catalog -> source/cache/Cargo-free run machine | Duplicate root/domain registration, hidden source fallback and generic starter-bin build policy | Clear-cache product identity stable; wrong target/Schema/digest rejects; compatible last-good is recoverable; CAS may remain independent |
| 6. Composable rendering | Independent two-pass effect in Runtime, Scene View and offscreen; stock features share the seam; durable material follows | Central package-specific payload/match growth and displaced fixed material path | View/color/time semantics agree; multiple streams retire independently; old packets and loss/recovery remain correct; retained geometry must win measured comparison |
| 7. Full production consumer | Two-level game crosses editing/tool/Behavior/content/Play/checkpoint/export, pause/exact-step/input and close | Remaining replaced entry points, duplicate canonical rules and obsolete mechanisms | Public workflows and relevant native evidence pass together; each replay/audio/prediction scope is reported independently, not implied complete |

Each wave includes independently compiled consumers, appropriate failure/lifetime tests, relevant
native evidence, current ADR/guide updates and deletion of its replaced paths. R11/R12 are not
reasons to delay all integration until wave 7. Rewrite a module internally when necessary; keep
the integrated product at coherent verified checkpoints rather than preserving old public APIs.

### Initial Executable Unit

R1a replaces the direct Application submission's boolean-only return with owned per-action
dispatch observations and a separate subsequent drive update. It is a production boundary
correction and the first executable prerequisite, not completion of R1, wave 1 or the architecture.
Its public tests cover exact order, partial per-action rejection, whole-batch rejection, close
filtering, workspace retirement and dispatch-versus-eventual Save outcome. R2/R5a work is still
needed to deliver the first wave's complete editing transaction.

## R0: Complete Review And Reconcile Research/Product Scope

**Design:** D01, D06, D18. **State:** documentation work completed in this round. An independent
review found two P1 design gaps and one P2 coverage gap; the corrected replacement-mode,
cross-owner code-custody and input-action designs passed the scoped follow-up. This is not an
independent rerun of all source/consumer evidence. No production behavior was changed by R0.

Deliverables: complete topic/ADR inventory; integrated alternatives and recommendation; Strategy;
0119 plus Proposed 0120/0121; scoped corrections to extension, package, UI and artifact/persistence
admission rules; ADR catalogue/ledger and engineering continuity. Preserve baseline titles/status
as provenance, distinguish a recommendation from an applied ADR amendment, and fix stale
0082/0084/0090 status narratives without promoting the remaining proposals.

Acceptance: every baseline ADR occurs exactly once, every topic maps to a substantive design,
all new IDs/status/links agree, and an independent reviewer challenges omissions/contradictions.
Document checks are sufficient for documentation closure; no inherited test count is new evidence.

The October 30 [research disposition](../research/2026-10-30-engine-direction-reconciliation.md)
reconciles the user-supplied archive against `b939da2`, preserving the ten original studies and
their different reference revisions. It refines the existing units below rather than adding a
parallel roadmap or reopening qualified source-free, Input-definition and Prefab replacements.
Its risk/capability findings are not new product qualification. Its
[implemented-successor checkpoint](../research/2026-10-30-engine-direction-reconciliation.md#implemented-successors)
records the qualified endpoint/reference-demand/backing successors and the current local-build
entry; the original receiving-baseline order is historical. Motion phases and render/UI
completion still constrain their later interfaces before they freeze.

## R1: Minimal Semantic Tool And Exact Operation Outcomes

**Design:** D13, D17. R1a is the owned direct-submission prerequisite; its qualified scope belongs
in the ADR implementation ledger and engineering evidence, not a full-R1 completion claim.
**Owners:** existing tooling/session, selected frontend
and project workflow adapters; no new Automation crate.

- Implement one pure semantic proposal over existing legal fields/patches. Expose preview and
  submit through the window-free public Session first; compare the current Dear contribution
  only where it supplies useful predecessor evidence. The user-selected visual destination is
  Nara's own custom-drawn frontend under ADR 0118. Do not wait for generalized queries, mixed
  transactions, full Nara UI or dynamic packages. Window-free qualification alone does not
  establish visual panel/session parity or an inline Inspector.
- Carry exact action/request feedback to the originator through Application and tool adapters.
  Keep dispatch, persistence/Play completion and supplemental diagnostics distinct.
- Resolve the earlier F1 Application feedback, F2 persistence correlation and F3 domain-diagnostic
  transport findings at their owning boundaries; do not manufacture a disk-loss claim for F2.
- Follow with a finite explicitly enabled child-process tool returning a proposal to the same
  owner, only after its bounded input/output and trust are stated. No sandbox or stable wire SDK
  claim follows from it. The [finite source-tool owner plan](2026-10-28-finite-source-tool-owner-plan.md)
  places this concrete operation in Project Session/Application, using existing Tasks custody and
  source preparation rather than retaining the EnemyStats fixture's parallel proposal owner.

Add an ordinary semantic command contribution whose lifetime is independent of panel visibility.
Use the external Snap operation from a menu and shortcut with its panel closed; both use the
same query/prepare/commit/results and one transaction/Undo boundary. A panel owns its draft,
while the admitted contribution or Session owns command availability and execution. Reconcile
ADR 0118 before replacing the panel-only contribution surface; a string command bus is unnecessary.
This consumer can use current fields without waiting for every R2 query or R5 evolution outcome.

**R1b placement and local lifecycle** is a separate deliverable beyond that proposal experiment.
The [finite inline facet consumer](2026-10-28-inline-source-tool-facet-plan.md) now implements one
real EnemyStats `health` property customization plus a small tool panel through the same Session
request/review and source transaction. Its linked qualification records exact property matching,
independent draft ownership, disabled/replaced route refusal, default fallback and actual native
Save/physical Close/fresh Open. Broader inline contributions remain separate. Keep the facet
finite and UI-neutral; the incumbent frontend supplies interaction evidence while Nara's own
custom-drawn runtime remains the destination. This does not select a recursive widget-description
protocol, toolkit callback ABI or an official C# SDK.
Use the same useful command plus inline Inspector customization to compare source-built companion,
replaceable semantic tool Host and a narrowly trusted native SDK. Establish selection from native
authority, author concepts, update/diagnostic/recovery behavior and measured calls/latency, without
implementing three production SDKs. The selected ordinary-tool path then completes local install,
update, enable/disable and remove under the phased Package contract, including required dependent
closure, stale proposals and modified/adopted file preservation. A panel alone cannot close this.
The [local source-tool release plan](2026-10-28-local-source-tool-release-plan.md) orders code-free
artifact capture before installation ownership, approved activation, dependency-safe removal and
actual public/native qualification. Its first capture API is not an installed or enabled tool.
The [owner-adoption successor](2026-10-28-authoring-owner-adoption-plan.md) addresses the current
installed consumer's pre-admitted metadata premise through source-owned reclassification/history,
bundle publication and whole-Session adoption. New-owner adoption, existing-owner evolution and
installed graph/removal retain distinct acceptance; the full R1b workflow must join them. The
[creation-metadata evolution plan](2026-10-29-installed-owner-creation-metadata-evolution-plan.md)
selects the next bounded existing-owner transaction: unchanged stored values/history, one actual
catalog successor and separately retired/approved tool execution. Its
[qualification](../knowledge/engineering/verification/2026-10/2026-10-28T223626Z-installed-creation-only-owner-successor-preserves-sources-history-and-separate-execution-approval-57dec00a6525410f9abb79ec24a99e69.md)
covers the implemented creation-only operation, retained dirty source/history and intermediate
Close/Open states. General stored-value migration remains separate.
The [protected-content successor](2026-10-28-protected-source-tool-content-plan.md) continues
the qualified exact graph through durable modification/adoption facts and reviewed reclamation.
Actual Windows named-stream creation during restrictive default-stream custody exposes a
whole-file deletion-proof gap; management/adoption can proceed without qualifying reclamation.
The [accepted-document protection qualification](../knowledge/engineering/verification/2026-10/2026-10-28T132217Z-accepted-document-protection-and-repairable-installed-content-close-qualification-8a03235372c74fcbb4a02f7ee23eaca2.md)
adds sticky accepted-change facts, original-checkpoint persistence and repairable physical Close
through existing owners and actual public/native consumers. Reviewed reclamation, interrupted
deletion recovery and the all-stream proof remain open; this does not close R1b or the full Goal.
The [artifact ownership successor](2026-10-28-managed-source-tool-artifacts-plan.md) now replaces
that future mixed-content reclamation premise with managed artifacts and explicit project-owned
template import. It retains real generic unlink and orders retained-source/physical-pending
corrections before store admission, import, reviewed removal intent and actual partial recovery.
Its [store/text-import implementation](../knowledge/engineering/verification/2026-10/2026-10-28T163225Z-managed-source-tool-artifact-store-and-project-owned-text-template-replacement-7c31339a78064eb8bc385a9bd82b372a.md)
removes mixed-content protection and optional-ledger persistence gates while qualifying project
copies through ordinary source history and persistence. The
[registered-artifact reclamation successor](../knowledge/engineering/verification/2026-10/2026-10-28T185322Z-reviewed-registered-artifact-unlink-and-truthful-partial-recovery-qualification-4c85687a35ef4b54b24b3fc98b6e5083.md)
adds reviewed intent, actual unlink, partial recovery and retained result-publication debt through
physical Close. The
[incomplete-publication replacement](../knowledge/engineering/verification/2026-10/2026-10-28T194040Z-incomplete-artifact-publication-custody-and-unified-reviewed-recovery-e4c2d017cc7f4000ac1b38588809102a.md)
adds actual partial artifact facts, retained cleanup handover and registration recovery using the
same reclaimer. The [native Prefab successor](2026-10-29-native-prefab-template-import-plan.md)
adds closed Prefab collection copying/import with source-owned identities, sidecars and reference
remapping, fixed preparation and retained directory publication. Scene/image collections, explicit
external bindings, existing-owner Schema evolution and full management controls remain open;
this is not a complete R1b claim.

The next coexistence consumer keeps two installed, pre-admitted tool owners available together.
Updating or disabling A must preserve B's Inspector route and still-valid review, reject A's
stale result and observe A's physical retirement. Replace singleton contribution routing rather
than repeatedly switching one active slot; child execution can remain serialized. The current
single-slot policy is deliberate, so reconcile ADR 0118 before extending it. First refuse an
overlapping property contribution explicitly instead of inventing a universal arbitration kernel.
Static Rust objects and finite children do not prove resident native-code unload. Before claiming
that capability, test retained Editor views, commands, candidates, history and destructors under
the selected R3c/R4a code-custody rule; ordinary static APIs need no speculative unload tokens.

Acceptance: panel/session parity, exact result, one Undo, Save/Reopen, rejected/stale proposal,
delayed completion after newer requests, multiple observers and bounded structured diagnostic
projection. Do not call a panel example inline Inspector or dynamic-package qualification.

Delete duplicated validation/result inference in consumers; no second Undo or document store.

## R2: Owner-Owned Prepared Editing And Selection-Independent Queries

**Design:** D04, D17. **Owners:** Scene/Prefab authoring and tooling adapters. Qualified
implementation scopes and remaining gaps belong in the ADR ledger and engineering evidence.

Expose bounded prepare/diff/commit over the same validated candidate and inverse; allow a
single-document entity construction plus multiple components/fields using transaction-local
references. Add document/object/field queries that do not change selection or active tabs;
bind cursors to query/source/revision/Schema, with explicit stale result and scan-work limits.

Acceptance: failure at the last operation publishes nothing; stale candidate/cursor, history
budget refusal, unknown record preservation, gesture cancel, exact Undo/Redo and saved identity.
No cross-document atomicity or multi-file atomic Save claim. Compare API concept count with
current action choreography and record preparation/commit costs.

Delete standalone dry-run validators, tool-built inverse logic and batch-as-transaction usage.
Amend 0026/0047/0118 for the selected contract, not an imagined universal document framework.

The first concrete interface uses explicitly named source entities, admitted templates and
direct entity-ID enumeration of open documents. Its owner-bound candidate and inverse are shared
by preview/commit and ordinary direct apply. A bounded one-level direct Record-member projection
now reuses the same basis, cursor owner and scan/row/metadata limits; the Session and registered
panel facades expose it without mutable document access. An independently compiled non-active
panel consumer proves entity -> component -> root -> direct member -> bounded read -> prepare ->
retain -> commit, Undo/Redo and Save/Reopen through its
[independent successor](../knowledge/engineering/verification/2026-10/2026-10-26T024129Z-pre-budget-query-ownership-and-independent-direct-member-panel-workflow-1acbda3fb8cc47218dd9b54bcd59f367.md).
The [retained construction qualification](../knowledge/engineering/verification/2026-10/2026-10-24T224939Z-retained-source-construction-pinned-public-and-native-qualification-e1813acad6f04a93898a1e342989fa44.md)
separately covers transaction-local allocation and references. Recursive or collection nested
discovery, Prefab overlay, full field query, shared-owner preparation locality and the remaining
preparation-cost comparison obligations stay open. Do not turn process-local incarnation counters
into durable allocated identities merely to complete the first consumer.

## R3: Behavior Representation And Immutable Module Publications

**Design:** D02, D03, D06. **State:** the closed native/data Scene family is adopted under ADR 0006
and qualified through public, managed and product consumers. C#/Lua Schema-driven execution and
body/lifetime comparisons remain bounded experiments; complete language workflow, executable
publication and replacement remain open. **Owners:** Schema, Scene materialization,
concrete language adapter and executable Host; Proposed 0120.

**R3a representation:** compare an explicit attachments envelope and owner-managed rows with one
bounded adapter and two distinct Behavior Schemas, not two Rust component types. Do not first
build each representation in each language. Runtime-independent address semantics come from
R5a; final Prefab compatibility waits for the selected R5b overlay, not qualification of a
known-to-be-displaced operation-patch format.

The [executed comparison](../research/2026-10-27-behavior-representation-comparison.md) selects
the data-only entity envelope provisionally for R3b. Rows reduce inspected records for selective
queries but require explicit cleanup on ordinary destruction. VM instance/code custody and real
dispatch costs may change the choice for a future executable adapter. The original private
fixture's hidden authoring/materialization Runtime has been removed; current language consumers
use the adopted public Scene family and real Project Sessions. The comparison's transformed
version-two data is not an owner-catalog migration transaction.

**R3b language workflow:** fix the representation enough to compare the same gameplay task with
C# and Lua feasibility adapters. Choose a production candidate on author workflow, targets,
lifetime and measured toolchain cost. This is not a commitment to maintain both SDKs.

The [October 27 extension study](../research/2026-10-27-language-extension-product-boundaries.md)
adds a complete Editor-tool discriminator: evaluate C# first for one `EnemyStats` component,
create command, property customization and dockable panel through public owner APIs. Compare
against the source-built Rust workflow, including diagnostics, one Undo, Save/Reopen, Rust access
to the same data, rejected update, disable/restore and stale task/callback refusal. Keep Runtime
and Editor assemblies/deployment separate. This prioritizes a candidate, not an official-language
announcement or a second maintained SDK. Rust remains complete without a VM. A universal
language-backend trait, native binary ABI or replacement package resolver is not a prerequisite.
Coordinate this tool with R1b rather than creating a second install/lifecycle mechanism.
The finite child-tool successor establishes the shared Session/Application owner. Its next
inline-property consumer is the same R1b workflow above, not a second language-tool lifecycle.

The [first executable slice](../research/2026-10-27-behavior-language-workflow-comparison.md)
passes the same Scene/Prefab task through real hostfxr and Lua calls with exact native command
authority. It is a fixed DTO/event harness, not generic Schema binding, an optimized domain-call
comparison or a language/SDK selection. Author iteration, data/VM authority and concrete owner
lifetime remain required before production integration.

The [Schema-driven successor](../research/2026-10-27-schema-driven-behavior-comparison.md)
replaces the fixed extraction path with external catalog admission and complete callback
availability checks. Three authored Schemas, stable/wire field mapping, Attach/Edit/history,
disk persistence and repeated Prefab identity have private evidence. Fresh catalog admission
is not successor migration. The body/lifetime comparison below adds bounded iteration/ownership;
complete Editor author workflow and representative domain-call cost remain unqualified.

**R3c replacement:** qualify only the modes needed by the selected owner, after its native
completion/retirement proof in R4a. Defining coexist-and-switch and Stop-first semantics does not
require every Host to implement both. Shared facilities and general phase-plan work are not
automatic prerequisites to the fresh-Runtime representation trial.

The [body/lifetime comparison](../research/2026-10-27-behavior-code-iteration-and-lifetime.md)
adds one-owner unchanged-Schema replacement at a synchronous safe point. Actual same-path managed
rebuild, Lua source edits, rejected last binding, native World continuity, old-result fences and
held callback/instance roots have private evidence. This informs language-owner selection, not
completion of R3c: arbitrary in-flight work, dependency cohorts, Stop-first, catalog migration,
post-publication cleanup failure and public integration remain open.

The [Scene-owned data representation plan](2026-10-27-scene-data-representation-plan.md) has
qualified its closed compiler/materializer/export family under ADR 0006, including public
independent, managed and product consumers. That adopted data storage does not promote
Proposed 0120's executable publications, migration or dependency cohorts. The obsolete private
preview helper is removed. Remaining execution and tool facets proceed independently of the
future owned UI.

Acceptance: independent attach/edit/Undo/Prefab/source round-trip, recursive refs, one-record
migration, missing-module preservation, mixed native materialization, exact absence/export/retire
and stale-generation rejection. Select same-type multiplicity explicitly. Compare dynamic Bevy
descriptors only against a named storage/query requirement; keep unsafe layout logic bounded.

First qualify fresh-Runtime replacement; then test module-body replacement and owner-catalog
replacement separately. Failed migration preserves old executable plus compatible data; callback,
task and drop custody prevent early unload. Native registry remains frozen. Independent projects
must not share accidental catalog authority. Complete representation proof precedes relaxing
native-only execution requirements. Promote or revise 0120 only with this evidence.

Add cross-owner A->B plus independent C: reject or jointly reprepare/migrate A on incompatible B
while C stays usable. Exact adapter read-sets cover retained access/query/codec plans, not just
changed Package releases. Even with no active callback, an old executable snapshot/prepared/binding
object keeps code alive until disposal and cannot submit through new routing. Pure data captures
need no executable lease.

Qualify coexist-and-switch and Stop-first separately. Fail the last required candidate member,
hold old retirement Pending and fail retirement after new routing publication. Report actual
active routing, compatible restartability and physical custody. Stop-first preserves a restartable
last-good, not an instance it already destroyed; post-switch cleanup failure is not route rollback.

Delete false universal TypeId/binding assumptions; preserve exact native candidate safety.

## R4: Domain Execution Rights, Activity Scopes And Native Close

**Design:** D07, D08. **State:** R4a local custody, typed-composite selection and concrete retained
Editor graphics have their bounded qualification under the
[owner-local close design](../architecture/native-close-ownership-design.md). R4b's first
[activity-custody unit](../architecture/task-activity-scope-design.md) has bounded Tasks/Image and
independent-consumer qualification; lazy/shared
comparison now has [bounded release Image evidence](../benchmarks/2026-10-26-task-facility-locality.md).
It challenges unconditional sharing for interactive progress. The selected private-lazy Editor
deployment and whole-Session physical close have their
[bounded qualification](../knowledge/engineering/verification/2026-10/2026-10-25T213241Z-private-lazy-editor-tasks-and-whole-session-physical-close-qualification-4c62c82789e4449dba02bb60f145760c.md);
broader activity-scope integration and R4c remain open.
**Owners:** App/Tasks, executable Host and domains.

The [I/O substrate study](../research/2026-11-03-io-substrate-and-engine-contracts.md) separates
crate-backed file/execution mechanisms from Nara's authority, publication and physical-return
contracts. Existing `rustix`/`windows-sys` adapters and injected import readers remain the baseline.
Before replacing private worker dispatch or strict directory-relative opening, compare one actual
operation against the candidate substrate under the same invariant tests and consumer. A different
crate must remove meaningful implementation while preserving the required guarantees. This is a
bounded reuse evaluation within these owner units, not a new universal VFS/Future runtime, an
executor selection or a reason to repeat qualified production workflows.

Three independently verifiable sub-units:

1. **R4a native close:** compare typed composite close owners with a minimal sealed completion
   dependency plan. Support owner-local participants and separate quiesce from destructive release.
   Pending or physically incomplete failure blocks provider release; failed-but-complete retains
   errors. This is a prerequisite to the selected dynamic replacement mode.
2. **R4b activity scopes:** compare lazy per-Runtime facilities with Host facilities plus scopes. Scope close
   proves owned work/disposal return without claiming shared worker join. Keep frame access-aware
   execution, blocking work, Future I/O and real-time callbacks distinct.
3. **R4c execution phases:** compare public sets, a sealed automatic phase plan and
   `bevy_app::App::empty()` using independently authored Physics-like/Animation-like flows.
   Preserve one driver; select success/finalization and retirement boundaries through the joint
   consumer, and reject bad topology before Startup.

The [November 3 joint unit](2026-11-03-joint-motion-execution-plan.md) has the initial three-way
comparison and thin real borrowed-solver consumer. It retains public sets and narrow consumption
Ack for that bounded workflow and makes the production automatic serial baseline explicit under
feature unification. Its successors qualify captured discontinuity and a closed same-World
prediction/offline-PCM model, including exact attempt identity and preservation of the confirmed
prefix. Private raw parallel execution is not a production profile. Remaining complete-consumer
gates keep R4c open; these experiments do not freeze a new phase interface.

Bring the R10 motion oracle into this comparison before selecting the schedule API: motion intent,
kinematic target/solve, solver pose commit and spatial publication need an explicit same-tick join.
Use a deterministic stand-in for initial sequencing, then verify the actual borrowed-job join
with a thin real solver before freezing the supported phase/executor interface. Borrowed same-tick
jobs join before pose commit; owned cross-frame Tasks keep their
existing admission/cancel/disposal custody. TaskUpdate before the fixed loop is not that join.

Select a supported executor profile at composition/seal and apply it to all automatically driven
schedules. An unrelated dependency's Bevy `multi_threaded` feature must not silently select engine
execution policy. Compare an explicit serial baseline and supported explicit parallel execution,
including feature-unified independent consumers, NonSend placement, deferred work and faults.
Unavailable requests refuse before Startup; ECS, solver and background worker budgets remain
distinct facts. Record exactly which profiles are admitted: a parallel comparison does not itself
qualify production parallel support. This needs no new executor implementation or global pool.

Resolve command consumption/Capture/Ack versus whole-tick success in this same consumer. ADR 0057
currently permits later unrelated failure after Ack, while D07's stronger successful-tick goal
would place every authoritative phase and deferred pose/spatial effect before finalization.
Either reconcile 0003/0039/0057 for that stronger boundary or retain narrow Ack and revise the
stronger design claim explicitly. Neither option implies rollback of partial World/native effects;
preserve truthful sticky-fault, close and new-generation recovery behavior. Select and reconcile
this contract before R4c phase/executor interface freeze. Exercise the same trace where command
consumption/Capture completes and a later authoritative phase fails; observe Ack, tick-success
and retained effects independently under the selected rule.

The same motion consumer also supplies R9's discontinuity/captured-history interleaving control;
select that shared temporal contract before freezing the R4c execution interface.

Also select the bounded prediction admission and effect-publication boundary from R10 before
that freeze. Ordinary fixed scheduling and ECS access analysis do not make an arbitrary system
reversible. Distinguish command completion, simulation completion and authoritative confirmation;
retain the chosen ordinary Ack contract independently of future speculative confirmation.
Define how an abandoned attempt of the same tick rejects late results, without treating physical
task cancellation as reversal of its I/O. This gate selects semantics and refusal behavior;
complete rollback implementation remains the separate R10 qualification.

Acceptance: two independent scopes, independent close, saturation/cancel/late/sub-work,
uncooperative worker, partial startup, local SDK controller and close cycles/missing dependencies.
Measure admission-to-start, interactive latency and worker/facility multiplicity; do not repeat
old worker counts as new performance evidence. Fix earlier F4 Stop observation retention using
an explicit retained/expired result policy, not a larger unexplained ring.

Replace 0008's long-term model only with a complete successor; reconcile
0003/0010/0039/0042/0052/0057/0082/0084 for affected contracts.
Delete extra physical owners and accidental hardcoded phase knowledge when superseded.

## R5: Durable Addressing, Prefab Overlays And Projection Lifetime

**Design:** D04, D05. **State:** qualified sub-units are recorded in the
[implementation ledger](../architecture/adr/implementation-status.md); broader work remains open.
**Owners:** identity/Schema/Scene and preview Host.

R5a selects address semantics before R2/R3 persistent contracts freeze. R5b replaces durable
Prefab overlay/rebase semantics before Behavior/Prefab production qualification. R5c projection/
history lifetime proceeds independently of language and package choices; it need not wait for R3.

R5c also owns the integrated Host's per-kind persistence-lane limitation exposed by the public
non-active editing consumer. Replace the singleton Scene/Prefab file lanes with concrete
per-document custody before claiming arbitrary same-kind multi-open. Keep source file Hosts,
active preview binding and executable startup selection distinct. Include exact file-alias
admission, independent Save/Reopen/Close, project shutdown and predecessor failure/cancellation;
simply disabling document retirement would strand an open slot without its persistence owner.
The [bounded custody design](../architecture/editor-document-persistence-custody-design.md)
maps the original singleton consumers, ordered replacement and failure/native acceptance. Its
selected custody, source/preview, refresh, additive Open and Play-source contracts now have
separately scoped implementation evidence in the ledger. Neither the design nor those bounded
qualifications complete the broader R5c projection/history lifetime work.

The bounded R5c projection successor adopts a [same-World disposable Scene epoch](../architecture/editor-preview-projection-lifetime-design.md),
not full authoring Runtime reconstruction or a second authoring history. Identity proves the
complete predecessor history/live-token responsibility before Scene prepares and again before
exclusive publication. Ordinary same-domain Scene consumers retain their existing history.
The [frozen incumbent and candidate comparison](../benchmarks/2026-10-24-preview-projection-lifetime.md)
records actual N+1 steady claims and bounded publication costs. The
[scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-23T225800Z-disposable-scene-projection-epochs-and-stable-native-preview-qualification-2478d897135549daacbc8d1fbc90b9d9.md)
records the separate held task/image/Save, joined public workflow and actual native evidence.
Incremental projection,
durable addressing and the remaining waves are not closed by an epoch policy.

Separate source identity, structured projection chain, durable property and runtime incarnation.
The [R5a design](../architecture/durable-property-addressing-design.md) bounds its qualified
implementation to a Schema-relative property selector: root field IDs are component-scoped and
nested IDs are sibling-scoped; root-relative declaration-path tombstones retain retired scopes.
Tagged selectors distinguish members, keyed items and map entries, and keyed selectors name the
stable identity member as well as its value. Component version is a value basis, and only
migrated/current values resolve. Same key
means same logical identity; arbitrary deletion history is not manufactured to detect accidental
reuse. Complete source/document/occurrence identity remains owned outside Reflect. The selected
metadata/lineage contract and ADR 0011/0081 amendments have separately scoped qualification; do
not repeat that work as a prerequisite to overlay preparation.

The earlier persistent UI-root collision has a
[complete-key successor](../knowledge/engineering/verification/2026-10/2026-10-24T185535Z-complete-persistent-ui-identity-and-neutral-conformance-successor-0cf84bdfd2f54766be84f3ca206f7931.md)
under ADRs 0038/0102; it is not an outstanding reproduction or an overlay implementation. Never
substitute runtime instance, preview epoch or process-local Workspace IDs for that identity.

The [declarative Prefab overlay design](../architecture/prefab-overlay-design.md) has progressed
through its private comparison into the production replacement under ADRs 0038/0043. Durable
leaf/component/instance intent, local additions, desired placement and typed reference bindings
are distinct from exact Editor operation/inverse patches. The implementation ledger and linked
verification own the completed scope; do not repeat the comparison as the next work item.
Remaining R5 work includes its Behavior/schema-evolution consumer and the broader source/runtime
lifetime outcomes below. Complete conversion has its separately qualified successor.
Qualify ordinary incremental projection versus epoch reconstruction only when an actual measured
projection workflow requires it, not by adding a generic incremental graph.

The October 30 [content study](../research/2026-10-30-engine-direction/content-and-persistence.md)
refines the remaining evolution consumer. A component-value migration does not migrate versioned
property claims, selectors or occurrence paths. Qualify an explicit reviewed project upgrade over
direct records, dormant overlays and dirty dependent documents, with a changed unit, an unaffected
stable pin and an unavailable owner. Choose a new history checkpoint or prove retained old-history
bases; do not reinterpret old inverses or run project migration code during ordinary generic Open.
Separately extract a nested Prefab subtree into another source using a complete old/new occurrence
map and dependent claim/reference rewrites. Keeping terminal entity IDs is insufficient when the
source/hop chain changes. Unknown records remain intact or cause a specific refusal. Reuse the
existing document transactions and per-document history; no parallel global Undo engine is needed.

Before the first source replacement, retain bounded recovery information for the reviewed operation,
including participating dirty inputs. Interrupt before publication, between replacements and after
publication before acknowledgment, then reopen in a fresh process. Identify verified old/new and
unresolved members, fence external edits and make retries idempotent. Per-file rename cannot attest
project-wide atomicity. Recovered Check/Play consumes a coherent admitted generation or refuses.
This is meaningful authored evolution, not a requirement to retain abandoned pre-release readers.

The [reviewed authored-upgrade unit](2026-11-01-authored-schema-upgrade-plan.md) now has
[qualified CP1/S2 implementation](../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md):
complete value/claim/selector transformation, retained all-profile inventory, explicit dirty-source
history checkpoints, guarded Ready adoption and actual fresh-process interruption recovery.
The fixed-v2 consumer covers ordinary follow-up editing/persistence and Check/Play. CP2
nested-source extraction remains open, including instance-root identity and cross-boundary
reference representation. This bounded qualification does not complete R5 or the foundation Goal.
The [pinned extraction study](../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md)
identifies instance-root identity and cross-boundary source references as representation gates.
Settle the expected branch-to-Prefab workflow before freezing a child-only extraction subset;
reuse neutral publication mechanisms without fabricating a Schema successor for topology changes.
The [complete CP2 unit](2026-11-02-prefab-extraction-plan.md) now owns representation selection,
source/compiler replacement, complete correspondence and the independent publication/recovery
consumer. Its selected direction combines explicit root binding/forest mounting, source-owned
typed placements, inline reference values and separate flat compiled output. The pure identity
ownership prerequisite is qualified separately; no extraction qualification follows from that
prerequisite, the earlier source study or CP1 result.
The [qualified final-reference-demand correction](../knowledge/engineering/verification/2026-11/2026-11-01T231259Z-final-reference-demand-and-preallocated-property-paths-qualification-fdb7dce644a84dcbb5b8b81fee9ec514.md)
removes eager lookup of overwritten property references and admits retained storage paths before
copying. The [qualified inline-reference replacement](../knowledge/engineering/verification/2026-11/2026-11-02T031431Z-inline-authored-references-and-contextual-default-production-qualification-6f60b71dc5c6458bbd6c9c63f1df239e.md)
uses lexical payload/input scope independently of structural target scope; explicit routes select
nested inputs. Closed compiled storage and its complete public compiler Interface now have their
[production qualification](../knowledge/engineering/verification/2026-11/2026-11-02T071856Z-closed-compiled-scene-production-and-independent-delivery-qualification-91cc04f45ba249f881cec4f9f78bd3d1.md).
Explicit root binding/source placements now have their
[production prerequisite qualification](../knowledge/engineering/verification/2026-11/2026-11-02T121917Z-explicit-root-binding-and-source-placement-production-qualification-0eada24ff287416bbfe94ccf511d2380.md).
Domain construction/correspondence, Session/Application publication and CLI-only recovery now have
[bounded integration evidence](../knowledge/engineering/verification/2026-11/2026-11-03T094705Z-recovered-extraction-process-and-integrated-source-owners-pass-current-qualification-e6d36a7a778d4f3494963eec011be399.md).
The [complete public/native successor](../knowledge/engineering/verification/2026-11/2026-11-03T115127Z-complete-public-prefab-extraction-and-native-workflow-qualification-c440d6b31843411897951e9ba2221a2e.md)
qualifies the selected CP2 independent workflow, strict Check, actual supervised Play and native
extraction controls within the recorded Windows/source/filesystem scope. Broader R5 and the full
foundation remain open; the next joint motion/phase/temporal/effect consumer keeps its own gates.

Acceptance: nested and repeated instances, base rename/reorder/delete, same-name members,
Behavior migration, convert-to-local and explicit remap. Long edit churn preserves stale-alias
protection without silently exhausting lifetime identity. Amend 0058 before changing classification:
eviction from a continuing bounded history requires Expired/HistoryUnavailable, while checkpoint
history has an explicit owner. A completely owned disposable projection instead may end its entire
domain epoch under the adopted proof, without evicting history from a continuing domain or adding
a global Expired interface. Test attempted readmission of a formerly Deleted identity after history expiry: refuse
without the required evidence or use an explicit distinct epoch/timeline contract, never silently
resurrect it. No high-watermark claim for arbitrary history. This resolves earlier F5 as a lifecycle
redesign, not by clearing tombstones or changing Scene data into World truth.

Keep the removed operation-patch-as-long-term-overlay path removed. Address any remaining
source/runtime identity conflation in its actual consumer rather than reintroducing that reader.

The [complete-conversion plan](2026-10-27-prefab-conversion-plan.md) now has a
[qualified Source/Editor successor](../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md):
selected compiler demand, semantic data retention, fixed local-ID/reference remap, exact source
history, current Host dependency fences and selected native interaction. This closes direct
selected-instance complete conversion, not outer-only unpack, cross-source Apply or full R5.

## R6: Resource Facets, Durable Continuity And Local Artifact Store

**Design:** D10, D11, D13. **State:** source/import, compound continuity and bounded Runtime
codec/base/Host and finite local-build slices qualified; general build scheduling and complete
packaged delivery remain open. Automatic dependent-resource refresh is a separate unimplemented
unit, not part of the finite-build qualification.
**Owners:** asset/domain contracts and project Host.

Build one independent compound importer through typed source, Runtime decode, optional cook and
Editor facets; separate format contracts from root/2D dependencies. Choose authored continuity
for subproducts without intrinsic IDs. Then implement local recipe-index/group-manifest/CAS
publication with tracked inputs; identity does not come from generated cache or byte hashes.

R6a is the facet/continuity/verified Runtime-product contract. R6b adds the durable recipe index
and local CAS; it does not block an independently qualified R7 immutable base catalog.

Before R6b reuse or target cooking becomes public, the executing owner must close its finite build
recipe: selected versioned importer/cooker, normalized settings, observed auxiliary/processed inputs
and consumed target facts. In-process `TypeId` and a profile label are not durable recipe authority.
Keep logical Product identity, recipe key, output-byte digest and immutable group manifest distinct.
Qualify two clean builds, auxiliary/settings/target changes and interrupted group publication with
the existing compound importer plus one real target-sensitive consumer. No stale-cache defect is
asserted for the current capture-local cache; this is the upcoming reuse contract's freeze gate.
The [local build design](../architecture/local-artifact-build-design.md) and
[implementation plan](2026-10-31-local-artifact-build-plan.md) now own this successor. First
replace synthetic source artifact records with actual import recipes through current Image/
reload/prepared consumers; that prerequisite does not qualify durable groups or target cooking.
The [qualified prerequisite](../knowledge/engineering/verification/2026-10/2026-10-30T201802Z-source-import-recipes-and-target-sensitive-cpu-image-cook-prerequisite-qualification-462c0179d20b44fd8545d4c135e814f9.md)
now records the actual recipe replacement and real CPU target-sensitive Image transform, with
owning/downstream/public/native evidence. Tracked closed execution, durable groups and target
build/delivery were not qualified by that prerequisite alone. The
[local-build qualification](../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md)
now closes the finite R6b outcome: typed tracked compound/processed Image execution, complete
local publication/reuse, actual interruption/fresh-process cases and original Runtime custody.
Its Root/Asset1252, final independent57, owner20 and feature/Clippy evidence retain their exact
scope. This does not qualify general dependency scheduling, target executable selection or a
packaged Player.

The [November 2 infrastructure comparison](../research/2026-11-02-infrastructure-comparison/README.md#c6--close-the-asset-change-propagation-loop)
adds a bounded dependent-resource refresh unit after that qualified finite-build outcome. The
selected project Host owns generation-bound refresh subscriptions derived from actual scoped input
observations. Successful adoption of a current resource generation replaces its complete published
dependency set. Artifact-store publication alone cannot publish live content. Warm reuse restores
the same subscriptions without re-executing the importer. Include present and absent raw paths;
auxiliary files need no invented asset UUID or metadata. Keep failed-attempt retry observations
separate from last-good dependencies. Failure, cancellation and superseded completion cannot
install stale relationships or discard the incumbent generation. Build invalidation, Runtime load
demand and live refresh remain separate; escaped immutable Runtime bases retain captured content.

Extend the existing independent `TextBundleDomain` through public Host APIs: mutate its required
suffix and optional `build-suffix.txt`, deliver existing semantic change notifications, and observe
actual typed text updates without manual graph mutation or an explicit rebuild call. Cover
create/change/delete, switching dependency A to B, first-load failure followed by input creation,
warm-reuse subscription restoration and superseded completion. Assert stable Product identities,
last-good behavior, obsolete-edge removal, unchanged escaped generations and zero unrelated
importer/decoder calls. Reconcile ADRs 0007/0037/0103 for this concrete ownership contract before
implementation. This unit adopts neither Proposed0087's general scheduler nor a new watcher
platform, and can proceed independently of complete CP2 extraction.

The [ordered importer and delivery plan](2026-10-29-asset-import-and-runtime-delivery-plan.md)
first moves the existing executing source facet below root/Scene/2D while preserving all actual
consumers, then qualifies compound continuity and processed delivery. Its
[source-contract qualification](../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md)
does not complete R6a or qualify a Runtime catalog.

The [compound successor](../knowledge/engineering/verification/2026-10/2026-10-29T004054Z-compound-product-continuity-and-independent-runtime-format-qualification-a2d6d4837fd44218b7cb87f67cb38063.md)
adds scoped primary/auxiliary identities under ADR 0122, explicit authored row continuity, a
deterministic processed text codec and actual two-Scene source/Editor consumers. Reimport validates
the admitted predecessor and retains metadata custody through physical task disposal. This closes
that named format/continuity slice, not generic Schema product-reference traversal, Runtime
catalog acquisition, source-free startup/later Scene loading or the complete R6a/R6b contract.

The [delivery-boundary audit](../knowledge/engineering/subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
locates the actual source-carrying loader and starter build-policy dependencies and recommends
one two-Scene external text-bundle consumer. Build-machine Cargo and detached-development-root
source delivery remain valid existing behavior. The audit is next-work evidence, not an adopted
Runtime format, processed catalog implementation or execution qualification.

Acceptance: clear-cache stable references, rename/reorder/ambiguity, dependency/settings/target
drift, member corruption, cache explanations and every group publication fault point. Preserve
old admitted groups and retained frame/content leases; deterministic content manifests are not
bit-identical executable evidence. AssetServer Clone/shared database refactoring (earlier F6)
must preserve database snapshot semantics and be measured, not assumed faster.

No generic erased importer executor, global semantic budget manager, remote cache or automatic
native trust. Delete duplicate root/domain registrations and incorrect recipe/content type reuse.

## R7: Target Build And Base Source-Free Delivery

**Design:** D12. **State:** in progress; immutable local base and bounded first-party startup/travel
qualified, including finite source Scene references and global source/processed Input;
explicit Cargo build/capture and Windows headless cooked Player delivery are qualified within
their recorded scopes. Native desktop packaged delivery and additional target/platform
qualification remain open.
**Owners:** Cargo tooling, Host capture/cook and Runtime loader.

The [explicit Cargo selection unit](2026-10-31-explicit-cargo-build-selection-plan.md) now owns
the qualified finite replacement: general Cargo-authoritative facts, current supervised/source
Export consumers and exact captured native/foreign artifacts. It retains separate target
execution and complete packaged delivery qualification.
Its [qualification](../knowledge/engineering/verification/2026-10/2026-10-31T035034Z-explicit-cargo-builds-captured-executables-and-supervised-protocol-qualification-4cfff5128fa64423b6f234ea7b312c63.md)
includes captured input binding and separate bounded Editor protocol transport, actual custom
native/foreign builds, source Export and final supervised Windows/DX12 trajectories. Source
observations, logical bounds and foreign build-only evidence keep their explicit limits.

The qualified [cooked Player delivery unit](2026-10-31-cooked-player-delivery-plan.md) joins retained
target content, a generated independent Player expectation and captured executable bytes through
one finite directory publication owner. Catalog preparation precedes Player compilation, and
exact generated-input/code/content binding rejects old Player A with new catalog B. Reuse the
existing pack, admission, Scene and last-good owners. Its
[qualification](../knowledge/engineering/verification/2026-10/2026-10-31T050529Z-cooked-content-assembly-captured-player-binding-and-relocatable-source-free-delivery-qualification-816fbb7867fb4c3288a2b2b34f27e1ff.md)
records complete member/decoder coverage, exact code/content binding and three-file publication,
actual relocated source-free driver-input/Scene travel, Deferred corruption and retained original
custody on Windows headless. Foreign artifacts remain build-only; additional native libraries,
server stripping, desktop presentation and further platforms keep separate qualification.

Separate Host tool selection, target executable artifacts, content cook, release candidate and
target-runner activation. Keep a source-carrying option but move fixed starter assumptions into
its adapter. Build a verified immutable base Runtime catalog; directory transport is sufficient.
Do not block it on patch/DLC, remote authenticity or full cross-file Editor recovery.

The [ordered delivery plan](2026-10-29-asset-import-and-runtime-delivery-plan.md) now separates
shared source-free executable composition, concrete processed Scene retention and verified
local-base acquisition from subsequent native resource residency and first-party Runtime Host
integration. The [base guide](../guides/runtime-content-base.md) states that boundary explicitly;
the source-backed launch entry is not silently reclassified as processed delivery.
The [local-base qualification](../knowledge/engineering/verification/2026-10/2026-10-29T072753Z-immutable-local-runtime-base-and-shared-processed-scene-lifecycle-qualification-140b1b89e14c4130a72ac1c6d8f3de89.md)
at `a3b235f` includes actual two-Scene same-plan managed consumption, complete compound identity,
file removal/corruption, concurrent decode/retirement budgets and native regression scope. It does
not complete first-party packaged launch, automatic family residency or target build selection.

The [first-party integration successor](../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md)
at `44470ed` closes bounded Host startup/Scene requests, stock Image/TileSet residency and typed
CPU reads using the original base/plan. It removes mandatory source execution from native
Asset/Image plugins and qualifies finite retained Scene data dependencies, cancellation, exact
baselines and pre-allocation budgets. It does not qualify the complete packaged Player.
The [source Scene qualification](../knowledge/engineering/verification/2026-10/2026-10-30T060408Z-finite-source-scene-data-closures-and-retained-editor-observations-qualification-deea8e4f86ac44f3a6faf27aefa53a4e.md)
at `f96951a` closes canonical source resolution, the actual Source Door/Save/Export consumer,
finite dependency custody and exact retained Editor observations. The active
[Input source unit](2026-10-30-input-definition-and-source-admission-plan.md) replaces mutable
live definitions and detached retention with immutable actual owners, move-only context state and
scope-only startup selection under amended ADRs 0035/0041/0051. Its
[qualification](../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md)
records actual-file, independent physical travel, Editor receipts and relocated source Export.
The [processed Input unit](2026-10-30-processed-input-startup-and-process-proof-plan.md) now owns
the independent Runtime Product selection, actual acquisition custody and prebuilt source-free
physical process proof. Its [qualification](../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md)
records the complete bounded owner/public process and selected Windows/DX12 results. It does not
close R10 endpoint/contribution admission or general target-build/package delivery. Future third-party native families
need a typed publication facet through the same owner before their Material/render interface
freezes; CPU decode alone grants no residency.

Decide the future content-set boundary now: Product identity never becomes a package/mount key.
A provider or overlay must contribute a separately admitted immutable context, with its selected
membership/dependency closure and original executable plan; it cannot mutate a live base or
retarget admitted acquisitions/recovery inputs. Equal catalog bytes do not establish the same
base/plan owner. This preserves the directory-backed implementation without selecting overlay
precedence, a package resolver or another storage executor.

Before freezing Scene travel/reference semantics, separate a valid deliverable target from a
dependency required to materialize the requested Scene. Both participate in strict Check/Export
closure and target validation; only selected demand requires current preparation/residency. The
present `Soft` edge permits absent membership and cannot implement validated lazy destinations by
renaming `Required`. Qualify three Scenes linked by Door/menu components, a shared atlas and heavy
later-level resources through both source and processed paths. Opening A resolves the mandatory
target bindings revealed by its actually demanded payloads, including B, without decoding B to
discover C or claiming either destination's deep payload validity. Their heavy resources remain
outside A's preparation.
Strict validation checks that delivery closure; an explicit request acquires its selected
destination under the original plan, cancellation owner and transactional publication tail.
The [selected CP3 design](../architecture/reference-validity-and-load-demand-design.md) and
[complete unit plan](2026-10-30-reference-validity-and-load-demand-plan.md) now define the
per-leaf demand, exact actual/catalogue contract and source/Editor validation split. The
[pinned source comparison](../research/2026-10-30-reference-validity-and-current-demand.md)
supplies source constraints; the [production qualification](../knowledge/engineering/verification/2026-10/2026-10-30T172312Z-reference-validity-and-current-demand-production-replacement-qualification-b16511fa33a74c2b808270394b28bf62.md)
records the implemented successor, actual source/processed travel, strict validation/Export,
ownership budgets and selected native scope. It does not claim performance or CP4 backing results.

Separately compare the qualified eager base with a concrete retained local pack and immutable
expected member meaning before
claiming larger content-store suitability. Immutable catalogue/codec authority, encoded member
backing and decoded/native residency have different lifetimes. A lazy acquisition retains its
original backing and needed data, never a newly opened mutable directory or current global base.
Test encoded eviction, overlapping generations, cancellation and an escaped Image. State each
backend's availability guarantee; retain the eager backend's captured-file-removal behavior.
Lazy backing alone does not reduce a Required preparation closure, and selective demand alone
does not release an eager base's encoded bytes. These are separate experiments within R6/R7,
not prerequisites to requalify the completed bounded source-free startup/Input units.
The [selected backing design](../architecture/runtime-content-backing-and-residency-design.md)
and [retained-pack plan](2026-10-31-retained-runtime-pack-plan.md) make this comparison concrete:
keep complete memory capture and compare one retained file with authenticated temporary member
reads, without claiming portable OS byte freezing or adding a general provider/cache framework.
Its [production qualification](../knowledge/engineering/verification/2026-10/2026-10-30T184847Z-retained-runtime-pack-and-independent-encoded-residency-qualification-386d2e5e9c124b66bc4390f69b796053.md)
now records Root1022, independent50, real travel/physical cancellation, exact overlapping work,
original generations and escaped prepared pixels. This closes the bounded CP4 comparison, with
no GPU/large-game streaming performance or general-provider claim.

Acceptance: two clean content builds, a different host/target selection, source/cache inaccessible
and no Cargo on the runtime machine, server stripping, wrong target/schema/digest rejection,
failed candidate retaining last-good executable/data. State non-hermetic inputs and actual
source-capture consistency. Foreign-target build-only is not native execution evidence.

Delete implicit profile-name-to-Cargo policy and code/source/package generation conflation.

## R8: External Render Feature And Host-Local Execution

**Design:** D14. **State:** private CPU/DX12/cost comparison, internal device ownership, original
UI custody and owned native desktop execution have scoped evidence under the
[render composition unit](2026-10-31-render-composition-and-host-execution-plan.md); complete
production replacement and matching qualification remain open. **Owners:** render semantics/domain
adapters/WGPU Host; Proposed 0121.

The preferred complete design is typed Features over an explicit resource-dependency graph, with
runner-owned GPU execution. Named slots are convenient placement that lowers into this same model.
Design for shared/branching intermediates, multiple views, custom Editor composition, offscreen
targets and recovery now; do not select an initial linear core merely because one effect is small.
Use pinned Bevy and Godot as primary architecture evidence without copying a historical graph API.

The [native ownership qualification](../knowledge/engineering/verification/2026-10/2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md)
at `37ca8bb` moves stock execution outside World through a retained Runtime/Host/window/pump session
and exact CPU capture/result connection. Post-Host UI/READY, finite telemetry seal, code-first and
project consumers have owner/public and selected Windows evidence. Its
[immutable-output successor](../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md)
now qualifies exact pixels/display occurrences, owned producer admission, borrowed final target
activation, original reconstruction and submitted-prefix responsibility through actual native and
independent public consumers. Continue the production graph/typed Feature and alternate-policy
transition through these owners. Proposed0121 and the complete foundation Goal stay open.

Accepted ADR0094 now selects the finite typed texture/buffer program prerequisite through that same
Host. Follow its [production design](../architecture/render-program-production-design.md) for actual
stock adapters, including a whole-candidate Image preflight before any upload/state creation,
shared native realization and submission-boundary instance accounting. Stock Runtime windows and
Scene/Canvas/Game/image consumers now use that program transaction, including its final surface
consumer and producer-local world permissions. The
[typed policy successor](../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md)
qualifies original policy captures and alternate-policy consumers; the central capture fields and
facade family have retired. Selected View/history and composition successors below retain their
own scope. Broader View/time facts, durable materials and the editable frontend remain open.

ADR 0121's [identity, wiring and candidate boundaries](../architecture/adr/0121-composable-render-programs-and-host-execution.md#identity-wiring-and-candidate-boundaries)
close three remaining shared contracts before interface freeze: stable Feature-instance and
candidate-local resource identities, explicit semantic-output/resource compatibility, and output
publication groups with scoped failure. These additions extend the already specified exact output
leases, history transitions, stream retirement and recovery basis. They remain Proposed design
requirements, with no implementation or qualification claim.

The [content import/UI output boundary](../architecture/adr/0121-composable-render-programs-and-host-execution.md#imported-content-and-ui-output-boundaries)
also closes the cross-owner seams before interface freeze. Capture exact domain content before
Host/device-local import; preserve procedural inputs and existing Image snapshots without a
mandatory universal lease type. UI composition retains exact Host outputs in its adapter-owned
packet, while kernel image keys remain opaque identities. Display placement/clip/input mapping
joins the producer output at composite admission. Qualify source/processed generation interleaving,
contribution withdrawal with retained work, and two UI placements of one output. Initial static
registration and one copy/direct-sampling path are legitimate staged capabilities within that
contract; dynamic loading, hot unload and a complete toolkit are separate work.

Close the repeated-display join with ADRs 0108/0112: output pixels, destination display attempts
and one-shot Runtime UI completion differ. Admission designates one exact interactive completing
attempt; passive mirrors and repeated repaints cannot complete it again or advance history.
Initially allow one interactive occurrence per logical surface. Passive mirrors retain pixels and
placement facts without producer route/session, Game capture, IME or actionable accessibility.
Active-to-passive transition revokes old occurrence authority and rejects queued old actions without
leaking them to gameplay. Mirror failure revokes only that occurrence; loss of the interactive one
cannot be repaired by keeping a passive copy visible. Use a Game output at 30 Hz, preview at 60 Hz
and Editor redraw at 144 Hz as test schedules, not performance claims. Exercise a mirror displaying
first, designated completion once, hide/failure/downgrade and producer-local retirement. The
current prepared/loss/late-commit Input correction is required independently of the full R8 graph.

Validate the architecture on an independent two-pass Scene-color blur, keeping Runtime HUD crisp,
then retain original color for an Editor before/after view and independently chain a second effect.
Compare viable slot, ordered-plan and open-schedule controls against the resource graph on that
complete consumer, including cross-view consumption. A slot can own both passes and private scratch;
two passes alone do not prove a graph necessary. Counterexamples must challenge the preferred
graph's dependency/lifetime model, author ergonomics and evolution cost, not just its initial line
count. Record concrete reasons if the full-workflow evidence selects another complete design.

Before freezing an interface, define physical resource versions, initialization, read/write aliases,
target usages and the supported 2D subresource scope. Initially keep live transient outputs distinct;
shared logical IDs alone do not preserve old pixels. Pair a typed immutable capture with its exact
native adapter at registration; erase only inside the Host at feature/operation granularity.
Separate registration, captured values and Host/epoch-local state. Move stock Sprite/UI encoding
through that same seam. Avoid a public Any bag or gameplay-owned Reset/Collect/Seal choreography.
Raw exact-wgpu adapters remain trusted Rust, not a sandbox; keep submission/finalization out of the
ordinary contribution API without implementing a second Queue/RHI or source-analysis verifier.
Design import/export and lifetime boundaries so future buffer/storage operations and temporal
resources have explicit access/initialization/retention semantics within the same graph. Initial
image-only qualification does not claim those operations implemented. Keep automatic aliasing,
pass fusion and execution parallelism separate from the foundational graph contract.
Dependency declarations must not depend on registration/insertion order; initial execution may
use a deterministic serial topological order. Design texture and buffer access together, including
sampled/attachment/storage/copy/uniform/vertex/index/indirect use and legal read-write distinctions.
Declare discard versus preserve-from-exact-predecessor for each write; a new allocation does not
inherit old data and cannot overwrite a still-live read/export/history version. Initially use
distinct storage plus explicit initialization/copy for preserving writes. Conservative whole-resource
tracking is an initial support choice, not an API assumption that every operation exchanges color.
Add a compute luminance-to-storage-buffer/custom-drawn-histogram consumer with preserved raw
statistics and a storage-to-draw-input path before interface freeze, rather than merely reserving
a future enum name.

Represent history as an import of a previous producer/View/Feature-instance/history-slot version,
not a cycle in the current graph. Pin history inputs during admission and require exact predecessor
agreement for coherent producer-output/slot publication; serialize slots initially if needed.
Failed/superseded candidates do not advance history, retry cannot silently change inputs, and
later present failure does not roll back published output. Qualify a bounded accumulation effect
with dropped, failed and out-of-order candidates. Effects define initialization and compatibility
for resize, policy changes and camera cuts versus compatible motion. Re-presenting the same
captured Scene image must not advance history. Same-execution cross-View reads use graph edges;
cross-capture composition selects exact Host output leases once at admission. Test independently
paced streams with one paused and an older preview retained while another publishes. Producer
close and input revocation do not silently release consumers' physical output custody.

GPU Host is runner-local. Distinguish stream/view incarnations, captures, policy revisions, target
generations and Host/device epochs through existing owners. Close requirements before device
creation; late requirements must fit the enabled device or explicitly request reconstruction while
other streams continue. Qualify window-free GPU initialization without a dummy window. Stream close
stops admission, terminalizes pending UI work and retains its submission prefix through physical
completion; whole-Host queue-empty drain is not a per-stream close requirement. Bound pending and
in-flight work per stream and cumulatively per Host drive. Preserve supervised ordinary project
Play; local GPU sharing does not authorize arbitrary project Rust inside the generic Editor.

Keep the physical native event loop distinct from each logical run. The complete source/processed
capture consumer must retain and transfer the same thread-bound pump only after original Runtime,
presentation, window and GPU retirement. It must not rebuild Winit's process-level loop, substitute
synthetic captures or bypass receiving Schema/content residency to arrange the interleaving. The
owning render unit also verifies unused-pump recovery after startup admission failure.

The [native-composition checkpoint](../knowledge/engineering/verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md)
at `2d9f251` qualifies that handoff, actual Source/Processed retained-capture replay, independent
two-window/image streams and clipped numeric composition within its recorded Windows/DX12 scope.
It replaces stack-heavy inline policy tables with bounded heap storage and keeps complete native
retirement separate from each logical Runtime. The subsequent
[public stock-composition unit](../architecture/render-program-production-design.md#next-production-unit-public-stock-composition)
exposes shared world/Canvas/UI lowering and original capture selection to an ordinary external
Scene-effect package. Its [qualification](../knowledge/engineering/verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md)
covers Runtime, actual native Scene View and offscreen consumers. Full R8/R9, material declarations,
broader View/time and the editable Inspector remain open.

Acceptance: Runtime/Scene View/offscreen, intermediate textures, independent renderer policy,
two producer streams/two windows/image target, one-stream close, old packets and device loss.
Reject cycles/uninitialized input and unauthorized acquire/submit/present. Generated/history
resources reset, external ones reacquire; multi-window present is not globally atomic.
Recovery promises retain exact CPU projections or same-generation immutable processed decode
leases independently of GPU completion. Release that input explicitly or reconstruct from it;
never look up current assets/source to recreate an older capture. CPU eviction/streaming can
follow this contract without requiring every resource to retain a full CPU copy permanently.
Add a clipped Nara-owned drawing tracer with a live Scene image and overlapping control/popup.
It validates image import, clip order, resize/DPI and exact composite/input joins through this Host,
without requiring the Editor to inherit Runtime UiFrame primitives or finishing the R11 toolkit.

Reuse the blur, accumulation and drawing tracers for three added minimal counterexamples:

- Two Views each install two instances of one accumulation Feature. Reorder Views/instances and
  remove/recreate one instance: live history remains associated with its instance, replacement
  rejects late old-incarnation work, and foreign-candidate handles/duplicate local producers reject.
  Cross-candidate reuse uses an exact-version import.
- Wire pre-UI Scene color explicitly while a same-descriptor final composite is available. Neither
  registration order nor descriptor equality may select the wrong output. A multisampled attachment
  needs an explicit resolved version for a single-sample consumer, or admission rejects the
  unsupported path. This negative case does not claim MSAA support.
- Hold the Scene effect's second pass pending or reject its preparation while an Editor popup uses
  an exact older Scene output and another producer advances. Reject the affected complete output
  group without partial output/history publication; any fallback is separately validated and
  independent candidates retain progress and truthful presentation/input outcomes.

The [rendering source/design review](../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
records the comparison, pinned upstream evidence, failure cases and staged deletion. It is research
evidence; ADR 0121 stays Proposed and no new rendering qualification follows from this amendment.

No mandatory render thread, RHI descriptor mirror, aliasing compiler or resurrection of all 0077.
Delete central package-specific payload/match growth and tests prohibiting any public topology.

## R9: Material Artifacts, Views, Color And Domain Render Locality

**Design:** D15, D16. **State:** View, retained-output and composition work shares the active R8
unit and its scoped evidence; complete R9a, durable materials and measured geometry locality
remain open. **Owners:** shader/material/view owners and domain projection.

Turn the palette consumer into durable typed parameters/references plus tracked WGSL/WESL shader
artifacts. Derive one inspectable binding model rather than maintaining Rust and dynamic metadata
separately. Introduce shared View envelope with typed projection, explicit transfer/alpha and
captured time; investigate distinct linear/sRGB types as an unreleased breaking change.
Keep persistent material meaning distinct from its generated binding ABI and shader interface.
Uniform values and ordinary texture-content changes do not create pipeline variants. Initially
qualify two admitted layouts and unchanged-layout shader editing. Before interface freeze, design
shader-authored layout admission into the same material/binding model, including authored stable
field identity; do not make Rust type identity the persistent layout model. Both authoring routes
publish the same versioned declaration, with separate semantic fields, binding ABI and shader
interface identities admitted as one compatible generation. Captures pin it; incompatible changes
explicitly migrate or refuse. Shared GPU bindings join graph resource versions and declared accesses
instead of hiding cross-Feature dependencies inside opaque bind groups. Broader prebuilt-Editor
layout creation still needs its own consumer qualification. Use actual shader-layout and
compiler dependency results rather than repository parsers. WGSL may qualify first; WESL is a
frontend choice, not the resource identity or a full-CAS prerequisite.

R9a View identity/target/transfer/alpha/captured-time facts are designed with R8's external pass,
not deferred until the complete asset/cache work. R9b is durable shader/material preparation and
dependency invalidation. R9c is optional measured geometry retention, not shared protocol admission.
The initial reusable transparent-output tracer uses linear premultiplied accumulated color with
explicit source/shader alpha facts and numeric readback controls. Authored color and image formats
remain separate; do not premultiply gamma-encoded bytes or infer transfer from texture format alone.
Captured shader clocks distinguish Editor real time, paused Play virtual time and exact Step.
Replaying one capture at a later Host wall time must not advance it. Coordinate-changing effects
must provide an admitted display/input mapping or explicitly disable precise picking. A bounded
depth/unit tracer follows the 2D seam; general 3D support does not gate the first effect.

Add one joint R4c/R9a/R10 interleaving acceptance using the motion consumer and existing
temporal-policy and stock-composition fixtures. Bind captured shader time to its original spatial
sample, clock domain and interpolation state. Select effect-specific history compatibility for
teleport/correction, camera cuts and material/shader edits independently of View lifetime and
binding ABI compatibility. Exercise capture A admitted before a discontinuity but first prepared
afterward, an already-prepared stale candidate, and a post-cut capture B. Specify whether A is
refused or rendered without advancing current history. Preserve retained output reads, the other
View's history and original submission custody. Cover zero/multiple fixed ticks, Pause/exact Step
and delayed offscreen execution while retaining existing pipeline-variant rules. This extends
acceptance within the current units without a new execution or identity kernel.

Acceptance: second material layout, shared shader-dependency invalidation, compile failure keeping
same-epoch last-good, pipeline variant accounting, drawing/picking inverse/DPI/mirror tests,
transparent offscreen composition readback and bounded 3D depth/unit sample. Do not claim 3D support.

Additional R9b acceptance exercises the declaration-generation contract above under interleaving.
Within one valid Host/device epoch, retain capture A with its exact material declaration, shader
artifact, binding ABI and graph resource versions. Publish a compatible edited generation B, reject
an incompatible interface/layout candidate C, then complete a delayed realization for A. C cannot
destroy compatible B or reinterpret A. Late A work may finish for retained A but cannot replace B
as current; current publication must match its expected declaration generation. A uses only its own
artifacts, bindings and declared resource accesses. Use value-only and ordinary texture-content-only
edits as controls: their bindings/resources update without new pipeline variants, and unrelated
materials remain usable.
The rejected edit can target an admitted declaration; arbitrary shader-authored layout creation
still needs its separate consumer qualification. This adds acceptance criteria, not executed evidence.

Compare bounds-only and domain-local immutable geometry blocks on representative map/view
workloads. Dirty hints alone are not authority; old packet captures remain immutable. Measure
actual lowering/queue/upload cost before selecting retention. Keep 0053/0077 historical rather
than requiring their public topology.

## R10: Motion, Time, Repeatability And Concrete Domain Seams

**Design:** D09. **State:** the [joint motion/execution unit](2026-11-03-joint-motion-execution-plan.md)
has bounded independent, captured-discontinuity Windows/DX12 and same-World prediction/offline-PCM
qualification. Broader repeatability/effects and generated collision authoring remain open. The
bounded keyboard/mouse Input endpoint successor is qualified.
**Owners:** Input and concrete Physics/Animation/Audio/Network integrations.

Implement a bounded motion consumer with root-motion intent, kinematic target, dynamic solver
pose, parent edits and interpolation. Define query/callback freshness and temporal authority.
Add a minimal audio semantic-effect consumer with sample-time and shutdown rules. Do not invent
generic physics/audio providers or enforce a World-wide writer ACL.

Use this minimum motion consumer during R4c selection, not after its interface freezes. Define
root motion as intent, dynamic pose as solver output and the owner of corrections/writeback;
Transform continues to own spatial projection. R4c's freeze gate is the joint motion ordering,
actual borrowed-job join and each admitted executor profile's deferred/fault/NonSend behavior.
Reuse R9's joint discontinuity/captured-history control rather than inventing another temporal owner.
The following generated-content and authoring workflow is the separate R10 domain qualification,
not an additional prerequisite for every R4c comparison. Exercise one authored collision definition that
generates solver body/collider/debug state. Attribute generated roles to source, occurrence,
component/property and input revision; edit/remove/duplicate it and prove current membership,
diagnostics and old-output retirement without persisting runtime handles. Compare private
post-spawn derivation with a domain cook only if startup/delivery pressure requires it. Preserve
exact native persistent bindings; no generic baker or World-mutating codec is implied.

Before freezing non-TRS spatial import, KeepWorld tooling or affected persistent animation
targets, reuse the [affine representation comparison](../architecture/spatial-affine-representation-design.md)
with a complete 2D import/edit/animate/Undo/Redo/Save/Reopen consumer. Settle authored-base
preservation/canonicalization, interpolation targets, inverse refusal and numerical error together.
Select and amend 0097/0100 and relevant codecs from that evidence. Ordinary TRS/KeepLocal motion
can proceed while this conditional gate remains open.

Acceptance: pause/exact-step/teleport/reparent, fixed versus presented snapshots and correction
cut. Define a closed-input same-build repeatability profile with explicit RNG/outcome policy;
different frame grouping and worker completion orders produce the selected authoritative checksum
or a truthful unclassified-input refusal. A bounded prediction tracer establishes resimulation
and effect-suppression seams separately from debugger fresh restore. No cross-platform bit-exact,
complete network stack or arbitrary VM snapshot claim.

For that tracer, explicitly select recoverable components/resources, entity lifecycle, RNG,
used system-local state, pending commands, authoritative asynchronous outcomes and participating
solver state. Restore or reconstruct the selected state through the concrete owner and refuse
uncovered state/effects. Do not infer qualification from Schema, Rust type registration,
serial execution or `FixedUpdate` membership; do not build a static purity analyzer.

Use the already planned motion/audio consumer to rerun a corrected tick. Compare complete authoritative
state checksums; hold an old-attempt result until the replacement attempt exists and prove it is
rejected. Distinguish attempt identity from the semantic effect identity used across resimulation.
Discarded attempts must not play audio or perform unmodeled irreversible I/O; confirmed effects
follow the selected owner's duplicate-suppression policy. Include an unmodeled native operation
that refuses this prediction scope, and retain the ordinary partial-effect/sticky-fault regression.
Prove the serial baseline before each actually supported parallel profile and real solver join.
These controls qualify only this finite state/effect model, not arbitrary Rust/native rollback,
distributed STM or exactly-once network/file delivery.

The implemented closed tracer uses at most 32 logical ticks in managed custom schedules; it does
not rewind ordinary App FixedTime. Complete persistent Rapier bytes, logical entities, explicit
RNG, commands and classified outcomes match a clean corrected trajectory and continued simulation.
Attempt tokens include Runtime generation, and restore validates the confirmed ancestor digest
before changing World state. Offline PCM publication has separate semantic identity, sample-time
refusal and managed close. Local/native-I/O and opaque worker-gate exclusions are explicit authored
admission, not static purity analysis. This result does not qualify the separate collision,
affine-authoring, physical-device or cross-platform requirements.

The input sub-unit compares global maps, seat-scoped maps and per-device callbacks. Prefer
seat/action/context aggregation with explicit map revision and device-connection generation.
Qualify a keyboard seat and gamepad/axis seat using same-name actions, focus/capture transfer,
disconnect/reconnect and rebinding while held. Raw pipeline replay pins normalization/bindings;
semantic replay records stable admitted actions at the application tick and must not change
meaning after a physical rebind. Reject incompatible vocabulary, never persist OS handles, and
do not conflate durable game player identity with a local input seat. Amend 0041 for selected rules.

The current [definition/source unit](2026-10-30-input-definition-and-source-admission-plan.md)
establishes global immutable ownership and independent World state first. Its finite keyboard/mouse
workflow does not qualify the seat/gamepad/replay requirements above. The endpoint successor
separately qualifies finite ingress/projection/contribution ownership; context/revision costs alone
remain distinct from the entire physical pipeline.

The [endpoint admission review](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md)
traces historical empty rows, live cross-frame memberships and platform-observed cancellation
that can clear ingress state without admitting terminal events. Its
[successor plan](2026-10-30-input-endpoint-admission-and-retirement-plan.md) now selects finite
raw-ID admission, private lifecycle/member identity and shared ordered terminal credits after actual
red regressions, adapter review and private shared/dedicated controls. Amended ADRs 0041/0102/0108
also require total prepared-route/UI invalidation: a late backend acceptance cannot restore a
cancelled interaction session. The
[production qualification](../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md)
records the actual owner/projection replacement, independent gameplay, terminal-tail/claim/basis
repairs, explicit candidate API, real Reference observation and selected Windows/DX12 evidence.
Its bounded keyboard/mouse scope does not qualify the broader Input/motion work above. Neither
preceding definition qualification nor the private count model substitutes for that evidence.

## R11: UI Content Association, Text Admission And Platform Qualification

**Design:** D13, D17. **State:** not started. **Owners:** UI compile/presented sessions, text and frontend.

The user clarified the destination: **Nara's own custom-drawn Editor UI**, not embedding a
reference toolkit. The [source study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md)
informs the [owned runtime direction](../architecture/foundation-architecture-reassessment.md#updated-custom-ui-and-language-research).
Preserve current semantic owners and use Dear's existing workflow behavior as the replacement baseline. Design
Nara-owned controls, identity, interaction, layout and drawing. Mature text/layout primitives
may be reused without importing a complete foreign UI runtime. No third-party embedding
experiment or demonstration that adoption fails is required first.

The first complete visual slice is an editable multilingual/numeric Inspector plus a clipped live
Scene image and the ordinary external semantic command. Preserve exact operation results, draft
conflicts, Undo and Save/Reopen from the working predecessor. Select editable cluster/caret,
selection/hit testing, font fallback and Editor font policy through this slice; display glyph
projection alone is insufficient. Keep model/property identity, live widget incarnation and saved
panel identity distinct. Extract toolkit-free draft logic only when actual frontend consumers
share it, and remove the corresponding Dear path after parity rather than waiting for every panel.

Before expanding the editable-control API, settle active-buffer Undo/Redo versus explicit
document-history commands, including Escape and focus-loss transitions. Preserve the predecessor's
explicit Apply and conflict-recovery semantics; typing and composition do not independently
publish document edits.

Select composition custody and cancellation/retirement admission with the input boundary. A late
preedit or commit from a retired field or endpoint must not target a same-label replacement.
The first editable slice must distinguish restoration of the original focus lifetime from a new
widget/endpoint lifetime; the bounded button/mouse Input qualification does not prove that policy.

Consume Scene render results through R8's exact Host output/composite boundary. Keep physical
leases in the UI adapter's owned render packet and display/input facts in its composite receipt;
do not put GPU custody in `UiImageId` or merge a panel's clipping with producer View identity.
Qualify the selected copy/direct-sampling path and its retirement cost before replacing the
working viewport integration. This preserves the current typed UI Image contract.

Use the same useful document tool and an actual Nara-owned window/render-view handoff. Establish
these observations before replacing the selected frontend:

- Reorder/filter/recycle a large list while preserving logical selection and compatible field
  drafts. Count live widgets and layout work against visible rows; mere clipping is not evidence
  of virtualization or bounded allocation.
- Keep Chinese preedit out of source/history; one accepted field commit makes one edit. Exercise
  actual OS candidate placement separately from backend event injection. Text Delete and viewport
  Delete must follow focus/capture policy, including focus loss and panel retirement.
- Retain Viewport receipt, texture-generation, color/alpha, clipping and input-mapping authority.
  Report device/version/resource sharing or copy costs, not just that both stacks use wgpu.
- Separate structural/layout/paint/semantic invalidation and measure idle work with a live
  Viewport. Qualify DPI/detached-window transfer and actual accessibility for any support claim.
- Disable/replace the selected tool with queued events and retained callbacks/resources. Restore
  only explicitly compatible state; observe physical retirement separately from route removal.

Keep this work independent of Scene data-family delivery and pure semantic tools. Qualify the
owned runtime incrementally through these workflows, then remove superseded frontend code
when matching complete workflows pass. The current Dear adapter is a working predecessor,
not a competing intended final toolkit or a requirement for permanent coexistence.

Repair exact theme revision/content association and Canvas/Runtime conflict admission (earlier
F7/F8). Move glyph preparation reservations ahead of raster allocation where the model can prove
them (F9), while distinguishing modeled bound from arbitrary allocator isolation. Keep terminal
and last-presented behavior explicit on candidate rejection.

Sharpen the existing color/mapping oracle: display the same translucent sprite, glyph edge and UI
rectangle directly and through an offscreen image. Before presentation encoding, half-opacity red
over transparent yields linear premultiplied `(0.5, 0, 0, 0.5)` and then over opaque blue yields
`(0.5, 0, 0.5, 1)`. Set format-dependent tolerances before GPU execution; include alpha-zero colored
texels and sRGB texture/tint controls. At nonzero letterbox origin, fractional DPI, clip and mirrored
placement, paint, picking and accessibility bounds use the same admitted mapping. Outside-content
input cannot clamp to a control; resize revokes the old mapping. Screenshots alone do not qualify it.

Qualify semantic draft/basis/result reuse and separate toolkit edit buffers. Test Unicode/IME
composition, focus, keyboard routing, DPI and actual affected native detach/accessibility seams.
Pure semantic tool delivery is not blocked on unrelated whole-toolkit parity, and Runtime
accessibility is not evidence of Editor assistive-technology support.

Delete duplicate state machines and ambiguous theme identity. A source study alone does not
justify deleting the working frontend or claim its native capability has transferred.

## R12: Consumer Integration And Canonical Contract Consolidation

**Design:** D01, D18. **State:** consumer integration accompanies the qualified owner units;
the registry ownership sub-unit is qualified; the complete production/consolidation outcome remains open.
**Owners:** affected modules and integration owner.

For the selected completed units, run independent direct/renamed/public consumers and relevant
root/Editor/native paths serially. Verify unknown data, exact operation results, content/identity
retention, close and old-generation rejection together. Review adapters, not only owner unit tests.

Name the Rust capture/rebuild/restore successor explicitly. The game owns checkpoint data and
compatibility; supervised Play owns build, immutable executable generation and process lifecycle.
Join body and structural edits to restoration of the same meaningful playable situation, including
capture/build/restore failures. Reconcile ADR 0116's current one-READY launch protocol before
adding post-launch capture exchange; ADR 0093 already admits reliable rebuild/restore. Preserve
saved-source receipts and exact last-good scope. Unchanged Restart is not recompilation, and
successful launch is not the correct playable-result endpoint.

Name the game-owned checkpoint DTO and exact restored-state oracle. The current Two Level
checkpoint reconstructs live position, velocity, input and simulation tick; restoring an authored
anchor does not qualify restoration of those values. Select the capture-consistency boundary and
compatibility rule for the new build, Schema and content. Reconcile ADR 0076 where applicable and
ADR 0116's one-shot READY contract before introducing a finite opt-in capture/restore exchange.
Exercise stale replies from a replaced generation and restore failure without promoting an
unqualified candidate. Build success or startup READY alone cannot satisfy that state oracle.

Add one Host-composed diagnostic capture that correlates existing owner work, waits and outcomes
in a standard trace viewer. Preserve operation lifetimes and the distinct App-frame,
render-transaction-frame and device-epoch identities. Explain a slow import, rejected edit and
delayed GPU completion; measure enabled/disabled overhead, retained bytes and dropped observations.
Collector failure or disabled tracing cannot change operation outcomes or cleanup. Keep
correlation observational under ADRs 0048/0068; reconcile any extension to the existing tracing-field
policy. This adds no global subscriber, budget manager or profiler UI and does not gate the current
compiled unit. App-frame-to-completion observations are not per-pass GPU execution timings.

Add one non-implementing author's bounded external-project task with current public guides:
persistent property plus semantic tool, edit/Play/Undo/Save/Reopen and the supported Export path.
Record assistance, undocumented steps, engine patches and the reproducible project artifact.
Completion with zero private engine changes is author-success evidence; automated independent
compilation remains a separate regression proof. If an external author is unavailable, keep
that cell explicitly unqualified rather than substituting maintainer execution.

Attach measurement and platform evidence to the changed workflow. Name hardware, source/toolchain,
cold/warm state, actual correct-result endpoint and numeric/sample budgets before comparison;
retain distributions and run-to-run variation. Short historical edit cohorts are not current P95
claims. For each supported OS, exercise actual filesystem guarantee tier, Open/new-Save/rename,
Export/Play/close and failure cleanup. Custom UI adds actual IME candidate placement and assistive
technology. Missing hardware leaves native cells unqualified. Reuse owner tests, existing harnesses
and native procedures; do not build a compiler-like validation script or rerun unrelated broad
qualification solely to close documentation.

The [Scene/runtime extension review](../audits/2026-10-27-scene-runtime-and-extension-plan-review.md)
adds bounded consumer-driven cleanup: verify whether the Schema-first registry still needs its
embedded Bevy TypeRegistry; exercise observer/Event authoring through renamed dependencies before
expanding facade-aware derives; and make unbound preparation, staged replacement custody, asset
reservations and current-live export naming match their actual workflows. Remove unused storage
or entry points only after tracing real consumers. These follow-ups do not block the current
closed Scene data-family implementation or require a generic extension framework.

The [registry ownership sub-unit](2026-11-01-component-registry-ownership-plan.md) addresses a
concrete repeated consumer failure: successful registration followed by `schema` on a Building
`ComponentRegistry` silently reports absence. The selected replacement separates mutable
`ComponentRegistryBuilder` admission from the existing immutable `ComponentRegistrySnapshot`,
removes Frozen wrappers and duplicate reader views, and keeps App build/preloaded dispatch
private. Failed admission returns the original candidate and error for repair/retry. Provider
atomicity, exact definition identity, preloaded receipt checks and installation mutation detection
remain mandatory. ADR0081 selects that ownership split. Implementation `7f69d53` and its
[qualification](../knowledge/engineering/verification/2026-11/2026-11-01T033745Z-consuming-registry-admission-and-exact-app-scene-ownership-qualification-c1678b22b0424f55b3dd73f149d8e7e2.md)
cover owning domains, migrated public/independent consumers and the selected Windows/DX12
trajectories. The registry sub-unit closes without closing complete R12 or the foundation Goal.

Consolidate affected 0117 amendments into owning ADRs; narrow or replace obsolete proposals and
remove duplicate current rules. Update guides, catalogue, ledger and immutable evidence. Keep
historical research/probes clearly historical. Do not add a migration guide or remove unrelated
root Markdown, vendor files or user changes. Final claims enumerate implemented units and
remaining Proposed designs instead of declaring the whole engine fearlessly refactored.

## Deferred Without Being Forgotten

Remote collaboration, arbitrary cross-document or external-effect rollback, a complete graph
Editor, marketplace/signing, all-native ABI compatibility, full native sandboxing, high-end 3D,
all platforms and complete network rollback remain unqualified. Their boundaries were considered;
they are not silently part of the first slice. A named new workflow can challenge these scopes.
