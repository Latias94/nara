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

### Current First Unit

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
  submit through a small current Dear ImGui contribution and a window-free public-session test.
  Do not wait for generalized queries, mixed transactions, full Nara UI or dynamic packages.
- Carry exact action/request feedback to the originator through Application and tool adapters.
  Keep dispatch, persistence/Play completion and supplemental diagnostics distinct.
- Resolve the earlier F1 Application feedback, F2 persistence correlation and F3 domain-diagnostic
  transport findings at their owning boundaries; do not manufacture a disk-loss claim for F2.
- Follow with a finite explicitly enabled child-process tool returning a proposal to the same
  owner, only after its bounded input/output and trust are stated. No sandbox or stable wire SDK
  claim follows from it.

**R1b placement and local lifecycle** is a separate deliverable beyond that proposal experiment.
Use the same useful command plus inline Inspector customization to compare source-built companion,
replaceable semantic tool Host and a narrowly trusted native SDK. Establish selection from native
authority, author concepts, update/diagnostic/recovery behavior and measured calls/latency, without
implementing three production SDKs. The selected ordinary-tool path then completes local install,
update, enable/disable and remove under the phased Package contract, including required dependent
closure, stale proposals and modified/adopted file preservation. A panel alone cannot close this.

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

**Design:** D02, D03, D06. **State:** private representation comparison executed; production
representation, language workflow and replacement remain open. **Owners:** Schema, Scene materialization,
concrete language adapter and executable Host; Proposed 0120.

**R3a representation:** compare an explicit attachments envelope and owner-managed rows with one
bounded adapter and two distinct Behavior Schemas, not two Rust component types. Do not first
build each representation in each language. Runtime-independent address semantics come from
R5a; final Prefab compatibility waits for the selected R5b overlay, not qualification of a
known-to-be-displaced operation-patch format.

The [executed comparison](../research/2026-10-27-behavior-representation-comparison.md) selects
the data-only entity envelope provisionally for R3b. Rows reduce inspected records for selective
queries but require explicit cleanup on ordinary destruction. VM instance/code custody and real
dispatch costs remain unproved and may reverse the selection. The private fixture uses hidden
authoring integration, not a public Runtime representation; its transformed version-two data is
not an owner-catalog migration transaction.

**R3b language workflow:** fix the representation enough to compare the same gameplay task with
C# and Lua feasibility adapters. Choose a production candidate on author workflow, targets,
lifetime and measured toolchain cost. This is not a commitment to maintain both SDKs.

**R3c replacement:** qualify only the modes needed by the selected owner, after its native
completion/retirement proof in R4a. Defining coexist-and-switch and Stop-first semantics does not
require every Host to implement both. Shared facilities and general phase-plan work are not
automatic prerequisites to the fresh-Runtime representation trial.

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

The next R5b unit follows the [declarative Prefab overlay design](../architecture/prefab-overlay-design.md):
first compare real root-patch lowering with preserved leaf intent privately, then settle nested
targets, local additions, structural placement, keyed identity changes and value-reference
bindings before freezing the complete replacement. Separate durable intent from exact editor
operation/inverse patches and retain unresolved migration/rebase conflicts. Reconcile owning ADRs
before the production format switch; a scalar comparison does not complete that replacement.
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

Delete operation-patch-as-long-term-overlay and accidental source/runtime identity conflations.

The [complete-conversion plan](2026-10-27-prefab-conversion-plan.md) now has a
[qualified Source/Editor successor](../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md):
selected compiler demand, semantic data retention, fixed local-ID/reference remap, exact source
history, current Host dependency fences and selected native interaction. This closes direct
selected-instance complete conversion, not outer-only unpack, cross-source Apply or full R5.

## R6: Resource Facets, Durable Continuity And Local Artifact Store

**Design:** D10, D11, D13. **State:** not started. **Owners:** asset/domain contracts and project Host.

Build one independent compound importer through typed source, Runtime decode, optional cook and
Editor facets; separate format contracts from root/2D dependencies. Choose authored continuity
for subproducts without intrinsic IDs. Then implement local recipe-index/group-manifest/CAS
publication with tracked inputs; identity does not come from generated cache or byte hashes.

R6a is the facet/continuity/verified Runtime-product contract. R6b adds the durable recipe index
and local CAS; it does not block an independently qualified R7 immutable base catalog.

Acceptance: clear-cache stable references, rename/reorder/ambiguity, dependency/settings/target
drift, member corruption, cache explanations and every group publication fault point. Preserve
old admitted groups and retained frame/content leases; deterministic content manifests are not
bit-identical executable evidence. AssetServer Clone/shared database refactoring (earlier F6)
must preserve database snapshot semantics and be measured, not assumed faster.

No generic erased importer executor, global semantic budget manager, remote cache or automatic
native trust. Delete duplicate root/domain registrations and incorrect recipe/content type reuse.

## R7: Target Build And Base Source-Free Delivery

**Design:** D12. **State:** not started. **Owners:** Cargo tooling, Host capture/cook and Runtime loader.

Separate Host tool selection, target executable artifacts, content cook, release candidate and
target-runner activation. Keep a source-carrying option but move fixed starter assumptions into
its adapter. Build a verified immutable base Runtime catalog; directory transport is sufficient.
Do not block it on patch/DLC, remote authenticity or full cross-file Editor recovery.

Acceptance: two clean content builds, a different host/target selection, source/cache inaccessible
and no Cargo on the runtime machine, server stripping, wrong target/schema/digest rejection,
failed candidate retaining last-good executable/data. State non-hermetic inputs and actual
source-capture consistency. Foreign-target build-only is not native execution evidence.

Delete implicit profile-name-to-Cargo policy and code/source/package generation conflation.

## R8: External Render Feature And Host-Local Execution

**Design:** D14. **State:** not started. **Owners:** render semantics/domain adapters/WGPU Host;
Proposed 0121.

Compare named slots, typed logical resource plan and open render schedules on the same independent
two-pass effect. Prefer typed Feature inputs/dependencies if they close composition without a
new GPU compiler. Move stock Sprite/UI encoding behind that same seam. GPU Host is runner-local;
captured producer streams, policy selection and whole-Host authority remain distinct.

Acceptance: Runtime/Scene View/offscreen, intermediate textures, independent renderer policy,
two producer streams/two windows/image target, one-stream close, old packets and device loss.
Reject cycles/uninitialized input and unauthorized acquire/submit/present. Generated/history
resources reset, external ones reacquire; multi-window present is not globally atomic.

No mandatory render thread, RHI descriptor mirror, aliasing compiler or resurrection of all 0077.
Delete central package-specific payload/match growth and tests prohibiting any public topology.

## R9: Material Artifacts, Views, Color And Domain Render Locality

**Design:** D15, D16. **State:** not started. **Owners:** shader/material/view owners and domain projection.

Turn the palette consumer into durable typed parameters/references plus tracked WGSL/WESL shader
artifacts. Derive one inspectable binding model rather than maintaining Rust and dynamic metadata
separately. Introduce shared View envelope with typed projection, explicit transfer/alpha and
captured time; investigate distinct linear/sRGB types as an unreleased breaking change.

R9a View identity/target/transfer/alpha/captured-time facts are designed with R8's external pass,
not deferred until the complete asset/cache work. R9b is durable shader/material preparation and
dependency invalidation. R9c is optional measured geometry retention, not shared protocol admission.

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

Repair exact theme revision/content association and Canvas/Runtime conflict admission (earlier
F7/F8). Move glyph preparation reservations ahead of raster allocation where the model can prove
them (F9), while distinguishing modeled bound from arbitrary allocator isolation. Keep terminal
and last-presented behavior explicit on candidate rejection.

Qualify semantic draft/basis/result reuse and separate toolkit edit buffers. Test Unicode/IME
composition, focus, keyboard routing, DPI and actual affected native detach/accessibility seams.
Pure semantic tool delivery is not blocked on unrelated whole-toolkit parity, and Runtime
accessibility is not evidence of Editor assistive-technology support.

Delete duplicate state machines and ambiguous theme identity, not the selected working frontend.

## R12: Consumer Integration And Canonical Contract Consolidation

**Design:** D01, D18. **State:** not started. **Owners:** affected modules and integration owner.

For the selected completed units, run independent direct/renamed/public consumers and relevant
root/Editor/native paths serially. Verify unknown data, exact operation results, content/identity
retention, close and old-generation rejection together. Review adapters, not only owner unit tests.

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
