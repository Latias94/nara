# External Source Domain Consumer

This independently declared workspace member adds `consumer.text` through public
`ProductRecipe::add_asset_domain`. The library owns typed metadata settings, UTF-8 parsing and
one auxiliary source dependency. It imports no Nara-private interfaces.

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
