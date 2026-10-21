# Compiled Asset Domain Export Qualification

## Result and Scope

The generated independent product compiles the external `consumer.text` domain through a real
Cargo dependency. Its shared product recipe supplies the domain to tooling, headless and desktop
targets. The actual Check/Play/Repair/Export trajectory passes on Windows, including two accepted
release-product observations and a subsequent run of the exported headless executable after
detaching the development project coordinate.

This qualification extends the [source-domain qualification](../plans/2026-10-21-external-asset-domain-qualification-plan.md)
at `1605871`. It changes test fixtures and documentation only; no production runtime change was
needed. The [implementation plan](../plans/2026-10-21-compiled-asset-domain-export-plan.md) defines
the bounded executable-delivery scope.

## Consumer and Assertions

The existing [compiled custom-project CLI test](../../tests/project_workflow_cli.rs) generates an
independent desktop starter, adds the [external package](../../tests/fixtures/module-consumer/asset-domain-consumer/Cargo.toml)
to its own Cargo dependencies, and shares the same recipe through its generated library. The
[Caption plugin](../../tests/support/project_tooling_caption_game.rs) contributes a persistent
`AssetRef` and reads the external domain's typed runtime collection. The root test process includes
the domain source only to construct the matching inspection recipe; the generated product builds
the actual package through Cargo.

The trajectory verifies:

1. Check builds the fresh product before strict schema admission and produces its independent
   lockfile. Neither Check nor startup repair executes the gameplay observers.
2. Headless Play preserves the existing custom component rename/migration assertion
   (`migrated-pulse=42`) and observes `Hello world` from the domain's primary and auxiliary sources.
3. Export performs real locked release builds for `headless` and `desktop`, and accepts two exact
   product observations through the existing Export protocol. Primary text, canonical metadata
   and auxiliary text are copied byte-for-byte and occur exactly once in the delivery receipt.
4. The existing declared second Scene, Prefab and Image closure remains loadable from the package;
   undeclared content is excluded. The development project is renamed away from its original
   coordinate before consuming the published project.
5. The exported headless executable runs from an unrelated working directory and observes both
   the migrated component and the parsed text. After removing the published auxiliary file and
   clearing both markers, the same executable exits unsuccessfully before either gameplay
   observer writes its marker. The Caption observer writes an entry marker before accessing its
   query or typed collection, then overwrites it with the parsed value only on success. Requiring
   that marker to be absent also detects an observer that enters and fails during resource use.

The Export owner's [synthetic observation tests](../../src/project_workflow/export/source_tests.rs)
remain the focused checks for source-change rejection and publication behavior. They are separate
from this actual executable trajectory.

## Execution Evidence

Date: 2026-10-21. Host: Windows 11, `x86_64-pc-windows-msvc`.
Compiler: `rustc 1.99.0 (b940084d7 2026-09-28)`.
Cargo commands ran serially with `CARGO_BUILD_JOBS=1` and the existing repository `target`.

```text
cargo nextest run -p nara --no-default-features --features project-workflow-cli --test project_workflow_cli --locked --run-ignored only -E 'test(fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay)' --test-threads=1
```

Result: **1 passed, 7 skipped**, Nextest run
`6fca488b-9951-4184-812a-3376a479728f`. The selected test completed in 777.418 seconds, including
nested development and release builds. This duration is execution evidence, not a performance
benchmark. The real release build selected `headless` and `desktop` for
`x86_64-pc-windows-msvc` with the generated product's locked manifest and `desktop` feature.

After adding the observer-entry marker identified during independent review, the same complete
trajectory passed again: **1 passed, 7 skipped**, run `384ba0d4-754a-4899-b4f1-d91323357057`,
66.427 seconds with cached builds. This second run qualifies the final failure assertion.

Focused regressions:

| Command | Actual result |
|---|---|
| `cargo nextest run -p nara --no-default-features --features project-workflow-cli --test project_workflow_cli --locked --test-threads=1` | 5 passed, 3 skipped; run `8945dc1b-c499-483e-aa4e-5379b1cc2410` |
| `cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-asset-domain-consumer --locked --test-threads=1` | 7 passed, 0 skipped; run `84c254f4-c4e2-4ada-9a66-eb0074a65e48` |

The CLI and external-domain builds retain their existing feature-specific unused/dead-code
warnings in production modules.

`cargo clippy -p nara --no-default-features --features project-workflow-cli --test project_workflow_cli --locked`
passes with existing dependency/production warnings; no diagnostic points to the changed CLI test
or Caption support module. Root `cargo fmt --all` and `cargo fmt --all -- --check` pass. A local-link
check of the seven affected Markdown documents resolves all 582 targets. `git diff --check` passes.

## Review

Independent correctness and Codex adversarial review completed with no unresolved findings:
`ce-code-review` run `20261021-compiled-domain-export-be79dd89`, status `complete`. The adversarial
pass identified that a success-only marker could miss entry followed by a consumer panic. The
entry sentinel described above resolves that evidence gap; the final complete CLI trajectory
was rerun after the correction. Review was static and did not independently rerun Cargo. The
implementation session supplied the execution results recorded here.

## Limits

The desktop evidence is the Export protocol's actual build and startup agreement. It does not
establish full native presentation/input qualification or replace the pinned-toolchain
[native Editor release procedure](native-editor-qualification.md). No explicit graphics backend
was selected for this run. Other operating systems, a clean-machine installation, complete
reproducible build-input closure, Editor Import/Reimport, cooking, scripting and hot reload remain
outside this qualification.
