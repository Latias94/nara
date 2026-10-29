# ADR 0122: Scoped Imported Product Identity And Authored Continuity

**Status:** Accepted
**Date:** 2026-10-29
**Amended:** 2026-10-29 by complete Runtime product-to-handle binding
**Owner:** `nara_asset`, concrete resource formats and authoring metadata owners
**Extracted from:** [Proposed ADR 0083](0083-durable-project-asset-and-document-entity-identity.md)
and [Proposed ADR 0087](0087-asset-dependency-import-product-and-artifact-publication-graph.md)
**Related:** [ADR 0103](0103-concrete-domain-owned-asset-import-execution.md),
[ADR 0049](0049-untrusted-project-input-and-parse-budget-policy.md)

## Context

The independent text-bundle consumer has one source asset, multiple separately selected text
products and an auxiliary source. Product names and row positions can change. Source identity,
product identity, source revision, processed bytes and runtime residency therefore require
different equality rules. The larger identity and artifact graph proposals contain these roles,
but their project-wide migration and crash-safe group publication are not prerequisites for
choosing the smaller logical product contract.

## Decision

`ImportedProductId` is either the reserved primary product or a non-nil opaque auxiliary UUID.
`AssetProductRef` combines one non-nil `StableAssetId` with that product ID. All product kinds
inside a source share the same ID namespace. Rust type, product kind, display name, path, content
digest and package/mount generation are not extra identity fields. A descriptor or concrete format
checks expected kind and Schema separately. The reference supplies no loading, mutation or
publication authority.

The primary identity names the source's main product, including a compound bundle. Its auxiliary
products keep their own IDs; the main bundle may own their complete immutable values. This does
not require independent filesystem members, generic subresource residency or a global product
registry. A complete reference can be consumed by that concrete bundle before every Scene source
or Runtime store is migrated. The declared-reference integration uses `AssetRef::Product` in the
existing Schema asset-reference leaf, traversal and rewrite. Ordinary path/stable source references
remain primary selectors. Traversal preserves the complete product reference; resolving its source
record is not proof that its product exists. The project Host asks the prepared domain to validate
the selected product before publishing content. Source-oriented primary-only handle resolution
and Prefab-source loading refuse auxiliary selection instead of discarding it. These integrations
retain their existing authority and require their own qualification.

`AssetServer` owns complete Product-to-runtime-ID bindings, with real source paths retained only
as authoring aliases of the source's primary product. Equal auxiliary IDs in different sources,
and primary versus auxiliary within one source, remain separate runtime identities. One complete
Product has one Rust type claim per catalogue. `stable_id`/stable-source lookup denote the primary
product only and cannot erase an auxiliary selection.

A concrete content owner may `reserve_product` after its own product admission. Runtime
`AssetRef::Product` resolution uses only an existing complete binding and rejects unknown or
wrong-typed products without allocating another identity. It does not require a fabricated source
database. Reverse conversion preserves auxiliary Product identity and pathless primary Product
identity. Source-record reservation joins its actual path and primary Product transactionally;
database-based source resolution still refuses auxiliary selection because a source record alone
does not prove membership. Nil source identities reject before binding.

These are weak runtime identities, not resource leases or catalogue integrity. The content owner
still validates actual product existence, selected codec/family and receiving resource values.
Scene preparation consumes that receiving World's bindings through the ordinary scratch batch,
then validates the same catalogue baseline at publication. Missing, mistyped or changed bindings
do not publish a replacement or change last-good products. No global subresource loader follows.

For later acquisition, the owner can reserve new complete bindings in the existing unpublished
component/Scene asset batch and prepare actual values through the existing product transaction.
This retains one originating-World/catalogue baseline and one Scene publication path; cancelled or
rejected candidates may consume process IDs but publish neither bindings nor resource values.

`AssetDomain::contains_product(product, id)` is a trusted, pure query over existing product data
with no allocation or I/O and the primary product as its default. The source Host invokes it
under the domain's existing preparation budget and panic boundary, after preparing the actual
value. Cached compiled Editor Play rechecks edited selections against that same prepared value
under its existing content budget and a contained query. Required targets are collected and deduplicated
with source discovery; they are not inferred from metadata or collected through a second walker.
`ProjectAssetCollection::get` selects only the primary product. Compound consumers explicitly
select the enclosing source, then pass the complete reference to their domain's own lookup API.

Processed acquisition additionally invokes the selected `RuntimeAssetCodec`'s
`contains_runtime_product(product, reference)` for every mapping of the verified member. This
pure query validates the complete enclosing-source and product identity carried by the actual
decoded payload. The file-local family query alone cannot establish enclosing-source agreement.
Catalog membership, format admission and native residency remain separate proofs under ADR 0007.

Source-copy remapping preserves the internal product ID while changing an explicitly mapped
enclosing stable source identity. A path-only destination cannot express that identity and
rejects. An explicit complete-product mapping may choose a different product. This does not
grant a Prefab-copy operation ownership of arbitrary compound sources or their sidecars.

Canonical auxiliary IDs use lowercase hyphenated UUID text. Primary and auxiliary forms are
explicitly discriminated; a nil UUID is not another spelling for primary. Duplicated source
content uses a newly allocated enclosing source identity. Equal product IDs in different sources
remain distinct references.

## Authored Continuity

The source format selects its durable continuity authority:

- Intrinsic stable source IDs may supply validated logical identity.
- Otherwise, author-persisted metadata or a sidecar owns the explicit source-element-to-product
  mapping. It is versioned data outside generated caches, travels with the source and is required
  for import. A lost or ambiguous mapping rejects instead of minting replacements.

For raw rows without intrinsic IDs, the named text-bundle consumer binds its complete row mapping
to the exact current raw-source digest. That digest is a revision guard, not product identity.
Changed bytes require an explicit authored rebind with complete assignments against those new
bytes. Equal bytes retain the existing mapping; the digest cannot detect an equal-byte exchange
or establish an edit history. Reorder, rename and content edits preserve an ID only because the
author explicitly selects that continuity. Names, row indexes, hashes and fuzzy matching never
infer it. Known-retired IDs
are not reassigned by that lineage. A reduced or manually replaced lineage does not prove
unlimited historical non-reuse.

A read-only importer consumes this authority and publishes no metadata. Initial authoring or
rebind prepares finite validated values. The existing source owner publishes them with its actual
persistence guarantees. A stale source/metadata pair rejects safely; this decision does not claim
atomic source-plus-sidecar Save or introduce another journal. A coupled source/reference rewrite
still needs its own qualified cross-file owner.

An Editor facet offering Reimport for such a domain validates the actual previous settings against
the candidate before publication. Candidate-only validation cannot establish retired-ID retention.
The existing exact metadata receipt fences that previous basis. A pure rebind helper does not
substitute for admission through the real authoring entry. A file manually rewritten or restored
outside that supported transition supplies only the current admitted lineage, not historical
proof that it retained every retired ID.

## Operation And Publication Boundaries

`AssetDomain` identifies a typed CPU product family. Source/import and processed Runtime codec
facets are independently implemented and selected under ADR 0103. The family marker alone
installs neither. A Runtime-only package does not implement an importer merely to name its product,
and source-only Check success does not imply Runtime delivery support.

The concrete Runtime format records its complete admitted source/product identities, product
kind/Schema and cooked values with bounded deterministic encoding. It has no authoring-source,
metadata or cache fallback. Source revision and dependency/recipe facts may inform build
provenance without participating in logical identity. Codec success alone is neither an
artifact-group publication nor a mounted Runtime catalog.

Root Hosts retain source receipts, aggregate budgets, task custody, exact selected composition,
Scene admission and last-good publication. The bundle owner retains parsing, continuity checks,
semantic validation and its concrete Runtime codec. Generated cache deletion cannot erase
continuity authority. A rejected whole bundle publishes none of its new products.

## Alternatives

Name/index/label identity is simpler to encode but can retarget old references after ordinary
source edits. Content-derived identity changes whenever a value changes and collapses equal
products. Cache-only allocated IDs disappear after a clean build. These alternatives are rejected.

A universal global object UUID loses source and product scope. One combined source/Runtime trait
would force absent capabilities into fake implementations. The selected contract preserves
scoped identity and independently executable operation facets.

## Verification And Limits

The [ordered plan](../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md) requires two
products, an auxiliary dependency, exact source binding, explicit rename/reorder, missing/duplicate/
retired refusal, source-copy scope and deterministic processed round trips. Public Check/Play and
Scene replacement must exercise the compound owner. Complete source-free startup/later Scene
loading, Runtime catalog publication, local CAS, dependency graphs, package overlays and
project-wide document migration remain separate work under their owning decisions.
