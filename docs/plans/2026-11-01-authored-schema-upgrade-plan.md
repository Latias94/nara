# Reviewed Authored Schema Upgrade

**Status:** Implemented and qualified for the bounded CP1/S2 consumer, including live
Session/Application adoption, fresh-process interruption recovery and fixed-v2 Check/Play.
The [qualification](../knowledge/engineering/verification/2026-11/2026-11-01T203621Z-reviewed-authored-schema-upgrades-and-fresh-process-recovery-qualification-cf7c8ecab10c4e21b4cfcff4906805de.md)
records exact cohorts and platform limits. CP2 remains a separate successor.
**Parent:** [Foundation R5](2026-10-23-foundation-architecture-rebaseline-plan.md#r5-durable-addressing-prefab-overlays-and-projection-lifetime).
**Scope:** CP1's coordinated value/claim evolution and S2's process-interruption recovery. CP2
cross-source extraction remains a separately qualified successor. This is not another roadmap.

## Complete Consumer

Extend the existing independent EnemyStats project used by
[creation-metadata evolution](../../tests/fixtures/behavior-languages/tests/support/installed_graph/creation_metadata.rs).
Use two Scenes, nested and repeated Prefab instances, an active Scene, an inactive dirty Scene
and an inactive dirty Prefab. Include a dormant declaration and an unaffected unavailable owner.

Change `trial.enemy.EnemyStats` from component version 1 to 2: represent whole-point `health`
as a closed Record containing identified `milli: I64`, using checked multiplication by 1000.
For example, `wire_health = 105` becomes `wire_health = { milli: 105000 }`. Retain the stable
root health identity; explicitly map its old scalar selector to the identified new member.
Keep `armor`'s identity, value and selector unchanged. Advance the owner catalog by one generation
with the exact actual predecessor. The existing creation-only successor is a different operation.

The public Session/Application workflow prepares the upgrade, shows the complete participating
set and changes, confirms, publishes and adopts it, then performs a new ordinary edit/Undo,
Save/Reopen and the supported Check/Play path. Use the existing frontend and window-free tools;
this unit does not claim a newly drawn upgrade dialog or the future custom Editor.

## Selected Direction To Challenge

The first implementation saves the reviewed participating working copies and establishes a
new per-document history checkpoint only after complete successful adoption. Review explicitly
reports those effects before publication. Preparation, cancellation and refusal before the first
replacement preserve both old history branches and their checkpoints.

Migrating retained history would require interpreting each old edge and inverse under its original
Schema, transforming complete snapshots and expected values, and proving every transformed edge
and its cost. Merely replaying old operations against a new Schema does not establish their meaning.
That alternative remains a separate consumer rather than a silent promise of this checkpoint.

Successful adoption preserves source keys, entity identity, tabs and compatible selection. It
creates new authoring revision/content-state identities and invalidates old queries and prepared
edits. A history checkpoint is distinct from a persistence checkpoint: saved state requires actual
matching file observations/publication evidence. Process recovery need not reconstruct arbitrary
pre-crash Undo/Redo history.

## Transformation And Scope

Use a project-owned pure Rust upgrade command beside the existing catalog producer for the first
consumer. It explicitly computes a data proposal. Catalogs, Adapter receipts and recovery records
carry no executable migration authority; ordinary generic Open does not run the command. Do not
add a migration DSL, universal callback registry or new binary ABI.

Bind the proposal to the original project/root/manifest/bundle observations, actual old catalog,
explicit new catalog, nominal `AuthoringSchemaBasis`, and every source's identity, binding,
revision/content state, old disk digest and reviewed working copy. Prepare fresh Adapter evidence
through its actual owner for the new catalog. Equal Schema bytes alone do not reproduce an old
nominal basis or its execution/adaptation authority.

Discover the bounded project source set from one captured manifest's default configuration and
every declared profile, because those configurations may share the upgraded bundle. Union their
Scene/Prefab roots, explicit startup/content Scenes, the complete open set and actual catalogue
Scene/Prefab targets, including identity sidecars and catalogue-declared documents outside a
suffix-only Browser listing. Deduplicate using complete logical source key, canonical location,
kind and content observations; reject conflicting identities or role claims.

The [source index](../../src/project_host/authoring/source_index.rs) discovers candidates; its
current-profile result and directory-identity checks are not proof of this complete stable
membership. Read and classify every participating file, including unopened sources. Reject
incomplete/truncated scans, unbound participants and unsupported source kinds. A proven Missing
root can contribute an explicitly observed empty set; unreadable/unavailable is not empty.
Re-enumerate under the same configuration at confirmation and final publication and compare the
full members, metadata and missing-root observations. Late additions/removals and external edits
are conflicts, not silently excluded dependencies.

Transform the complete authored declaration tree, including:

- Direct component records and Prefab addition component records.
- Complete component Set records and versioned Properties batches.
- Nested overlays carried by additions and instance claims, including dormant declarations.
- Affected binding coordinates and property selectors, with explicit old/new correspondence.

The [overlay classifier](../../crates/nara_scene/src/authoring_decode/overlay.rs) already traverses
the complete stored tree. The compiler's surviving values cannot substitute for that inventory.
Update a Properties batch's component version even when it contains only the unchanged armor pin.
Preserve pin presence, including a pin equal to an old default. An old root `Absent` cannot be
mechanically converted to unsupported nested absence. The first consumer must refuse that case
specifically unless its reviewed transformation proves an equivalent supported declaration.

Unaffected unknown records, claims, bindings and opaque payloads remain semantically equal.
Refuse affected versions/selectors/payloads whose meaning cannot be proven, integer overflow and
invalid new shape/capability/reference state. The generic owner independently computes the actual
difference and affected-version coverage from the complete authored tree, including empty
Properties batches. A producer's self-reported footprint or successful effective compilation is
insufficient. Validate the permitted owner, component, version and selector correspondence and
every resulting declaration. Unchanged armor, topology and out-of-scope fragments require
structural equality under the understood wire grammar. The project command and independent
consumer prove conversion semantics; the Host does not invent semantic conversion rules from
names or become a general semantic equivalence prover.
CP1 keeps source and instance topology unchanged.

Inventory readiness is not a value-validity proof. Before accepting the proposal, validate every
affected complete record on both sides against its own admitted Schema, including addition and
Set payloads that contain no asset references. Resolve every claim in each affected Properties
batch, including unchanged claims, and validate its selected Set payload or supported root Absent
default. Share selector-work and source-work admission across the complete tree. A version-only
change cannot admit an old-shaped payload or an unresolved selector. Keep this proof in the
explicit upgrade owner rather than changing ordinary lossless inventory into eager activation.
The selected field's member tombstones remain exact; a value conversion does not authorize wider
permanent retirement. Both old and new Properties batches must satisfy the compiler's existing
non-overlap rule, even when each payload is individually valid. Prepare the successor through the
original source's encoded and wire-shape publication limits as well as its value limits, so a
broader proposal decode policy cannot install a checkpoint that its original source cannot save.

## Owners And Reused Kernels

| Owner | Reuse | Upgrade responsibility |
| --- | --- | --- |
| Schema and Adapter owners | `ComponentSchemaOwnerSnapshot::admit`, exact predecessor checks, `project_authoring_adapters` and fresh basis-bound evidence | Admit the selected structural/value successor without relaxing creation-only equality guards. |
| Scene/Prefab authoring | `admit_authoring_with_basis`, declaration manifests and existing preparation budgets | Prepare transformed source, classification and a new history checkpoint together, bound to the exact old source/basis. Ordinary whole-record editing restrictions remain. |
| Workspace and persistence | Complete-open-set capture, [bind-before-swap reclassification](../../crates/nara_tooling/src/workspace/schema_reclassification.rs) and existing Reopen installation | Bind source/classification/history, fresh revision/content state, actual saved token/digest, each persistence Host's new leaf and the lane's `authoring_source` observation before infallibly installing the complete set. A classification-only commit or old saved token is insufficient. |
| Project Host | [Existing owner preparation](../../src/project_host/runtime/editor/schema_owner.rs), source revalidation and project receipts | Own the complete reviewed operation, execution isolation, disk outcomes, reconciliation and final project adoption. |
| File publication | [FilePublicationHost](../../src/project_host/file_publication.rs), locks, bounded temporary writes and identity/digest checks | Publish each exact member and retain truthful per-file outcomes; it does not attest cross-file atomicity. |

Retire the affected tool route and wait for actual pending work to return, including child and
cleanup custody. Stop and physically retire affected supervised Play and Runtime execution before
destructive publication using the existing owners; cleanup failure is distinct from logical route
withdrawal. Prevent affected execution from restarting throughout publication/reconciliation. Do not
thaw a Runtime or mutate an admitted native registry. Independent retained definitions keep their
original responsibility. The exact preview/source-result invalidation must follow current owners.
Prepare new engine-owned data Adapter evidence separately from runtime agreement: the successful
EnemyStats Check/Play consumer explicitly supplies exact v2 Runtime recipe/bindings. Keep v1 as a
strict agreement refusal control; an admitted authoring Adapter alone grants no execution.

ADRs 0035/0043/0047/0090/0110/0114 now select this bounded upgrade contract. Keep additive
adoption and creation-only evolution as their existing narrow operations. Do not extend them by
weakening one comparison or turning generic authoring into arbitrary record replacement.

Complete-set preparation retains every open source, its classification and both history branches
before allocating any unopened source classification. A move-only Workspace input binds
that original set and its retained source proofs to the same cumulative Work; later preparation
borrows those proofs without charging the same incumbent again. Open and unopened groups cannot each claim an
independent peak allowance while both remain alive. Confirmation scans retain only temporary
collections: drop them before refunding their storage, including refusal paths, and preserve all
performed work in the original allowance. A large string is not a complete Schema/entity node per
byte; adaptive reservations must constrain the real owning decoder's shape ceiling rather than
truncate accounting or permanently retain unused decode capacity.

Transfer preadmitted source observations together with their cumulative allowance. A successor
capture or project installation that receives already charged original custody must not charge
that same scope again as a new acquisition. A separate validation operation admits its own live
proof scope; unrelated retained handles or aggregate counter differences cannot stand in for that
admission. Release an unused offered observation before refunding its storage and handle capacity.

Preparation consumes the previously captured inventory and returns that same owner on refusal.
Once the failed candidate and its source preparations have dropped, restore both Host and source
preparation retention to their entry reservations. Preserve cumulative work, nominal work identity
and observed peak. A corrected proposal must fit the same live-storage ceiling as one successful
attempt; failed output allocations cannot remain charged as though they still coexist with it.
No preparation or retention token from a refused attempt may escape that settlement boundary.

Capture the complete Workspace's source inputs without transferring its live mutation authority
to the worker. The owned input shares each original source/classification, retains its actual
revision/content-state and history limits, and binds its source proofs to the same cumulative
Work and Workspace identity. Background preparation reuses the direct source transformation
kernel. It never fabricates a Session, history checkpoint or Workspace to make a borrowed API
appear owned. Refusal returns the unchanged captured inputs; final binding still compares the
real live Workspace. Retain every incumbent before preparing any unopened classification.

The pre-Pending metadata owner admits the successor bundle, actual Adapters and nominal basis
once. Every live source preparation and the final project use that exact admission. Observing the
same bytes after publication does not permit a second independent basis to replace it. Before
final Ready, prepare the saved startup closure, Session/index storage and private mutable source
observations under the actual Pending leaf and successor bundle. The exclusive final publisher
supplies the real Ready capability; installation only moves prepared ownership and updates the
manifest's Copy observations. Ordinary readers never receive an unfilled future receipt, and
ordinary Open continues to refuse Pending throughout this private preparation.

## Publication And Process Recovery

Before the first source replacement, publish and verify an operation-owned bounded recovery
package containing the operation identity, project and old/new Schema facts, complete source-key/
path membership, old disk digests, reviewed dirty inputs, deterministic new canonical bytes and
digests, and the explicit history policy. The record is durable project recovery data, not
disposable build cache. Charge retained inputs, outputs and recovery work before allocation.

Select an explicit required `Ready`/`Pending` authoring state in `nara.toml`. `Pending` binds the
recovery package's project-relative path and digest. Missing/invalid state never defaults to
Ready; missing, corrupt or mismatched referenced recovery data refuses specifically. Update all
unreleased manifest producers and fixtures directly under the existing canonical envelope rather
than adding a compatibility reader. The recovery package is verified before publishing Pending;
Pending must be confirmed before the first destructive source replacement.

The operation freezes the old persistence lanes it owns: drain every already accepted Save to its
actual return before Pending, then refuse new Save, ordinary Reopen and Close/Exit-triggered Save
through reconciliation. This is not cross-process exclusion. A write from an independent Editor
is an external edit checked against each member's original expected file facts and the complete
set's final observations; detecting that conflict does not mean the other process was prevented
from writing. Do not describe ordinary file-local Save checks as a project-wide readiness lock.

Capture and finally revalidate that state through `AuthoringProjectAdmissionHost::prepare`, all
`check_project_source*` paths including `from_manifest_bytes`, `ProjectContentLoader`'s
`load`/`load_scene`/`load_delivery`, external Play readmission, source Cook adapters and Export
publication checks. Even a retained source candidate cannot bypass a later Pending state. Generic
Open may expose a code-free recovery/conflict shell without executing the upgrader or installing
a mixed ready project. Freeze old Session writes during partial publication and reconciliation.
Already captured independent source-free products retain their original generation.

Expected-object comparisons retain the actual original leaf and every parent/directory object
whose native identity they use, continuously from capture through publication or refusal. A
copied `FileIdentity` and a surviving root do not preserve a closed leaf's incarnation. Admit
retained handles plus transient scan/publication overlap against the configured handle ceiling
before opening; refuse before Pending when the complete review cannot fit. Sharing retained
custody does not reopen the file or create a new physical handle. After a completed replacement,
historical evidence remains distinct from an independently read matching output; such a read
establishes current content and live custody, not continuity with a closed historical candidate.

Pre-Pending admission also checks the known final overlap, including the original retained set,
successor verification, confirmation scan, open replacement leaves, storage/Ready publishers and
transient opens. A prospective check is neither a handle allocation nor observed peak usage.
Refusal returns existing move-only admitted custody, and drops/refunds only this attempt's new
storage and handle reservations. The complete successor inventory prepares its own disposal
before Ready: installation discards source bodies and scan tables while transferring native
custody into the Session and its escaped source validations. Continuing shares keep their original
allowance. The private final receipt records the settled operation budget after installation.
The file publication owner supplies the content-dependent read/hash cost for all three upgrade
publishers; Hosts add only their own path, metadata and scratch admission.

Capture requests and escaped dependency validations must also own the live objects supporting
inherited source observations. Startup validation alone is insufficient. Explicitly hand off an
observation from the actual admission/persistence owner: compare the current opened capability
against that owner's still-live original and bind its parent and digest before transfer. A genuine
Save/Reopen replacement supplies new custody; the same path or a different copied ID is not
permission to bypass the original check. Preserve old-token rejection for inherited observations.
This admission/Save/Reopen-to-capture handoff remains an integration prerequisite; do not label
the complete dependent-capture workflow qualified before its owner and public tests exist.
Ordinary fresh read-only dependencies use an explicit current-content/logical-binding observation
under ADR0047, not one persistent native handle per file. Equal-content replacement is allowed
only after actual key/kind/path/metadata readmission; Deferred binding authenticates no body and
therefore preserves that same distinction across in-place edits and replacement. Neither mode
can stand in for the complete upgrade's retained original-object set or historical reclamation.

Publish reviewed source outputs, then the bundle, and revalidate the complete new set. Bind every
new Workspace, project and persistence-lane commit object before the final manifest transition.
Publish Ready as the last disk replacement, then perform only infallible in-memory installation
and acknowledge completion. Partial, `PublishedWithoutAdoption`/`RequiresReconcile` and `Uncertain`
remain distinct; they do not revive old Session writes. A Ready replacement with uncertain evidence
requires reconciliation instead of assuming either old or new state. A crash after confirmed Ready
but before memory installation opens the fully verified new disk generation in a fresh process.

Prepare the successor project/Schema receipt through an upgrade-specific owner while disk remains
Pending. Ordinary Open still refuses Pending. Bind that preparation to the verified operation,
actual Pending leaf and exact deterministic Ready bytes, using the exclusive Ready publisher;
an arbitrary accepted publication supplied later cannot stand in for that binding. Complete all
fallible parsing, Adapter/classification attachment and record/storage preparation before Ready.
Only its bound publisher's confirmed Published result supplies the real final manifest capability
for infallible receipt installation. Private unshared preparation storage may reserve that future
capability slot; ordinary receipt readers never observe an unfilled phase. Uncertain publication
retains the recovery fence and does not manufacture a Ready receipt.

Retain each actual per-file outcome from the publication owner. Post-replacement failure never
restores old files or saved tokens by assertion. Treat an externally rewritten readiness manifest
as a competing project edit requiring new admission; the operation does not authorize overriding
it or promise safety against arbitrary deletion of all recovery authority.

| Interruption | Fresh-process observation and allowed continuation |
| --- | --- |
| Before the first replacement | Sources remain old. Recover reviewed dirty inputs; transformation is not automatically executed. |
| Between replacements | Identify verified old, verified new, unresolved and external conflicts per member. Only publish outputs for members freshly verified against their original expected input. |
| All bytes published, before Session acknowledgment | Revalidate the complete new set and explicitly adopt new checkpoints. Reuse deterministic outputs; never multiply already-upgraded values again. |

`FileIdentity` is capability-session evidence, not a durable identity usable in another process.
The package retains complete re-admittable old/new catalog data and deterministic inputs/outputs,
not process-local nominal tokens. Reopen through fresh project-root capabilities, validate the
whole participant set and establish a new shared basis and Sessions. Members matching new bytes
need no write; members freshly matching old bytes can bind their new leaf and continue. Identical
old/new digests require no claim that replacement happened. All other members are unresolved or
external conflicts. Do not reuse a creation-only `AlreadyAdmitted` result as whole-set recovery.

A read of matching new bytes is a current observation that can establish a fresh saved checkpoint,
not a fabricated old replacement receipt. Original publication/Uncertain history retains its actual
scope. Malformed/missing/conflicting recovery data must yield a specific refusal within its
evidence scope. Recovery data never authorizes paths outside the project or code execution.

### Fresh Recovery Admission Boundary

Share the private Schema-model admission kernel with live preparation; do not recreate the old
`AuthoringSchemaReceipt`. The verified recovery package carries old/new bundle definitions, while
`ObservedPendingRecoveryStorage` carries this process's actual root, Pending/recovery leaves and
parent/digest custody. Keep those responsibilities separate. Live preparation still proves its
original receipt and nominal basis. Recovery admits new model authority from verified data and
actual Adapter evidence; equal bytes do not restore a vanished nominal token.

Remove the final Ready kernel's dependence on `selected.incumbent` for location/current-file
facts. Supply a private native context from the actual Pending owner and actual observed bundle.
Attach successor models through `AdmittedAuthoringSchemaBundle::from_upgrade_observation` only
after that bundle is really the successor. Reuse the existing bounded model/Adapter/basis kernel,
successor inventory, saved project preparation and exclusive Ready publisher. A broad provider
abstraction across ordinary Open/capture is unnecessary for this consumer.

The inventory owner must freshly enumerate and classify the complete mixed old/new set. Join its
actual retained leaves/parents to the existing file-publication kernel: write stored deterministic
outputs only for freshly verified old members, observe already-new or identical members without
inventing publication evidence, and refuse third content, membership, metadata or recovery changes.
If declaration-difference validation needs reuse, extract its data-only kernel; never construct a
replacement old Session/history merely to call a live preparation API. Verification of stored
outputs does not execute the project converter again.

Expose recovery through one finite, headless owner. Inspection is read-only and borrows the
authenticated old/reviewed/successor bodies and inspection-time member classifications for review.
Explicit confirmation revalidates that same complete inventory before any source publication.
Advancement uses the existing source and final Ready publishers, retaining the same move-only
preparations, native custody, cumulative Host work and source-validation allowance on refusal.
Only actual completed cleanup permits a safe retry; uncertain evidence requires a fresh complete
observation instead of another attempt through the uncertain publisher. Limits cannot grow during
the operation. Keep complete-operation policy and history effects independent of Editor types so
the CLI-only consumer cannot accidentally depend on Workspace or a vanished live Session.

The recovery receipt exposes this process's actual final Ready publication and filesystem
guarantee tier. Privately preparing the saved successor project before that write proves startup
admission, but does not return the previous process's basis, history or saved tokens. Ordinary
Open after recovery independently admits its own nominal generation and fresh checkpoints. Report
inspection-time Old/New/Unchanged observations separately from writes actually performed by this
owner; an already-new member must retain its current leaf and receive no replacement witness.

Qualify explicit child-process recovery with only project location and finite limits: all-old
Pending, mixed old/new sources with old/new bundle, all-new Pending, external/recovery conflicts,
and Ready published before acknowledgment. The last case uses ordinary fresh Open. Preserve strict
ordinary Pending refusal, one shared successor basis per recovery, real saved observations and
exactly-once value conversion. Live-publication tests alone do not establish fresh-process
recovery; the linked subprocess qualification supplies that evidence.

The first claim is process-interruption recovery with the actual filesystem guarantee tier.
Windows currently provides no directory-sync proof through this interface; do not claim power-loss
durability, a multi-file atomic rename, arbitrary rollback or recovery after every external deletion.
Check/Play after recovery consumes the complete admitted set or refuses specifically.

## Acceptance And Ordered Implementation

1. Reconcile the affected contracts with the reviewed complete inventory, manifest readiness,
   fresh-process read admission and complete persistence-lane installation selected above.
2. Add Scene/Prefab preparation for the explicit source/Schema/history checkpoint transition and
   complete Workspace binding. Keep the current reclassification kernel's strict scope intact.
3. Add the Host-owned bounded operation, recovery publication and mandatory project-read gate.
   Exercise actual before/between/after-write interruption and fresh-process recovery.
4. Complete the independent EnemyStats Session/Application consumer and ordinary follow-up
   editing/persistence/Check/Play. Keep a successful control, preserved dormant unknown data and
   a surviving-unavailable-record refusal distinct. Verify old prepared edits reject.
5. Review independently, run focused serial nextest/public/native checks matching changed behavior,
   remove displaced experimental paths, update evidence/guides and commit the complete unit.

The fixed v2 product must also be an ordinary compiled Editor source consumer. Source admission,
editing, Apply Changes and Scene/Prefab persistence use each document's actual retained
Native/Data authoring basis, independently of trusted execution placement. Runtime availability
does not imply complete source classification. Verify an inactive degraded source changes the
aggregate authoring view and that closing it removes only its own degraded fact. Compiled Scene
preview interprets the same source basis but restricts materialization to the preview Runtime's
actual installed Native coverage, retaining exact direct/projected omissions for Data and
uninstalled game Native components. The compiler admits those omission costs before allocation;
source, history, nominal basis and saved bytes remain unchanged by preview. These controls prevent
the upgrade proof from passing through a native-only source path that ordinary v2 editing cannot
subsequently use.

Use existing jobs=1/root-target practices and native procedures. No new compatibility reader or
migration guide is required: canonical engine envelopes stay at version 1; component version 2 is
meaningful authored-data evolution. Proposed 0120 and unrelated execution/reload capabilities are
not prerequisites or implied qualification.

### Session And Application Integration Order

One Session-owned operation fences new mutations, Save/Reopen, Close/Exit-save and competing source
operations at acceptance, then drains accepted Save to its actual return and installs its outcome
before baseline capture. Capture the complete original inventory before requesting a proposal.
Expose that inventory and the reviewed working copies as bounded borrowed inputs to the explicit
project command. Source preparation consumes the same move-only original inventory and admitted
custody when admitting the returned proposal, rather than capturing a second full preparation
inventory. Confirmation and final re-enumeration remain validation within that operation. The
Session neither executes catalog-supplied code nor retains another duplicate complete inventory.

Admit the complete proposal's owned bytes, source/key/selector metadata and request storage before
an existing Task can retain its queued request. The finite worker uses that same admitted request
to prepare recovery data, the successor Schema and source checkpoints. Refusal returns the original
baseline for correction; it does not recapture a new project or discard performed work. Review
borrows the complete prepared set and states the save/history policy explicitly.
Keep the move-only request outside the disposable Task callback until actual execution begins.
Queue refusal returns that same baseline after releasing only the discarded proposal and Task
storage; physical return remains required before recovering an admitted worker's input or result.

Select the complete operation's finite Host work ceiling at acceptance, before capture. Ordinary
single-source capture policy does not define the allowance for all-profile inventory, proposal
attempts, recovery, confirmation and publication together. The new work meter starts at acceptance
with inherited content and original custody admission. Its selected ceiling cannot increase during
a retry. Report work exhaustion
specifically; cancellation can end that operation before a caller starts another with an explicit
ceiling. Source preparation, retained storage and native handles keep their independent limits.
The installed Schema's ordinary capture policy and live storage/custody admission are separate
from the completed upgrade's work evidence. Ready installation must not export the upgrade's
larger allowance or consumed historical work to subsequent ordinary captures or a new upgrade.
Keep continuing resources charged through their actual owners, without replaying Schema admission
or resetting any work performed inside the current operation.

Select the operation's finite open-handle ceiling through the same acceptance limits. The complete
original set, verified successor, confirmation scan and final lane guards coexist deliberately;
ordinary one-source capture's default ceiling may correctly refuse that overlap. Keep the existing
default, report refusal before Pending and allow a caller to start a new operation with an explicit
larger finite ceiling. Do not widen ordinary project capture, renew capacity during a retry or omit
still-live custody to admit the complete workflow. The successor keeps its ordinary Schema policy.

Separate `work_pending` from persistent `writes_fenced`; these describe owner facts, not fixed
public API names. Unlike ordinary additive adoption, reconciliation retains CP1's write fence
after finite work returns. Once Pending is confirmed or its publication is uncertain, Close/Exit,
workspace-intent cancellation and result acknowledgment cannot withdraw that fence or authorize
old-lane Save, Reopen or Close/Exit-save.

Before Pending, Close/Exit asks the operation owner whether cancellation is still allowed, requests
it and waits for actual worker return before ordinary dirty-document arbitration. Project replacement
Open refuses while the incumbent write fence is held and keeps that incumbent's cancellation route
available. Additive Schema reconciliation obeys the same shared admission fence. Frontends do not
maintain a second list of cancellable operation phases.
While Close/Exit waits, repeat that owner query after finite work returns: a proven pre-publisher
refusal can restore cancellation that was temporarily unavailable while the Task could run.

A Browser refresh reserved before the fence remains queued without starting another scan. Frame
maintenance observes actual admitted workers and runnable query work, rather than treating a
suspended refresh as work requiring periodic wakeups. Cancelling the pre-Pending operation resumes
the original reservation; it does not silently discard the accepted refresh.

Confirmation joins actual tool retirement and Application-owned supervised Play retirement before
Pending, including actual child and cleanup return. Accept the physical completion acknowledgment
only for the exact operation and incumbent published project generation; an Idle display state or
accepted Stop/Close is insufficient. Keep cleanup failure distinct from completion. Before Ready,
the Application prepares an empty replacement for its permanently closed Play owner; replacement
also precedes resumption if cancellation follows physical Close.

The Application registers one nominal supervised-Play owner binding for its actual published
project generation. Only that binding can take external launch requests or attest an upgrade's
retirement request; an unattached independent Session cannot launch through that interface.
Retirement binds the operation, recovery digest, incumbent Schema/basis and published generation.
The concrete Play owner supplies the attestation only after its actual close and cleanup complete.
Root verifies the registered owner and exact request; it does not independently inspect another
crate's child process. No public boolean, reconstructed ID or Idle observation substitutes for it.
Once this handoff is issued, pre-Pending cancellation waits for physical return and installation
of the prepared empty Play owner before releasing the fence.

Final installation binds all persistence Host guards, Workspace source/history/saved commits,
lane source observations and the project installation destination in one local owner turn. Hold
those borrows across final Ready. Only confirmed publication consumes the already-bound commits
and moves the new project; no fallible admission or storage preparation remains after Ready. Do
not store borrowed guards across Tasks or call a whole-Session mutable method while they exist.
Move the preprepared Play owner in the coordinated adoption turn before admission resumes. The
linked qualification covers this coordinated installation and physical-retirement handoff.
Each persistence guard compares its accepted predecessor with the exact receipt frozen for this
operation, including an accepted Save drained before capture. Neither the lane's historical base
fact nor a newly sampled Host observation can substitute for that captured receipt.

Use consuming private preparation parts to move the reviewed Workspace into its binder while
retaining original inventory/frozen inputs/unopened proofs through final publication. Keep Host
guards local, then borrow persistence commits from them; do not return a structure that owns
guards and borrows itself. The final synchronous owner turn receives disjoint Workspace,
persistence, lane and project borrows. Its successful branch moves the prepared Play replacement
before admission resumes. A Ready refusal or uncertainty retains the prepared owner and fence.

Final guards must return their exact prepared owners without installation when Ready refuses;
binding failure also returns the original prepared Workspace and saved observations. Reusing a
candidate cannot require another Schema admission, source/history preparation or body clone.
Keep the finite publisher input outside an opaque queued closure until the Task actually begins,
so queue refusal returns that same owner. Disable cancellation while the Pending Task can run,
then restore it on a proven pre-publisher refusal. Lost results and uncertain replacement remain
fenced. Prepare continuing Session storage/native counters before Ready, excluding the original
inventory, recovery and binding scratch that the completed operation will discard; preserve its
cumulative work and peak as operation evidence. Admit the final persistence-leaf reads/hash and
transient native overlap before executing those guards.

Safe live retry continues the exact returned publisher and its prepared source/Workspace/project
objects. Run pending native cleanup and the next finite step through the existing publication Task,
with positive cleanup work and bounded result scratch admitted before each attempt. A failed
cleanup does not refund performed work or release its custody; an exhausted allowance does not
authorize another call. No cleanup means no cleanup charge. Cancellation uses the same worker
admission rather than performing native cleanup on the Session owner thread.

Retry is continuation of an accepted operation, so a waiting Close/Exit must keep that route
reachable while the existing Task facilities remain open. It does not admit a second upgrade or
resume old persistence lanes. Action availability comes from the retained owner. Unknown worker
returns, uncertain publication and a publisher requiring reconciliation refuse blind retry and
retain the fence. Final guard refusal returns the same candidate for a later bind; it cannot
trigger another model admission, source preparation or allowance reset. Native source-lock and
actual Close/Exit controls belong to the Application proof as well as the Root owner tests.

## Separate CP2 Successor

`Nested Prefab source extraction with complete occurrence remap` reuses the selected review,
complete-set adoption and recovery owners after CP1. It changes source topology with a complete
old/new occurrence map, rewriting claims, placement and reference bindings, including full
`through` hops and `expected_base`. Keeping terminal entity IDs is insufficient. Unknown semantics
that cannot be safely remapped refuse. CP2 needs its own consumer and qualification.
Its [source study](../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md) records
the fixed Godot/Bevy comparison, reusable private mechanisms and the instance-root/reference
representation gates. Anchor-preserving extraction is an experiment, not a frozen restriction
on the ordinary branch-to-Prefab workflow. This research does not add a CP1 delivery condition.
