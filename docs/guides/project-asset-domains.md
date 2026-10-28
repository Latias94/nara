# Compiled Project Asset Domains

Use a source domain when one referenced source produces immutable typed CPU data for project
Check and Play. Select the package in the same compiled `ProductRecipe` used by the game and
project tools:

```rust,ignore
fn recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    game_schema_recipe()?.add_asset_domain::<text_package::TextDomain>()
}
```

Implement the pure source/import facet with `nara_asset` and its `serde` feature. Its settings,
input, limits and error types require no root facade, Scene, 2D or Editor dependency. Product
selection, typed collections and project workflows remain in the root Host and require
`runtime-2d` and `serde`; `project-workflow` enables both. The
[independent text package](../../tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs)
and its [public workflow tests](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/workflow.rs)
are complete examples outside root workspace inheritance.

## Domain Contract

Implement `nara_asset::SourceAssetImporter` with a stable `SOURCE_KIND`, associated `Settings`
and `Product`, conservative `AssetImportLimits`, and generic `prepare<E>`. Persist
`AssetSourceKind::Other(SOURCE_KIND)` in the source's
canonical `.meta` identity. The Host routes declared persistent `AssetRef` fields by that identity.
It does not infer a handler from file extensions or crate presence. Two selected owners of the
same kind reject; a referenced kind without an owner fails Check/Play. Unreferenced files need no
handler and are not rewritten.

`AssetImportSource::metadata()` returns an identity-first candidate. Admit the package's settings
with `admit::<Self::Settings>()` and validate only domain invariants. Source-only settings need
deserialization; serialization and editable defaults belong to the optional Editor facet.
`bytes()` provides the bounded
primary source. `read_dependency(path, limit)` reads a raw auxiliary source under the same Assets
capability and records its exact digest. Paths are Assets-relative; no ambient filesystem or
project-root handle is exposed. Auxiliary files have no implied metadata or decoder. This is not
yet a cross-domain prepared-product dependency API.

All required source reads must use this context. Repeated auxiliary reads share one immutable
observation per source-domain load while each declared edge counts against the global limit.
Check visits declared Scene roots sequentially and unions their exact members. Export reopens the
receipt members and verifies their digests instead of rediscovering dependencies.

Source limits bound reads. The Host reserves parser work and retained product allocations before
calling the parser; collection and identity overhead are charged separately. Settings parsing,
temporary strings and any shared source buffers retained by a product must fit the declared
reservations. This is trusted Rust: limits are a domain promise, not an allocator sandbox. Keep
products and their destructors effect-free, and do not panic. Return a bounded `Diagnostic` with
`AssetImportError::domain` for actionable format/settings failures. `read_dependency` returns
`AssetImportError<E>`, so ordinary `?` preserves the caller's concrete read failure without
requiring a root product error type. The project adapter maps only domain diagnostics; source
IO, authority and budget errors retain their original classification. Host-reported parser
panics omit the panic payload from the diagnostic report.

```rust,ignore
impl nara_asset::SourceAssetImporter for TextImporter {
    type Settings = TextSettings;
    type Product = TextResource;
    const SOURCE_KIND: &'static str = "studio.text";

    fn limits() -> nara_asset::AssetImportLimits {
        text_import_limits()
    }

    fn prepare<E>(
        source: &mut nara_asset::AssetImportSource<'_, E>,
    ) -> Result<TextResource, nara_asset::AssetImportError<E>> {
        prepare_text(source)
    }
}
```

The generic parameter belongs to the reader's caller. Importers propagate it and author their
own domain diagnostics; they do not need to inspect filesystem or project Host errors.

The compiled-project tool preserves retained domain code/severity/safe-summary records through
`ProjectToolInvocationError::Project`; it does not infer them from `Error::to_string()`.
Inspect the bounded diagnostics and loss facts described in the
[tool failure guide](rust-product-recipes.md#inspect-compiled-tool-failures). General diagnostic
fields are deliberately not transported. This does not change the domain's preparation contract
or execute runtime plugin hooks during Check.

## Runtime Access

Borrow the installed resource from an ordinary system:

```rust,ignore
fn show_text(assets: nara::ecs::Res<nara::project_host::ProjectDomainAssets>) {
    if let Some(texts) = assets.collection::<text_package::TextDomain>() {
        for (identity, text) in texts.iter() {
            // Consume the typed CPU value together with its admitted identity.
        }
    }
}
```

`collection::<D>()` distinguishes domain types even when they use the same Rust product type.
`get(&AssetRef)` accepts either admitted path or stable identity. No mutable collection is exposed.
Clone `ProjectDomainAssets` when retaining data beyond a borrow; this keeps its generation and
budget alive. A domain whose products share allocations must retain this owner alongside them.

The Host installs products before gameplay startup. Ready Scene requests expose target
`domain_assets()` for preparation, while the current runtime keeps its old collection. An explicit
successful Scene commit changes Scene and collection together. Failed preparation or rejected
commit preserves the current values. A Scene with no domain sources installs an empty collection.
Normal runtime shutdown releases the runtime owner; escaped collection clones retain their budget.

## Scope

This source preparation runs during Check and during source-backed Play. Ordinary runtime plugin
hooks still do not run during Check. An Editor importer alone does not install this capability.
Image and TileSet retain their concrete typed stores and GPU/native publication rules. Editor
native/GPU import adapters, cross-domain products, cooking, source-free export, watchers and dynamic plugin
loading need their own concrete consumers. The metadata-only Editor facet below is a narrower
source-backed workflow. See [ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md).

## Editor Metadata Import

A trusted compiled Editor package can implement `EditorSourceAssetDomain` with `type Domain`
naming its source domain. The adapter can live in a separate crate without forwarding the
domain's parser, limits or identity. The domain's one `Settings` type owns typed admission;
the Editor facet additionally requires serialization, and `default_settings` seeds a panel-local
draft. The facet reuses `SourceAssetImporter::prepare` to validate primary and
auxiliary sources. The Host discards that temporary CPU value and publishes canonical `.meta`
only when validation succeeds. It does not create a cooked artifact or install a preview resource.

The [external text adapter and panel](../../tests/fixtures/module-consumer/editor-panel-consumer/src/text_import.rs)
show the complete typed contribution. Select the contributor in the Editor executable:

```rust,ignore
let mut composition = nara_editor::EditorNativeComposition::new();
composition.register_source_asset_domain::<text_editor::TextImportDomain>()?;
text_editor::install_panel(&mut composition)?;
composition.run(options)?;
```

For an embedded shell, consume `composition.build(options)` to obtain the fully configured
`(application, frontend)` pair. This is the same construction used by `run`; it transfers both
Host facets and panels together. The explicit advanced `build_trusted_compiled` and native
trusted runner preserve the same selection for project replacement. Their initial compiled
execution lane has no generic source index, so metadata Import remains unavailable there;
replacing it with an ordinary authoring session enables the selected facet.
`install_panels` attaches only frontend contributions to an existing shell and rejects a
composition with selected Host facets before changing its workspace.
The compiled selection persists across project replacement; operation
state belongs to each fresh session. Ordinary Open remains data-only and never loads game Rust.
The game independently selects its source domain through `ProductRecipe` for Check and Play.

Panels read `EditorFrameModel::source_asset_import`, retain their typed draft and submit
`EditorAction::SourceAssetImport`. Use the facet's `import_action(path, &settings)` to derive the
exact domain kind and encode that domain's settings without repeating transport details.
Encoding failure remains local feedback; the Host still admits the resulting dynamic action.
The action uses its current frame and indexed source authority; extensions and registration order
do not choose a format. Dispatch `Started` is not terminal success. Later frames report preparation,
`MetadataPublished`, actionable failure or a reconciliation requirement. The view follows the
Browser selection, or the latest operation target when nothing is selected.

Import creates a stable identity. Reimport preserves it and changes only the requested metadata
settings after validating the same bounded source closure. Invalid/conflicting existing metadata
is never overwritten implicitly. Failure preserves its exact prior bytes. No metadata is written
on Open, selecting a source or editing a draft. The Host reuses the existing metadata identity scan,
task runtime, file-publication receipts and cleanup custody; the panel receives no directory writer.
After publication, the previous metadata detail is unavailable until its successor catalog is
ready. A newer frame containing a retained Browser snapshot cannot start another operation against
the old sidecar binding. The existing metadata-refresh owner supplies native maintenance and
settles failed discovery without a second source-import refresh loop.
The Shell observes import-view and pending-state changes for presentation; unchanged idle polls
do not request another redraw.

This validate-before-publish policy differs from Image's separate desired settings, in-flight
attempt and last-good pixel publication. Image retains unresolved metadata observations across
rejection, failure and supersession until the current recipe publishes; old pixels do not choose
the settings for a later source-only edit. See
[publication and recovery](file-backed-tilesets.md#publication-and-recovery) for Image/TileSet
admission and stale-result handling. This source-domain facet still validates before changing
metadata bytes.
Editor native/GPU publication, arbitrary import-only products, automatic reimport, durable caches
and dynamic project plugin loading remain separate contracts.

## Executable Delivery

Declare the domain package in the game's own Cargo dependencies and register it in the product
recipe shared by the tooling, headless and desktop targets. A recipe supplied only by a test or
development Host does not install the domain in a separately compiled product. Export carries
the admitted primary source, metadata and auxiliary members; domain code is compiled into the
product executables through Cargo.

The [compiled CLI trajectory](../../tests/project_workflow_cli.rs) adds the independent text package
to a generated product, references its source from a persistent Caption, and observes the parsed
text in gameplay. It follows the ordinary Check/Play/Repair/Export path, then runs the exported
headless executable from an unrelated directory after detaching the development project path.
Its missing-auxiliary-source check requires rejection before gameplay. The separate
[source Export tests](../../src/project_workflow/export/source_tests.rs) retain their synthetic
build observations for focused source-change and publication checks. The
[Windows qualification record](../verification/2026-10-21-compiled-asset-domain-export.md) captures
the actual execution and its platform limits.
