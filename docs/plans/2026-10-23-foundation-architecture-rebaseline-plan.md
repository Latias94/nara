---
title: Foundation Architecture Rebaseline And Fearless Refactor
date: 2026-10-23
status: Design baseline recorded; executable units not started
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

This is not a requirement to finish the whole engine before any extension can run.

```text
R0: design and scope reconciliation
  -> R1: minimal semantic tool / exact outcomes
  -> R2: prepared document editing / queries
  -> R3: Behavior representation / module iteration
  -> R4: automatic execution / scopes / native close

R2 + R3 -> R5: durable addressing / Prefab / projection lifetime
R0 -> R6: resource facets / continuity / local artifact store
R6 -> R7: target code / base source-free delivery
R0 -> R8: render composition / Host execution
R6 + R8 -> R9: shader/material / view/color / render projections
R4 -> R10: motion / time / repeatability / bounded network-audio seams
R1 + owning changed UI seam -> R11: UI/text/input qualification
selected changed units -> R12: final consumer integration / canonical documentation
```

R1 can run on current APIs before R2. R3 can use fresh-Runtime replacement before proving narrow
module replacement. R4's close/scopes and phase-plan comparisons are independent sub-units.
R8's resource-plan experiment can start without CAS or complete material assets. R7 needs a
bounded immutable source capture, not the full R5 projection or multi-file recovery design.
Order is practical default priority; these edges identify real dependencies, not authorization.

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

**Design:** D13, D17. **State:** not started. **Owners:** existing tooling/session, selected frontend
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

Acceptance: panel/session parity, exact result, one Undo, Save/Reopen, rejected/stale proposal,
delayed completion after newer requests, multiple observers and bounded structured diagnostic
projection. Do not call a panel example inline Inspector or dynamic-package qualification.

Delete duplicated validation/result inference in consumers; no second Undo or document store.

## R2: Owner-Owned Prepared Editing And Selection-Independent Queries

**Design:** D04, D17. **State:** not started. **Owners:** Scene/Prefab authoring and tooling adapters.

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

## R3: Behavior Representation And Immutable Module Publications

**Design:** D02, D03, D06. **State:** not started. **Owners:** Schema, Scene materialization,
concrete language adapter and executable Host; Proposed 0120.

Compare explicit attachments envelope and owner-managed rows with two distinct Behavior Schemas,
not two Rust component types. Exercise one complete authoring/gameplay task with C# and Lua
feasibility adapters; choose a production candidate only on author workflow, targets, lifetime
and measured toolchain costs. This is not a commitment to maintain both SDKs.

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

**Design:** D07, D08. **State:** not started. **Owners:** App/Tasks, executable Host and domains.

Three independently verifiable sub-units:

1. Compare expanded public sets, a sealed automatic phase plan and `bevy_app::App::empty()`
   using independently authored Physics-like/Animation-like flows. Preserve one driver and
   complete fixed-tick success before Ack; reject bad topology before startup.
2. Compare lazy per-Runtime facilities with Host facilities plus logical scopes. Scope close
   proves owned work/disposal return without claiming shared worker join. Keep frame access-aware
   execution, blocking work, Future I/O and real-time callbacks distinct.
3. Compare typed composite close owners with a minimal sealed completion dependency plan.
   Support owner-local participants and separate quiesce from destructive release. Pending or
   physically incomplete failure blocks provider release; failed-but-complete retains errors.

Acceptance: two independent scopes, independent close, saturation/cancel/late/sub-work,
uncooperative worker, partial startup, local SDK controller and close cycles/missing dependencies.
Measure admission-to-start, interactive latency and worker/facility multiplicity; do not repeat
old worker counts as new performance evidence. Fix earlier F4 Stop observation retention using
an explicit retained/expired result policy, not a larger unexplained ring.

Replace 0008's long-term model only with a complete successor; amend 0003/0010/0042/0052/0082/0084.
Delete extra physical owners and accidental hardcoded phase knowledge when superseded.

## R5: Durable Addressing, Prefab Overlays And Projection Lifetime

**Design:** D04, D05. **State:** not started. **Owners:** identity/Schema/Scene and preview Host.

Separate source identity, structured projection chain, durable property and runtime incarnation.
Select named nested member identity and keyed-item addressing; unkeyed list positions remain
revision-local. Separate declarative Prefab overrides from exact editor operation/inverse patches;
retain unresolved migration/rebase conflicts. Qualify ordinary incremental projection versus
epoch reconstruction, not a generic incremental graph.

Acceptance: nested and repeated instances, base rename/reorder/delete, same-name members,
Behavior migration, convert-to-local and explicit remap. Long edit churn preserves stale-alias
protection without silently exhausting lifetime identity. Amend 0058 before changing classification:
bounded history returns Expired/HistoryUnavailable, while checkpoint history has an explicit
owner. Test attempted readmission of a formerly Deleted identity after history expiry: refuse
without the required evidence or use an explicit distinct epoch/timeline contract, never silently
resurrect it. No high-watermark claim for arbitrary history. This resolves earlier F5 as a lifecycle
redesign, not by clearing tombstones or changing Scene data into World truth.

Delete operation-patch-as-long-term-overlay and accidental source/runtime identity conflations.

## R6: Resource Facets, Durable Continuity And Local Artifact Store

**Design:** D10, D11, D13. **State:** not started. **Owners:** asset/domain contracts and project Host.

Build one independent compound importer through typed source, Runtime decode, optional cook and
Editor facets; separate format contracts from root/2D dependencies. Choose authored continuity
for subproducts without intrinsic IDs. Then implement local recipe-index/group-manifest/CAS
publication with tracked inputs; identity does not come from generated cache or byte hashes.

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
