# Asset Extension Design: Mature Engine Precedents

**Status:** Research recommendation, not an accepted architecture decision

**Baseline:** Nara at `16a655a`; pinned Bevy and Godot source checkouts listed below

**Scope:** Asset-domain extensions through project Check, Play, and Export. No benchmark or platform guarantee.

## Recommendation

Use a **core-owned workflow coordinator with domain-owned asset behavior**. The coordinator owns project
identity, authorized source access, demand, scheduling, shared dependency/result custody, and Check/Play/Export
handoffs. A registered domain owns source interpretation, settings, dependency meaning, typed products, and
domain-specific work required by those operations.

This is not a generic core decoder or a registry that merely lists importers. It is a product extension contract:
an external package contributes the domain behavior required by its workflows, while the core coordinates each
operation and enforces cross-domain invariants. This refines [ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md):
rejecting an unconsumed erased importer executor remains sound, but should not rule out a workflow-level
extension contract whose registrations product hosts actually consume.

The extension unit should be a **domain package**, not an isolated importer callback. It may contribute editor
import/authoring, Check, runtime Play, and Export handlers as needed. These can remain concrete Rust APIs; this
design does not require a stable dynamic-library ABI or commit Nara to an authoring language.

## What the engines show

| Engine | Evidence from the implementation | Design lesson |
|---|---|---|
| Bevy `0.20.0-dev` at [`8550394`](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs#L573) | `AssetApp` registers typed loaders, sources, and processors. `AssetReader` supplies source IO; `AssetLoader` interprets bytes into a runtime asset. `LoadContext` tracks dependencies and labeled subassets. `Process` is a separate source-to-processed-bytes step, optional behind a Cargo feature and overridable per source. | Keep byte access, format interpretation, runtime loading, and optional offline processing separate. Do not make a preprocessed artifact mandatory for runtime loading. |
| Godot at [`81ab961`](../../repo-ref/godot/editor/import/editor_import_plugin.h#L36) | `EditorImportPlugin` is an editor import pipeline contribution; `ResourceFormatLoader` resolves and loads resources and can report type, UID, and dependencies; `EditorExportPlugin` contributes export-time transforms, files, options, and platform features. `ResourceUID` maintains stable IDs and path resolution. | Import, ordinary resource loading, and export are distinct extension seams. A domain that needs all three must be able to compose them; one import hook does not imply the other capabilities. |
| Unity `6000.2` | `ScriptedImporter` responds to matching source extensions and `OnImportAsset` populates an `AssetImportContext` with main and additional objects. The context records source/artifact/custom dependencies for invalidation. Build callbacks are a separate pipeline. | Let a domain declare produced objects and true dependency edges; keep project/build work outside the importer callback. Import output is a reusable product, not proof of complete Check, runtime, or export support. |

### Bevy: reuse the useful seams, not its whole asset model

[`AssetApp`](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs#L573) explicitly registers typed `AssetLoader`,
`AssetSource`, and `Process` implementations. A loader selects extensions and consumes a `Reader` plus
`LoadContext` ([`loader.rs`](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L32)); IO is separately
abstracted by [`AssetReader`](../../repo-ref/bevy/crates/bevy_asset/src/io/mod.rs#L195). The server routes and
tracks loading while a typed loader owns decoding.

`LoadContext` records dependencies and offers nested loads. Labeled-asset APIs let one source produce multiple
addressable assets while retaining subasset dependency context ([`loader.rs`](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L377),
[`loader.rs`](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L488)). This is a good precedent for dependency
declarations and multi-product import; Nara need not require every domain to emit Bevy `Asset` values or use its
runtime loading model.

Offline processing is optional: Bevy's separate `Process` contract converts source bytes to processed bytes which
a selected `AssetLoader` later loads ([`process.rs`](../../repo-ref/bevy/crates/bevy_asset/src/processor/process.rs#L31)).
An `asset_processor` feature and per-source override control it; docs recommend enabling it during development,
not requiring it for normal operation ([`Cargo.toml`](../../repo-ref/bevy/crates/bevy_asset/Cargo.toml#L20),
[`lib.rs`](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs#L310)). Nara should distinguish editor import/cache
generation from runtime needs; a plugin may consume sources directly or need no import step.

### Godot: separate the three jobs

`EditorImportPlugin` advertises source extensions, settings/presets, output extension/type, import order,
format version, and threading eligibility. Its callback receives source ID/path and settings and can report
generated/platform files ([header](../../repo-ref/godot/editor/import/editor_import_plugin.h#L42),
[implementation](../../repo-ref/godot/editor/import/editor_import_plugin.cpp#L151)). It extends editor-side
`ResourceImporter`; it is not the general runtime resource loader.

`ResourceFormatLoader` separately resolves and loads resources. Its virtuals expose extensions, type, UID,
dependency listing/renaming, and `_load` ([header](../../repo-ref/godot/core/io/resource_loader.h#L48)). Godot's
`ResourceUID` maps IDs to paths and maintains a cache ([header](../../repo-ref/godot/core/io/resource_uid.h#L78),
[implementation](../../repo-ref/godot/core/io/resource_uid.cpp#L188)). Stable identity/path lookup is shared;
format-specific dependency discovery belongs to the loader/domain.

`EditorExportPlugin` has export-start/end and per-file hooks, resource customization, options, and platform
features ([header](../../repo-ref/godot/editor/export/editor_export_plugin.h#L125)). Import, runtime load, and
export stay separate. Nara can compose their handlers at package registration without collapsing them into one
large importer interface.

### Unity: import products and invalidate from declared inputs

Unity's [Scripted Importer manual](https://docs.unity3d.com/6000.2/Documentation/Manual/ScriptedImporters.html)
describes extension matching and calls to `OnImportAsset` for new/changed sources. Its scoped
[`AssetImportContext`](https://docs.unity3d.com/6000.2/Documentation/ScriptReference/AssetImporters.AssetImportContext.html)
provides the source path and output API: `AddObjectToAsset` registers identified main/additional objects and
`SetMainObject` selects the primary object. `DependsOnSourceAsset`, `DependsOnArtifact`, and
`DependsOnCustomDependency` declare distinct invalidation causes. The importer owns conversion and outputs;
the engine owns scheduling and dependency-driven reimport.

Unity documents build hooks separately in [customizing the build pipeline](https://docs.unity3d.com/6000.2/Documentation/Manual/BuildPlayerPipeline.html).
An importer produces Asset Database objects and declares invalidation; it does not itself provide project Check,
runtime preparation, or export packaging. Avoid copying this editor-only API shape: Nara's contract should also
support headless Check/Play/Export and ordinary Rust packages.

## Proposed Nara contract

### Ownership and flow

1. **Core coordinator:** owns stable identity/references, authorized source snapshots, project/catalog views,
   operation demand, task custody/cancellation, dependency edges, freshness/publication receipts, diagnostics,
   and dispatch. It does not decode formats or dictate domain layouts.
2. **Domain package:** owns source matching/settings, decoding/import or direct runtime loading, typed products and
   optional sub-assets, dependency declarations, domain checks, runtime behavior, and export representation.
3. **Operation hosts:** Check, Play, and Export request only needed capabilities/products. They share identity and
   dependency facts without an all-purpose “import everything first” phase.

The core should expose narrow, operation-specific contribution APIs consumed by Editor and headless hosts.
Implementations stay typed where statically linked. A package that only imports editor data is not Play-ready;
it may implement each lane independently and declare the capabilities it supplies.

For Nara, source-carrying project workflows and runtime preparation are separate from editor-time import. A host
may admit an authorized source snapshot for domain inspection/checking, runtime-ready CPU data, or direct
packaging; another domain may choose a cached imported product. Backend preparation (such as creating a GPU
resource from backend-neutral image data) is a later runtime responsibility. Neither an import artifact nor an
ECS asset registration should universally gate Check, Play, or Export. This follows Nara's existing
[domain-owned Image import and typed publication](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md#L39)
and separates it from the rendering backend.

### Minimum registration metadata

Require only dispatch and selection data: stable domain ID, source selectors, produced resource type IDs when
needed, and operation handlers/capabilities supplied. Keep settings, schemas, labels, dependency parsing, and
specialized options in the domain. Avoid speculative priorities, universal sniffers, generic decoder settings,
or another provider-ID taxonomy. Reject registration only for real ambiguity, such as duplicate IDs or
conflicting exclusive source selectors.

### Unknown formats, missing plugins, and validation

Unknown/unsupported resources remain addressable; discovery and reference inspection need no matching plugin.
Report an unavailable handler at the operation that requires it: Check for a required invariant, Play for a
required runtime product, Export for a required package product. Unrelated assets/operations proceed. Never
silently delete, rewrite, or claim to validate unknown data.

Validate at the owning boundary: bounded/authorized source admission on read; format/domain semantics in the
parser; duplicate IDs, stale results, dependency closure, and publication ownership at coordinator handoff; and
operation readiness at Check/Play/Export. Reuse accepted metadata/products while source, settings, and
dependencies are unchanged. Avoid repeated whole-project scans/deep parsing, speculative preflight, and core
validation of domain rules. Every check protects a named invariant at a named boundary.

Test third-party packages through the first-party public registration path and a real fixture's full sequence:
discovery/selection, settings/dependency declaration, Check, Play, and Export. A missing lane is an explicit
operation-specific capability gap, not a reason to force unrelated work through a monolithic importer.

## Nara fit and limits

The design builds on stable identity and domain-owned execution in ADRs 0103/0115, extending the current
Image-specific slice. Project content dispatch admits only Image and TileSet
([`project_content.rs`](../../src/project_content.rs#L1663)); replacing the branch with an extension map alone
would not supply source ownership, operation dispatch, dependency receipts, or Export. Image import remains
concrete and domain-owned as ADR 0115 specifies.

Retain `bevy_ecs` for now. No cited precedent proves Nara needs a custom ECS; resource extensions and ECS choice
are separate decisions. Revisit only if a concrete workflow cannot be expressed cleanly on the current substrate.

This does not select a universal cache format, atomic multi-product publication protocol, marketplace, sandbox,
dynamic Rust ABI, or import-on-every-load policy. These need separate product evidence. The precedents establish
neither comparative performance nor cross-platform guarantees.

## Source versions

- Bevy: `repo-ref/bevy`, commit `85503941e488152711a4695bbe248e9ed9f453fc`, `bevy_asset` `0.20.0-dev`.
- Godot: `repo-ref/godot`, commit `81ab961f91066a63bcbf3ec340962bb1a1ada558`.
- Unity: official `6000.2` Manual and Scripting API pages linked above.
