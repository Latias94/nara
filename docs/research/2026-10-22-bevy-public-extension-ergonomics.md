# Public Extension Ergonomics: Nara and Bevy

Date: 2026-10-22, Asia/Shanghai. Status: source-grounded research and proposed follow-up, not an
Accepted architecture decision or an implementation qualification.

## Summary

Keep Nara's existing compiled recipes, typed source preparation, managed runtime and independent
Editor composition. These already support meaningful external contributions. The highest-value
changes are to make one source domain own its settings type, let a separate Editor adapter name
that domain without forwarding its implementation, and make complete Editor composition an
ordinary single construction operation. These improve real consumer workflows without a generic
plugin kernel, replacement ECS, dynamic ABI or a mandatory scripting language.

The findings below distinguish a wiring omission visible in source, interface friction exercised
by the external text consumer, and future capability questions that need a real consumer. No
Cargo command, application or native qualification was run for this research.

## Versions and Evidence

| Subject | Inspected source |
|---|---|
| Nara | Working tree on `test/compiled-asset-domain-export`, starting from `fe37b1536fa9386d4650b3de6777b88e8e4dde55`, including the current Editor source-import implementation |
| Nara ECS dependency | Root `Cargo.lock` selects `bevy_ecs 0.20.0`; root and independent consumer manifests request `0.20` |
| Fixed comparison baseline | Read-only `repo-ref/bevy`, commit `85503941e488152711a4695bbe248e9ed9f453fc`; its manifest is `0.20.0-dev`, Rust `1.97.1` |
| Current official supplement | On this date, `docs.rs/bevy/latest/bevy/asset/struct.LoadContext.html` returned HTTP 200 and identified `bevy-0.20.0`. The versioned `0.19.1` loader page also exists but is not the latest-version basis here. |
| Current source confirmation | Official `v0.20.0` `AssetLoader` source returned HTTP 200; its associated asset/settings and typed `load` contract match the mechanism discussed below. |

Context7 MCP was unavailable and `Get-Command ctx7` found no CLI. The web tool returned a gateway
HTTP 400 (`E5001`). Direct read-only HTTP requests to official docs.rs, bevy.org and raw GitHub
succeeded. Source assertions use the fixed Bevy commit; current links use explicit `0.20.0` or
`v0.20.0` where possible. This is a Rust-library comparison, not an external service integration;
no external service or OAuth API is selected and no service-sunset conclusion is needed.

Primary source index:

- **B1**: [Bevy plugin lifecycle and function plugins at the fixed commit](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/plugin.rs).
- **B2**: [Bevy App composition at the fixed commit](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/app.rs),
  [headless example](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/app/headless.rs),
  and [current official plugin introduction](https://bevy.org/learn/quick-start/getting-started/plugins/).
- **B3**: [Bevy loader and LoadContext at the fixed commit](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/loader.rs),
  [released 0.20.0 loader source](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_asset/src/loader.rs),
  and [0.20.0 AssetLoader reference](https://docs.rs/bevy/0.20.0/bevy/asset/trait.AssetLoader.html).
- **B4**: [Bevy AssetApp registration](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/lib.rs)
  and [complete custom-loader example](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/asset/custom_asset_loader.rs).
- **B5**: [Bevy AssetServer and typed settings builder](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/server/mod.rs)
  and [nested loader builder](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/loader_builders.rs).
- **B6**: [Bevy asset source/watch factories](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_asset/src/io/source.rs)
  and [dynamic ECS example](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/ecs/dynamic.rs).
- **N1**: [Nara strategy](../../STRATEGY.md), [product-recipe guide](../guides/rust-product-recipes.md),
  [App configuration methods](../../crates/nara_app/src/lib.rs), [Plugin lifecycle](../../crates/nara_app/src/plugin.rs)
  and [ProductRecipe implementation](../../src/product.rs).
- **N2**: [Source-domain contract and stores](../../src/project_content/domains.rs),
  [public source-domain guide](../guides/project-asset-domains.md),
  [external text domain](../../tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs)
  and [its Check/Play workflow](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/workflow.rs).
- **N3**: [Editor facet implementation](../../src/project_host/runtime/editor/source_asset_import.rs),
  [composition implementation](../../editor/src/platform/composition.rs),
  [application constructors](../../editor/src/app.rs),
  [public panel guide](../guides/editor-panels.md),
  [external text adapter/panel](../../tests/fixtures/module-consumer/editor-panel-consumer/src/text_import.rs)
  and [external source-import tests](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/source_import.rs).
- **N4**: [ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md),
  [scoped Editor validation](../../src/project_content/domains/editor.rs),
  [metadata publication work](../../src/project_host/runtime/editor/source_asset_import/work.rs)
  and [runtime cleanup ownership](../../crates/nara_app/src/runtime.rs).

## Compare Complete User Tasks

### Compose Rust gameplay and run headless

Bevy packages ordinary registration in `Plugin::build(&mut App)`. App supports plugins, tuples and
groups; function plugins cover small stateless registration functions. A headless application
uses the same App/systems with a schedule runner and selected Cargo features. It does not require
a separate gameplay programming model. Plugin setup has `build`, `ready`, `finish`, and `cleanup`
stages; its `cleanup` stage runs before schedule execution, so it is not Nara's runtime shutdown
obligation. See B1/B2.

Nara already offers `App::insert_resource`, `init_resource`, `add_systems`, observers and custom
schedules. Its fallible `Plugin` hooks and declaration have additional admission, ordering and
cleanup meaning. `ProductRecipe` is a replayable value consumed by direct App, file-backed
headless/desktop and strict project tooling. Check deliberately does not execute ordinary runtime
plugins. `RuntimeInstance` owns execution and retirement outside the configuring App. These are
coherent product requirements, not missing Bevy-style extension mechanisms. See N1/N4.

Borrow Bevy's consistent registration vocabulary and examples with one contribution entry point.
Do not remove recipe replayability, checked errors or runtime retirement merely to shorten a
sample to `App::new().run()`. An optional package can already expose `package(settings)` or a
normal Rust helper which constructs a recipe contribution; no universal package trait is needed.

### Add an asset format, settings and auxiliary input

Bevy's `AssetLoader` associates one `Asset`, one `Settings` type and one error type. The loader
receives typed settings separately from its primary byte reader and `LoadContext`. `AssetApp`
registers assets and loaders explicitly; a package plugin can hide these paired calls. The custom
loader example demonstrates typed handles and `Assets<T>::get`. `LoadContext::read_asset_bytes`
records raw loader dependencies; `load` instead records a typed asset-handle dependency. These
are different operations. See B3/B4/B5.

Nara's `ProjectAssetDomain` already groups the source kind, CPU product, reservations and parser.
`ProjectAssetSource` supplies admitted identity, primary bytes and bounded raw auxiliary reads;
`ProjectDomainAssets::collection::<D>().get(&AssetRef)` supplies committed typed data. Unlike a
Bevy asynchronous handle request, this resource represents the Scene's admitted content generation
and is installed before gameplay startup or with a successful Scene transaction. Retaining the
owner retains its memory reservation and generation. See N2/N4.

The useful borrowing is one typed settings owner and context-owned dependency observation. Copying
mutable `Assets<T>` storage or arbitrary `AssetServer` loads would bypass Nara's current Scene
publication and export-receipt owners. Bevy `extensions()` is optional and its example permits a
typed loader for a nonmatching extension; Nara should retain its stronger persisted-kind routing
rather than introduce extension-based validity checks. See B3/B4 and N2/N4.

### Compose Editor support alongside a game package

Nara's external fixture keeps the source parser in `asset-domain-consumer`, and the ImGui adapter
in `editor-panel-consumer`. The game recipe selects the source domain independently from the
Editor executable's selected facet and panel. This preserves a headless package without GUI
dependencies and prevents ordinary project Open from activating native game code. It is a useful
separation, not duplication that a single global App should erase. See N2/N3/N4.

Bevy provides useful package-composition precedent, but the inspected sources do not establish a
Nara-equivalent generic Editor importer that should replace these explicit Hosts. The correct
question is whether the same domain implementation and settings can be reused while each Host
retains its own admission and publication authority, not whether every Host has the same builder.

## Concrete Improvements

### 1. Associate the Editor adapter with its actual source domain

**Classification:** confirmed external-consumer friction and a type-contract gap, not an observed
memory-safety defect. **Priority:** first source-interface refactor.

Today `EditorSourceAssetDomain: ProjectAssetDomain` requires the facet type itself to implement
the source domain. The external text Editor crate owns neither the external trait nor the text
domain type, so it cannot implement that trait directly on `TextDomain`. Its local
`TextImportDomain` repeats `Product`, `SOURCE_KIND`, `limits` and `prepare` solely to forward to
the actual parser. Also, `Settings` exists only on the Editor facet; the source parser manually
chooses `metadata.admit::<TextSettings>()`. No associated-type relationship prevents the Editor
from serializing a different settings type. The current text consumer is consistent, but the
interface does not express that consistency. See N2/N3.

Current callsite, abbreviated from the external adapter:

```rust,ignore
impl ProjectAssetDomain for TextImportDomain {
    type Product = TextResource;
    const SOURCE_KIND: &'static str = TextDomain::SOURCE_KIND;
    fn limits() -> ProjectAssetLimits { TextDomain::limits() }
    fn prepare(source: &mut ProjectAssetSource<'_>) -> Result<Self::Product, ProjectContentError> {
        TextDomain::prepare(source)
    }
}
impl EditorSourceAssetDomain for TextImportDomain {
    type Settings = TextSettings;
    fn default_settings() -> TextSettings { TextSettings { max_characters: 64 } }
}
```

Proposed shape, not an existing API:

```rust,ignore
pub trait ProjectAssetDomain {
    type Settings: serde::de::DeserializeOwned;
    type Product: Send + Sync + 'static;
    // Keep SOURCE_KIND and conservative limits.
    fn prepare(
        source: &mut ProjectAssetSource<'_>,
    ) -> Result<Self::Product, ProjectContentError>;
}

pub trait EditorSourceAssetDomain {
    type Domain: ProjectAssetDomain<Settings: serde::Serialize>;
    fn default_settings() -> <Self::Domain as ProjectAssetDomain>::Settings;
}

impl EditorSourceAssetDomain for TextEditorImport {
    type Domain = TextDomain;
    fn default_settings() -> TextSettings { TextSettings { max_characters: 64 } }
}
```

The domain continues to admit its own settings through identity-first metadata, using
`admit::<Self::Settings>()` and its existing actionable settings diagnostic. Merely associating
the types removes the demonstrated mismatch; changing preparation arguments or making the source
reader generic is unnecessary. Semantic checks such as `max_characters` remain in the
domain. The source-only contract need not require `Default` or settings serialization just
because an Editor can author it; those requirements belong to that optional facet. The Editor
routes and validates through `Facet::Domain`, while game recipes still explicitly select that
domain. This borrows B3's settings association without granting the Editor recipe authority.

Alternatives are keeping the forwarding wrapper or implementing the facet in a feature of the
source crate. Both work, but the latter requires the source package to know about this Editor
contract and prevents a fully independent adapter crate from contributing without a wrapper.
An associated domain states the actual relationship directly. Qualification should use the two
independent crates, invalid settings, raw auxiliary reads, preserved identity and Check/Play;
existing freshness, reservations and publication checks must remain effective. N4 remains the
owning architecture contract and must be reconciled if this proposal is adopted.

### 2. Construct the complete Editor composition in one operation

**Classification:** confirmed wiring omission by source inspection plus a documented ordering
burden; no native failure was executed for this research. **Priority:** close alongside the
composition interface.

The current ordinary `run` calls `create_application` before installing the frontend, so it
preserves selected importers. Embedded consumers must remember that same two-step sequence.
`install(self, &mut application)` installs panel descriptors/factories but does not install
`source_asset_importers`. Using the older panel-only pattern with `EditorApplication::new` is
therefore valid Rust and can display a panel while omitting its selected Host facet. More
concretely, `run_trusted_compiled` constructs its application through
`EditorApplication::open_trusted_compiled`, whose current session and replacement Open Host use
default importer selection, then installs only panels. That path drops the composition's selected
importer configuration. See N3.

The advanced trusted lane deliberately has no generic authoring source index, so carrying a
selection does not by itself make metadata Import available there. The concrete correction is
to retain that selection for a later ordinary authoring replacement and make construction honest
about the initial capability, not to invent a new source-index authority for this legacy lane.

Current embedded callsite:

```rust,ignore
let mut application = composition.create_application(options);
let frontend = composition.install(&mut application)?;
```

Proposed ordinary callsite:

```rust,ignore
let (application, frontend) = composition.build(options)?;
```

`build` consumes the selected composition and returns both configured owners after successful
admission. Native `run` should reuse that construction. The advanced trusted compiled path must
explicitly supply the same importer selection to the initial session and the replacement Open
Host without claiming initial metadata-authoring capability. A genuinely panel-only late attachment can retain a clearly named, narrowly documented
operation if a real consumer needs it; it should not appear to install the complete composition.

The first-principles requirement is a consistent selection transaction, not another registry:
the executable selects the contributors once, construction transfers that selection to every
session owner, and per-project attempt state starts fresh. Preserve UI-thread-only factories,
data-only ordinary Open, rejection without workspace mutation, and explicit trusted execution.
Qualification should compare ordinary embedded and native construction, the existing trusted
lane, and project replacement using a registered external facet. Do not infer that panel presence
proves the Host was configured.

### 3. Offer a typed action constructor only at the source-domain interface

**Classification:** confirmed repeated encoding knowledge in the panel, native canary and test
caller; ergonomic follow-up, not a correctness failure of the Host. **Priority:** after the
settings/domain association, if those callers still repeat this knowledge.

The text panel constructs a dynamic `EditorSourceAssetImportAction` by repeating its source kind
and serializing `TextSettings`; other consumers build the same envelope. `nara_tooling` should
remain an immutable frontend-neutral transport and should not become a serde/reflection registry.
However, a typed helper where the domain is known can derive the kind and serialize only that
domain's settings. See N3.

Current callsite:

```rust,ignore
EditorSourceAssetImportAction {
    relative_path: view.relative_path.clone(),
    source_kind: TextDomain::SOURCE_KIND.into(),
    settings: serde_json::to_string(&draft)?.into(),
}
```

Candidate callsite, not an existing API:

```rust,ignore
let action = source_import_action::<TextDomain>(view.relative_path.clone(), &draft)?;
context.submit(EditorAction::SourceAssetImport(action))?;
```

Return a small encoding error rather than panic or silently convert to an unavailable domain.
This helper does not grant admission: current frame, indexed source, exact kind, typed semantic
validation, freshness and conflict checks still belong to the Host. Dynamic frontends may keep
the existing transport. A settings-decoding helper should be added only if it hides repeated
domain-specific projection logic rather than creating a second state machine for panel drafts.

Bevy is a useful but imperfect comparison: `LoadBuilder::with_settings<S>` avoids caller JSON,
yet its documented behavior for a settings-type mismatch is to ignore the changes and log an
error. Do not copy that silent fallback. Nara can bind settings to the domain statically while
preserving explicit runtime admission failures. See B5.

## Optional Scripting and Reload: What Is Already Real

| User task | Existing useful interface | Remaining consumer-specific question |
|---|---|---|
| Compile or interpret a source format into immutable CPU input | `ProjectAssetDomain`, typed settings, bounded primary/auxiliary reads and exact source closure | A real language package must show its allocation limits, diagnostics and executable-product lifetime; a generic script host is not required. |
| Execute language-owned gameplay | Normal `Plugin`, resources, systems/observers and a product contribution | A concrete VM determines thread affinity, access policy, fault behavior and scheduling. The source-domain callback has no World and is not the gameplay execution hook. |
| Contribute an Editor inspector or importer | Compiled source facet, panel-local drafts, immutable projections and semantic actions | A concrete adapter determines its controls and terminal feedback. Ordinary Open must not activate project native code. |
| Retire workers or external VM work | `Plugin::shutdown`, declared shutdown obligations and `RuntimeCloseParticipant` | Prove actual cancellation and physical return using the owning runtime thread. A `Drop` callback alone does not establish completed work. |
| Automatically reload source data | Existing preparation and generation/publication owners provide reusable correctness | No watcher-driven source-domain replacement is qualified here. A watcher must invalidate/request preparation; it must not become a second writer or publish stale products. |
| Reload Rust code or a VM across incompatible structural changes | Fresh managed runtime and explicit game-owned state provide a possible starting point | Compatibility, state restoration and code/VM lifetime need measured real workflows. No stable ABI or universal state-migration contract follows from the existing plugin trait. |

Sources: N1-N4. These are interface capabilities and open proof obligations, not a claim that a
scripting package or reload feature exists. In particular, the hidden `__RuntimeDriverPort`
plumbing is not a documented public extension bus and should not be advertised as one.

Bevy also separates the mechanisms: plugins configure systems/resources; asset readers and
watcher factories emit change events; AssetServer handles reload; the dynamic ECS example uses
lower-level descriptors and unsafe layout work for runtime-defined data. None of those source
files establishes a stable dynamic Rust plugin ABI. The lesson is to expose the specific owner
that a real adapter needs, not to expose all internal mutable state in anticipation of scripts.
See B1/B5/B6.

## Recommended Order and Limits

1. Close current Import/Reimport correctness and native presentation qualification first.
2. Refactor the settings/domain relationship with the existing independent source and Editor
   crates as the primary interface test. Keep the source package usable without GUI dependencies.
3. Make ordinary Editor construction transfer the complete composition once; close the trusted
   path omission and prove replacement retains configuration with fresh operation state.
4. Evaluate the typed action helper against the resulting actual panel/canary callsites. Stop if
   the helper merely moves a single encoding line without reducing knowledge for callers.
5. Let a real optional language or reload consumer identify the next missing interface. Do not
   preempt it with a generic plugin kernel, self-written ECS, universal importer, dynamic native
   ABI or unlimited World/capability access.

This ordering improves type agreement, ownership locality and complete user workflows while
retaining the safety facts the caller actually needs. It makes no performance claim, no
cross-platform execution claim and no claim that Bevy's full asset pipeline can be transplanted
without Nara's source receipt, generation and publication contracts.

## Implementation Disposition

The [ergonomics plan](../plans/2026-10-22-public-extension-ergonomics-plan.md) adopts the associated
settings/domain relationship, a typed facet action constructor, and complete consuming Editor
construction. It retains domain-owned identity/settings admission rather than adding Host parsing.
The named findings above describe the inspected pre-refactor working tree; the owning ADRs and
current guides describe the supported interface. Execution results and remaining limits belong
to the [source-import qualification](../verification/2026-10-21-editor-source-domain-import.md).
