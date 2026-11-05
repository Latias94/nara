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

The November 4 UI clarification deepens R11/R12 in place. The default Editor remains Nara-drawn;
the engine supplies public attachment interfaces so users can integrate another UI implementation.
An official ImGui extension is not a deliverable. Shader Graph-like documents, automation and
multiple logical Views constrain ownership and dependencies now; complete graph products,
agent transports, component catalogues and native multi-window support remain staged work.
The revised [UI design](../architecture/ui-foundation-design.md), ADRs0093/0107/0109/0118 and the R11a-f
units below separate those decisions from their still-unimplemented contracts and cost evidence.
The subsequent development-route clarification keeps the complete ImGui Editor as a reference
and develops the self-drawn Editor as an independently runnable composition over the same
semantic owners. Add workflows inside the successor's own shell, then switch the default and
retire the reference after complete acceptance. Mixed first-party panels and reciprocal toolkit
embedding are not required; optional product embedding remains allowed.
The component-interface follow-up adds scoped identity/effects, repeatable preparation and an
explicit custom layout/draw contract. R11f names Editor extension edit/build/reconstruct separately
from R12's game/Play restoration; declarative syntax alone qualifies no native reload.

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
R11a work/cost baseline -> R11b public panel hosting -> R11c complex-editor/custom-draw pressure
R11b + existing exact actions -> R11d public UI observation/operation -> R11e measured consolidation
R11c + R11d -> R11e; logical Views reuse R8/R9, native detach keeps separate platform qualification
R11b + R11d + existing build/close/persistence owners -> R11f Editor edit/build/reconstruct
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
qualification. The [authored collision unit](2026-11-04-authored-collision-derivation-plan.md)
adds a concrete public Schema/Scene-to-Rapier workflow; its selected scope and qualification stay
separate from broader repeatability/effects and affine authoring. The bounded keyboard/mouse
Input endpoint successor is qualified.
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

**Design:** D13, D17. **State:** in progress; the [owned Inspector unit](2026-11-04-owned-inspector-and-editable-text-plan.md)
has qualified shared text/field prerequisites, native scalar/boolean/reference editing, scrolling
and supported structured drafts. Its shared picker covers nested reference leaves; retained-input
Copy and exact capacity recovery extend field recovery. The finite source-tool property shares
the original Session candidate, review and history with the ordinary tool panel. The owned Scene
directly samples the retained GPU output with Nara controls, exact clip/placement authority and
native edit/Undo/retirement evidence. Component creation/removal and fold/recovery use the original
Scene/Prefab actions with exact displayed basis and native history/persistence evidence. The
ordinary Scene/Prefab Document Inspector now uses Nara controls by default, including authored
placement and revision-bound Prefab-source navigation; the competing optional tab and Dear
whole-panel painters are removed. Complete embedding uses the original native owner through
`EditorNativeHost`; `show_contributions` explicitly serves unattached source-built panels.
The ordinary source-declaration Hierarchy successor adds Nara-owned creation, selection, authored
Parent/Order and confirmed removal, local committed-text filtering and visible-row construction
for source and placement-choice lists. Its original Session/history/persistence and hosted
submission owners remain authoritative; source declarations are distinct from an effective tree.
The specialized UI Prefab Hierarchy now uses Nara tree/choice controls and complete
Container/Text/Image/Button creation, move and subtree-delete forms through the original UI
author, Session and result ledger. Shared immutable text paint windows retain the creation
owner's 4,096-byte input capacity under unchanged source/output limits. Same-document Source/UI
replacement retires pending predecessor frames and keyboard/text authority; its owning unit
records the matched CPU/native evidence and review corrections.
The independent self-drawn Editor composition, workspace and specialized workflows, offscreen keyboard traversal,
full matching/idle/allocation/GPU cost measurements and broader OS/platform qualification remain
open. **Owners:** UI compile/presented sessions, text and frontend.
The selected-panel evidence above predates the route clarification below. It is not a requirement
to continue mixed-toolkit development or proof of a complete pure-ImGui reference.

The default destination is **Nara's own custom-drawn Editor UI**. Users may integrate another UI
implementation through public panel/Host ports; Nara need not supply an official ImGui extension.
That optional attachment is distinct from embedding a reference toolkit as the default. The
[source study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md)
informs the [owned runtime direction](../architecture/foundation-architecture-reassessment.md#updated-custom-ui-and-language-research).
Preserve current semantic owners and use a named complete ImGui reference's workflow behavior
as the comparison baseline. Record its exact revision and missing coverage; some original Dear
painters are already removed in the current checkout. Develop an independently runnable
self-drawn Editor with its own shell, controls, identity, interaction, layout and presentation.
Mature text/layout primitives may be reused without importing a complete foreign UI runtime.
Development requires no reciprocal embedding or mixed first-party composition. User-authored
product embedding and Host conformance retain their separate scope.

The qualified historical visual slice was an editable multilingual/numeric Inspector plus a clipped live
Scene image and the ordinary external semantic command. Preserve exact operation results, draft
conflicts, Undo and Save/Reopen from the working predecessor. Select editable cluster/caret,
selection/hit testing, font fallback and Editor font policy through this slice; display glyph
projection alone is insufficient. Keep model/property identity, live widget incarnation and saved
panel identity distinct. Reuse toolkit-free draft logic where actual frontend consumers share
it. Add future complete workflows to the independent self-drawn composition. Retain the reference
for whole-Editor comparison until complete acceptance rather than deleting it panel by panel.

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

Keep this work independent of Scene data-family delivery and pure semantic tools. Qualify
workflows incrementally within the independently runnable successor. Default selection and
reference removal follow complete incumbent workflow/native/performance acceptance in R11e.
Optional user-authored frontends use the public Host attachment; they do not preserve the
reference or require a permanent ImGui-shaped core interface.

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

### R11a: Execution model, dependency closure and work baseline

**Goal:** establish the state/invalidation and cost contract before further catalogue or workspace
expansion. ADR0107 selects persistent interaction/artifacts with declarative composition and local
invalidation for Nara-owned Editor UI. Runtime game UI, Editor components and document-specific
tools keep independent product ownership. Optional frontends choose their internal mode.

**Files:** `crates/nara_ui_kernel/src/compiler/`, `crates/nara_text/src/paint.rs`,
`crates/nara_ui_render/src/planner/`, current `editor/src/frontend/imgui/owned_*` controllers and
their manifests. Extend `tests/fixtures/ui-module-consumer/` for dependency closure; add planned
`editor/tests/ui_work_costs.rs` for matched frontend work attribution. Reuse current native
qualification and R12 observation facilities rather than inventing a benchmark or tracing kernel.

**Approach and verification:**

1. Record current description construction/comparison, layout, shaping, raster, allocation,
   upload and composition costs, including work before an unchanged-frame fast path. Attribute
   the existing last-good cache separately from active incremental reuse. Separate model/query
   refresh, filtering/key-order/index work and editing from bounded visible-row construction;
   unchanged frame or geometry-cache hits must not hide those costs.
2. Fix reference hardware, source/toolchain, build mode, font/DPI, cold/warm workloads, sample
   counts and numeric CPU/GPU/input-latency/retained-byte budgets before optimization. Commit
   that table with the measured baseline; do not infer budgets from this unmeasured design update.
3. Compare a static Inspector beside a live View, a local field/color change, and 10,000/100,000
   logical rows at the same viewport. Count live controls and layout separately from filtering.
4. Verify that text/kernel/widgets need no Editor/project/ECS facts, the game's `nara_ui` adapter
   needs no Editor, and Editor components do not import that adapter merely for controls. Record
   `nara_ui_render`'s current combined neutral planner and App/ECS/window integration for R11b.
   Attribute the current mixed composition, named ImGui reference and future independent
   self-drawn composition separately; missing consumers remain unmeasured.
5. Attribute existing repeated preparation, local work and theme/typography changes. Extend the
   same baseline with scoped component/custom-draw cases when R11b/c supplies them; their
   unimplemented cells do not block R11a. Idle refresh demand and queue/retention limits remain
   explicit; native extension execution is cooperative, not preemptively time-bounded.

Success is a reproducible baseline and executable work-count assertions, not a performance win.
No particular persistent layout arena, reactive language, public cache key or new crate is frozen.
A bounded owner-local reuse change may use a matched current-source baseline and predeclared
work, allocation and latency budgets for its complete selected consumer while the broader native
matrix remains open. It must include changed-input/refusal controls and retain the unmeasured
cells explicitly; this does not satisfy R11a's complete budget table or R11e's whole-Editor exit.
The [pinned Zed/Open-GPUI follow-up](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-4-state-reuse-and-component-mechanisms)
supplies notification, cached-view replay and fixed/measured list comparisons. Zed's inspected
layout tree is cleared between draws; its retained state and cached views do not require Nara to
retain a whole Taffy arena. Open-GPUI's `virtualizer` is a workspace core module, not a selected
third-party dependency. Borrow the mechanisms under existing owners and measured pressure.

The first [work-observation prerequisite](../benchmarks/2026-11-04-ui-work-baseline.md) adds
owner-local text/compiler/planner counters, description-before-reuse assertions and dependency
closure checks. Its debug/release Inspector and explicitly read-only 10,000/100,000-row projection probes
show bounded live layouts but repeated description/scanning work. An actual 10,000-entity project
Open was refused by startup source preparation before a Session existed; this is recorded
separately rather than hidden by widening admission. Three release native repetitions now retain
the exact Inspector beside an advancing Game for 128 matching current primary composites, with
observed geometry, original-device health and actual terminal-loss refusal. This remains the
current Dear-hosted composition, with repeated description work. Separate opt-in allocation
processes reuse the CPU workloads and count current-thread Rust requests; unchanged frame reuse
still allocates/frees temporary descriptions. Full process/native allocation/retention, upload/GPU
timestamps, OS input/display latency, remaining matched workloads and fixed numeric budgets keep
R11a open; neither an independent frontend nor a cache improvement is claimed.

The [native capability/timestamp prerequisite](../benchmarks/2026-11-04-ui-work-baseline.md#primary-window-gpu-markers)
now contributes frontend device requirements through the existing `WgpuRenderHost` before its
first acquisition, sharing the closed Feature request and late-refusal/recovery rules. Its
independent native adapter proves actual query readback, unchanged pixels and physical retirement.
Three release repetitions preserve both complete 128-frame lanes and original receipts, marking
only primary window drawing in its original encoder. Timestamp reordering/observer limitations,
unmeasured image-producer GPU/upload work and absent Dear write counters remain explicit. This
concrete device seam is not R11b's component/panel-hosting interface; primary timestamps alone do
not complete R11a.

The next [native-work and change-pressure observations](../benchmarks/2026-11-04-ui-work-baseline.md#upload-and-copy-observation-boundaries)
record returned preparation Queue requests, successful initial mapped copies and encoded
whole-resource Preserve copies in the existing Host transaction, retaining reached work after
later failure. An independent graph/Feature consumer checks exact counts, retries, aggregation,
reset and unchanged pixel/buffer/retirement outcomes. CPU Inspector pressure covers hover color,
palette, font environment revision, DPI and width while preserving source/history/draft identity.
The font case does not substitute actual fonts/fallback; native input/display, full process
retention and final numeric budgets remain open. These observations add no reuse optimization or
new telemetry/budget owner.

The [repeated native lifetime probe](../benchmarks/2026-11-04-ui-work-baseline.md#repeated-native-lifetime-process-memory)
extends this same complete Inspector/Game workflow across healthy and actual device-loss
retirement. It observes whole-process private commit and Working Set before construction, live
and after teardown, beside the existing logical Host payloads. Original cleanup, fresh Host
identity, weak Rust-window release, exact platform-window destruction and source bytes remain
required in every cycle. Three ordinary release processes pass all 48 complete lifetimes; their
36 measured lifetimes keep process/path distributions and observed residual variation explicit.
Keep these short observations separate from UI-exclusive retention, full GPU memory,
long-running leak/plateau evidence and numeric memory budgets. No allocator trim or additional
post-cleanup delay may manufacture a lower residual observation.

### R11b: Public panel hosting and component composition

**Dependencies:** R11a and the current qualified Application/Session query/result contracts.
R1 continues to own closed-panel semantic command/menu/shortcut registration; UI hosting consumes
that contribution rather than creating another command registry.

Actual typography changes require preserving the text owner before whole-Host replacement.
The bounded text/field prerequisite explicitly reconfigures ordered fonts under the original
limits and logical size, atomically admitting current/preedit/history states without replacing
field identity or adding an edit. Independent public consumption must prove real glyph/fallback
changes, immutable prior paint provenance, refusal/retry and input lifetime continuity. This does
not close the native all-panel font transaction: the Host must still coordinate compiler/planner,
old/new presented geometry and routed input without silently dropping drafts or pending results.

**Files:** `editor/src/platform/composition.rs`, `editor/src/frontend/imgui/workspace.rs`,
`editor/src/panels/` and `editor/src/native/`,
`editor/src/platform/imgui/owned.rs` and keyboard/IME adapters, their actual successor modules,
`crates/nara_ui_render/Cargo.toml` and `crates/nara_ui_render/src/integration.rs`,
`tests/fixtures/module-consumer/editor-panel-consumer/` and
`tests/fixtures/editor-ui-host-consumer/` with separately compiled public/native tests.

**Approach:** replace the ImGui-specific public composition shape with contribution/document
access plus per-instance frontend attachment and finite Host services. Ordinary component authors
compose reusable Nara controls without manually rebuilding journal, reducer and text-lifetime
plumbing. Low-level custom implementations retain access to the documented geometry, input,
text-service, overlay, refresh, output/resource and presentation-feedback contracts. Preserve the
existing domain and GPU owners; private field visibility changes alone do not satisfy the goal.
Separate the neutral packet/planner dependency closure from its current App/ECS/window plugin
integration, retaining that game/runtime adapter through an explicit installation path. Whether
this needs feature gating or an extracted crate follows the independent presentation consumer;
the dependency result, rather than a prescribed package count, is the acceptance contract.
Build the self-drawn frontend as a whole-Editor composition with its own workspace/shell
presentation and native input/presentation path over existing workspace/Host services. Its selected build/run closure must
need no Dear context, package, renderer or docking/input service. Keep the reference separately
selectable for workflow comparison; exact executable/feature/crate organization is not prescribed.

Qualify component composition separately from whole-frontend hosting. Scoped keyed state hides
ordinary identity/reducer/text lifetime work; explicit update/effects are separate from repeatable
compose/measure/paint. Custom layout can measure/place children without exposing the layout
arena. Third-party components consume scoped theme/text services, supported motion/keyboard
input, hit/semantic facts and admitted drawing/resources without modifying a central widget enum.
The current button event vocabulary and Rect/Text output do not satisfy that complete contract.
Keep exact trait shapes provisional until the independent composite/custom component uses them.

**Test scenarios:**

- Independently compile and launch Nara content through the self-drawn composition and the
  small non-Nara conformance implementation through a separately selected Host composition.
  Each draws, edits text, opens a popup and reports actions without private engine changes.
- When claiming optional mixed-content hosting, qualify it in a separate extension fixture:
  transfer keyboard/IME/capture, close during composition or queued output, and reject late input/results to a
  replacement. This tests the product extension contract, not a required mixed default or an
  ImGui embedding step in successor development.
- Relocate and reconstruct GPU state while preserving compatible panel state. Target change,
  layout reset and unsupported capabilities follow explicit retirement/refusal outcomes.
- Keep an independently registered semantic command usable with its panel closed, through R1's
  contribution and the original edit/history/result owner.
- Independently author a composite with two same-label keyed editors. Reorder, insert, filter
  and change one state type/document binding; compatible drafts remain correctly associated,
  collisions/incompatible reuse are explicit and retired tokens cannot act on replacements.
- Compose custom row content through a wrapper that owns placement, identity, focus and
  collection interaction. Exercise focused-row eviction, deletion and same-index replacement
  with drafts, caret/selection and preedit; pin/preserve/cancel is explicit. Document which local
  edit, listener and external model result invalidates each consuming artifact. Custom layout
  includes child intrinsic measurements and constraints without exporting Taffy storage.
- Repeat a sizing/composition pass without repeating a command, task or subscription. Close
  with a local async request pending; its original result retires without reaching a replacement.
- Override one library's scoped style and change typography/fallback. Preserve unrelated style
  and work; measure the required layout/text invalidation separately from color-only paint.

Before claiming compatibility with a named external toolkit, qualify its actual user-authored
adapter separately. Shipping an official ImGui/egui package or stable plugin ABI is not required.
Optional embedding/coexistence qualification does not gate the independent default's replacement
unless that capability is explicitly part of the selected support claim.

#### Neutral presentation consumption prerequisite

The concrete dependency investigation confirms that `surface.rs` shares outcome permits,
submission binding, abandoned-drop handling and completion custody with `integration.rs`.
Move that existing mechanism into shared neutral implementation before gating runtime integration;
do not duplicate the protocol. `RenderTarget`/`RenderResourceSnapshot`, typed Asset handles/slot
revisions and `PreparedImageSnapshot` also reach App/ECS/Window through their current owner
manifests. A UI-only feature switch cannot remove those transitive edges. Compare values versus
integration features inside the existing owners with a bounded extraction of Render/Window
values; select the simpler complete consumer closure rather than optimizing package count.

Use the existing independently locked UI consumer for public code-first Image store/capture,
`UiPreparedImages`, compile, presenter preparation, original backend completion, journal/input
and retirement. Check normal/build dependencies without workspace feature unification: text,
kernel, widgets and neutral presentation must not require App/ECS, game UI, project/Editor,
WGPU/Winit/Dear or filesystem/task installation. Value-only Window identity may remain in its
existing owner if its runtime integration is absent. Preserve typed resource identity, nominal
slot revisions, retained pixels/import leases, deferred retry and late-outcome disposal. Then
reuse the complete native `ui_surface` consumer with real captured Image content, and verify the
default game installation still selects its original plugins. Public panel attachment and the
independent Editor are later complete consumers, not consequences of a manifest change.

The selected implementation separates values and runtime installation inside the existing
Asset, Identity, Diagnostic, Image, Render, Window and UI Render owners. Default features stay
empty; explicit `runtime` enables App/World integration and conditional ECS derives. Diagnostic
CPU values remain neutral so Asset/Image codecs keep their existing typed diagnostics without
pulling in the diagnostic plugin. Image's `render-prepare` exposes capture/resource values;
its automatic prepare system additionally needs runtime. Render's original View identity/error
definitions stay shared while World extraction moves behind runtime. The UI presenter and
World adapter use one private backend-custody module. Runtime product/dependent manifests select
their actual integration requirements explicitly; no new package, raw-ID namespace or facade
compatibility layer is added. ADR0102 owns this decision. The qualification below distinguishes
neutral library consumption, installed Runtime behavior and actual native presentation; public
panel attachment and the complete independent Editor remain separate R11b consumers.

Pure/serialization qualification exposes two additional values incorrectly hidden by the
original reload module: `AssetSourceRoot` and the bounded `AssetSourceChanges` queue. They now
live in neutral modules under the same Asset owner and root public paths. Runtime reload uses
those exact definitions; its path/canonicalization, metadata coalescing, admission and overflow
rules are unchanged. All nine queue tests move with their owner and still run without runtime.
The first test build catches the moved test's missing `LoadState` import; restoring the explicit
import preserves every assertion. An independent source review confirms the extraction without
new P1/P2 findings. Earlier review also removes unnecessary runtime feature requests from
Material and the value-only Image edges in Sprite/Tilemap, and fixes the independent native
fixture's optional clip intersection before its actual build and run.

Executed qualification uses Rust 1.95.0 on Windows 11, serial Cargo and the shared root `target`:

| Scope | Executed result |
| --- | --- |
| Seven owning crates, no runtime features, independent serialization plus Image `render-prepare` | 317/317, no skips; run `33bb9d6b-23cf-484f-bb74-d6f2d98d009a` |
| Same owners plus game UI and WGPU, all features | 1,165/1,165, 57 skipped; run `ed927c16-a489-4f77-b1f5-d6ea75502195` |
| Default game UI library without serialization | 124/124, no skips; run `ef43608f-7979-45b5-9068-1b4b13f01542` |
| Independently locked UI component/presentation library | 30/30, no skips; run `36c0540c-5ac3-4815-99eb-a04fdb84736b` |
| Root product capabilities, independent normal/build graphs with and without serialization, public-prelude positive/negative consumers | 14/14, no skips; run `1f9f5abb-a5fc-4f4e-b7f4-0ad21c6b5385` |
| Actual independent Windows/DX12 surface trajectory and separate failure controls | 2/2, no skips; run `5e35a0ee-074a-4319-b10c-0b705f05e7b4` |
| Editor library plus public panel, authoring-loop and first-game integrations | 410/410, 61 skipped; run `cd7f8e0d-6c6a-4bf7-9975-18be9eae7f73` |
| Selected actual Windows/DX12 Editor authoring, Canvas, Scene/Game, panel, cadence, recovery and close trajectories | 33/33, 426 outside this filter; run `d00fb9a7-2105-4299-a436-67b3b5fea5ef` |
| Actual WGPU typed UI Feature metadata/budget, mixed mesh/quad pixels, sRGB, allocation refusal/retry and retirement | 4/4, 455 outside this filter; run `333a40af-76c4-4ce2-af46-91b60b2c07d7` |

The native consumer imports the same procedural Image library and preserves all 19 actual
presentations, 18 journal commits, keyed text/layout/curve/drag/wheel assertions and four
error/panic child modes. It checks the original typed resource, shared pixels, metadata and
retention in the submitted packet plus visible Image geometry and actual presentation. There
is no new Image readback oracle. Native pointer/IME/logical-key delivery is injected. The six
new independent CPU tests use an explicitly simulated trusted backend and cannot establish
native presentation by themselves. These cohorts overlap; their counts are not additive.

All 20 independent fixture locks resolve offline for the Windows host. Inspection finds no
existing package-version changes: the UI library adds 31 package records for neutral presentation,
the native lock records its four added library dependency edges, and the root lock removes three unused
Image dev-dependency edges. An initial unfiltered offline metadata request reaches an uncached
`js-sys` version in another fixture; target-filtered resolution completes without an upgrade.
The all-target normal/build dependency assertions above retain their separate scope.
The seven owning crates also pass an all-target check with no default or requested features;
that check includes their dev dependencies and is not the neutral production-closure assertion.
All-target, whole-workspace checks pass independently for module-consumer, behavior-languages,
motion-execution, render-policy-consumer, render-program-consumer and
render-composition-consumer under their own locked offline graphs. Native Render frame capture
and Window handle consumers explicitly request their existing `runtime` integration instead of
depending on a backend's incidental feature propagation. These checks establish compilation;
they do not rerun those fixtures' distinct ignored native or language-execution trajectories.
The native surface consumer also passes its locked offline all-target check after explicit
Render/Window feature selection. Correctness Clippy passes all targets/all features for the
seven owners, game UI, WGPU and Editor, plus the independent presentation library's all targets.
The actual default Editor executable builds successfully. Root and both UI/native fixture
format checks pass; changed-document local file targets resolve, with remote URLs and anchors
outside that file-existence check. These are not warning-free checks:
existing style/dead-code warnings and neutral-mode unused import-budget helpers remain. No
timing, memory improvement, general foreign-frontend hosting or complete R11b/foundation claim
follows from this dependency prerequisite.

#### Toolkit-independent panel custody prerequisite

Move the reference frontend's existing panel incarnation, candidate/submission identities,
original action-slot filtering and exact result resolution into one toolkit-independent
implementation. Reuse that implementation from the Dear reference and a public typed panel
Host; do not add another application dispatch path or document/history owner. Read/query/source
preparation remains the original Application/Session access. Compatible relocation and GPU
reconstruction preserve frontend state; hide/close, target/resolved-document change, layout reset
and project replacement retire the exact incarnation. Foreign/superseded candidates cannot
settle another Host. Retired actions retain holes in the original slot sequence.

Make the reference toolkit an explicit default-enabled `imgui-reference` feature until complete
successor acceptance. Independent Editor/application/panel consumers disable defaults and must
need no Dear package, context, renderer or docking/input service. This is source selection, not
an empty frontend or a stubbed Editor. Preserve default reference workflows and its compiled
Host contribution selection. The neutral panel Host owns semantic lifecycle/action custody;
native input, overlay, text/platform services and render execution keep their existing owners.

Use an independently locked consumer with reusable Nara controls and separately authored panel
state. Exercise bounded non-active queries, prepared source edits, exact callbacks, Undo/Redo,
Save/Reopen, relocation, hide/reopen, layout/project replacement, retained/foreign candidates and
closing filters through the public interfaces. Keep drawing/presentation observations separate
from semantic CPU probes. Qualify the resulting native attachment through the same original
WGPU Host; broader capability arbitration and the full independent Editor remain R11b work.

The selected implementation adds `nara_editor::panels::EditorPanelHost<P>`, scoped context and an
opaque owned submission. The reference and typed Host use one extracted private
`SubmissionOwner<R>` implementation for candidate identity, original-slot filtering and exact
result resolution. The public context forwards bounded reads/preparation to Application and
Session; consuming edit/construction queue refusal returns the original candidate. Typed
factories and descriptors register atomically. A process-local, nonserialized workspace-instance
lifetime marker fixes hide/reopen, close/reopen and target-away/return transitions between
frontend calls. The reference gesture owner also cancels after that exact instance retires.
Compatible placement/native-window/render-view changes preserve local state.

The separately locked `editor-ui-host-consumer` disables defaults and authors its own retained
field/buttons/modal. Its CPU workflows preserve non-active navigation, complete source records
and placements, history and persistence through the public context. The native executable uses
the original `WgpuRenderHost` and public `UiSurfacePresenter`, with the existing fixture's public
image sampler. Six actual composites include one retired candidate; five enter the input journal.
Replacement input requires actual presentation and focus. Exit cancels active preedit through
the reducer, clears interaction facts, drops both panel instances and physically drains the
original GPU Host with no cleanup faults or abandoned submissions.

Qualification corrected three fixture assumptions without reducing acceptance. The internal
Prefab-open helper now waits for the exact public Browser request/result rather than guessing
completion from a source locator; both originally failing Host tests then pass. The external
CPU consumer uses public Application states to observe replacement, without exposing the private
workspace generation. The first native run fails at Apply because a root layout offset was
treated as screen placement. A fill modal root with a positioned child and clicks derived from
actual presented clipped semantics correct the fixture. The complete native trajectory then
passes. Independent review additionally strengthens retired-action evidence from an invisible
read to an otherwise valid prepared mutation, verifies property/basis/history/disk remain exact,
correlates every persistence callback with its attempt, and checks reducer/input/panel cleanup.
Final independent production and strengthened-consumer reviews report no P1/P2 findings; those
read-only reviews do not substitute for the executed qualification below.

Executed on Windows 11 with Rust 1.95.0, serial Cargo and the shared root `target`:

| Scope | Executed result |
| --- | --- |
| Editor without `imgui-reference`, library plus workspace-layout/generic-composition/open-recovery integrations | 188/188, two skipped; includes all 15 semantic Host tests; run `3ede544a-2df1-438d-8d4e-5a381c7adda3` |
| Default reference Editor library, Application/panel/authoring-loop/first-game/workspace-layout integrations | 450/450, 61 skipped; run `3acafc01-8ccb-4e35-b07c-59caa1a94708` |
| Independently locked public panel CPU workflows after stronger retirement/persistence assertions | 7/7, no skips; run `2e95472c-c94d-4fdd-a211-a56382aadcad` |
| Independently built public panel native Windows/DX12 trajectory | 1/1, no skips; six presentations/five journal commits; run `f704f2a9-b954-442a-87cd-b5bfbd10e494` |
| Selected default-reference native authoring/Inspector/Hierarchy/Canvas/Scene/Game/panel/recovery/close trajectories on Windows/DX12 | 33/33, one slow; 441 outside this ignored-only filter; run `52760c9b-9b5f-4cdd-8a9b-033b2eedbfca` |
| Existing independently compiled reference-panel/document/tool consumers | 46/46, four skipped; run `642e084a-b100-404c-a5f8-705163164e7c` |
| Existing external public embedded native Host on Windows/DX12 | 1/1, no skips; run `8c2174a7-8d99-44ab-b439-0cbbf56322b0` |
| Independently locked panel consumer's normal/build all-target graphs, with and without `native` | 1/1, 14 outside this filter; no Dear package; run `1b7a2dbc-1ddd-4433-871c-ea4c9ec10a49` |

These cohorts overlap with later default-feature regressions; counts are not additive. The
independent lock resolves offline without Dear, and its `native` build uses explicit WGPU/Winit.
The 447-package lock adds no dependency version beyond the root lock; only the consumer itself is new.
Editor still has those broader product dependencies even with the reference disabled. The
semantic Host owns no native capability arbitration, input route or presentation transaction.
Injected pointer/IME delivery and accepted composites establish neither OS event/accessibility
parity nor a pixel readback oracle. No new performance, incremental cache, extension hot reload,
foreign-toolkit compatibility or complete independent Editor/R11b/foundation claim follows.

Correctness Clippy passes Editor all targets with all features and separately without defaults,
plus all targets/all features of the independent panel consumer. Both the selected reference
Editor executable and the independent native example build under locked offline resolution.
The final review preserves the original source/result/lifetime assertions and removes no
incumbent workflow. These checks are not warning-free: existing style/dead-code diagnostics,
reference-only fields/helpers in the neutral build and large owned result/candidate returns
remain. Root and independent-consumer format checks pass; changed-document local file links
resolve, excluding remote URLs and fragment validity. Documentation closure adds no native or
performance claim beyond the executed scopes above.

#### Image-backed native panel attachment

Add the first concrete public native attachment above the existing semantic panel Host and
WGPU image/composite owners. It borrows the caller's original native window and admits finite
image-backed panel outputs, their exact immutable frontend snapshots, placement/clip and
capabilities. An opaque panel-instance token comes from the original semantic Context; matching
saved IDs or labels cannot bind another incarnation. The shared Host performs no layout,
widget reduction, document mutation or GPU creation/submission. Nara content keeps its existing
presenter/journal/interaction, while a small non-Nara consumer keeps its own buffer and controls.
Each native attachment Host binds the original semantic Host identity. Foreign attachment and
dispatch refuse before routing or draining cancellation. This preserves one exact ordered
candidate for focus transfer; heterogeneous frontend state may be composed by that original
Host's own concrete panel type, without a new action or widget dispatcher.

Stage one complete window candidate with an ordered, bounded set of panel images. Adopt its
input mapping only after the original successful native outcome covers every exact image
occurrence, window and display frame. Deferral, foreign/superseded candidates, missing composites,
stale native geometry, retired panel tokens and invalid device outputs cannot publish new routes.
Atomic window adoption preserves compatible provider state; changed geometry, omitted outputs,
focus loss, native resize/scale and retirement cancel interaction before later input. Existing
backend permits, pending submissions and physical retirement remain with their original owners.

The finite single-window Host arbitrates keyboard, pointer capture and window-modal blocking
between attached providers. It maps native positions through the admitted image placement,
supports out-of-bounds captured motion without clamping and preserves wheel units/modifiers.
An explicit focus operation admits an already presented keyboard provider independently from
pointer capability; it cannot focus an OS window or steal an underlay from a window-modal owner.
Frontend callbacks consume their original presented snapshot and use the same semantic Context
to queue domain work. Cancellation and ordinary input settle through one existing submission
candidate, in actual delivery order. The Host coordinates IME readiness/candidate area, shared
clipboard access, cursor and coalesced immediate/deadline refresh. Required unsupported services
refuse before attachment/output publication. Direct rendering, out-of-panel overlays, native
detachment and native accessibility remain explicit unsupported capabilities of this first path.

Text service ownership additionally includes an opaque provider-local focus scope. Switching
fields within the same panel must retire the original IME readiness, just as switching panels
does. Text and composition delivery retains its original scope; a drain cannot select the latest
field token. This scope is an ordinary nominal lifetime observation, not a shared widget-ID
registry or source identity. Native acknowledgement gating does not prove arbitrary OS-event
ordering or actual platform candidate delivery; those retain their separate qualification.
The native mapping owner clips nonzero exclusion areas to accepted visible content, refusing
fully invisible areas and outside zero-width carets without relocating insertion geometry.

Move the existing Dear-backed clipboard's text access to a shared native owner and leave only
the reference toolkit shim at the old seam. Preserve failure and retry behavior. Use the already
resolved text-only clipboard backend; no alternate input/GPU owner or package/version family is
required. Nara's adapter must reuse its content transform and field tokens rather than turn
target pixels into a new text-coordinate authority.

Qualify independently built Nara and non-Nara frontends through these same ports, including
actual rendering, text, modal behavior, clipboard/cursor/refresh, exact prepared source edits,
history/persistence and pending-output retirement. Keep the complete preceding Nara trajectory.
Add observable valid-mutation refusal, focus/capture cancellation and replacement-token controls,
fractional placement/clip tests, original outcome correlation and physical GPU cleanup. CPU
simulation, injected native input, OS event delivery and performance remain separately scoped.
This path is source-level native consumption, not a stable ABI, named-toolkit compatibility,
automatic reload or complete independent Editor/R11b acceptance.

Independent review corrects original semantic-owner binding before attachment/cancellation
drain, adds a real focus entrance for keyboard-only providers, and moves final IME clipping
into the native mapping owner. The first full-containment rule correctly exposed a partially
clipped real text exclusion area in the fractional fixture; clipping nonzero areas preserves
the source text/caret owner while refusing invisible caret lines. Additional review retains
the original retired image across a no-output retry without constructing/re-retiring its
replacement, and adds Nara adapter refusals for Point against unpresented text/preedit.
The deterministic no-receipt control injects an empty outcome and then presents the same
original image through the real Host; it does not simulate a successful receipt or qualify
OS surface deferral. Initial two-panel fixture failures were focus setup and consumed-event
expectations: dropped unready text is consumed but never reaches the provider callback.

The Zed/Open-GPUI source follow-up also records cache/state membership as joint acceptance:
cached existing children retain membership/dependency associations through parent recomposition;
actual omission keeps its declared eviction policy. Open-GPUI ordered-key preparation has
separate logical-entry, comparison and key-copy costs. These refine R11e's future reuse tests
without selecting a generic retained tree, virtualizer dependency or performance claim.

Serial Rust 1.95.0 qualification on November 5 uses the existing root `target` and original
Windows/DX12 hardware Host:

- Shared native services and semantic panel custody pass **33/33**, run
  `0f76f02d-8382-4a35-afd7-df6b58bb6258`. The complete no-reference Editor library later passes
  **192/192**, with two skipped; these cohorts overlap.
- Default Editor library and selected public workflow integrations pass **443/443**, with
  61 skipped, run `ed85d2b6-9e99-46df-9ea0-836419b9b58f`.
- The independent UI Host's CPU/panel/painter cohort passes **14/14**, with two native cases
  skipped, run `75c95fd8-e107-482f-b197-a18503362bd8`. The seven painter/control tests do not
  import Nara UI widgets/rendering in their owning module.
- Both independently built native workflows pass **2/2** after review corrections, run
  `d2d3c707-5fdd-4954-9bbb-202f24347edb`. Nara preserves **6 presentations / 5 journal commits /
  7 exact callbacks**. The non-Nara frontend completes **18 actual presentations / 7 exact
  callbacks / 144,000 original image pixels compared**, including two-panel atomic refusal/
  adoption, old cancellation before new focus, injected no-receipt retry and original GPU cleanup.
- The affected reference Editor native ordinary authoring, shared clipboard/keyboard,
  retained-draft capacity recovery and panel/GPU reconstruction workflows pass **4/4**, run
  `3b003216-0456-46c9-970f-1d0110226d7e`. Capacity recovery takes about 93 seconds in this debug
  run; it is correctness evidence, not an interaction-cost measurement.
- Existing independently compiled panel consumers pass **46/46**, with four skipped, run
  `4f8e72b5-bcc3-4141-b48f-4b60edd202c8`. The independent UI Host's normal/build dependency
  assertions exclude every Dear package in ordinary and `native` configurations, **1/1**, run
  `9a8690d6-5fed-4326-abd0-cd96eed41d57`.
- The public embedded reference Host's actual hidden-window startup/paint/close passes **1/1**,
  run `1856b634-cb7d-47a6-9cab-7fb7d6373f82`, preserving all fixture source bytes.
- The shared image-painter helper's original two-native-window/independent-image workflow
  passes **1/1**, run `733ae60d-a45e-4a5f-bf43-09ced8eb8714`. Its existing clear/placement
  behavior is retained alongside the new fractional viewport/multiple-image load operation.
- Default/all-feature and no-reference correctness Clippy, independent all-feature correctness
  Clippy and the default Editor executable build pass. Existing non-correctness warnings remain;
  no warning-free or whole-workspace claim follows.

The fixture locks add the already resolved desktop text clipboard dependency without upgrading
existing package versions. Root/shared/independent tests remain separate scopes, not an additive
coverage percentage. Actual OS input and candidate ordering, system clipboard interoperability,
final swapchain pixels, Linux/macOS, arbitrary toolkit embedding, reload, full independent
Editor and R11a/e cost budgets remain open.

#### Independent drawn source workspace

The [first successor workflow](2026-11-05-independent-drawn-source-workspace-plan.md) now launches
an independently selected Nara composition without the reference toolkit. The ordinary source
and specialized UI Hierarchy controllers move out of the Dear adapter and serve both
compositions. A drawn document shell connects the original semantic panel/native image/
presenter/WGPU owners through presented input. Its Windows/DX12 trajectory qualifies source
create/move/delete, all four UI recipes, full 4,096-byte text, document switching, history and
exact Save/Reopen. Source identity refresh, conflicting placement refusal, projection/instance
retirement and explicit graphics reconstruction keep their original owner checks. The unit
records overlapping CPU, independent public-entry, reference regression and build scopes
separately. Subsequent Inspector, Scene/Game, Browser/import and workspace workflows extend
that same composition. The default switch still requires the full agreed matrix; this first
workflow cannot close R11b/foundation or establish R11e performance budgets.

#### Styled source workspace and component-library prerequisite

**State:** the [styled source-workspace unit](2026-11-05-styled-source-workspace-plan.md) implements
the first shared typography/appearance recipes, measured controls, anchored menu and complete
shared Inspector in the independent composition. Its actual qualification and remaining limits
belong to that unit. The first workspace remains an engineering authoring page, with source IDs
and explicit workflow feedback; it does not establish a finished component catalogue or full
Editor visual acceptance. Qualify further recipes through complete visual consumers rather than
treating the Shell's arrangement as their public interface.

**Sources:** the [November 5 style/layout study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-style-layout-and-component-library-readiness)
pins Zed GPUI/Open-GPUI, upstream `repo-ref/imgui` and `repo-ref/godot`. The
[UI design](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
records targets and current gaps. Reuse existing state/layout/drawing/text/native owners;
select exact style and component API shapes through this consumer.

The user-supplied [Design Kit v0.4 reference](../design/editor-reference/README.md) supplies a
complete 32-file design reference, sixteen UI images and an English adoption map. Carry its graphite/teal
hierarchy and explicit component-state design into the next complete visual comparison. Reconcile
13/20 versus current 14/20 typography, actual admitted faces, encoded color conversion and
unsupported tool/preferences before changing defaults. The reference does not expand game scope
or move document, rendering, input or layout ownership into a design-token file.

**Complete consumer:** extend the independently drawn source workspace with visually coherent
document tabs, a compact toolbar, selectable source/hierarchy rows, one editable property group,
inline disabled/read-only/invalid/pending/result feedback and one anchored action menu. Preserve
exact selection, editing, history and Save/Reopen. Add one separately authored decorated row with
scoped styling and ordinary text/keyboard behavior. These elements must work together in a real
page, including narrow-window fallback, rather than only in disconnected widget demos.

**Work and acceptance:**

1. Complete the [style design inventory](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness):
   font identities/fallback/features/readiness, line metrics/baselines/ink bounds, unit/scale
   conversion, numeric density/spacing/size recipes, color/state roles and extension slots,
   constraints/overflow, overlays and accessibility preferences. For each record its owner,
   inputs/units, precedence/reset, fallback/error, invalidation, lifetime/cost and acceptance.
   Include mature frontend token/typography/component systems as comparative primary evidence;
   no CSS/DOM implementation or universal design-token registry is selected.
2. Review a small visual specification and actual fixed-content captures: surface hierarchy,
   text emphasis, baseline/alignment, spacing/density, states, focus/error contrast and icon/text
   balance. Record normal/compact metrics independently from DPI and user text scale.
   Keep an [implemented numeric recipe table](../guides/editable-text.md#scoped-appearance-and-measured-controls)
   and explicit support gaps alongside the source study. Before expanding the catalogue, qualify
   a mixed-font icon/label/value row with actual baselines, optical icon placement, stable hit
   targets and combined focus/selection/error states. Drag-out/reentry appearance must agree
   with the original button behavior. Broader font/style preferences and asynchronous resources
   need their own admission/reconstruction consumer rather than implied support from token names.
3. Define required semantic tokens, typography/metrics, component variants and state precedence;
   scope/default/local overrides have explicit removal/reset. Color/brush changes preserve
   compatible text/layout work; font/fallback, line-height, locale/direction, density and
   constraints invalidate actual dependent measurement, placement, hit/semantics and paint.
4. Use existing intrinsic child measurement for min/preferred/max sizing, baseline-aligned
   label/value rows, shrink/grow and bounded overflow/scroll. Long paths/CJK/combining/RTL labels
   keep full source and semantic text through their stated wrapping or display-truncation policy.
   Ordinary page authors do not manually maintain control rectangles.
5. Measure and anchor the menu in one explicit coordinate space; constrain it to available
   bounds, reconcile scroll/resize/scale/anchor removal and preserve dismissal/focus/modal
   behavior. Define live inheritance versus opening-snapshot theme scope explicitly.
6. Independently consume the decorated row under default and local styles; siblings remain
   unchanged, removing an override restores defaults, and compatible theme changes retain
   drafts/preedit/history. Custom drawing and full foreign-frontend attachment keep their
   separate public capabilities; no central widget enum or new global theme owner is required.
7. Qualify the actual drawn composition with the existing native procedure. Target
   100/125/150/200% scale, window/font changes and both densities; report which combinations
   actually ran. Keep native OS candidate/accessibility and untested platforms explicit.
   Attribute composition/style resolution, measurement/layout, shaping/raster, packet/upload
   and retained bytes at idle, hover, one edit, resize and theme/density changes against R11a.
   Visual consistency and functional tests alone do not meet R11e's performance budget.

**Exit:** one approved complete visual workflow and an independently consumed styled composite
with documented scope, actual captures/measurements and native results. Broader component
catalogues, advanced effects, docking/multiple windows and reload retain their own consumer
requirements. This adds no CSS parser, copied foreign toolkit ownership or styling-script layer.

#### Custom child-measurement and layout prerequisite

Provide one public immutable layout description on an admitted node/root. Scope child operations
to the current direct children, without exposing Taffy storage: repeat measurement under explicit
content-space constraints, then place an exact outer rectangle or explicitly hide each child.
Keep compiler text/glyph/paint limits and final paint/hit/focus/semantic geometry authoritative.
Use optional real baselines and explicit padding/percentage bases. Default Flex/Absolute behavior
must survive the private layout-storage replacement, including nested and hidden subtrees.

Invalid geometry/index/order, missing/duplicate placement, callback refusal, caught unwind and
swallowed child-operation errors follow root-local inert-fallback rules. Candidate-wide layout
work charges dispatches/cache hits, callbacks and child requests before work. Overflow remains
global; local failure cannot reset its cost. Failed candidates cannot advance public revision or
cache state. The count is neither a wall-time watchdog nor an arbitrary user-payload byte bound.

Qualify a reusable measured-column layout in the independently compiled component library. Its
actual keyed editors use measured title and child sizes rather than row-index absolute placement.
Run the same full native presenter/input/popup/reconstruction/retirement trajectory under that
layout, preserving prior assertions. Add direct child constraints/remeasurement, nested layout,
baseline/padding/clip behavior, finite failure recovery, exact work boundaries and an independent
Taffy default-layout oracle. Existing Editor/game consumers must still compile and pass relevant
CPU/native qualification because the private default layout adapter changes as well.

This prerequisite does not close custom drawing/path geometry, routed canvas motion/scroll/key
input, scoped effects/services, virtualization, foreign panel attachment or the whole independent
Editor. Source callback equality is nominal immutable-instance identity; updated captures require
a new description. Snapshots retain callback code, while final frame/fallback/packet facts do
not. Code reload/unload and automatic dependency tracking remain separate work.

Review corrects three owning contracts before final qualification. Text requests now distinguish
natural, finite-wrap and actual Parley min-content modes; the same shaped layout supplies the
minimum rather than an approximate word-width scan or a second shaping pass. Prepared snapshots
expose their actual first-line baseline. Public child measurement separately propagates genuine
text/custom baselines through visible in-flow children in authored order, leaving Taffy's
synthetic box baseline private. Exact placement also prechecks each child padding sum: Taffy's
Flex main axis may otherwise keep a known outer size below its insets while reporting that same
size. Row/Column below-boundary and exact-equality controls cover this distinction.

The custom-layout subtree marks its ancestors so final full layout cannot replay a cache over
exploratory descendant placements. Measure-after-place/hide refuses instead of introducing a
restoration ledger. Source descriptions use nominal equality, but changing children under one
description still compiles current content. A captured-owner weak/drop control keeps original
frames and last-good compiler facts live after all source descriptions are released; none may
retain the callback. A separate swallowed-local-error control continues requests past the shared
layout limit and requires global overflow, never a cheaper root-local fallback.

Initial production checking catches the removed layout block's still-needed paint scale local;
restoring that original environment value fixes compilation. The first combined game-UI test build
then exposes an existing cooked-image test compiled without its required `serde` feature.
`bced793` gates only that test and its codec import; default and enabled-feature runs both remain
required. `c3010d2` separately commits actual min-content requests and immutable snapshot baselines.

The first executable shared run passes 415/419 and fails four new fixtures. Three assumed root
insets relocate the authored origin, whereas the original and replacement Taffy root paths anchor
at logical `(0, 0)`; positioned child nodes now express the original expected nonzero geometry.
The fallback case similarly places the live sibling as a child, retaining both inert-background
and live-hit checks. The intrinsic test also mistook available width for a hard cap even when an
unbreakable word overflows; its width now matches the independent text oracle plus padding.
Expected fractional bounds, clip, physical mapping, nested placement, text metrics and hit
assertions remain. Focused 12/12 and the subsequent full 419/419 pass after those corrections.
No root-positioning or word-breaking product semantics change to satisfy the fixtures.

| Final qualification | Result | Nextest run |
| --- | --- | --- |
| Shared text/kernel/widgets/presentation and default game-UI regression, including 12 custom-layout and five min-content cases | 419/419 | `ebca498d-4ec4-44a1-a2aa-d61d2ab2e0f9` |
| Game-UI library with `serde`, including the cooked-image case | 125/125, overlapping the default suite | `3628047b-2bb8-4fb4-bf20-f6927f0918cf` |
| Render-WGPU ordinary library with all features, including existing glyph/pixel readback | 396/396, 52 ignored | `dfd27c91-8e11-45ae-a077-589289129663` |
| Editor library and selected public integrations after the final production corrections | 406/406, 60 ignored | `ed9c16c7-7ba7-499b-900a-089efc8508f2` |
| Selected Windows/DX12 ordinary editing, Hierarchy, Canvas, Game, panel/GPU lifecycle, Scene and stock composition | 33/33, 421 unrelated/ordinary cases filtered | `6bb9add1-3dcd-4a58-9145-ef6c33c17684` |
| Independent public text/kernel/widgets component library | 15/15 | `f26ae8f7-9071-4c1b-84fe-dce20e089ff1` |
| Independent native measured-composite trajectory and original error/panic child controls | 2/2 | `2a832531-1b80-43e5-ad37-e70955255459` |
| Final focused child-layout regression after equivalent conditional normalization | 12/12, overlapping the shared suite | `718b1cc3-d7cb-40db-98de-67b88aa15e55` |
| Neutral component, native executable dev graph and game-UI dependency boundaries | 3/3 | `f1d0431c-8504-4906-bc06-c740f97b338c` |

The complete independent native normal path still has 11 actual presentations and ten journal
commits, with new per-compile callback/child-request deltas proving that the imported measured
layout actually runs. It retains original modal, local text history, reorder/preedit, filtering,
same-position replacement, focused binding-token refusal, Host reconstruction and physical
retirement assertions. These injected events do not qualify physical OS IME/candidate delivery.
Default Flex equivalence uses a separately constructed pinned `TaffyTree` oracle; it is not a
second independent layout algorithm. No retained callback code survives in compiled frame/cache
facts. Independent architecture/correctness reviews find no remaining P1/P2 after the three
owner corrections and their regression cases.

The native retained-draft capacity case completes in 143.732 s in this run; it is correctness
evidence, not a matched before/after latency experiment. The existing work-baseline document now
keeps its older source counts/distributions explicitly separate from this changed private
dispatch and intrinsic measurement. Full R11a numeric budgets, incremental reuse and the
remaining R11b-f capabilities stay open.
Correctness Clippy passes for text/kernel/widgets/presentation/game-UI/Editor with all targets
and features, and for both independent consumers. A new nested conditional is normalized to an
equivalent let-chain; its focused layout/error/budget suite passes afterward. Existing unrelated
warning categories remain outside this correctness gate. Root and fixture formatting, diff
whitespace and local-document link checks complete the review without another product cycle
for prose/format-only closure.

#### Keyed local-state composition prerequisite

Implement a bounded panel-instance `UiStateStore` with exact root/parent/explicit-child paths,
typed store/incarnation handles and explicit state-schema/model binding. Compatible composition
reads retained state; local edits occur in a separate update phase. Lazy initialization creates
only new local state. Successful `finish` adopts membership; duplicate keys, capacity, failed
initialization and incompatible ordinary reuse poison the pass. Failure/abandonment keeps the
previous owners. Explicit replacement starts a new input incarnation without copying old tokens.

Qualify an independently authored row composite with two same-label editors through the neutral
text/kernel/widgets consumer, then import that actual library in the existing independent native
presenter executable. Preserve its original popup, text history, backend reconstruction and
original-outcome retirement assertions. Add reorder/insert/repeated composition with active
preedit/selection/history, focused filtering, same-position replacement and explicit model/schema
replacement. No private receipt creation or duplicate native/interaction owner may be introduced.

This row fixture deliberately evicts omitted state. Existing Editor form drafts that survive
hidden controls retain their longer-lived owner; do not mechanically migrate their policy.
Membership commit is neither compiler nor presenter acceptance. Native focus/IME/capture and
external task/result custody stay with the Host and existing semantic owners. Slot counts and
exact key bytes are bounded separately from arbitrary user payload memory. Record actual work
and limits without claiming virtualization, automatic dependency tracking, layout reuse, hot
reload, performance improvement or complete R11b component/custom-layout acceptance.

At source base `0f558a7` plus this prerequisite, the public widget module implements those state
contracts and the independently compiled `KeyedEditors` library supplies the real consumer.
Replacement counts as initialized/replaced/retired; failed passes preserve `last_changes`.
Captured initializer unwind poisons both ordinary creation and replacement. The neutral consumer
uses public local field focus and fractional compilation, not a private receipt factory. A
separate widget integration test uses test journal authority to exercise real routed focus,
blur/preedit cancellation, refocus and old-dispatch isolation; it is explicitly CPU evidence.

The independent native frontend imports the actual composite library. Its original five
presentations/four commits remain asserted before six more presentations/commits exercise keyed
composition. Reorder/insert preserve the original draft, combining preedit, caret/selection,
focus token and Undo/Redo. Filter eviction, same-visible-position replacement and explicit
document/schema replacement reject original captured input. Closing cancels the original seat,
evicts local state and rejects saved tokens before presenter and original Host retirement.
Error/panic controls retain their original outcome-settlement and exact cleanup requirements.

Initial fixture compilation finds a trailing struct-update comma, then the native build finds
incompatible boxed-error conversions and a nonexistent layout-source getter. The corrections
preserve original error objects through explicit trait-object coercion and compare logical glyph
positions/advances and face/glyph identities with full metrics. They do not require placed
target-scale raster keys to equal original editing keys. Native runs pass; the final one follows
explicit close cancellation, the edit-adapter borrow simplification and old binding-token
refusal after its replacement actually receives focus.

| Qualification | Result | Nextest run |
| --- | --- | --- |
| Shared text/kernel/widgets/presentation regression, including the first nine keyed-state cases | 271/271 | `5f0c6790-9895-4620-b0f5-0f41453d2077` |
| Keyed-state and routed-text custody, adding caught unwind and actual focus-dispatch integration | 11/11, including nine overlapping cases above | `fa550458-99b4-4413-a212-a178353a383f` |
| Independent neutral module consumer, including three full keyed-composite workflows | 13/13 | `dc7dfcf3-f7a1-4d7c-9cf4-d3135e7a033a` |
| Independent native UI trajectory plus original error/panic child controls | 2/2 | `0d786d8c-bc8e-4583-8678-539decafc11f` |
| Neutral component and native executable dev-graph dependency checks | 2/2, 10 unrelated cases filtered | `7f759fbf-d445-4bab-bc7f-83368cbcc80a` |

The final normal Windows/DX12 trajectory observes 11 actual native presentations, ten journal
commits and exactly one original popup action. Input is injected pointer/IME and logical keys;
physical OS input/candidate delivery and accessibility remain separate. Logical glyph checks are
not independent pixel readback. Slot bounds and lazy-initializer counts do not establish timing,
allocation reduction, automatic dependency invalidation, cached layout or virtualized total work.
No Editor controller, hidden-form retention policy or frontend default changes in this slice.
Custom geometry/layout/drawing, scoped services/effects, async retirement, neutral presentation
dependency separation and the complete independent Editor remain open under R11a/R11b-f.
Independent correctness and architecture reviews find no P1/P2 in this bounded prerequisite.
Review-driven coverage adds caught initializer unwind, routed blur/refocus/old-dispatch refusal
and old binding-token refusal while the replacement is focused. Correctness Clippy passes for
the widget crate and both independent consumers with all targets; root and both fixture format
checks pass. All 1,068 local Markdown link targets across the seven changed current documents
and diff whitespace checks pass. Existing unrelated warning categories remain outside the
correctness gate. Formatting/document closure after these results does not change product scope.

#### Local font reconfiguration prerequisite qualification

At source base `6395ae0` plus this prerequisite, the shared text/field interface adopts new ordered
fonts without replacing buffers, preedit, local history, field keys or focus tokens. Current,
composition-origin and all 32 retained history layouts must pass under the original limits and
logical size before adoption. Raster/target-scale admission remains separate, including a tested
layout-success/raster-refusal case. Same configuration is a no-op; changed unused fallback still
requires fresh paint provenance.

| Qualification | Result | Nextest run |
| --- | --- | --- |
| Full text/kernel/widgets/render regression, including ten new text-owner and two field cases | 252/252 | `951dc87c-0556-49a1-bb98-76ebea30a4fe` |
| Expanded combining/ZWJ/ligature selection case in both font-change directions | 1/1 | `d0a3c112-1c9f-460e-a6c8-555885ec9cf0` |
| Independent public text/kernel/widgets consumer, including three actual font-change workflows | 10/10 | `6078cfd7-790f-4758-a929-bdff052f58dc` |
| UI backend routing/capture and real GPU readback cohort | 32/32 | `a62aacd1-f3a2-4375-9bdc-09c8a8ebb7a1` |

The independent consumer uses public local field focus, committed semantics, freshly compiled
fractional/clip geometry, point editing and actual glyph masks without Editor/App/ECS/platform
dependencies. It does not submit journal presentation receipts. The new GPU case consumes a
`TextEditor` snapshot through matching compiler/planner fonts and reuses one existing device,
queue and texture cache for old/new/old draws. Single-face coverage and actual clipped ink are
checked; retained packet/atlas pixels survive source-editor/engine release. Its expected pixel
placement uses packet rects and compiled raster keys, so it is not an independent absolute
position/subpixel or rasterizer proof. Public field consumption and backend snapshot rendering
are complementary evidence, not one native field/Host workflow.

Independent correctness/testing reviews find no P1/P2. Public component layout/drawing, live
all-panel font adoption, old/new presented input/IME handoff, typography costs and the complete
R11a/R11b-f acceptance remain open. This prerequisite selects no incremental cache optimization.
Correctness Clippy for all five affected crates with all targets/features and for the independent
consumer passes. Root/independent formatting, diff checks and 1,052 local link targets across the
six changed current documents pass. Existing warning categories outside correctness remain.

#### Independent presented-surface prerequisite

The finite `UiSurfacePresenter` in `nara_ui_render` supplies the missing public
packet-to-presented-journal bridge for an independently authored Nara frontend. It composes the
existing planner and hosted journal with original outcome-permit custody. It owns neither
interaction/reducers nor a GPU, platform or document authority. One Host-assigned non-reused
surface lifetime has a fixed target and at most one active presentation attempt.

Preparation stages a journal candidate before returning the original backend frame. Only a
matching original one-shot presentation commit adopts the staged journal; input state is never
restored from preparation. Foreign completions return unchanged. Deferred retries preserve the
original attempt, packet, epoch and required backend generation. Retirement revokes routes
immediately, terminates local retries and drains original returned/dropped permits to quiescence.
Late presentation is discarded rather than reactivating the retired route. Physical GPU/native
use retirement remains independent under the render Host.
Attempt faults keep the previous valid presentation. Target/device faults withdraw accepted
history and close admission while retaining the installed Slot for the Host's explicit later
retirement generation; the presenter never substitutes an epoch for that generation.

The independent `render-output-consumer` example uses public text/kernel/widgets/render and
WGPU stock UI interfaces without Dear or the game's UI ECS adapter. Its hidden native window
executes `submit_for_display`, paints the designated display occurrence, observes its exact
composite, returns original outcomes and drains before routing field/button/modal-popup input.
CPU controls exercise foreign ownership, abandonment, original deferred retry and close/late
results. An executable dependency check includes the example's dev graph, not only its smaller
offscreen library closure.

This prerequisite does not complete the Editor's toolkit-independent panel attachment, reusable
component/custom-layout interfaces, neutral presentation dependency split, non-Nara frontend
conformance or independent default shell. The existing Dear bridge keeps its current evidence;
any later migration must replace its planner/journal/promote bookkeeping rather than retain two
publication authorities. OS input/candidate delivery, accessibility and other native platforms
require their own evidence.

Independent review finds two pre-existing WGPU contract gaps exposed by this consumer. Lowered
packet facts previously compared value identity/target only, allowing a different replan of the
same frame with different image bindings to accompany another packet's permit. Facts now retain
and compare the original packet `Arc`; their shared storage is charged through the existing
metadata reservation. An already bound Submitted sidecar now also validates its backend
instance/device epoch before reuse, preserving the original sidecar on refusal. These repairs
belong to the original backend owners, not another presenter-side identity ledger.

The first native run fails the popup-action assertion because the fixture uses a fixed click
coordinate. The correction targets the actual presented action's semantic geometry; it keeps
the modal/presentation and exactly-once action assertions. The error/panic controls already pass
that run, but do not substitute for the corrected complete normal trajectory. Review also finds
and corrects target/device-fault route withdrawal, with an Attempt-fault preservation control.
Missing reservation-failure injection and old completion arriving after replacement is active
remain distinct evidence limits; CPU ownership and native close tests do not silently claim
those interleavings.

| Qualification | Result | Nextest run |
| --- | --- | --- |
| Kernel and presentation regression, including journal withdrawal and nine presenter cases | 175/175 | `f6ce24a8-3e1d-4262-a293-7c7fd0577517` |
| Complete ordinary render-WGPU library with all features | 396/396, 52 ignored | `99755911-f635-4ed1-8f35-e6d25c89160b` |
| Independent native output/surface consumer, including both UI tests and error/panic child modes | 6/6 | `753a4b63-ad48-4fe3-85aa-88111e27839b` |
| Native dev-graph and existing neutral/game UI dependency constraints | 3/3 | `bbace96a-4d62-4e00-9c98-a5df2db94888` |
| Existing native Editor Canvas plus normal/fixed-world Game workflows | 3/3 | `9664d2f3-5ca1-4e8b-84fa-8056b8211c30` |

The independent native run uses Windows/DX12, an explicit non-fallback request, the bundled
font and a hidden actual window. Its complete normal trajectory observes five native window
presentations and four journal commits, one modal action without underlay activation, local
text Undo/Redo, stale route/token rejection and original Host reconstruction/retirement. Two
additional child runs inject error and panic after actual image submission and require original
cleanup before reporting success. Events are injected pointer/IME and logical keys, not physical
OS delivery. The first popup-coordinate failure is retained as `e980b774-0a02-45d3-89b6-f6cbbc6344ee`;
the final six-case result above follows its correction and the later kernel fault fix.
Independent architecture/correctness and testing review find no remaining P1/P2 in this scope.
These results use source base `5fc5c11` plus the presenter work and the backend repair committed
as `6a7a8be`. Final correctness Clippy covers kernel/presentation/WGPU/Editor with all targets
and features, plus all independent consumer targets. Root/independent formatting, diff checks
and 1,091 local link targets across eight changed current documents pass. Final example cleanup
only normalizes two equivalent conditional spellings for Clippy after native qualification;
it changes no workload, acceptance or lifetime boundary. Warnings outside correctness remain.

#### Same-stream custom geometry prerequisite

This unit adds bounded immutable triangle facts to the original kernel paint stream,
neutral packet and WGPU `UiFeature`. An independently authored curve component supplies its
own tessellation and ordinary controls without changing a central widget catalogue. Follow the
pinned GPUI path-builder/scene mechanism, while preserving Nara's existing layout, resource,
presentation and input owners. The [selected geometry contract](../architecture/ui-foundation-design.md#captured-custom-geometry)
specifies local coordinates, content clipping, color/alpha, ordering and aggregate limits.

Required evidence includes exact fractional mapping and quad/mesh/quad barriers; complete
source/output/retention accounting, including repeated and hidden geometry; typed refusal and
recovery before publication; original buffer/pipeline custody and work observations; and an
independent native presentation with pointer/keyboard-operable node semantics. Extend the
existing full field/popup/reconstruction/retirement trajectory rather than replacing it with
a standalone pixel demo. A real GPU pixel oracle separately verifies color, clipping and order.
No new Device, Queue, submitter, document owner, executor or live paint callback is introduced.

This is a prerequisite for R11b/c. Canvas drag/wheel routing, final-size drawing, complete graph
editing/history/persistence, foreign panel attachment, automatic invalidation, virtualization
and the full independent Editor remain open. Record the actual results before qualification.

The implementation captures immutable `UiMesh` data in kernel frames and transformed
`UiPacketMesh` payloads in neutral packets. Source envelopes count geometry before rejecting
root bodies; fresh/retained output has a separate aggregate count. Hidden/shared occurrences
cannot bypass admission. Meshes use content-relative coordinates and mandatory content clipping,
with background/mesh/text fragments in ordinary painter order. The external `CurveCanvas`
library uses fixture-only `lyon_path`/`lyon_tessellation` 1.0.16 to tessellate a cubic path and
composes existing node/port buttons. The core has no Lyon, path-model or graph-domain dependency.

The original WGPU UI operation prepares all quad buffers once and a single indexed geometry
buffer pair, then encodes ordered quad runs/meshes in one pass. Mesh barriers prevent merging
same-material/same-clip quads across custom geometry. Capture, Prepared, Report and presentation
binding retain the exact original packet/facts allocation. Mesh calls/vertices/triangles have
their own facts and do not become sprites/quads. Native preparation reserves payload bytes and
retains actual buffers/guards under the original submission. Known capture/slice/pipeline/
reservation metadata is charged for direct programs as well as stock policy.

Early source review corrects nearest-rounded mesh culling bounds to conservative floor/ceil
edges, and a precision counterexample computes physical extrema from actual vertices rather
than reconstructing them from `origin + extent`. Public local bounds conservatively enclose
those source vertices. Independent review finds and closes per-Arc alignment padding and the
missing native capture-allocation charge. Independent `repr(C)` layout assertions cover three
and six indices without using the production cost helper as their expected value. Packet budget
tests require exact admission and one-byte-short refusal without replacing last-good state.

The first all-feature backend regression has one new fixture failure: a purported foreign
`WindowId::PRIMARY` is a legitimate PrimaryWindow alias. Replacing it with an actually distinct
window keeps the same source-count refusal and foreign-target assertions; production target
matching is unchanged. An earlier default-feature selector runs no mesh tests because stock UI
is not enabled and contributes no evidence. Later runs explicitly select all features.

The full independent native trajectory extends the original field/IME/local history/modal/
reconstruction/keyed-state proof. It presents the same external curve component, uses actual
presented node/port bounds for pointer and keyboard operations, changes selection, explicitly
invalidates old geometry before pan, and verifies new geometry/semantics together. A third Host
sentinel makes forward/reverse Tab distinguishable. Surface cancellation clears all local button
state before final route retirement. Four child failure modes cover error/panic after initial
and mesh image submission, requiring original permit settlement, presenter quiescence and Host
retirement. The normal run observes 14 actual native presentations and 13 journal commits.
Input delivery remains injected pointer/IME/logical keys. Pixel readback separately qualifies
the typed Feature's geometry/clip/color behavior; the window trajectory does not claim readback.

Four actual Windows/DX12 typed-Feature cases cover linear pixels, same-instance sRGB format
replacement before any explicit old-prefix wait/readback, direct-program metadata refusal and
recovery, and index-allocation failure after successful vertex initialization. The failed
preparation keeps its original submission token, reports reached work without refunding it,
releases reservations after physical settlement, discards touched state and retries the same
program/capture successfully. Both old/new format outputs retain their exact pixels until
retirement. These tests do not require a minimum GPU-pending interval or use a second queue.

| Qualification | Result | Nextest run |
| --- | --- | --- |
| Shared text/kernel/widgets/presentation and default game UI, after source-bounds/alignment fixes | 450/450 | `5d3b0784-adfb-41b3-938e-ff3387d276dc` |
| Full ordinary all-feature render-WGPU library, after native cost fixes | 403/403, 56 ignored | `0e633bd8-660c-4438-87d7-ccc2ae31bd7b` |
| Actual typed UI Feature linear/sRGB pixels, direct budget and partial allocation failure | 4/4, 455 filtered | `cfe438bb-1d76-4cc0-812b-06d8390f7746` |
| Editor library and selected public integration targets | 406/406, 60 ignored | `0818dbc5-64f1-43d6-9d8d-c1e98cbb5f54` |
| Existing Windows/DX12 Editor trajectories | 33/33, 421 filtered | `4d4cbe78-d918-4e54-9787-d555c04f91c8` |
| Independent component library, including the actual Lyon-backed curve component | 19/19 | `50236aa1-5210-45c7-8738-da6b09e79690` |
| Independent full native surface and four error/panic child controls | 2/2 | `85290550-326d-4284-bdb4-ba13505a9cf3` |
| Neutral component, native dev graph and game-UI dependency boundaries | 3/3 | `6ecf6228-a78c-4553-919b-529046e35b0e` |

These are overlapping owner/consumer suites, not summed unique cases. They use Rust 1.95.0,
serial root-target builds and the source base `9af8cc4` plus this unit. The GPU/native results
use Windows 11/DX12 and an explicit non-fallback request. The existing retained-draft recovery
trajectory takes 90.145s in this uncontrolled run and is correctness evidence only. It neither
demonstrates a regression nor improves the earlier unmatched timing claim. Kernel/planner and
native-fixture reviews have no remaining P1/P2 after the corrections above. Final naming,
formatting and explanatory-document edits preserve the qualified workload and assertions.
Correctness Clippy passes for text/kernel/widgets/presentation/game-UI/WGPU/Editor with all
targets/features and for both independent consumers. Existing warnings outside correctness
remain. The Editor binary builds; root and both independent-workspace formatting, diff checks
and 1,117 local Markdown targets across ten changed documents pass.
The pixel oracle skips samples within a `0.00001` barycentric edge tolerance rather
than claiming a portable triangle-edge tie-breaking or antialiasing result; more than 5,000
other samples cover actual geometry, excluded coverage and ordinary foreground blending.

The shared byte/operation limits and work counters are correctness/admission evidence. They do
not measure arbitrary tessellation time, whole-process memory, allocator/BTree overhead, driver
allocation, cache effectiveness or matched CPU/GPU latency. Geometry construction and projection
need their current matched probes before any R11a/e performance claim. The complete graph,
gesture, foreign-panel and independent-Editor requirements above remain open.

#### Presented pointer samples and viewport capture prerequisite

The next consumer pressure joins captured geometry to actual drag and wheel input through the
original journal/session. Add a neutral event-time pointer sample, declared motion/scroll/button
interest, matching-button capture and finite nearest-listener selection. Preserve line/pixel
origin, modifiers and exact logical positions without importing gameplay Input/ECS or native
toolkit types into the kernel. Text/IME and general logical-key delivery remain separate.

Explicit `TargetGeometry` capture continuity permits a fixed viewport to pan/zoom descendants
through successive actual presentations. Preserve default strict routing. The same owner and
keyed ancestor path, geometry, clips, input declarations and modal scope remain mandatory under
the same Surface transform/generations. Test movement outside the viewport, child disappearance,
return/release, wrong-button refusal, old epochs, reparent, policy/owner replacement, fallback,
native focus loss and Surface retirement. No candidate may renew capture before presentation.

The independent `CurveCanvasState` owns only desired local pan/zoom and drag anchors. It reduces
already admitted target batches and composes mesh/node/port geometry through the existing
library. The full native field/popup/reconstruction/keyed/curve trajectory must remain intact,
then continue a drag across changing presented descendants, preserve an anchored world point
for both line and pixel scroll, and cancel a held gesture on native focus loss/retirement.
Nested scroll listeners target one nearest owner; child button presses must not pan a parent.

This is no general propagation callback, logical-key framework, Runtime wheel-source upgrade,
cross-window capture, full graph document or R11b/c closure. Existing Runtime wheel events have
insufficient unit/modifier information and remain explicitly unhandled rather than reinterpreted.
The [pointer contract](../architecture/ui-foundation-design.md#pointer-samples-and-local-canvas-views)
and ADR0108 own the decision. Record actual verification and limits before qualification.

The implementation replaces three separate pointer input variants with one opaque immutable
`UiPointerInput`. Routed operation samples retain resolved event position, button, modifiers
and scroll units; generated cleanup carries no sample. The layout facts retain the exact
interaction declaration. One capture remembers its accepted button, and a wrong-button
release refuses without clearing it. A fixed viewport opts into target/ancestor continuity;
default widgets retain strict equivalence. Same-slot historical samples cannot clear a renewed
newer capture, while a successor slot can restart epochs and clean predecessor state normally.
Non-finite source data is rejected before any pending reconciliation or session mutation.

Existing Runtime and reference Editor callsites migrate directly. The Runtime's original
untyped Wheel source remains unhandled. The independent native adapter preserves Winit line
units and converts pixel displacement through its direct Surface scale. Its actual component
uses desired local state without creating a second journal or receipt. Ordinary child buttons
still own their presses; their scroll selects the nearest interested viewport. Native capture
continues while descendant buttons move/disappear and the pointer leaves the viewport, then
terminates on the accepted button release. Native window/Surface loss retains original cleanup.

Independent review identifies a bounded-view interaction bug: after a drag reaches the pan
limit, wheel adjustment leaves a hidden excess displacement in the old drag baseline. A
stationary release then moves the canvas. Wheel now rebases the drag anchor to the actual
sample and constrained resulting pan. The native trajectory asserts the exact boundary
sequence `14990 → 15000 → 14984 → 14984` through admitted events, preserving the later focus-loss
and retirement assertions. Kernel, component and migrated-adapter reviews have no remaining
P1/P2 after correction.

The first kernel interaction run is 54/55 because a new environment-change fixture uses theme
revision 2 with a revision 1 theme. Matching the actual theme to the environment corrects that
fixture without weakening capture cancellation; the later shared suite executes all 55.
The independent native trajectory preserves its earlier complete workflow and extends to 19
actual window presentations/18 journal commits. It still uses injected pointer/IME/logical
keys, not physical OS event delivery. State admission/geometry CPU cases and real native
reducer behavior remain distinct evidence. No measured-cache, input-latency or performance
improvement follows from these changes.

November 5 qualification uses Rust 1.95.0, serial Cargo with one build job and the shared
`target`. Native cases run on Windows/DX12. The following suites overlap and must not be added
into a unique test total:

| Scope | Actual result | Nextest run |
| --- | --- | --- |
| Shared text, kernel, widgets, presentation and game UI | 469/469; includes all 55 interaction tests | `312bb407-f907-410a-9bdc-9a84d330bc89` |
| Editor library and selected public integrations | 406/406; 60 ignored/native tests excluded | `aabda47e-44c9-404d-adeb-40a77732117e` |
| Selected native Editor workflows | 33/33; includes original Canvas, field, hierarchy, View and retirement trajectories | `484e1d44-75dc-4374-b507-354a88fe3d57` |
| Independent UI component library | 24/24; view admission, geometry and bounds without hosted-input claims | `d4438bf7-b7d2-445c-960a-caff1ef3813a` |
| Final independent native surface | 2/2; complete 19-presentation/18-commit trajectory plus four error/panic child modes | `af5f42af-f182-4964-b41f-c7c696876a42` |
| Neutral component/native presenter/game adapter dependency closure | 3/3 | `f8db03ba-7c2a-437f-9570-f3fe8de17437` |

The actual Editor binary and final independent native example build. Correctness Clippy passes
for text/kernel/widgets/presentation/game UI/WGPU/Editor with all targets/features and for both
independent consumer workspaces. Existing warnings remain; this is not a warning-free claim.
Root and independent workspace formatting, whitespace and changed-document local links pass.
The renderer implementation is unchanged in this unit; the preceding mesh GPU qualification
keeps its original scope rather than being reported as a new pointer run. The complete R11a
numeric baseline, public panel Host, logical-key/services, graph/history and independent Editor
workflows remain open.

### R11c: Complex document editor, custom drawing and multiple Views

**Dependencies:** R11b, existing `EditorExtensionDocument` lifecycle and selected R8/R9 render
Feature/program/output contracts. Do not wait for a full material system or native multi-window.

**Files:** the component/custom-draw owner selected by R11b, `crates/nara_ui_render/`, current
render Feature/output adapters and planned
`tests/fixtures/editor-ui-host-consumer/tests/graph_document.rs`. Extend actual
`tests/fixtures/render-composition-consumer/tests/composition.rs` and
`crates/nara_render/src/runtime/tests/view_lifecycle.rs` only for changed shared behavior.

**Approach:** use a finite typed node/link document with composable property controls, custom
connections, pan/zoom and a retained preview as pressure for future Shader Graph-like editors.
The document validates links and owns one transaction/Undo/persistence path. The panel owns
transient connection/selection/view state; a compile/preview consumer retains exact source and
request identity. Reuse render Features and immutable outputs for geometry/effects, with explicit
bounds/clip/color/alpha/resource closure. No universal graph, UI-owned compiler or new GPU owner.

**Test scenarios:**

- Create, connect and edit typed nodes; reject an invalid link without mutation; Undo/Redo and
  Save/Reopen preserve the exact graph. Two panel instances retain independent transient state.
- Pan/zoom and clip a curved connection and node control at fractional scale; paint, pointer
  capture and semantic bounds use the same admitted mapping.
- Display two logical Views in one window with independent producer/output/input lifetimes;
  relocate one and reject its old display route while the other continues.
- Complete an old preview request after a newer graph edit or close; retain appropriate last-good
  diagnostics/output without publishing that old result as current.
- Build the custom canvas/connection component outside the engine, using public child
  measurement/layout, draw, pointer motion/capture, wheel/keyboard and semantic contracts.
  Draw a curved path and useful keyboard-operable node/port semantics without adding a
  graph-specific kernel enum. Rejected geometry/resource/capability work cannot publish input.

This is a bounded architectural consumer, not delivery of a full Shader Graph product. Existing
multiple Render Views do not qualify multiple UI Surface Slots on one target; reconcile ADR0108
and the relevant runtime adapter before changing their existing aggregation rules.

### R11d: Public observation and visible-UI operation

**Dependencies:** R11b and the existing exact semantic action/operation outcomes. Graph cases can
join after R11c; the first ordinary field workflow does not wait for a complete graph editor.

**Files:** current `crates/nara_ui_kernel/src/interaction/semantics.rs` and `journal.rs` as reused
contracts, Editor panel/Host observation and input adapters, the existing complete
`tests/fixtures/editor-ui-host-consumer` native consumers and the independent drawn workspace
trajectory under `editor/src/platform/drawn/`. Reuse rather than publicize test-only receipt issuers.

The selected [presented UI operations unit](2026-11-05-presented-ui-operations-plan.md) implements
the first complete ordinary-field consumer through these owners. Its qualification and remaining
R11d requirements are recorded separately; remote transport is not required for this initial port.

**Approach:** expose structured observation and operation through the same production ports for
tests and future agent/debug adapters. Identify presented panel/control incarnation, route,
role/value/state/actions and mapped bounds. Keep domain automation, UI semantic actions and
physical pointer/key/text routing distinguishable; all retain their actual owners and results.

**Test scenarios:**

- Find two same-label controls in different panel/document scopes and operate exactly one.
  Recycle/filter its row; an old observation refuses rather than retargeting the replacement.
- Edit and Apply through the visible controller, then match its original outcome, one Undo and
  exact Save/Reopen. Compare with the independent domain-command path without substituting it
  for the UI test. Verify blocked/disabled/covered pointer input separately from semantic action.
- Wait for one named persistence or preview result and a subsequent matching presented state
  while an unrelated View keeps animating. Exercise timeout/cancellation and stale operation IDs.
- Move keyboard focus into/out of text, cancel composition and close the panel through Host
  routing. Unsupported provider semantics are explicit; no action creates its own input authority.

Public CPU/hosted tests establish operation contracts; native rendering/input, actual OS IME and
assistive-technology support keep their separate evidence. An agent/MCP server or remote-control
transport is deferred; the public test driver is the initial real consumer.

### R11e: Measured reuse, remaining replacement and extension qualification

**Dependencies:** R11a baseline/budgets and R11b-d consumers; reuse selected-panel mechanisms
within their recorded scope. Optimize private layout/cache/render implementations against those
same public consumers. Keep R12's independently authored and native/platform evidence distinct
from maintainer execution.

**Files:** the owning compiler/text/frontend/planner/backend modules identified by R11a;
`editor/tests/ui_work_costs.rs`, shared owner tests, the independent UI/panel/render fixtures and
`docs/verification/native-editor-qualification.md`. Update the current cost record with actual
distributions rather than creating registry snapshots or multiplying benchmark scripts.

**Test scenarios and exit:**

- After warm-up, static subtrees beside active Views avoid repeated description construction,
  layout, shaping and unchanged glyph uploads. Count comparison and composition work honestly.
  Recompose a changed parent while a child with draft/preedit repeatedly hits its cache: retain
  the child's declared state membership and dependency associations, then invalidate it through
  an external result. Actual removal applies its eviction/cancellation policy and refuses the
  old input token. Skipped composition cannot be interpreted as omitted state.
- Local paint/interaction changes preserve unrelated text/layout; font/constraint/DPI changes
  invalidate exactly their dependent work. Failed/rejected work preserves the original route rules.
  Revalidate early cache hits when later phase facts change; regenerate or refuse affected
  output before publication instead of accepting stale facts for correction on the next frame.
- Compare fixed-height range arithmetic and, where the real consumer needs it, estimated/measured
  extents with keyed intra-row scroll anchoring and changed-size invalidation. Attribute dense
  metadata, filter/order/height-index rebuild and visible/overscan/pinned rows separately. A
  geometry hit or 40 live nodes is not proof of total O(visible) work. Compare unchanged output
  reuse before deciding whether persistent layout storage pays for its lifetime complexity.
  Invalidate stale measurement records themselves before rebuilding the height index. Change
  an offscreen same-key row's content/width/typography, reorder, then scroll back and verify
  actual height, the keyed intra-row anchor and original input association together.
- Live collection work scales with visible/overscan/pinned-edit rows. Attribute model refresh,
  filtering and index construction separately; clipping alone is not virtualization.
- Actual retained CPU/GPU bytes plateau through repeated attach/close/rehost and device recovery;
  output resources and original completion obligations retire through their existing owners.
  Also keep a bounded collection mounted while repeatedly replacing its keys; historical
  measurement storage must obey its declared bound/eviction policy and plateau without unmounting.
  Charge pinned editing state separately rather than discarding its original owner to meet a bound.
- Meet R11a's fixed numeric budgets in the selected workloads. Report cold/warm distributions,
  P50/P95/P99 scope, cache hits/misses/evictions, allocation bytes, invalidation reasons, copies,
  submissions, refusal behavior and diagnostic overhead. Missing measurements keep this unit open.
  Distinguish a cold geometry index with retained measurements from fully cold measurement/
  snapshot state. The [pinned GPUI follow-up](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md#november-5-phase-validity-and-measurement-retention)
  supplies these additional cases as source-derived requirements, not new runtime qualification.
- Run the complete agreed incumbent workflow matrix in the independently runnable self-drawn
  Editor: project/document navigation, ordinary and specialized authoring, commands/history/
  persistence, Browser/import, Scene/Game Views, workspace layout/restore, overlays/focus/text,
  diagnostics, cadence, recovery and physical close. Compare the same domain outcomes and native
  behavior against the exact named reference; document reference gaps instead of inventing parity.
- Prove the successor's selected dependency/build/run closure needs no Dear package or initialized
  context, renderer, docking/input service or migration-only image-panel bridge. Shared domain,
  Host and render services retain their owners. This does not prohibit rendering Scene/Game
  images or optional user-authored frontend embedding.

Implement specialized workflows and workspace incrementally inside the independent successor.
Switch the default composition after complete acceptance, then remove the first-party reference
and migration-only mixed paths. Optional product embedding keeps its own admission/evidence.
Component breadth and native detachment retain their named workload/platform scope.
Future font/effect/component work may change private algorithms without exposing their storage or
reopening document, Host or render ownership. Full R11/R12 remains open until its complete scope
has matching evidence; this design update neither reopens qualified units nor claims the successor.

#### Source Hierarchy matched reuse unit

The next selected consumer is the existing complete ordinary source-declaration Hierarchy,
including filter/selection, Parent/Order choices, removal, exact action results and history/source
preservation. Repeat the current 10,000/100,000-row read-only projection probes at the current
source before changes, and add real admitted 128/1,024-entity Application consumers. Keep their
editing/native evidence separate from synthetic projection scale. Reuse stays private to the
existing controller; no public cache key, universal virtualizer or model/history owner is added.

Use exact immutable Editor-frame identity for source projection reuse, with explicit local paint
invalidation and matching environment/theme/read-only facts. A newly published model conservatively
refreshes the projection even when the document revision is unchanged. Selection, Schema, source,
permissions, changed row metadata/order and closed/retired action controls retain their original
revalidation. Auxiliary frame-enrichment methods must be inspected before treating its nominal
revision as proof of any consumed property. Cache hits confer no presentation/input authority.

Predeclared acceptance at the existing hardware/toolchain/font/viewport is: after warm-up, 512
unchanged calls construct zero descriptions, visit zero source/filter rows, perform zero compiler
work and allocate zero current-thread Rust bytes. Their per-process P99 target is 25 microseconds
for three ordinary release processes; report every process and background-load limits. Scroll
under the same immutable projection must avoid full-source/filter scans and retain fewer than
128 live layout facts at the 700-by-800 viewport. Changed committed filters may scan all logical
rows once; unchanged filter/scroll may not repeat that work. Retained filtering indexes are
explicit dense logical-row metadata with at most `row_count * size_of::<usize>()` requested
capacity plus the bounded filter text, separate from visible controls and retained frame payloads.
New models and failed candidates retain their separately charged conservative paths. General
cross-frame dependency reuse, variable-height lists, whole-process/GPU memory and OS input latency
remain in the full R11a/e matrix.

The implemented private `SourceProjection` captures the exact frame/read-only basis once and
retains one committed filter's matching source indices. Empty queries and Parent/Order choices
derive direct indices. Local input/selection/focus/preedit, scrolling, modes, error/result and
observation changes dirty the painted artifact independently. Same-frame unchanged calls return
before basis cloning, description construction or compiler entry; new model publication always
refreshes conservatively. The original action and queue boundaries still recapture their exact
basis rather than accepting cache identity as authority.

The [matched report](../benchmarks/2026-11-04-ui-work-baseline.md#november-5-matched-source-hierarchy-reuse)
records three current-source before processes, the protected final three-process release probe,
independent allocation observations and limits. All final 128/1,024 admitted and 10,000/100,000
read-only warm distributions meet the 25-microsecond P99 target, reporting a quantized 0.1
microsecond P50/P95/P99. Input/output optimization barriers preserve that observation; timer
granularity prevents an exact sub-microsecond or before/after speedup claim. Work counters add
zero descriptions, authored nodes, source/filter visits or compiler work across each 512-call
group. Separate current-thread allocation windows add zero allocation/reallocation/free requests;
construction, model publication and native/background allocation are outside those windows.

Four new hosted regressions cover source and choice scrolling, committed-filter/preedit reuse,
same-document-revision changed rows/old controls, and local field/error redraw. Existing tests
retain real commands, queue refusal, history, source and Save/Reopen. Editor/public 410/410 and
selected actual Windows/DX12 workflows 5/5 pass; the equivalent final choice-parity spelling is
covered by a later 37/37 owner/adapter run. Build, correctness Clippy, formatting and changed-doc
checks pass. This qualifies the selected same-frame Hierarchy unit. Running Game publishes new
models, so cross-frame dependency reuse, full R11a budgets and R11b-f remain open.

### R11f: Editor extension edit/build/reconstruct

**Dependencies:** R11b attachment/state scopes, R11d matching presentation observations and
existing build, persistence and physical-close owners. This is distinct from R12's Play checkpoint
workflow. Exact source-built composition and recovery types follow the bounded experiment;
no native ABI or process supervisor framework is frozen by this plan.

**Files:** the actual Editor launch/composition and build/close/persistence owners selected by
the experiment, planned `tests/fixtures/editor-ui-host-consumer/tests/editor_iteration.rs` and
the corresponding native trajectory. Reuse existing Cargo artifact/diagnostic handling and task
custody; do not add a UI-specific executor, language interpreter or compiler-analysis verifier.

**Approach:** first deliver an explicitly started development workflow that builds a candidate
Editor, captures consistent supported owner/UI recovery data, supervises Editor restart and
reports restoration plus correct presented UI. Keep document owners unchanged in the first UI
code experiment. Separate saved content/checkpoint/history, workspace preferences, opt-in
versioned component state and disposable interaction/resource state. Unsupported dirty owners,
incompatible drafts or active publication refuse/defer before losing the old state. No implicit
Save/Discard or history reset is an acceptable substitute. A broader document-owner-code update
needs its own compatibility/reconstruction evidence.

Capture happens against the actual current state after candidate construction; revalidate its
source/owner basis before handover. Follow existing Save/Close decisions and physical retirement,
without granting old and new processes concurrent source-publication rights. Keep a last-good
artifact and usable recovery capture until restore/presentation succeeds. Failure after teardown
may relaunch that artifact; it does not promise uninterrupted windows or transactional rollback.
Fresh incarnations reject every predecessor route/result; OS IME/capture, code pointers, tasks and
GPU handles are never deserialized as live objects. Native function patching or reloadable data
descriptions are optional later accelerators with separate supported change classes.

**Test scenarios and exit:**

- Edit an independently compiled panel's layout/control behavior; the explicit workflow builds
  changed source and presents the new behavior automatically. GPU reconstruction or unchanged
  executable Restart is not this test.
- Preserve one dirty Scene and one inactive document's declared compatible content/checkpoint/
  history, plus panel placement and opted-in drafts. Undo and subsequent Save/Reopen match the
  chosen owner oracle. Refuse unsupported owner/state versions without losing the active state.
- Fail compile/link and retain the current usable Editor plus diagnostics. Fail capture, restore
  and startup separately, and prove the exact last-good recovery route without publishing an
  unqualified candidate or writing a recovery snapshot into project source files.
- Change component state shape; supported codec migration is explicit, otherwise preserve a
  recoverable dirty draft or refuse. Cancel composition/held input, physically retire old scoped
  callbacks/tasks/resources and reject delayed old-generation results after reconstruction.
- Change only drawing/behavior code while keeping state and layout equal. Invalidate code-derived
  caches/registrations so the new presentation and action run the candidate implementation.
- Record hardware/toolchain/change class, P50/P95 change-to-correct-result and separate compile/
  link, capture, retirement, startup, restore and first-presentation costs. Set numeric budgets
  from the baseline before optimizing; include repeated-run retained memory and recovery costs.

This workflow is unimplemented. Neither optional scripting nor universal native hot reload is a
prerequisite. Completion requires the actual independent/native path and its failures, not a file
watcher or a successful build log; full R11/R12 remains open.

## R12: Consumer Integration And Canonical Contract Consolidation

**Design:** D01, D18. **State:** consumer integration accompanies the qualified owner units;
the registry ownership sub-unit is qualified; the complete production/consolidation outcome remains open.
**Owners:** affected modules and integration owner.

For the selected completed units, run independent direct/renamed/public consumers and relevant
root/Editor/native paths serially. Verify unknown data, exact operation results, content/identity
retention, close and old-generation rejection together. Review adapters, not only owner unit tests.

R11b-d add public panel/hosted-UI and observation/operation consumers to that integration. Compare
UI, semantic-command and future agent use through the same domain owners and exact outcomes.
Keep capability coverage explicit per frontend: a semantic snapshot does not prove pointer hit
testing, OS IME, native accessibility or third-party toolkit compatibility. The independently
authored task below can later exercise a user-authored adapter without making an official ImGui
extension or agent server part of this refactor's current delivery.

Name the Rust capture/rebuild/restore successor explicitly. The game owns checkpoint data and
compatibility; supervised Play owns build, immutable executable generation and process lifecycle.
Join body and structural edits to restoration of the same meaningful playable situation, including
capture/build/restore failures. Reconcile ADR 0116's current one-READY launch protocol before
adding post-launch capture exchange; ADR 0093 already admits reliable rebuild/restore. Preserve
saved-source receipts and exact last-good scope. Unchanged Restart is not recompilation, and
successful launch is not the correct playable-result endpoint.
R11f separately owns Editor extension reconstruction and its document/UI recovery oracle; neither
workflow can substitute for the other one's compatibility or correct-result evidence.

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
