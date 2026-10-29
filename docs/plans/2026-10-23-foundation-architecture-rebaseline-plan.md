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
R8 render composition + R9a View / color / captured-time facts qualify together
R6 shader inputs + selected R8 seam -> R9b durable shader/material resources
R9c retained geometry is a separate measured mechanism
R4c + motion/input consumers -> selected R10 domain / repeatability qualification
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
It challenges unconditional sharing for interactive progress; actual Editor lifecycle/deployment
qualification and R4c remain open.
**Owners:** App/Tasks, executable Host and domains.

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
   Preserve one driver and whole successful fixed-tick finalization; reject bad topology before startup.

Acceptance: two independent scopes, independent close, saturation/cancel/late/sub-work,
uncooperative worker, partial startup, local SDK controller and close cycles/missing dependencies.
Measure admission-to-start, interactive latency and worker/facility multiplicity; do not repeat
old worker counts as new performance evidence. Fix earlier F4 Stop observation retention using
an explicit retained/expired result policy, not a larger unexplained ring.

Replace 0008's long-term model only with a complete successor; amend 0003/0010/0042/0052/0082/0084.
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

**Design:** D10, D11, D13. **State:** source/import ownership and the compound continuity/Runtime
format slice qualified; catalog delivery and durable artifact store remain open.
**Owners:** asset/domain contracts and project Host.

Build one independent compound importer through typed source, Runtime decode, optional cook and
Editor facets; separate format contracts from root/2D dependencies. Choose authored continuity
for subproducts without intrinsic IDs. Then implement local recipe-index/group-manifest/CAS
publication with tracked inputs; identity does not come from generated cache or byte hashes.

R6a is the facet/continuity/verified Runtime-product contract. R6b adds the durable recipe index
and local CAS; it does not block an independently qualified R7 immutable base catalog.

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

**Design:** D12. **State:** in progress; immutable local capture/acquisition qualified,
first-party packaged Host and target-build selection remain open.
**Owners:** Cargo tooling, Host capture/cook and Runtime loader.

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

Acceptance: two clean content builds, a different host/target selection, source/cache inaccessible
and no Cargo on the runtime machine, server stripping, wrong target/schema/digest rejection,
failed candidate retaining last-good executable/data. State non-hermetic inputs and actual
source-capture consistency. Foreign-target build-only is not native execution evidence.

Delete implicit profile-name-to-Cargo policy and code/source/package generation conflation.

## R8: External Render Feature And Host-Local Execution

**Design:** D14. **State:** not started. **Owners:** render semantics/domain adapters/WGPU Host;
Proposed 0121.

The preferred complete design is typed Features over an explicit resource-dependency graph, with
runner-owned GPU execution. Named slots are convenient placement that lowers into this same model.
Design for shared/branching intermediates, multiple views, custom Editor composition, offscreen
targets and recovery now; do not select an initial linear core merely because one effect is small.
Use pinned Bevy and Godot as primary architecture evidence without copying a historical graph API.

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
sampled/attachment/storage/copy use and legal read-write distinctions. Conservative whole-resource
tracking is an initial support choice, not an API assumption that every operation exchanges color.
Add a small compute luminance-to-storage-buffer/custom-drawn-histogram consumer to challenge that
model before interface freeze, rather than merely reserving a future enum name.

Represent history as an import of a previous producer/View/Feature-instance/history-slot version,
not a cycle in the current graph. Define initialization, producer-output publication/advance and
invalidation for resize, camera/policy changes and device loss. Re-presenting the same captured
Scene image must not advance history. Per-capture graphs and Host composition must let independent
streams progress without requiring every World to supply a fresh frame in lockstep.

GPU Host is runner-local. Distinguish stream/view incarnations, captures, policy revisions, target
generations and Host/device epochs through existing owners. Close requirements before device
creation; late requirements must fit the enabled device or explicitly request reconstruction while
other streams continue. Qualify window-free GPU initialization without a dummy window. Stream close
stops admission, terminalizes pending UI work and retains its submission prefix through physical
completion; whole-Host queue-empty drain is not a per-stream close requirement. Bound pending and
in-flight work per stream and cumulatively per Host drive. Preserve supervised ordinary project
Play; local GPU sharing does not authorize arbitrary project Rust inside the generic Editor.

Acceptance: Runtime/Scene View/offscreen, intermediate textures, independent renderer policy,
two producer streams/two windows/image target, one-stream close, old packets and device loss.
Reject cycles/uninitialized input and unauthorized acquire/submit/present. Generated/history
resources reset, external ones reacquire; multi-window present is not globally atomic.
Add a clipped Nara-owned drawing tracer with a live Scene image and overlapping control/popup.
It validates image import, clip order, resize/DPI and exact composite/input joins through this Host,
without requiring the Editor to inherit Runtime UiFrame primitives or finishing the R11 toolkit.

The [rendering source/design review](../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
records the comparison, pinned upstream evidence, failure cases and staged deletion. It is research
evidence; ADR 0121 stays Proposed and no new rendering qualification follows from this amendment.

No mandatory render thread, RHI descriptor mirror, aliasing compiler or resurrection of all 0077.
Delete central package-specific payload/match growth and tests prohibiting any public topology.

## R9: Material Artifacts, Views, Color And Domain Render Locality

**Design:** D15, D16. **State:** not started. **Owners:** shader/material/view owners and domain projection.

Turn the palette consumer into durable typed parameters/references plus tracked WGSL/WESL shader
artifacts. Derive one inspectable binding model rather than maintaining Rust and dynamic metadata
separately. Introduce shared View envelope with typed projection, explicit transfer/alpha and
captured time; investigate distinct linear/sRGB types as an unreleased breaking change.
Keep persistent material meaning distinct from its generated binding ABI and shader interface.
Uniform values and ordinary texture-content changes do not create pipeline variants. Initially
qualify two admitted layouts and unchanged-layout shader editing. Before interface freeze, design
shader-authored layout admission into the same material/binding model, including authored stable
field identity; do not make Rust type identity the persistent layout model. Broader prebuilt-Editor
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

Acceptance: second material layout, shared shader-dependency invalidation, compile failure keeping
same-epoch last-good, pipeline variant accounting, drawing/picking inverse/DPI/mirror tests,
transparent offscreen composition readback and bounded 3D depth/unit sample. Do not claim 3D support.

Compare bounds-only and domain-local immutable geometry blocks on representative map/view
workloads. Dirty hints alone are not authority; old packet captures remain immutable. Measure
actual lowering/queue/upload cost before selecting retention. Keep 0053/0077 historical rather
than requiring their public topology.

## R10: Motion, Time, Repeatability And Concrete Domain Seams

**Design:** D09. **State:** not started. **Owners:** concrete Physics/Animation/Audio/Network integrations.

Implement a bounded motion consumer with root-motion intent, kinematic target, dynamic solver
pose, parent edits and interpolation. Define query/callback freshness and temporal authority.
Add a minimal audio semantic-effect consumer with sample-time and shutdown rules. Do not invent
generic physics/audio providers or enforce a World-wide writer ACL.

Acceptance: pause/exact-step/teleport/reparent, fixed versus presented snapshots and correction
cut. Define a closed-input same-build repeatability profile with explicit RNG/outcome policy;
different frame grouping and worker completion orders produce the selected authoritative checksum
or a truthful unclassified-input refusal. A bounded prediction tracer establishes resimulation
and effect-suppression seams separately from debugger fresh restore. No cross-platform bit-exact,
complete network stack or arbitrary VM snapshot claim.

The input sub-unit compares global maps, seat-scoped maps and per-device callbacks. Prefer
seat/action/context aggregation with explicit map revision and device-connection generation.
Qualify a keyboard seat and gamepad/axis seat using same-name actions, focus/capture transfer,
disconnect/reconnect and rebinding while held. Raw pipeline replay pins normalization/bindings;
semantic replay records stable admitted actions at the application tick and must not change
meaning after a physical rebind. Reject incompatible vocabulary, never persist OS handles, and
do not conflate durable game player identity with a local input seat. Amend 0041 for selected rules.

## R11: UI Content Association, Text Admission And Platform Qualification

**Design:** D13, D17. **State:** not started. **Owners:** UI compile/presented sessions, text and frontend.

The user clarified the destination: **Nara's own custom-drawn Editor UI**, not embedding a
reference toolkit. The [source study](../research/2026-10-27-custom-drawn-editor-ui-boundaries.md)
informs the [owned runtime direction](../architecture/foundation-architecture-reassessment.md#updated-custom-ui-and-language-research).
Preserve current semantic owners and use Dear's existing workflow behavior as the replacement baseline. Design
Nara-owned controls, identity, interaction, layout and drawing. Mature text/layout primitives
may be reused without importing a complete foreign UI runtime. No third-party embedding
experiment or demonstration that adoption fails is required first.

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

Qualify semantic draft/basis/result reuse and separate toolkit edit buffers. Test Unicode/IME
composition, focus, keyboard routing, DPI and actual affected native detach/accessibility seams.
Pure semantic tool delivery is not blocked on unrelated whole-toolkit parity, and Runtime
accessibility is not evidence of Editor assistive-technology support.

Delete duplicate state machines and ambiguous theme identity. A source study alone does not
justify deleting the working frontend or claim its native capability has transferred.

## R12: Consumer Integration And Canonical Contract Consolidation

**Design:** D01, D18. **State:** not started. **Owners:** affected modules and integration owner.

For the selected completed units, run independent direct/renamed/public consumers and relevant
root/Editor/native paths serially. Verify unknown data, exact operation results, content/identity
retention, close and old-generation rejection together. Review adapters, not only owner unit tests.

The [Scene/runtime extension review](../audits/2026-10-27-scene-runtime-and-extension-plan-review.md)
adds bounded consumer-driven cleanup: verify whether the Schema-first registry still needs its
embedded Bevy TypeRegistry; exercise observer/Event authoring through renamed dependencies before
expanding facade-aware derives; and make unbound preparation, staged replacement custody, asset
reservations and current-live export naming match their actual workflows. Remove unused storage
or entry points only after tracing real consumers. These follow-ups do not block the current
closed Scene data-family implementation or require a generic extension framework.

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
