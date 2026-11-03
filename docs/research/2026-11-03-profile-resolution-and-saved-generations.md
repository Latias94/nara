# Profile Resolution And Saved Generations

Date: 2026-11-03. Status: pinned source comparison and design inference, not complete Project
extraction qualification. Nara baseline: `88fd7289c0d680481e0f964561650de05f90cbad`, with the
uncommitted extraction replacement on `refactor/foundation`.

The [complete extraction plan](../plans/2026-11-02-prefab-extraction-plan.md) owns implementation
and acceptance. This study supplements the [context/traversal comparison](2026-11-03-prefab-context-and-bounded-traversal.md)
and [immutable-save study](2026-11-03-immutable-save-checkpoints-and-publication.md).

## Pinned Mechanisms

Bevy revision: `85503941e488152711a4695bbe248e9ed9f453fc`.
Godot revision: `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

| Mechanism | Evidence | Relevance and limit |
| --- | --- | --- |
| Bevy asset location includes source, path and label. The server selects the actual source and reader. | [`AssetPath`, path.rs:54](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/path.rs#L54-L61); [`get_meta_loader_and_reader`, server/mod.rs:1391](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/mod.rs#L1391-L1395). | Short lookup spelling alone cannot identify an interpreted resource. This does not require Nara to put configuration labels into canonical physical identity. |
| Bevy reuses an existing path/type handle and discards a later settings transform in that branch. | [`get_or_create_path_handle_internal`, server/info.rs:245](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/info.rs#L245-L272). | A cache hit does not prove every supplied interpretation has been independently admitted or validated. |
| Godot can remap logical resource paths according to locale while retaining local-path cache identity. Replace can copy new contents into an old resource object. | [`_path_remap`, resource_loader.cpp:1442](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource_loader.cpp#L1442-L1485); [`_run_load_task`, same file:537](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/io/resource_loader.cpp#L537-L596). | Logical path, physical read selection, object identity and content generation are different facts. Reusing a pointer does not establish equal interpreted contents. |
| Godot prevents ordinary file reload of an editor PackedScene and has a dedicated reload-from-memory path that packs the live root before instantiation. | [`editor_can_reload_from_file`, packed_scene.h:264](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/packed_scene.h#L264); [`reload_scene_from_memory`, editor_data.cpp:770](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_data.cpp#L770-L785). | Editing state has its own owner and cannot be replaced by ordinary resource-cache policy. This is not Nara's immutable checkpoint or publication protocol. |
| Godot excludes PackedScene from generic external-resource saving. File moves separately update cached paths, rename on-disk dependencies and reload open scenes; ordinary reload clears history. | [`_save_external_resources`, editor_node.cpp:2455](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_node.cpp#L2455-L2469); [`filesystem_dock.cpp:1668`](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/docks/filesystem_dock.cpp#L1668-L1730); [`reload_scene`, editor_node.cpp:7286](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/editor_node.cpp#L7286-L7295). | Cached objects, disk dependencies and open editing histories need coordinated but distinct actions. Clearing history globally would violate Nara's explicit Guard and SaveCurrent contracts. |

These observations support responsibility boundaries; they do not claim that either engine
implements Nara's complete multi-file extraction and recoverable publication guarantees.

## Configuration-Local Catalogue

The actual Nara inventory now retains one physical participant table and every configuration's
membership and metadata admission in
[`source_inventory/environment.rs`](../../src/project_host/authoring/source_inventory/environment.rs).
An open native document can participate without a catalogue binding. That fact must not expose
its metadata to arbitrary resource resolution in that configuration.

Complete physical membership is a separate fact from explicit native participation. The future
inventory review exposed this counterexample: a closed source is discovered under mobile Prefabs
while the same directory is another profile's Scripts root. Replaying a complete physical claim
as an explicit native claim adds a new Scripts context and native-only metadata visibility. The
file, canonical key and bytes stay equal, yet its interpretation set changes. Suppressing the
final continuity comparison would hide this semantic change.

The selected correction records explicit native participation in each actual source participant.
An ordinary open source establishes it; merely listing a discovered physical source does not.
Both extraction and Schema-upgrade packages carry that required logical field so fresh recovery
can preserve an originally open Guard without inventing a Workspace. The scanner uses the exact
recorded flag for extra native contexts while retaining ordinary capability/identity admission.
Configuration definitions, physical membership, native participation, metadata catalogue visibility
and publication dispositions remain separate facts. This refinement is implemented in the working
tree and undergoing actual producer/recovery validation, not a completed Project extraction claim.

The selected policy is a further input to explicit ProjectLocation participation. A source opened
under the default policy can be discovered under a mobile root while remaining outside the default
catalogue roots. Restoring the explicit flag must also restore its original selected context.
Record the selected profile once per operation, not once per physical source, and resolve that
selection from the actual retained Pending configuration. Recovery without a new selection follows
the record; a conflicting explicit selection refuses before model/Project adoption. This preserves
the captured interpretation set without another identity axis or an invented open-document fact.
The serialized selection must be present: explicit null selects the default policy, while an
absent field refuses. Plain optional-field deserialization would otherwise erase a named selection
when the key is deleted, so both concrete recovery packages use required nullable deserialization.

The distinction matters for Q visibility. Suppose P belongs to desktop and mobile, while Q's
proposed metadata is admitted only under desktop's configured roots. Filling both environment
databases from the all-profile union would let mobile resolve Q through StableId or primary
Product, even with a different Path scope: those resolver branches query the database directly.
The [Asset owner](../../crates/nara_asset/src/database.rs) intentionally separates stable lookup
from authored alias scope. Scope changes alone cannot repair a database with false membership.

Build each frozen catalogue from the actual configuration's admitted records. Exclude
`catalogue_binding_index() == None`. Derive finite before/after Prefab associations through the
existing catalogue resolver, with proposed Q visibility checked through the same actual metadata
scope rule. Keep all inventory configurations and missing-root observations, while omitting
empty configurations from the Core's source-compilation environment table.

The [Core environment owner](../../crates/nara_scene/src/prefab/extraction/corpus/environment.rs)
can then validate the finite associations, including Q availability in every environment that
contains P. It cannot turn a caller-supplied Path association into proof of filesystem visibility.

## Saved-Generation Counterexample

The pinned sources reinforce two constraints on this separation. Godot's disk dependency
renamer preserves each external-resource ID while changing its path/UID and copying the rest
of the file ([resource_format_text.cpp:982](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/resource_format_text.cpp#L982),
`ResourceLoaderText::rename_dependencies`). Its branch operation instead
duplicates the current editor selection before packing
([scene_tree_dock.cpp:3736](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/docks/scene_tree_dock.cpp#L3736)).
Neither source is a substitute for the other input. Bevy reads serialized entity-map keys into
the admitted dynamic entities and constructs the complete installation mapping before applying
their reflected values ([serde.rs:355](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_world_serialization/src/serde.rs#L355);
[dynamic_world.rs:98](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_world_serialization/src/dynamic_world.rs#L98)).
This supports retaining stored identities independently of installation; it
does not prove Nara's persistent declaration or occurrence correspondence.

Even a fresh top-level load need not observe fresh dependencies. In Godot's text loader,
ordinary `CACHE_MODE_IGNORE` still permits external dependencies to use `CACHE_MODE_REUSE`;
`IGNORE_DEEP` propagates the deeper isolation policy
([resource_format_text.cpp:1408](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/resources/resource_format_text.cpp#L1408)).
Nara's recovered saved-generation proof must therefore constrain every nested
lookup to its captured finite generation, including defaults and references. A live Workspace
or ordinary resource cache cannot satisfy that proof. These are source-level design constraints,
not evidence of either engine supplying Nara's Pending/Ready or multi-file recovery guarantees.

This is a design counterexample, not a claim of an executed product failure:

1. Dirty Prefab U adds entity v in its working copy; original disk U has no v.
2. Dirty Scene R already replaced its old P route with a reference to U.v, so extraction leaves
   R's working source unchanged. Original disk R still requires a route rewrite.
3. Review selects SaveCurrent for R and Guard for U.
4. Working correspondence and R's exact encoded output association both pass.
5. The proposed disk generation contains saved R referring to U.v, but original disk U lacks v.

Therefore the Project owner must prove a separate finite saved generation:

```text
saved_after[source] = exact reviewed output, if this member is published
                      captured original disk source, otherwise
```

Validate that combination in every relevant actual environment through the existing compiler.
An unpublished dirty working copy cannot satisfy its dependencies or declared inputs. Refuse
before Ready or explicitly change the reviewed publication set. Do not infer publication from
capture, source-change flags, working correspondence or a per-leaf file receipt.

## Implementation Consequences

Retain physical identity, configuration membership/visibility, original disk data, actual open
working allocations and publication dispositions as separate facts. One physical output per
source does not mean one interpretation or one observed content state. Keep original cumulative
Host and source allowances through capture, refusal, review and late validation.

Catalogue admission must distinguish an out-of-scope sidecar from a sidecar encountered under
that configuration's scan whose authored alias is invalid. An optional binding alone loses that
distinction. The actual index and proposed metadata checks now share the same fallible location
admission, including configurations with no original document participants. Frozen catalogue
transfer separately retains its database reservation and releases destroyed temporary indices;
resolved aliases carry their own move-only storage reservation.

The physical inventory also retains the actual file capability read for each source. Native
policy interpretation in another configuration can borrow that same observation instead of
opening the source again. Stable metadata still passes its actual policy admission, and final
inventory validation still checks current names, ancestors and bytes. This separates source
acquisition from interpretation without treating a canonical key or borrowed file as current
namespace proof. Logical path/key construction remains charged even when a native reopen is
avoided. No default work or storage ceiling is increased by these corrections.

Ready revalidation has a distinct obligation from initial capture. It now reuses the actual
namespace walker and source-kind rule to check current membership, spelling, hard-link and
directory observations, then verifies retained source/sidecar names, parents and content digests.
It checks the current manifest and Schema bundle names against their retained objects and bytes.
It does not acquire another encoded corpus or repeat each source parser/native admission.
Only external original identities are seeded before enumeration; scanned originals must still
be compared against the newly observed entries so equal-byte replacement is detected correctly.
External sidecar spellings also seed the case-conflict checks. Every validation retains the
original work allowance and refunds only its dropped temporary storage and handles.

This evidence refines the current plan; it does not justify another resolver, overlay evaluator,
universal graph, configuration identity axis or generic transaction kernel. Full Session capture,
Q/sidecar publication and fresh-process recovery still require their own implementation and
executed evidence. The subsequent owning saved-generation and Project Review implementation
has its separate
[consumer evidence](../knowledge/engineering/progress/2026-11/2026-11-02T233154Z-saved-generation-proof-and-complete-review-refusal-ownership-pass-actual-consumers-8ca79f0732a34e799c8e58a4e4ba061f.md);
that narrower result does not qualify the complete publication workflow.

## Remaining Publication Responsibilities

The next ownership comparison inspected the actual extraction Review and the existing upgrade
publication, storage and recovery owners. This is a design recommendation, not evidence that
the complete extraction operation is implemented.

The physical inventory already serves two real consumers. Its neutral scanner, configuration
visibility, native observations and continuing custody should have an operation-neutral owner.
Schema-upgrade recovery generations and successor proofs remain specific to that operation.
Ordinary capture still requires Ready. A concrete operation must admit its actual Pending
authority before providing finite physical inputs; a generic flag that skips Ready would erase
that boundary. This split removes the neutral scanner's dependency on the upgrade recovery AST
without turning physical equality into approval of a semantic transition.

Do not pass extraction through `SourceUpgradePublication` or `PreparedSourceUpgradeReady`.
The former assumes one incumbent identity per member and a final Schema-bundle entry; the latter
binds an actual successor Schema. Extraction preserves the nominal basis and creates Q plus its
identity sidecar. Reuse `FilePublicationHost`'s Existing/Missing binding, canonical encoded
checkpoints, `PersistenceLeafCommit` and the source/saved binding owners. Give every physical
member one publisher and retain Guard inputs as observations rather than outputs.

Recovery storage can share package-to-Pending transport after its concrete package dependency
is removed. Keep explicit SchemaUpgrade and PrefabExtraction package meanings. Extraction must
distinguish not-yet-created Q from a deleted original source, and already-current output from
a replacement actually performed by this process. Neither a matching digest nor a current file
can manufacture the predecessor's replacement receipt. The original-Ready validator, which
requires Q and its sidecar to remain absent, cannot validate the partially published generation.

The pinned Godot branch workflow saves the new PackedScene before replacing the live branch;
the replacement has its own Undo action. That supports distinct output and editing authorities.
Sources: [branch save](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/docks/scene_tree_dock.cpp#L3784),
[branch replacement](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/editor/docks/scene_tree_dock.cpp#L573).
The pinned Bevy Scene module still lists `.bsn` asset files as future work;
[its module documentation](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_scene/src/lib.rs#L78)
does not establish a comparable persisted extraction workflow. Neither source supplies Nara's
complete multi-file process-interruption or final Ready guarantee.

An independent Nara Review audit found an additional move-only ownership counterexample:
after one actual file commit, refusing a revised save selection retained its persistence stage
but dropped the original saved-generation proof. Retrying could not recover the previously valid
Review. Refusal must return the complete original valid Review, including its mask, proof and
accepted evidence. This is separate from preparing a corrected selection before publication;
the two outcomes need different owners. Actual execution evidence belongs in engineering memory.

## Stored Extraction Recovery

The existing authored-field recovery supplies an important limit on transport reuse. Its actual
Pending manifest names one exact package; package loading checks length/digest and retains current
manifest, package and parent capabilities. Concrete payload admission checks format, complete
modeled configurations, paths, metadata, blob digests and canonical outputs. The fresh-process
owner then checks every stored reviewed-working/successor pair, without calling the project's
conversion function. The permitted field difference remains a concrete Schema-upgrade policy;
the selected target value is fixed by Review and the Pending-bound package.
Sources: [package loading](../../src/project_host/authoring/source_upgrade_storage.rs),
[payload admission](../../src/project_host/authoring/source_upgrade_recovery.rs),
[pair validation and replay](../../src/project_host/authoring/project/source_upgrade_resume.rs),
[field-difference validation](../../crates/nara_scene/src/authoring/schema_upgrade/recovery.rs).
This is source evidence; the current comparison did not rerun the older process tests.

Extraction therefore needs its own stored-pair verifier under the actual unchanged Schema basis.
The current [live corpus preparation](../../crates/nara_scene/src/prefab/extraction/corpus.rs)
calls source split construction and dependant rewriting, so it is not a recovery entry point.
Reconstruct validation indices and the actual cut; verify stored complete declaration relations,
injective occurrences and the independent mixed saved generation through existing semantic owners.
Do not generate another full candidate corpus or replace stored Q identities and input names.
In particular, canonical encoding can reorder entity records without changing source meaning;
reassigning extracted inputs from that new visitation order would reject a legitimate stored
generation or reconnect a reference incorrectly.

A concrete extraction package needs complete original source membership, original disk and
reviewed working bytes, explicit per-member dispositions and fixed outputs, the selected source/
root/profile, Q and sidecar as distinct new roles, unchanged bundle, original/final Ready
configuration, metadata guards, roots and missing-sidecar observations. Original members can be
old or exact selected output; Guard must stay original. New Q/sidecar can be absent or exact
output. Missing originals and third contents are conflicts. Fresh-process admission creates its
own nominal basis and current file observations; native identities, history tokens and historical
replacement receipts are not recoverable serialized authority.

The reusable storage seam is code-free package/manifest transport: bounded actual Pending reads,
path/length/digest checks, package-before-Pending publication, retained cleanup and uncertain
outcomes, and exclusive Pending-to-fixed-Ready file publication. A transport observation must not
grant semantic admission. Keep Schema-upgrade and extraction payload decoders, generation proofs,
publication members and final project/live binding outside that transport. A boolean Pending
bypass, arbitrary policy callback or universal commit token would hide these responsibilities.

The implementation successor separates [code-free transport](../../src/project_host/authoring/recovery_storage.rs)
from the [upgrade data adapter](../../src/project_host/authoring/source_upgrade_storage.rs) and the
[extraction data adapter](../../src/project_host/authoring/project/prefab_extraction/recovery_data/storage.rs).
Its private read-only package interface retains concrete immutable data types through native
publication; fresh reads bind separately decoded data to the same actual Pending/package capture.
No payload decoder or semantic policy callback lives in transport. Actual unchanged Schema models
have their [neutral bounded owner](../../src/project_host/authoring/schema_models.rs), independently
of file/readiness authority and Schema-upgrade lineage.

Confirmation exposed another ownership distinction: proving the original corpus unchanged does
not require admitting a second complete native corpus beside it. The [neutral observation validator](../../src/project_host/authoring/source_inventory/validation.rs)
can reuse complete namespace discovery and retained identity/digest/negative-fact checks after the
concrete operation admits its actual authority. Both recovered workflows use that path and retain
the first bundle fact with its continuing capability. This avoids a doubled native handle set
without weakening membership or current-content validation. Qualification and remaining complete
workflow obligations belong to the implementation ledger and engineering memory.

The concrete [extraction file owner](../../src/project_host/authoring/project/prefab_extraction/publication.rs)
now keeps the live confirmed Storage/Review and the recovered semantic generation as distinct
origins for one physical loop. It retains complete accepted file results beside the original
encoded checkpoints, while distinguishing a fresh already-output read from replacement evidence.
That separation leaves checkpoint acceptance and all-bound Ready installation with their real
owners. The initial live handoff uses the actual Pending transition receipt before reobserving
the retained original configuration. Later own replacements end the original unchanged-corpus
proof; per-step exact target/ancestor checks and final complete future inventory have different
lifetimes. An ordinary case-insensitive child open cannot prove a stored spelling even when its
identity and bytes match, so continuing prefix validation uses the existing exact-file API.
