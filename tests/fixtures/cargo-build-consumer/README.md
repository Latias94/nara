# Explicit Cargo Build Consumer

This independent workspace imports Nara as `engine`, with default features disabled and only
`project-tooling` selected. Its ignored integration tests qualify actual builds through
`engine::project_tooling::cargo_build::build_executable`. Root-workspace tests do not run this
consumer, and ordinary fixture tests do not start nested Cargo.

Generate this workspace's independent `Cargo.lock` with Cargo after integrating the parent
implementation and before locked qualification. Do not copy the root lockfile or hand-author
dependency facts.

## Coverage

`custom_workspace_captures_host_tool_and_runs_native_player_after_source_removal` generates an
ordinary virtual Cargo workspace with custom packages, binaries and the named `consumer-checked`
profile. A decoy workspace member has the same Host binary name and intentionally cannot compile.
The explicitly selected Host binary requires both a local feature and a dependency-qualified
`support/eligible` feature enabled through an expanding feature alias. The decoy's library
dependency enables a `metadata-only` Host feature in workspace metadata; compiling that feature
for the selected binary intentionally fails. The default feature also intentionally cannot compile.
Its actual output checks the expanded feature, generation A, and the named profile's disabled
debug assertions. The selected compiler artifact, rather than metadata's workspace feature set,
supplies the recorded compiled features. Cargo decides dependency-qualified target eligibility.

The test checks exact selected tool digests, the compiler's actual Host triple, requested versus
resolved features, the declared input paths and digests, and metadata/provenance digests. Equivalent
feature closures with distinct requests retain the same compiled features and observable Host
behavior but distinct provenance. This does not require byte-reproducible Rust executables.
The outer `AfterBuild` callback attempts the same request exactly once: the inner call must return
`TargetBusy` before building, and the outer capture must still run the exact generation-A canary.
All build requests explicitly select the shared repository target directory with `with_target_directory`.
The first Host build limits its output below the selected compiler file sizes and admits tool
files at their exact independent `tool_bytes` allowance; one byte less rejects tool observation.
A relative explicit target directory rejects with `InvalidTargetDirectory`.
Omitting the workspace manifest, lockfile, package manifest or metadata-selected source rejects the
request/build. An explicitly declared Rust module changes just before return to require `Stale`;
the Host can also return `Aborted` before metadata.

A successfully captured A remains byte-for-byte unchanged and executable after B fails to compile
or omits a required feature. Replacing the mutable Cargo outputs and removing the generated source
workspace leaves A and the separately selected native Player usable from their captured bytes.
Build operations must not create the Host's execution marker.

The native Player copies the existing `processed_input_main.rs` and `processed_input_probe.rs`
unchanged into its generated source. Its generated package imports `engine` with only `runtime-2d`
and `serde`. The shared original cooking/launching fixture runs all seven existing cases:
physical input, missing input, malformed input, wrong identity, conflicting input, wrong catalog
digest, and wrong target. Its content expectation uses the actual selected triple/profile. Each
child runs after authoring source/cache removal with a cleared environment and an unavailable
`PATH`; the existing watchdog and exact output/payload oracles remain in force. The existing
runtime-runner test still obtains its child through `CARGO_BIN_EXE_processed_input_player`.

`foreign_wasi_build_returns_bytes_without_execution_and_keeps_native_host_tool_usable` generates
a separate ordinary workspace without a Nara dependency. It actually builds a custom binary for
the already installed `wasm32-wasip1` target with the same explicit feature/profile policy, checks
the Wasm header and captured/provenance facts, and subsequently runs the independently captured
native Host tool. It never executes the foreign artifact or looks up a target runner. This is
Cargo foreign-build evidence only; it does not qualify Nara's WASI Runtime, target content or Ready.

## Run Serial Qualification

Use the already installed `1.95.0` toolchain and `wasm32-wasip1` standard library. Tool selection
uses read-only `rustup which`; the tests do not install or update a toolchain. Dependency caches
must already be available: each generated workspace prepares its own lockfile with
`generate-lockfile --offline` before making locked API build requests.

Run these commands from the repository root, without another Cargo process in this checkout.
For PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location).Path 'target')
$fixtureCargo = (& rustup which --toolchain 1.95.0 cargo).Trim()
$env:RUSTC = (& rustup which --toolchain 1.95.0 rustc).Trim()
& $fixtureCargo generate-lockfile --offline --manifest-path tests/fixtures/cargo-build-consumer/Cargo.toml
& $fixtureCargo nextest run --locked --manifest-path tests/fixtures/cargo-build-consumer/Cargo.toml --test cargo_build --run-ignored only --test-threads=1
```

All builds reuse the repository's absolute `target` directory and one build job. The ignored
tests must also run with one test thread, since generated builds share that target directory and
deliberately replace only their known fixture output names. The generated workspaces and relocated
executables live in uniquely owned temporary directories; recursive cleanup verifies that custody.

The inventory establishes revalidated per-file observations of explicitly declared inputs. It
does not snapshot the whole Cargo workspace or make builds hermetic: transitive source, toolchain
libraries, ambient configuration/environment, build scripts and proc-macro inputs outside the
declared inventory retain their normal trusted Cargo/native meaning. Captured artifact bytes do
not constitute content packaging, activation, an execution Host or Runtime readiness.
