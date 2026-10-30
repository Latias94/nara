# Local Artifact Build Authority And Publication

Date: 2026-10-31. Status: finite typed local-build contract implemented and qualified in the linked scope.
Parent: [foundation R6/R7](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r6-resource-facets-durable-continuity-and-local-artifact-store).
Owners: [ADR 0007](adr/0007-asset-identity-and-import-pipeline.md),
[ADR 0103](adr/0103-concrete-domain-owned-asset-import-execution.md) and the bounded
artifact-group slice of [Proposed ADR 0087](adr/0087-asset-dependency-import-product-and-artifact-publication-graph.md).

## Required Outcome

One compound source importer and one real target-sensitive cook complete two clean builds,
dependency/settings/target changes, interrupted group publication and fresh-process reopen.
Logical Product identity survives deletion of generated cache. Reuse is justified by current
inputs and a complete verified successful group, never by a profile label, runtime TypeId,
synthetic cache filename or previously successful but now stale output.

This work follows the qualified reference-demand and retained-pack units. Runtime bases remain
separate consumers of selected build outputs; Runtime acquisition does not invoke a builder.
The source recipe correction and CPU Image transform now have
[bounded qualification](../knowledge/engineering/verification/2026-10/2026-10-30T201802Z-source-import-recipes-and-target-sensitive-cpu-image-cook-prerequisite-qualification-462c0179d20b44fd8545d4c135e814f9.md).
The [complete local-build qualification](../knowledge/engineering/verification/2026-10/2026-10-30T231610Z-tracked-local-artifact-builds-and-complete-group-publication-qualification-d3fcd08e15e04d45812ae09d7c81e944.md)
records tracked compound/processed Image execution, verified reuse, complete publication and
original Runtime custody. The bounded contract is selected in
[ADR 0103](adr/0103-concrete-domain-owned-asset-import-execution.md#finite-typed-local-builds-2026-10-31-refinement),
without accepting the rest of Proposed ADR 0087 or qualifying target executable/package delivery.

## Identity And Ownership

| Fact | Meaning |
| --- | --- |
| Product | Authored source and primary/auxiliary identity; generated storage cannot allocate it |
| Import recipe | Actual source/settings/dependency fingerprint and versioned importer meaning |
| Closed build recipe | Selected import/cook definitions, effective settings, consumed target facts and observed raw/processed inputs |
| Artifact content digest | Exact output bytes and length |
| Group manifest | Complete output membership, formats, Product mappings and provenance for one successful recipe |
| Recipe lookup index | Generated discovery hint for the last successful group; not output identity or permission |

At baseline `96f4519`, `ImportArtifactKey` hashes recipe inputs, while `ImportArtifactRecord` invents
a path without publishing bytes there. Replace this model with `ImportRecipe` and
`ImportRecipeDigest`. Keep source identity/hash, dependency digest, importer identity/revision and
effective settings hash. Remove the synthetic record/path and unconsumed profile, output-label
and artifact-format fields from source Image provenance. Exact Image pixels keep their existing
independent content hash. Native asset/reload state names the import recipe accurately.

This source recipe is not a claim that an output file or successful reusable group exists.
Persistent reuse additionally needs the closed build recipe, actual output digests and complete
group admission below. Old names/readers/compatibility aliases are removed.

## Executing Contract

Source preparation stays a typed, effect-free CPU operation below root/Scene/presentation.
Source-only `Settings: DeserializeOwned` does not acquire an unconditional Serialize/default
requirement. Optional compiled build/cook facets name explicit versioned implementations and
normalize their own effective typed settings. The executing owner records the same selected
definitions and normalized values that it uses; arbitrary unused settings bytes or an in-process
TypeId cannot stand in for these facts.

The optional source-build facet admits/normalizes its typed settings before preparation. The
executing owner canonically serializes those normalized values together with the actual source
identity, then passes that admitted metadata to the same source importer. The recipe records
those exact normalized bytes. Domain-semantic set normalization happens in the domain; JSON
object ordering alone cannot normalize a continuity assignment Vec. Original metadata file
digests remain publication/revalidation observations, not a substitute for semantic settings.
This avoids hashing one settings claim while executing different unresolved/default values.

A finite tracked context distinguishes raw bytes/presence observations from processed inputs.
Raw observations retain the actual scoped logical locator and digest of the private bytes used.
Processed observations retain the selected committed group, member content/format and Product
selection, rather than merely that Product's raw source hash. Successful execution replaces its
dependency set. A prior manifest can supply a lookup/revalidation read plan; it cannot decide a
hit before current observations agree. Optional absence probes matter if later presence can
change execution.

Cook target facts belong to the selected concrete cooker. They must affect actual output
selection/representation; hashing an unused target label does not qualify target cooking.
Build-only inputs remain distinct from Immediate/Deferred runtime references. Runtime decode
selection and native publication retain their existing owners.

The first concrete transform is a pure CPU Image cook. Its explicit `Reject` or proportional
`FitNearest` policy consumes a maximum 2D texture dimension and produces Runtime-compatible
pixels with the same Product. Nearest selection preserves source texels/color interpretation;
it does not silently convert a TileSet atlas. Two valid 4K/2K outputs and an actual Runtime
roundtrip distinguish transformation from a target-label-only key or admission-only experiment.

Trusted in-process Rust must honor the tracked-input and declared allocation contracts. The
typed context is not an OS sandbox or automatic proof of compiler/environment hermeticity.
An implementation that cannot meet tracked execution cannot claim reusable output.

## Finite Execution Protocol

Normalize before a hit is possible: admit current metadata identity/path/kind, normalize domain
settings, construct the build seed from the actual normalized identity/settings and primary
digest, probe a prior observation plan, replay current observations, then verify the complete
group. The seed also binds the selected importer/encoder/cooker definitions, their explicit
implementation authority and output contracts. Generated metadata cannot select code. The
implementation authority must name its source, such as the actual compiled tool digest or
bounded implementation-input digest; a manually declared revision alone is no automatic proof
that a changed binary can reuse an old group.

The first consumer needs required raw reads, optional raw reads and processed Image reads.
Optional absence is an actual missing-input observation; permission, parse and I/O errors do not
become absence. Processed reads bind a verified upstream group/member/format and complete Product.
The Image cook adapter decodes that fixed input, then calls the pure CPU transform. Changing
upstream color-space settings while PNG bytes remain equal must invalidate the cook. A real
optional branch in the compound importer proves absence-to-presence invalidation.

The shared layer's protocol is admission, `Hit | Miss | Invalid` probe, typed execution and
commit. It does not discover importers or schedule a general graph. The compound adapter derives
primary/auxiliary mappings from its actual typed output; it does not invent unused output blobs.
Member removal replaces the whole group mapping. A finite multi-member build consumer supplies
the separate multi-blob publication interruption oracle.

## Local Publication

Prefer a finite local recipe index, immutable expected manifests and content-addressed blobs.
Publish verified blobs first, then the complete manifest, and only then the reusable recipe-index
entry. The final index publication is the visibility boundary. A failure before that point may
leave unreferenced generated objects but cannot expose a successful half group or discard an
already admitted predecessor.

Use the filesystem owner's exact capabilities, bounded reads, temporary-file publication and
actual sync/cleanup receipts. Do not reimplement a pathname-based transaction framework.
Reopen validates the index, manifest and every selected member before reporting a hit. Missing,
truncated or corrupt generated objects are an explicit invalid-cache result requiring rebuild;
they cannot become Product identity or silently usable last-good output for a changed recipe.
State durability tiers honestly when file/directory sync is unsupported or publication is
ambiguous. Content-addressed names fix expected bytes, not portable OS-enforced immutability.

Same-recipe/different-output results are a determinism conflict; do not silently overwrite a
successful recipe association. Concurrent identical publication may reuse only an independently
verified complete existing object. Existing Runtime acquisitions retain their own immutable base,
plan and decoded/native custody regardless of later cache removal or new builds.

Keep a seed-to-closed-recipe hint separately from a closed-recipe-to-successful-group association.
The latter catches A-to-B-to-A executions whose second A produces different bytes. A changed
recipe's failure leaves its predecessor as last-good, never a current `Hit`. Generated-store
limits bound these indices; cache reset does not preserve an unlimited determinism history or
destroy authored Product continuity.

Commit serializes cooperating store owners through one stable retained lock, then reopens and
verifies the bounded current index, original lock/parent bindings, request fence and current input
observations before publication. Filesystem lock guarantee tiers remain explicit; advisory
coordination is not a hostile-writer sandbox. `ExpectedTarget::Identity` is a binding check, not
an atomic compare-and-swap primitive. Candidate members and manifest precede the single final
index publication. Terminal results distinguish committed group plus actual durability evidence,
rejected with cleanup custody, and uncertain publication with evidence/cleanup custody. Do not
report a post-publication ambiguity as definitely unpublished.

The initial verified group captures its complete encoded member bytes under a retained budget
owner. Both the actual cook and delivery adapter use those exact bytes, rather than validate then
reopen a name. Decode/cook/encode overlap is admitted by the executing owner. Delivery emits the
existing Runtime catalogue/pack and uses its original executable plan; no new Runtime reader is
needed. Actual cache deletion, successor build and independent base admission must leave escaped
old acquisitions unchanged.

## Implementation Order And Open Decisions

First replace the misleading source recipe/artifact types through complete existing Image,
reload, rendering-preparation and independent consumers. Then implement tracked closed recipes
and local group publication through the actual compound source/runtime codec consumer. Close
the concrete target-sensitive and processed-dependency cook before the general build/cook
interface freezes. Do not count the source provenance correction as completed R6b.

The consumer audit selects the pure optional `nara_asset::SourceAssetBuild` facet below root,
Scene and presentation. The root `asset_build` module, enabled by `project-workflow`, owns
`ArtifactSourceRequest`, `build_source`, concrete `build_image_import` / `cook_image_group`
adapters and `ArtifactBuildStore`. Reuse of filesystem capabilities, aggregate operation budgets
and Runtime-delivery consumers belongs at this Host boundary; it does not justify a new crate
or moving Image orchestration into `nara_asset`.

`ArtifactBuildResult` owns either a verified reuse group or a successful publication receipt
with its rebuild reason. `Hit`/`Built`, current bytes and optional `publication_receipt()` are
views of that one outcome. Store commit and the build facade preserve the actual index
replacement, lock and directory-sync evidence. An `Uncertain` error retains that receipt and its cause. Runtime
catalogue/pack admission and typed publication remain the existing delivery owners. No generic
erased executor, new VFS, remote cache, language-neutral build graph, package resolver or
automatic native trust is selected.
