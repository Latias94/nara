# Explicit Cargo Builds

Enable the root `project-tooling` feature for the finite native build adapter. Its pure selection,
metadata and incremental message decoder are also available through
`project-workflow-cargo-facts`, without the native adapter.

`project_workflow::cargo::CargoBuildSelection` names one package, binary, profile, target triple,
default-feature policy and requested features. Cargo owns dependency resolution, feature
expansion and target eligibility. A Host tool and a Player are separate selections, even when
they share a workspace.

## Capture One Selected Binary

The caller supplies a workspace, a package manifest relative to that workspace, an existing
lockfile, an explicit input inventory and trusted build tools. For example, inside a caller
that handles the selection and build error types:

```rust
use nara::{
    fs::RelativePath,
    project_tooling::cargo_build::{
        CargoBuildLimits, CargoBuildRequest, CargoBuildTools, build_executable,
    },
    project_workflow::cargo::CargoBuildSelection,
};

let selection = CargoBuildSelection::new(
    "game-tools",
    "content-cooker",
    "dev",
    target_triple,
    ["content-tools"],
    false,
)?;
let inputs = [
    "Cargo.toml",
    "Cargo.lock",
    "tools/Cargo.toml",
    "tools/src/main.rs",
]
.into_iter()
.map(|path| RelativePath::new(path).expect("compiled relative input path"));
let request = CargoBuildRequest::new(
    workspace,
    RelativePath::new("tools/Cargo.toml")?,
    selection,
    CargoBuildTools::discover_in(workspace)?,
    inputs,
    CargoBuildLimits::default(),
)?
.with_target_directory(target_directory)?;
let captured = build_executable(&request, |_| Ok(()))?;
```

`with_target_directory` requires an absolute path. Otherwise the adapter uses `CARGO_TARGET_DIR`
or the workspace's `target`. Cooperating build calls retain an exclusive directory lease through
capture. Reentrant calls using that directory return `TargetBusy`.

`CargoBuildTools::discover_in` resolves tools in the selected workspace's rustup context without
installing a toolchain. Absolute `CARGO` and `RUSTC` overrides take precedence over absolute `PATH`
entries. `CargoBuildTools::new` accepts explicit absolute tool paths and resolves any rustup proxy
in the current directory. The adapter subsequently observes and revalidates the actual tool files;
an MSRV, channel label or project profile is not a compiler identity.

Every build uses `--locked`. Preparing or updating a lockfile is a separate authoring action.
The inventory must include the workspace manifest, lockfile, selected package manifest and
metadata-selected binary source. Include other source, configuration and dependency files needed
for the caller's declared revision.

## Interpret The Result

`CapturedCargoExecutable` owns exact executable bytes and their digest, independently of Cargo's
mutable output pathname. Clones share that captured result. Overwriting or removing the original
output cannot change `bytes()`. Building does not run the result, activate a Host, establish Runtime
Ready or package game content.

Its provenance records the requested selection, actual compiled features, selected tool
observations, declared input digests and metadata digest. Metadata's workspace feature observation
can include features absent from the selected compiler unit. Only the selected compiler artifact
supplies the built feature set. Dependency-qualified `required-features` are left to Cargo.

Declared inputs are checked before and after work and at return. This is a per-file revalidation
envelope, not an instantaneous workspace snapshot or a hermetic build. Ambient trusted Cargo
configuration, build scripts, proc macros, native tools, toolchain libraries and inputs outside
the declared inventory retain their normal meaning. The finite work and retained-byte limits are
conservative logical allowances, not operating-system RSS limits.

`CargoBuildLimits::tool_bytes` limits each actual Cargo/rustc file observation independently of
`executable_bytes`, which limits the selected output. `tool_identity_bytes` separately bounds
version-command output. A small Player or Host-tool output allowance does not restrict compiler
installation size.

The synchronous control callback can revoke work at explicit checkpoints by returning `Aborted`.
It does not interrupt a running compiler. The Editor keeps its existing supervised process,
cancellation, immutable generation and Ready owners while reusing the pure Cargo facts.
Its machine stdout capture has a separate cumulative limit from the lossy diagnostic tail;
late polling or stderr truncation cannot evict admitted metadata/Cargo JSON. Protocol overflow
or cancelled capture fails the build rather than accepting a partial stream.

## Retain Failed Cleanup

`CargoBuildFailure::CleanupIncomplete` carries `CargoBuildProcessCustody` when direct-child
termination or reaping could not be confirmed. Keep this value and retry `retry_cleanup()`.
It retains the child and every output-directory lease until physical return is confirmed.
Converting that error to a string or dropping it loses the caller's cleanup handle.
Project-tool bootstrap preserves the same value inside `ProjectToolInvocationError::ProcessOwner`.

A foreign-target result is build-only until an appropriate runner separately qualifies it. The
closed desktop starter's source Export still performs its exact native product/startup agreement
and stages captured bytes through its existing publication owner.

The [independent consumer](../../tests/fixtures/cargo-build-consumer/README.md) demonstrates custom
Host/Player selections, workspace-only metadata features, dependency-qualified eligibility, a
foreign WASI build without execution, retained A after failed B, and processed-content execution
after source removal. See [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#explicit-cargo-selection-and-captured-build-results)
for the contract and the [unit plan](../plans/2026-10-31-explicit-cargo-build-selection-plan.md)
for qualification scope.
