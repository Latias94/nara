# Content And Persistence: Architecture Freeze Risks

Research date: 2026-10-30. Nara baseline: `75e1284ed28bc5a96686609f7dae8805d1924788`
(`main`). Scope: persistent Schema, Scene/Prefab authoring, import products and Runtime delivery.
This is source research, not an implementation proposal accepted by an ADR or a new qualification.
No implementation, index, ADR or engineering-memory file was changed; no Cargo command or
subagent was used. Referenced tests were inspected, not executed.

The current foundations should be retained: source documents own authored truth, persistent IDs
are independent of ECS entities, unavailable records remain recoverable, durable Prefab intent
is separate from exact Undo transactions, and processed content is admitted against its original
executable plan. The most expensive remaining mistakes would be freezing the boundaries between
these owners before exercising evolution and larger content sets. Replacing them with general
Rust reflection, mutable runtime objects or a universal asset executor would not solve those
problems.

## Evidence Baseline And Completed Work

All reference checkouts were clean when inspected. Source links below pin the inspected commits;
these are reference snapshots, not claims about the latest stable engine releases.

| Reference | Fixed commit | Mechanisms inspected |
|---|---|---|
| Bevy | `ae6152152d6fb8308ad6038314b48ef295e80fbc` | Asset processing hashes, dependency readiness, render preparation, world serialization and scene composition |
| Godot | `7be59b463a4798834d72d6d0a2942ddd01d920ee` | PackedScene IDs and recovery, importer metadata/version checks, threaded resource acquisition |
| Fyrox | `5495d012b32145674501af2e67d2fd06a06f04dd` | Inheritable properties, resource loading states and unused-resource retirement |

The findings were reconciled against the full
[October 29 import/delivery plan](../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md),
its completion evidence and the current source. The following records anchor the landed boundaries:

| Landed boundary | Evidence and consequence for this review |
|---|---|
| Source import contracts and compound product continuity | Plan steps 1-2 and [ADR 0122](../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md): source-scoped primary/auxiliary IDs, authored continuity, declared product traversal and actual target checks already exist. Product identity is not generated-cache identity. |
| Immutable base and first-party processed startup/travel | [Local-base qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T072753Z-immutable-local-runtime-base-and-shared-processed-scene-lifecycle-qualification-140b1b89e14c4130a72ac1c6d8f3de89.md), [first-party integration at `44470ed`](../../knowledge/engineering/verification/2026-10/2026-10-29T152520Z-first-party-processed-runtime-hosts-and-explicit-source-facets-qualification-c2d2a14cd90243f0a200a287f1ce4d20.md): same-plan codecs, CPU collections, Image/TileSet residency, physical cancellation and finite Scene back-edges are implemented. Source-free startup is not a remaining finding. |
| Actual decoded-output charging | [Output-retention qualification](../../knowledge/engineering/verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md): retained output uses actual checked cost after upfront admission; escaped pixel owners retain custody. The old whole-format retained-charge criticism is obsolete. |
| Canonical locations and authored Scene references | [Source Scene qualification at `f96951a`](../../knowledge/engineering/verification/2026-10/2026-10-30T060408Z-finite-source-scene-data-closures-and-retained-editor-observations-qualification-deea8e4f86ac44f3a6faf27aefa53a4e.md): actual readers, cyclic/diamond closures, Product travel, demanded dirty working copies, exact receipts and relocated source Export are implemented. Cross-root lookup collisions are not a remaining finding. |
| Unknown-data recovery with retained history | [Source reclassification](../../knowledge/engineering/verification/2026-10/2026-10-28T032419Z-source-owned-schema-reclassification-with-exact-retained-history-and-bounded-interpretation-becb85b221e241aebeb4845c692a7379.md) and [whole-Session adoption](../../knowledge/engineering/verification/2026-10/2026-10-28T080917Z-whole-session-installed-schema-owner-adoption-and-explicit-publication-recovery-5a02037cad1b4af49cbec2b24306524a.md): additive owner adoption proves original Undo/Redo edges and preserves dirty source state. This is not general schema/value migration. |
| Declarative Prefab composition and complete conversion | [Production overlay design](../../architecture/prefab-overlay-design.md), [conversion qualification](../../knowledge/engineering/verification/2026-10/2026-10-27T064545Z-complete-editor-prefab-conversion-and-source-admission-qualification-bc6d938e4ea5421fa42d0028230a11dc.md): selected-demand composition, explicit pins, suppression, typed provenance and complete selected-instance localization already exist. Persistent operation patches are no longer the overlay model. |
| Immutable Input source workflow | [Input qualification](../../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md): source definition capture and global installation are implemented. Processed Input and complete packaged delivery remain separately planned. |

The report has five findings: four **architectural risks** and one **intentional gap** with a
specific freeze gate. It establishes no new **confirmed defect** violating the current accepted
contracts. A deterministic limitation in source is distinguished from a measured production
failure; no latency, RSS or large-project benchmark was run.

| Finding | Classification | Decision needed before |
|---|---|---|
| CP1. Value migration does not close authored Schema evolution | architectural risk | Promising project/component upgrade compatibility |
| CP2. Prefab occurrence identity depends on ownership topology | architectural risk | Shipping extraction, cross-source promotion or persistent external entity addresses |
| CP3. Reference validity implies transitive preparation | architectural risk | Freezing Scene travel references and catalog dependency semantics |
| CP4. Base identity retains every encoded member | architectural risk | Treating one immutable base as a large game's general content store |
| CP5. Import execution identity is not yet durable build authority | intentional gap | Implementing R6b persistent reuse and target-dependent cooking |

## CP1 - Value Migration Does Not Close Authored Schema Evolution

**Trigger.** A game upgrades `EnemyStats` from v1 to v2, changes the representation of one field,
and retains the stable ID of another field already pinned by hundreds of Prefab instances.
Registering a native component-value migration can make the base value current, but does not make
the v1 property claims current. A data-only owner has an additional boundary: its admitted Schema
snapshot is metadata, not an executable migration provider.

**Code evidence.**

- [schema_authority.rs:132](../../../crates/nara_scene/src/schema_authority.rs#L132) dispatches
  native records to registered migrations. Mixed data records and generic Authoring use
  `current_version_value`; [line 191](../../../crates/nara_scene/src/schema_authority.rs#L191)
  rejects a different version.
- [overlay/properties.rs:26](../../../crates/nara_scene/src/prefab/overlay/properties.rs#L26)
  rejects a property batch when its version differs from the current Schema, before interpreting
  its stable selectors. [overlay.rs:251](../../../crates/nara_scene/src/prefab/overlay.rs#L251)
  persists that version with the property intent. Field-ID continuity does not carry value semantics.
- [reclassification.rs:22](../../../crates/nara_scene/src/authoring_decode/reclassification.rs#L22)
  admits declaration extension or the bounded creation-metadata successor. Existing declaration,
  owner and Adapter equality is enforced at
  [line 47](../../../crates/nara_scene/src/authoring_decode/reclassification.rs#L47). This cannot
  be reused as arbitrary existing-owner evolution.

This is a reachable combination of already supported native migration and durable overlays,
not a request for a speculative scripting system. A harmless-to-this-pin component version bump
can still require an explicit upgrade of its authored claim. Conversely, automatically relabeling
that claim could silently change its meaning when units or structure changed.

**Existing safeguards and counterevidence.** Stable root/member IDs, keyed selectors and lineage
tombstones already prevent name-based guessing and retired-ID reuse; see
[member_lineage.rs:87](../../../crates/nara_reflect/src/registry/member_lineage.rs#L87) and
[ADR 0011](../../architecture/adr/0011-component-schema-ids-and-migrations.md). Older/future records
are explicitly classified without executing native migrations in
[authoring_decode.rs:882](../../../crates/nara_scene/tests/authoring_decode.rs#L882).
[prefab_overlay_comparison.rs:257](../../../crates/nara_scene/tests/prefab_overlay_comparison.rs#L257)
retains the old intent and unchanged history when current Schema cannot interpret it.
[structure.rs:639](../../../crates/nara_scene/src/authoring_decode/structure.rs#L639) protects
unrelated unavailable declarations during explicit instance repair. These are preservation and
refusal guarantees, not evidence of silent data loss. Unknown wire tags and unknown file envelopes
remain intentionally strict under ADR 0090; lossless authoring means understood semantic records,
not arbitrary future syntax or comments.

**Reference mechanism.** Fyrox separates inheritance from local modification in
[variable.rs:510-538](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-core/src/variable.rs#L510),
and its
[visitor at lines 326-395](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-core/src/variable.rs#L326)
handles an older plain-value representation separately from the inheritable wrapper. This supports
separate migration of representation and authored intent. It does not establish Nara-style
versioned selector migration or unknown-owner history safety. Bevy's inspected
[world serializer](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_world_serialization/src/serde.rs#L169)
uses reflected type paths; copying that reflection model would not supply Nara's upgrade contract.

**Recommendation and delay cost.** Build one explicit owner-controlled upgrade workflow covering
direct records, property claim values/selectors, dormant overlays and dirty dependent documents.
Use an old-to-new Schema basis and explicit address/value mappings; preserve unrecognized payloads
or refuse the affected rewrite. Decide whether a breaking migration establishes a new history
checkpoint or proves old history under a retained basis. Do not silently reinterpret old inverses,
and do not execute arbitrary project migration code during ordinary generic Open.

Prefer a reviewed offline/project upgrade first over a universal migration DSL or automatic
live-history translation. The acceptance fixture should include two nested instances, an unknown
owner, a changed unit, one unaffected stable pin, Undo and Redo branches, and a failed multi-file
publication. Delaying this until many projects depend on the format multiplies both authored
overlay rewrites and compatibility fixtures.

**Plan state.** R5 explicitly leaves the Behavior/Schema-evolution consumer open
([rebaseline plan:449](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L449)).
[ADR 0043:75](../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md#L75)
already says component migration does not migrate persistent selectors. Additive adoption and
creation-metadata evolution must not be reported as closing this issue.

## CP2 - Prefab Occurrence Identity Depends On Ownership Topology

**Trigger.** A team extracts a wheel subtree from `Car.prefab` into `Wheel.prefab` while keeping
the wheel's local entity ID. Existing Car instances have wheel property, placement and reference
overrides. The wheel now belongs to another source occurrence reached through another instance
hop. Its old address is not preserved merely by retaining its terminal ID. The same pressure
appears when a shared UI template is decomposed or a large authored Scene is split between teams.

**Code evidence.**

- [overlay.rs:51](../../../crates/nara_scene/src/prefab/overlay.rs#L51) stores each hop's anchor
  and `expected_base`; `PrefabEntityTarget` carries the full `through` chain.
- [staged.rs:1062](../../../crates/nara_scene/src/prefab/overlay/compile/staged.rs#L1062) follows
  that chain exactly and rejects missing targets or a different canonical base. It does not
  search other occurrences for an equal terminal ID.
- [runtime_scene/identity.rs:182](../../../crates/nara_scene/src/runtime_scene/identity.rs#L182)
  derives projected logical IDs from the containing source, root anchor, source/addition steps
  and terminal entity. Ownership-topology changes can therefore affect processed IDs as well as
  authored overrides. This is not a claim that runtime locators are valid save-game IDs.
- [document.rs:19](../../../crates/nara_scene/src/document.rs#L19) stores a complete document
  with ordered roots, entities and per-entity child sequences. The representation has stable
  records, but a document file remains a shared editing/publication unit. File partitioning is
  not automatically independent of the logical source boundary.

**Existing safeguards and counterevidence.** Ordinary display-name changes and hierarchy
reordering within the same ownership occurrence do not require a new occurrence chain. A
StableAsset source key survives a correctly performed whole-file move; a ProjectLocation key
explicitly does not promise that continuity
([source.rs:16](../../../crates/nara_scene/src/source.rs#L16)). Repeated Prefab instances are
correctly disambiguated. Exact base checks prevent applying an old claim to a different source.
Complete conversion already builds a reviewed localization result through the selected compiler
([localize.rs:30](../../../crates/nara_scene/src/authoring/prefab_conversion/localize.rs#L30)).
That qualified conversion is not the inverse operation of extracting a source and migrating
every external dependant. Multi-open custody and per-document Undo already work; the missing
proof here concerns a coordinated source refactor, not multiple Editor tabs.

**Reference mechanism.** Godot stores per-scene IDs and attempts ID recovery when an instantiated
node can no longer be found by name
([packed_scene.cpp:353-379](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/resources/packed_scene.cpp#L353)).
It assigns/reconciles IDs during packing
([lines 1180-1204](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/scene/resources/packed_scene.cpp#L1180)).
This is evidence that mature scene composition needs continuity/recovery machinery; it is not
proof that arbitrary cross-source extraction is automatic or that Nara should adopt heuristic
retargeting. Separating physical collaboration files from logical source identity remains a Nara
design recommendation, not a guarantee established by the inspected reference implementation.

**Recommendation and delay cost.** Keep explicit occurrence identity. Before freezing extraction
or promotion APIs, prove one source-refactoring operation with a complete old/new address map,
dependent overlay/reference rewrites, reviewable conflicts and recoverable publication. Preserve
unknown records; if their semantics are necessary for a remap, refusal is safer than textual
substitution. ADR 0047 already requires explicit cross-document transactions
([line 83](../../architecture/adr/0047-editor-workspace-and-scene-document-state.md#L83)); reuse
per-document history instead of inventing a global parallel Undo engine.

For file splitting, compare physical shards retaining one logical source identity against new
logical sources with an explicit migration map. Do not equate a shard's path, a runtime cell,
an entity identity and a Prefab occurrence. A two-author branch/merge and extraction fixture is
enough to expose the choice; CRDTs, a world-partition framework and per-entity files everywhere
are not prerequisites. Delaying this until external tools and authored templates persist these
addresses makes topology refactors project-wide migrations.

**Plan state.** R5 and the overlay design already leave cross-source promotion and broader
addressing outcomes open. Complete selected-instance conversion is qualified. This finding
identifies the remaining address-continuity decision; it does not reopen the old operation-patch
overlay replacement or the completed source-location correction.

## CP3 - Reference Validity Implies Transitive Preparation

**Trigger.** Scene A has a Door to B, B has a Door to C, and each Scene references its own textures.
A valid future travel destination should be distinguishable from data needed to display A now.
At this baseline, a connected travel graph can become one recursively prepared resource closure,
even though only the requested Scene is instantiated. A destination-selection menu produces the
same pressure without an open-world game.

**Code evidence.**

- [runtime_content.rs:719](../../../src/runtime_content.rs#L719) collects every effective
  Schema-declared asset reference in a processed Scene. Each must be a Product and satisfy
  `validate_required_dependency`;
  [line 701](../../../src/runtime_content.rs#L701) requires an actual catalog member and a
  `Required` edge. Merely changing a Door edge to `Soft` fails admission.
- [prepared.rs:50](../../../src/runtime_content/prepared.rs#L50) walks the transitive Required
  graph. [Line 135](../../../src/runtime_content/prepared.rs#L135) decodes dependent Scenes,
  prepares their domain collections and collects Image/TileSet acquisitions for the current
  prepared generation. The traversal includes the targets' own Required resources.
- The source workflow has the corresponding semantics:
  [project_content.rs:2335](../../../src/project_content.rs#L2335) discovers effective references
  and enqueues Scene targets. The recent source-reference work therefore closed correctness and
  custody, but did not separate reference validation from load demand.

**Existing safeguards and counterevidence.** Cycles and diamonds are finite; each catalog member
is visited once. Only the selected root instantiates. Declared-root authority separately prevents
travelling to a dependency-only Scene. Actual products and Required edges are checked, source
capture includes actual files, and failures preserve current content. Preparation is already
offloaded through the existing IO task owner
([preparation.rs:84](../../../src/project_host/runtime/scene_requests/preparation.rs#L84)), with
processed preparation at [line 141](../../../src/project_host/runtime/scene_requests/preparation.rs#L141).
The problem is the selected working set, not absence of asynchronous loading or infinite recursion.

**Reference mechanism.** Fyrox has an explicit resource handle state with no loaded payload
([state.rs:160-181](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-resource/src/state.rs#L160)).
Bevy separately exposes asset, direct-dependency and recursive-dependency readiness
([server/mod.rs:2114-2198](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/server/mod.rs#L2114)).
Neither excerpt proves an equivalent validated lazy Scene-reference format, but both provide
concrete mechanisms for distinguishing identity, acquisition and dependency readiness.

**Recommendation and delay cost.** Make reference semantics explicit at the owning Schema/Scene
boundary: distinguish a required-to-materialize dependency from an existing, deliverable target
that may be acquired later. Both can participate in build closure and target validation without
both becoming the current Scene's resident closure. Do not globally reinterpret today's `Soft`
edge: it currently also permits absent membership
([catalog/tests.rs:252](../../../src/runtime_content/catalog/tests.rs#L252)).

Use a three-Scene Door/menu fixture and one shared atlas. Require all travel targets to be
valid and included in delivery, but prepare the destination's heavy resources only for an explicit
request or bounded prefetch. Keep startup-critical resources strict, original-plan admission,
cancellation custody and the existing transactional publication tail. This needs a deliberate
Schema/codec/catalog change; hiding the destination in an opaque string would evade the reference
visitor and weaken Check/Export. Once many custom components encode all references with the
current single demand interpretation, correcting it requires coordinated component, cooker and
catalog migrations.

**Plan state.** The `44470ed` and `f96951a` qualifications intentionally establish finite Required
closures, not selective residency. R6/R7 leave streaming open, but the narrower decision here is
needed before the newly introduced Scene-reference semantics become a lasting public contract.

## CP4 - Base Identity Retains Every Encoded Member

**Trigger.** A content base contains several large later levels but starts in a small menu.
Alternatively, a thumbnail or decoded resource outlives the Host's direct base handle. The current
base captures every encoded member up front, and an escaped acquisition retains that entire base.
This remains true even if CP3 is solved and none of the later levels is prepared.

**Code evidence.**

- [runtime_content.rs:293](../../../src/runtime_content.rs#L293) stores every member in
  `Box<[Vec<u8>]>` alongside catalog and original plan.
- [Lines 421-499](../../../src/runtime_content.rs#L421) sum all member lengths, admit the whole
  retained base, read and verify each file, and keep all byte vectors. The defaults allow
  512 MiB aggregate encoded bytes and 1 GiB modeled retention
  ([line 105](../../../src/runtime_content.rs#L105)). These are adjustable limits, not measured RSS.
- [Lines 761-767](../../../src/runtime_content.rs#L761) retain `RuntimeContentBase` inside every
  decoded `RuntimeContentAsset`. An isolated acquisition can consequently keep all base byte
  vectors alive. Actual decoded-output charging does not change this encoded ownership graph.

**Existing safeguards and counterevidence.** This is an intentional, well-defined eager capture.
It binds a trusted digest, target, Schema and codec agreement, does not reopen paths, and survives
member deletion after capture. The base is shared, not copied per acquisition. Encoded memory and
overlapping decode work/output are bounded, and typed products retain their original leases.
The source-free worker test at
[processed.rs:58](../../../src/project_host/runtime/scene_requests/tests/processed.rs#L58)
explicitly exercises captured bytes. A small local base is a valid backend. Raising its limits
would increase the admitted working set rather than introduce streaming.

**Reference mechanism.** Fyrox retires resources according to their actual resource usage and
lifetime, rather than requiring every registered resource's payload to remain loaded
([manager.rs:1333-1361](https://github.com/FyroxEngine/Fyrox/blob/5495d012b32145674501af2e67d2fd06a06f04dd/fyrox-resource/src/manager.rs#L1333)).
Godot separates threaded request, status and acquisition
([resource_loader.cpp:700](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/core/io/resource_loader.cpp#L700),
[line 887](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/core/io/resource_loader.cpp#L887)).
These mechanisms support per-resource physical ownership. Their path/cache policies are not
evidence of Nara's stronger immutable-base agreement and should not replace it wholesale.

**Recommendation and delay cost.** Separate immutable catalog/executable authority from encoded
member backing and decoded/native residency. Retain the present eager implementation for its
qualified use case. Compare it with one concrete immutable local backing, such as a retained sealed
pack object with bounded member reads and digest verification. An acquisition must retain the
original backing authority and its own needed data; it must never resolve through a newly opened
directory or a mutable global current base.

Qualify encoded-byte eviction, cancellation, overlapping generations and an escaped Image after
its requesting Scene retires. Explicitly state which availability guarantee a lazy backend can
provide if its backing disappears; preserve the existing eager backend's captured-file-removal
guarantee. No remote cache, package overlay or universal virtual filesystem is required. The
decision is cheap while backing is private; it becomes expensive if game APIs, native families
and package tools assume every content context owns all encoded data for its whole lifetime.

**Plan state.** The October 29 plan expressly limits this to a local eager base, and R7 already
requires future content contexts to remain immutable and bound to their original plan
([rebaseline plan:560](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L560)).
This finding refines that boundary; it does not question the completed source-free delivery
prerequisites. CP3 and CP4 need separate checks: lazy backing does not reduce a Required decode
closure, and lazy reference semantics do not evict the current base's eager bytes.

## CP5 - Import Execution Identity Is Not Yet Durable Build Authority

**Classification: intentional gap; conditional freeze gate, not a current stale-cache defect.**

**Trigger.** R6b starts reusing imported/cooked products across runs or machines, or a concrete
material/texture cook selects a target-dependent representation. Persisting the current import
key or in-process provider match as the entire build proof would leave important distinctions
implicit: the executed importer/cooker definition, consumed settings/dependencies, selected target,
and output-byte identity. This issue matters at that named upcoming boundary; the absence of a
remote cache or a generic importer registry is not itself a risk finding.

**Code evidence.**

- [import.rs:90](../../../crates/nara_asset/src/import.rs#L90) intentionally exposes a typed
  source facet with source kind, limits and preparation. Its bounded auxiliary reader is at
  [line 145](../../../crates/nara_asset/src/import.rs#L145). It does not claim a durable build
  recipe identity or target cook policy.
- [domains.rs:364](../../../src/project_content/domains.rs#L364) compares selected source
  providers by source kind and Rust `TypeId`. That is meaningful within the executable, not a
  cross-build persisted identifier. [Line 545](../../../src/project_content/domains.rs#L545)
  executes the importer; [line 585](../../../src/project_content/domains.rs#L585) defines a
  capture-local source-byte cache, not a durable derived-artifact store.
- [artifact.rs:400](../../../crates/nara_asset/src/artifact.rs#L400) has a useful recipe-like key:
  source identity/hash, dependency digest, importer version, settings, profile and output format.
  Its digest at [line 493](../../../crates/nara_asset/src/artifact.rs#L493) hashes those inputs,
  not the produced bytes. A profile label alone does not prove which target facts a cooker used.
  The current Image importer supplies explicit built-in revisions at
  [import.rs:846](../../../crates/nara_image/src/import.rs#L846).

**Existing safeguards and counterevidence.** Real source/metadata/auxiliary reads are already
captured by the bounded Host reader. Runtime codecs already have versioned definition IDs and
an independent selected-codec fingerprint
([product.rs:386](../../../src/product.rs#L386)). Runtime members have content digests;
compound Product IDs and authored continuity are separate. No observed stock persistent cache
hit uses this incomplete combination to return wrong data. The current local runtime integrity
proof is sound for its stated purpose; it is not a proof of reproducible production inputs.

**Reference mechanism.** Bevy keeps source/metadata hashes and processed-dependency full hashes
([meta.rs:94-109](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/meta.rs#L94)),
checks dependencies before skipping processing
([processor/mod.rs:1121-1141](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/processor/mod.rs#L1121)),
and publishes processed metadata separately
([line 1208](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_asset/src/processor/mod.rs#L1208)).
Godot distinguishes importer revision/settings checks from resource UID and generated-file hashes
([editor_file_system.cpp:680-691](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/file_system/editor_file_system.cpp#L680),
[lines 2684-2757](https://github.com/godotengine/godot/blob/7be59b463a4798834d72d6d0a2942ddd01d920ee/editor/file_system/editor_file_system.cpp#L2684)).
Neither inspected mechanism proves automatic hashing of all Rust/compiler/environment behavior.
Borrow tracked execution and publication boundaries, not an unsupported claim of hermeticity.

**Recommendation and delay cost.** Have the actual import/cook owner produce a finite recipe
record from its selected versioned definition, normalized settings, target-relevant facts and
observed inputs. If it consumes another processed product, record the selected product/recipe or
content digest, rather than only that product's raw source bytes. Keep four concepts separate:
logical Product identity, execution recipe key, output content digest and immutable group manifest.
Publish a whole compound group before making its recipe reusable; cached files never allocate
authoritative product IDs. Keep source parsing and Runtime decode independent.

Implement this through the existing compound consumer and one target-sensitive real consumer,
with two clean builds, an auxiliary/settings change and an interrupted group publication. Prefer
a small local recipe index and immutable blobs over a remote service or universal build language.
Deferral is reasonable until R6b/cooking begins, but exporting the current recipe-like key as a
universal cache protocol would create a migration burden for every importer and build tool.

**Plan state.** R6b already owns recipe-index/group-manifest/CAS work
([rebaseline plan:479](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#L479)).
Target build selection remains separately hardcoded in
[export/cargo.rs:91](../../../src/project_tooling/export/cargo.rs#L91): host triple, release,
desktop feature and two starter binaries. The October 29 plan's steps 3-4 retain processed Input,
target selection and the complete packaged process as open. None of that negates the now-working
processed base, compound products or first-party source-free startup/travel.

## Recommended Decision Order And Limits

Prioritize CP1 and CP2 before promising durable authored-content compatibility. They change what
an upgrade or source refactor means, and therefore affect every future editor frontend. Prioritize
CP3 before freezing travel-reference semantics. Evaluate CP4 with a concrete larger base while
its storage remains private. Gate CP5 on the already planned local reuse/cook workflow; it need
not delay a correctly qualified small processed Player.

The future custom-drawn Editor should consume these same document transactions, Schema bases,
typed occurrence addresses and asynchronous preparation observations. Drawing widgets must not
introduce a second serializer, migration interpreter, source owner or asset identity scheme.
Bevy's current reference snapshot even separates
[composable scene authoring](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_scene/src/lib.rs#L1)
from world serialization; a runtime reflection/serialization capability alone is not a complete
content-authoring architecture.

Third-party native publication is a known adjacent extension gate, not a sixth core finding.
[RuntimeCodecRole](../../../src/runtime_content/codecs.rs#L91) is closed to CPU/Image/TileSet, and
[content_residency.rs:45](../../../src/project_host/runtime/content_residency.rs#L45) owns concrete
native receipts. The October 29 plan already requires a typed publication facet before another
native family freezes. A real Material or custom-UI resource consumer should drive that work.
Bevy's
[RenderAsset preparation/retry/unload boundary](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_render/src/render_asset.rs#L40)
is relevant mechanism evidence, but does not prove Nara's same-transaction publication or physical
retirement. No generic World-mutating callback is recommended.

No new requirement for CRDT collaboration, universal reflection, remote build caching, world
partitioning, DLC or all-platform export is inferred from this study. The proposed fixtures target
specific existing ownership boundaries whose later changes would force content/API migration.
