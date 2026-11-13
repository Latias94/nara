# External Source Domain Consumer

This independently declared workspace member adds `consumer.text` through public
`ProductRecipe::add_asset_domain`. The library owns typed metadata settings, UTF-8 parsing and
one auxiliary source dependency. It imports no Nara-private interfaces.

The parser implements `nara_asset::SourceAssetImporter` using only `nara_asset` with its
`serde` feature, `nara_core`, `nara_diagnostic` and Serde as normal dependencies. The root
`nara` facade is a development dependency for the project workflow tests. Public
`AssetImportSource` inputs keep domain diagnostics distinct from the caller's concrete
auxiliary-read errors; the parser does not depend on a project Host, Scene or 2D API.
The pure input tests in `tests/import.rs` exercise that error distinction, typed settings
and caller-enforced dependency bounds.

`bundle::TextBundleDomain` is the compound consumer. Its raw JSON has a suffix path and named
text rows without intrinsic IDs. `TextBundleSettings::initial` requires explicit product IDs;
`rebind` requires complete `Continue`/`New` decisions against exact new source bytes. Authored
metadata retains the row mapping and known-retired IDs. Neither cache contents nor name/index
matching allocates continuity. Equal-byte row exchanges remain indistinguishable unless an
author explicitly supplies another mapping.

`AssetProductRef` checks the enclosing source as well as the stable product ID. The whole typed
bundle owns its entry map. Its separately implemented `RuntimeAssetCodec<TextBundle>` encodes
only bounded cooked text and identity in deterministic product-ID order. Decoder inputs need no
raw source, sidecar or suffix reader. This is a concrete codec; source-free project startup,
Runtime catalogs and independently resident subresources remain separate integration work.

The original single-text format remains a distinct consumer for existing source workflows and
Editor settings behavior, not a compatibility reader for the compound format. Both use the same
lower operation contracts. The compound tests and `tests/compound_workflow.rs` exercise explicit
continuity, processed bytes and actual two-Scene Check/Play/travel without a second loader.

The public integration tests qualify Check receipts, headless Play, actionable domain failures,
missing contributions, shared dependency reads, parser reservations, panic diagnostics, retained
generation budgets and explicit Scene transitions. Preparation and commit failures preserve the
current products; successful replacement changes the collection; an empty Scene retires it.

```text
cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-asset-domain-consumer --locked --test-threads=1
```

Use the shared repository `target` with `CARGO_BUILD_JOBS=1` and `CARGO_TARGET_DIR`, and run Cargo
commands serially. The parent consumer workspace owns the independent lockfile. No root workspace
dependencies or features are inherited.

Export source publication and digest-revalidation tests live in the Export owner's
`src/project_workflow/export/source_tests.rs`, which can construct its existing adapter-only build
receipts. Those tests use synthetic executable observations and make no desktop/Cargo-artifact
acceptance claim. This external package proves the public Check/Play/source-receipt boundary;
it does not bypass the build receipt through `__private`.

The root [compiled CLI trajectory](../../../project_workflow_cli.rs) supplies the executable
qualification path. It adds this package as a real dependency of an independently generated
product and shares its recipe across tools, headless and desktop. The test checks source-member
bytes and gameplay text, detaches the development project path, and runs the exported headless
executable from an unrelated working directory. Removing the published auxiliary file must
reject before gameplay.

```text
cargo nextest run -p nara --no-default-features --features project-workflow-cli --test project_workflow_cli --locked --run-ignored only -E 'test(fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay)' --test-threads=1
```

This explicit test runs nested Cargo builds serially and uses the Export owner's real product
observations. The [Windows qualification record](../../../../docs/verification/2026-10-21-compiled-asset-domain-export.md)
records actual execution and platform limits. Full native presentation/input, Editor import,
hot reload and cooked products remain separate.

The peer [Editor adapter](../editor-panel-consumer/src/text_import.rs) wraps this same domain
with typed default settings and a real Import/Reimport panel. The wrapper keeps this package
independent of GUI dependencies and selects the Editor facet separately from the game's recipe.
Its [qualification record](../../../../docs/verification/2026-10-21-editor-source-domain-import.md)
covers canonical metadata publication followed by Check/Play; it does not install a native or
GPU product in the Editor.

The [durable build consumer](tests/durable_build.rs) separately selects this domain's optional
`SourceAssetBuild` facet. It exercises normalized settings, actual raw/optional observations,
two clean stores, fresh-process verified reuse, Product continuity after cache deletion and
whole-group interrupted publication. Its real Scene/bundle outputs feed the existing retained
Runtime pack and managed travel through public APIs. The sibling
[Image consumer](../tests/durable_image_build.rs) executes actual processed-input cooking for
different target dimensions. These build operations do not install an importer registry or
qualify target executable selection, a packaged Player or GPU cooking.

The [dependent-resource refresh consumer](tests/dependent_resource_refresh.rs) installs the
compiled `SourceAssetBuildPlugin` alongside the public Asset, source-change and Task plugins.
It reserves a real catalogue record and advances a managed App from one initial source
observation to `Assets<BuiltSourceAsset<TextBundle>>`. Subsequent refreshes use real file edits
and `AssetSourceChanges`: required suffix change/deletion/recreation, optional build-suffix
creation/change/deletion, explicit A-to-B selector and metadata rebind, and removal of obsolete
inputs. Atomic worker counters check that obsolete and unrelated observations perform no build
or decode. A fresh App reuses the stored group without executing the domain and restores both
required and optional subscriptions through actual subsequent refreshes.

The same consumer checks first-load recovery from a missing required input, last-good retention
and retry subscriptions after selecting a missing new input, creation while a real failing
build is held, and superseded successful decode completion. Finite consumer-owned worker gates
make the failure and stale-result cuts deterministic without engine-private hooks. The wrapper
delegates import, build, codec, identity and budget promises to the actual `TextBundleDomain`.
The blocked-failure trajectory also repairs a tracked source-byte refusal before its retry
subscription exists, then checks that a persistent oversized input stops in Failed until a real
repair observation arrives.

Two further trajectories hold a successful decode while its newly discovered input changes
before subscription, and hold a metadata-only warm decode until current adoption acknowledges
the original dirty observation. The first consumes the only early raw event and still reaches
the current content; the second retains the original group without executing the build again.

Its original and successor live groups also feed the retained Runtime delivery helper; escaped
built-value owners and immutable Runtime bases retain their respective cooked text after live
refresh, App replacement and source/cache removal. These are semantic-input and CPU/managed
Runtime tests; native filesystem watching and GPU residency require their own qualification.
