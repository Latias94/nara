# Source Locations And Reference Resolution

**Status:** Source addressing/resolution implemented and qualified under ADRs 0007/0038;
Source Scene dependencies remain the next implementation unit.
**Date:** 2026-10-29.
**Owner plan:** [Asset import and Runtime delivery](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md#source-scene-reference-successor).

## Problem

The predecessor source catalogue uses a root-relative `AssetPath` as a global database key across
Assets and Prefabs. Adding Scenes to the same scanner would make another document kind visible,
but would preserve the location ambiguity. `assets/shared.json`, `prefabs/shared.json` and
`scenes/shared.json` are different source locations with the same local spelling. A kind-based
namespace also cannot distinguish two resource directories serving the same kind.

Several existing consumers use the record path both as a catalogue key and as a path relative to
an already admitted directory. Other consumers export that path back to authored data. Merely
prefixing the stored string would produce reads such as `assets/assets/textures/a.png` or save
an invalid Assets-relative alias. The correction must reach both directions of that contract.

Relevant current owners are the [database](../../crates/nara_asset/src/database.rs),
[AssetServer](../../crates/nara_asset/src/server.rs),
[Image directory](../../crates/nara_image/src/import.rs),
[source content loader](../../src/project_content.rs),
[delivery union](../../src/project_content/delivery.rs) and
[Authoring catalogue](../../src/project_host/authoring/project/catalogue.rs).

## Selected Representation

Separate three facts before implementing the Source Scene dependency consumer:

| Fact | Example | Owner |
| --- | --- | --- |
| Canonical source locator in one admitted catalogue | `assets/textures/player.png` | Asset record, identity index, source binding and change observation |
| Authored reference alias | `textures/player.png` in an asset field; `player.prefab.json` in a Prefab declaration | Explicit reference-resolution context |
| Read path relative to a particular capability | `textures/player.png` under the Assets directory | Concrete admitted source reader |

`nara_asset` owns a validated lexical locator and its indices without depending on
`nara_project::ProjectPath`, a manifest or filesystem capabilities. The project Host binds actual
roots and sidecar-relative paths to canonical project-relative locations. The low-level meaning
is a canonical logical location in an admitted catalogue, not an enum of Assets/Prefabs/Scenes.
A future concrete package source can bind a disjoint logical prefix to its own reader without
changing Product identity or requiring the record to know package execution policy.

The selected contract must keep standalone source consumers usable with one explicitly bound
logical directory. It must not require a project manifest to resolve an ordinary local asset.
No mount registry, URI grammar, virtual filesystem or new dependency executor is selected here.
The selected owner API uses `AssetSourcePath`, an owned `AssetPathScope` and a borrowed
`AssetReferenceResolver`. The catalogue owns no default reference root; callers bind that context
explicitly. Standalone callers use `AssetPathScope::ROOT`. The
[source/UI qualification](../knowledge/engineering/verification/2026-10/2026-10-30T015832Z-canonical-source-locations-and-typed-ui-image-binding-qualification-0ce5bafdc65f4893aa0079009668e41a.md)
records the public consumer, source loaders, watch/reload, Export and typed UI results.

Sidecar paths can remain relative to their authored source directory. Converting metadata into a
catalogue record needs the actual binding; an unconditional `From<AssetMetaIdentity>` cannot
invent it. Global stable-ID uniqueness, complete primary/auxiliary Product identity and source
content revisions remain independent of location. A source move with admitted metadata can keep
its Product while changing its locator.

Under [ADR 0038](adr/0038-scene-prefab-authoring-identity-and-provenance.md), a `ProjectLocation`
source key still uses its authored universe and complete document path. `StableMetadata` requires
the actual UUID. A catalogue locator cannot silently upgrade one origin to the other.

## Alternatives

| Candidate | Useful property | Problem to resolve |
| --- | --- | --- |
| Canonical logical locator with explicitly bound alias/read paths | Fits current receipts and source capabilities, supports same-named sources, keeps identity separate | Requires migrating every current implicit conversion together |
| Open source-scope identity plus a scope-relative path | Expresses independent source namespaces | Requires scope equality, allocation, capability binding, snapshot continuity and export rules before any current consumer needs that extra identity |
| Separate document identity index and resource database | Keeps document data storage distinct from importer output | Joint UUID checks alone do not resolve Scene references through current validation/cook, and do not repair two resource roots sharing local names |

Prefer the first representation for the named workflow. Keep document data separately owned where
appropriate; a common source identity catalogue does not turn Scene documents into ordinary
importer products. A closed directory-role enum is not the second candidate's complete design.

## Consumer Migration

- Asset records, database path indices and source changes use the canonical locator. Global UUID
  duplicate checks compare actual locations; repeated observation of the same source through
  overlapping roots must not fabricate another source.
- AssetServer source bindings use that locator. An authored Path resolves through its explicit
  context before reserving the same primary Product as StableId/Product. No second runtime identity
  may appear because another call used the unprefixed spelling.
- Handle export uses the encode context to recover a valid authored alias. If a cross-root target
  cannot be expressed by the selected Path policy, preserve a complete Product or reject an
  explicitly forced Path policy. Never strip a string prefix without a bound directory relation.
- Prefab loading resolves the declaration in the Prefabs context, then locates the actual document
  by its bound canonical location. Its source key remains supplied by the document owner. Direct
  and snapshot resolvers must agree. Primary Product selection uses the resolver's admitted
  source-asset association, not an inferred change to the document identity policy.
  `ResolvedPrefabSource::with_source_asset` validates the actual Prefab record against the key;
  file and metadata receipts remain the Host's authority. A location-authored document keeps its
  original key for Path, StableId and an associated primary Product lookup.
- Image source readers retain the narrow directory capability and convert an admitted locator to
  a local read path. Do not widen their authority to the project directory to accommodate strings.
- Watchers bind a native watch root and logical prefix once, then emit canonical source changes.
  Metadata suffix processing, removal ordering and stable-ID dependency edges retain their
  existing owners. Prefix boundary checks reject similarly named unrelated directories.
- Source content, Authoring capture, Editor image catalogues and residency use the same binding.
  Delivery/export uses the record's actual project-relative location, removing the current guess
  that every non-Prefab source belongs under Assets. Receipt verification remains the authority
  for exact source files; it does not rediscover dependencies during export.

Reconcile ADRs 0007/0038 and affected guides when selecting concrete public APIs. Expected code
owners include `nara_asset` identity/database/server/reload, `nara_asset_watch`, `nara_image`,
Reflect decode/encode context, Sprite/Tilemap reference codecs and the source/Authoring Hosts.
Processed catalogue framing, complete Product wire grammar and pathless Runtime bindings need
no replacement merely because the source locator contract changes.

## Executable Comparison

Use one independent source consumer to challenge the complete representation before freezing it:

1. Index same-named sources in all three current roots, overlapping roots and two directories of
   the same role. Distinct files with one UUID still reject; one actual source observed twice keeps
   one identity. Include Windows case aliases and custom configured root names.
2. Resolve Assets-relative Path, StableId and primary Product to the same actual source/runtime
   binding. Save/Reopen cannot accidentally reinterpret the canonical locator as a local alias.
3. Resolve the same Prefab through direct and snapshot APIs, then move its real source and update
   metadata. Stable identity persists; location-authored identity changes according to ADR 0038.
4. Exercise source/metadata modification, deletion and rename through the watcher and reload
   owners. Wrong logical-prefix/capability pairs refuse before file reads; dependency propagation
   still uses the original stable identities.
5. Export from exact source receipts, relocate the export, then complete authored Door Product
   A-to-B-to-A travel. Preserve caller selection, declared-root authority, effective Prefab demand,
   flat dependent Scene retention, last-good publication and physical cancellation.

The source-contract qualification covers canonical/scoped identities, separated and nested real
source roots, the independent consumer, watch/reload owners and source-carrying CLI Export. Its
exact public UI inventory also qualifies manual image IDs, native asset bindings, Runtime/Canvas
and retained presentation packets. Step 5's authored Door travel, dependent Scene data retention,
root admission and dependent working-copy Save/refresh remain in the owner plan. Earlier processed
Scene dependency evidence does not establish that source-backed successor.
