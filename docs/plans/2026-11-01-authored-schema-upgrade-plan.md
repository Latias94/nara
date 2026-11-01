# Reviewed Authored Schema Upgrade

**Status:** Proposed next R5 implementation unit, revised after independent adversarial review;
the affected Accepted contracts must be reconciled before implementation.
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

## Owners And Reused Kernels

| Owner | Reuse | Missing responsibility |
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

Before implementation, reconcile ADRs 0043/0090/0114 and the Workspace contract. Keep additive
adoption and creation-only evolution as their existing narrow operations. Do not extend them by
weakening one comparison or turning generic authoring into arbitrary record replacement.

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

Capture and finally revalidate that state through `AuthoringProjectAdmissionHost::prepare`, all
`check_project_source*` paths including `from_manifest_bytes`, `ProjectContentLoader`'s
`load`/`load_scene`/`load_delivery`, external Play readmission, source Cook adapters and Export
publication checks. Even a retained source candidate cannot bypass a later Pending state. Generic
Open may expose a code-free recovery/conflict shell without executing the upgrader or installing
a mixed ready project. Freeze old Session writes during partial publication and reconciliation.
Already captured independent source-free products retain their original generation.

Publish reviewed source outputs, then the bundle, and revalidate the complete new set. Bind every
new Workspace, project and persistence-lane commit object before the final manifest transition.
Publish Ready as the last disk replacement, then perform only infallible in-memory installation
and acknowledge completion. Partial, `PublishedWithoutAdoption`/`RequiresReconcile` and `Uncertain`
remain distinct; they do not revive old Session writes. A Ready replacement with uncertain evidence
requires reconciliation instead of assuming either old or new state. A crash after confirmed Ready
but before memory installation opens the fully verified new disk generation in a fresh process.

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

Use existing jobs=1/root-target practices and native procedures. No new compatibility reader or
migration guide is required: canonical engine envelopes stay at version 1; component version 2 is
meaningful authored-data evolution. Proposed 0120 and unrelated execution/reload capabilities are
not prerequisites or implied qualification.

## Separate CP2 Successor

`Nested Prefab source extraction with complete occurrence remap` reuses the selected review,
complete-set adoption and recovery owners after CP1. It changes source topology with a complete
old/new occurrence map, rewriting claims, placement and reference bindings, including full
`through` hops and `expected_base`. Keeping terminal entity IDs is insufficient. Unknown semantics
that cannot be safely remapped refuse. CP2 needs its own consumer and qualification.
