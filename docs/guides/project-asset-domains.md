# Compiled Project Asset Domains

Use a source domain when one referenced source produces immutable typed CPU data for project
Check and Play. Select the package in the same compiled `ProductRecipe` used by the game and
project tools:

```rust,ignore
fn recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    game_schema_recipe()?.add_asset_domain::<text_package::TextDomain>()
}
```

The source domain API requires `runtime-2d` and `serde`; `project-workflow` enables both. A package
may depend on the root facade without Editor, window or graphics dependencies. The
[independent text package](../../tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs)
and its [public workflow tests](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/workflow.rs)
are complete examples outside root workspace inheritance.

## Domain Contract

Implement `ProjectAssetDomain` with a stable `SOURCE_KIND`, an associated `Product`, conservative
`ProjectAssetLimits`, and `prepare`. Persist `AssetSourceKind::Other(SOURCE_KIND)` in the source's
canonical `.meta` identity. The Host routes declared persistent `AssetRef` fields by that identity.
It does not infer a handler from file extensions or crate presence. Two selected owners of the
same kind reject; a referenced kind without an owner fails Check/Play. Unreferenced files need no
handler and are not rewritten.

`ProjectAssetSource::metadata()` returns an identity-first candidate. Admit the package's settings
with `admit::<Settings>()` and validate only domain invariants. `bytes()` provides the bounded
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
`ProjectContentError::asset_domain` for actionable format/settings failures. Host-reported parser
panics omit the panic payload from the diagnostic report.

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
import adapters, cross-domain products, cooking, source-free export, watchers and dynamic plugin
loading need their own concrete consumers. See [ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md).
