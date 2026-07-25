---
type: "Research"
title: "Unity primary-source research: content and package graph"
description: "Evidence on Unity Asset Database, import artifacts, UPM packages, extensibility, and content builds for Nara package and asset architecture decisions."
timestamp: 2026-07-22T05:33:13Z
record_id: 200c27a7e80c4a48b14e9206456a4f57
producer_id: codex-subagent-research-unity-content-package
run_id: 2026-07-22-content-package-graph
tags:
  - unity
  - asset-pipeline
  - package-manager
  - primary-sources
---

# Scope

This research examines Unity's official documentation for the boundaries between
source assets, imported artifacts, packages, editor/runtime extensions, and built
content. It is evidence for Nara design work, not a claim that Unity's file formats,
runtime, or language model should be adopted.

# Primary-Source Findings

## Asset identity is distinct from source location and derived output

Unity's Asset Database treats an original file as a source asset and its imported
counterpart as an artifact. Import settings and a GUID live in metadata stored next
to the source asset; the GUID connects the source to the artifact. The source and
artifact databases also retain hashes and import-dependency information. Artifacts
are cache data and can be regenerated from source assets, import settings, and
project settings. Platform is part of the cache key for built-in importer output.

This is a useful separation of concerns:

- a stable logical identity survives a move or rename;
- the source file and its editable import settings are durable project input; and
- importer output is derivable, target-aware cache state rather than authoring truth.

The API confirms that a project-relative path maps to a GUID and that a GUID maps
back to a current path. The API also documents that a deleted asset's GUID can
remain queryable for the rest of an Editor session, which is a reminder that
identity-resolution state and filesystem existence are different facts.

Sources:

- [Contents of the Asset Database](https://docs.unity3d.com/Manual/asset-database-contents.html)
- [AssetDatabase.AssetPathToGUID](https://docs.unity3d.com/ScriptReference/AssetDatabase.AssetPathToGUID.html)
- [AssetDatabase.GUIDToAssetPath](https://docs.unity3d.com/ScriptReference/AssetDatabase.GUIDToAssetPath.html)

## Importers are extension points, but their execution must be isolated and deterministic

Unity packages can provide custom importers. Scripted Importers register file
extensions and receive import callbacks for matching source changes; each import
must nominate one main asset and may add sub-assets. Asset postprocessors can alter
or inspect built-in import flows.

Unity's parallel-import documentation explicitly requires postprocessor code to be
self-contained and deterministic, and warns it not to mutate editor-global state or
create files as an import side effect. Parallel import may execute in separate
worker processes, so static mutable state is not an authoritative channel between
the importer and the editor.

This supports a Nara rule that an importer is a bounded transformation with declared
inputs, output records, diagnostics, and dependency edges. It must not acquire a
general editor/world mutation capability merely because it participates in import.

Sources:

- [Scripted Importers](https://docs.unity3d.com/Manual/ScriptedImporters.html)
- [AssetPostprocessor](https://docs.unity3d.com/ScriptReference/AssetPostprocessor.html)
- [Parallel asset import](https://docs.unity3d.com/Manual/ParallelImport.html)

## A UPM package is a distributable dependency unit, not merely a source-asset archive

Unity defines a package as a container for editor tools and libraries, runtime tools
and libraries, asset collections, or templates. Its root `package.json` provides a
package name, version, dependencies, compatibility information, and user-facing
metadata. The Project manifest is the dependency request that the Package Manager
resolves against registries and other package sources.

The Package Manager UI and scripting API support registry, local-folder, Git URL,
and tarball installation, as well as update and removal. Removal changes a direct
project manifest dependency; a transitively required package remains available.
This is dependency-graph behavior, not a destructive delete of every file or cached
copy. Unity also distinguishes imported `.unitypackage` asset archives from UPM
packages; importing an archive copies selected content into the project `Assets`
tree, which has different update and removal semantics.

Sources:

- [Unity's Package Manager](https://docs.unity3d.com/Manual/Packages.html)
- [Package manifest](https://docs.unity3d.com/Manual/upm-manifestPkg.html)
- [Adding and removing packages](https://docs.unity3d.com/Manual/upm-ui-actions.html)
- [Removing an installed package](https://docs.unity3d.com/Manual/upm-ui-remove.html)
- [Importing local Asset packages](https://docs.unity3d.com/Manual/AssetPackagesImport.html)

## One package can contain editor, runtime, asset, test, sample, and documentation contributions

Unity's documented custom-package workflow permits C# scripts, assemblies, native
plugins, models, textures, animations, audio, and other assets in one package.
Its recommended layout has distinct `Editor`, `Runtime`, `Tests`, `Samples~`, and
`Documentation~` areas. The folder names alone are mostly convention; assembly and
manifest metadata determine code participation. Samples and package documentation
are intentionally excluded from normal asset metadata tracking through the `~`
suffix convention.

The important product lesson is not the exact layout. It is that a package can be
a coherent installable product unit with multiple contribution kinds, while the
engine still needs explicit lifecycle and compatibility rules for each kind.

Sources:

- [Creating custom packages](https://docs.unity3d.com/Manual/CustomPackages.html)
- [Package layout](https://docs.unity3d.com/Manual/cus-layout.html)
- [Naming your package](https://docs.unity3d.com/Manual/cus-naming.html)

## Content build and package resolution are separate pipelines

Unity Addressables processes groups into a content catalog and AssetBundles. It can
run as part of a player build or as a separate content-build step, and it supports a
previous-build update path. This demonstrates that deployable/cooked content is a
third graph: it consumes resolved source/import artifacts and target/profile policy,
but is neither the package dependency graph nor the editor's imported-artifact
cache.

The cited Addressables document is a package-specific system, not a mandate for
every Unity project. Its relevance to Nara is architectural: package resolution,
asset import, and target-specific cooking should remain independently inspectable
operations with explicit handoffs.

Source:

- [Addressables content builds](https://docs.unity3d.com/Packages/com.unity.addressables@1.20/manual/Builds.html)

# Implications for Nara

## Adopt the separation, not Unity's implementation

Nara should retain four explicit, linked but non-interchangeable graphs:

1. **Package dependency graph**: immutable package identity, version, source,
   dependencies, capabilities/contributions, trust/provenance, and activation
   state.
2. **Project content graph**: durable source asset identity, logical path, `.meta`
   equivalent/import settings, source dependencies, and diagnostics.
3. **Derived artifact graph**: importer version, settings digest, source/dependency
   digests, target/profile key, artifact records, and cache location. It must be
   disposable and regenerable.
4. **Cooked delivery graph**: selected platform/profile, content catalog/chunks,
   runtime dependency closure, and reproducible output metadata.

A package may contribute source assets, schemas, importers, editor extensions,
runtime adapters, templates, samples, and documentation. Each contribution should
be declared in package metadata and admitted through its own boundary; package
installation must not silently grant arbitrary in-process editor or runtime code
execution.

## Do not copy Unity's weak points

- Do not make a path or an artifact-cache filename the persistent asset identity.
- Do not make generated artifacts version-controlled authoring truth.
- Do not collapse UPM-style managed packages and imported asset archives into one
  install/remove/update mechanism. Nara should have one user-facing Package product,
  but internally distinguish managed immutable packages from explicit project-local
  content imports and overrides.
- Do not let package metadata select arbitrary Rust plugin/provider identities by
  string. This remains incompatible with Nara's accepted manifest and composition
  boundaries.
- Do not expose importer code to the editor `World`, runtime `World`, or unrestricted
  filesystem state. Worker/process isolation and deterministic import contracts are
  more important than copying a C# callback API.
- Do not make an Addressables-like remote-content/cooking product a prerequisite for
  the basic package manager. It needs a target build tracer and a separate admission
  decision.

# Questions Raised for the Main Design

1. What is Nara's durable package identifier and version/compatibility format, and
   what lockfile preserves a resolved package graph reproducibly?
2. Which package contribution kinds can be enabled without restarting the editor,
   and which require an isolated Extension Host restart or a fresh runtime?
3. What is the minimum importer declaration that makes input budgets, dependency
   tracking, deterministic execution, artifact invalidation, and diagnostics
   enforceable before third-party importers are admitted?
4. How does a removed or unavailable package preserve scene/schema references for
   inspection, recovery, migration, and later reinstallation without treating
   missing code as valid runtime behavior?
5. Which build/cook features are needed by the first real 3D vertical slice before
   a content-catalog or remote-update design is considered?

# Citation Notes

All external sources in this record are Unity-owned documentation. The cited pages
were consulted on 2026-07-22. Their terminology is preserved only where it helps
compare product boundaries; Nara names and public formats remain independent design
decisions.
