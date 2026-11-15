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
`CargoBuildRequest::workspace()` returns the original canonical admitted workspace for a concrete
Host's project comparison; it does not attest to current source freshness.

`CargoBuildTools::discover_in` resolves tools in the selected workspace's rustup context without
installing a toolchain. Absolute `CARGO` and `RUSTC` overrides take precedence over absolute `PATH`
entries. `CargoBuildTools::new` accepts explicit absolute tool paths and resolves any rustup proxy
in the current directory. The adapter subsequently observes and revalidates the actual tool files;
an MSRV, channel label or project profile is not a compiler identity.
The read-only `cargo_path()` and `rustc_path()` accessors expose those resolved absolute choices
for another explicit native tool operation; they grant no source, build or execution attestation.

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

The finite native adapter obtains declarations with `cargo metadata --no-deps --format-version 1`.
It binds the selected workspace package/bin, required features, source and output directory,
then validates the actual selected compiler artifact through the same decoder. It does not
request a full dependency graph or cache metadata. `metadata_digest()` hashes the exact
declaration-only response; it is not a resolved-graph observation.

Pure callers can use `CargoBuildInvocation::from_metadata_declarations(bytes, manifest, selection)`
for this explicit mode. Its input requires an explicit null `resolve` field. Full
`CargoMetadataFacts::parse` retains its strict selected resolve-node requirement and graph
observations. `CargoBuildInvocation::metadata_features()` now returns `Option<&[String]>`:
`None` means declarations only, while `Some(&[])` means a resolved empty set. Callers using full
metadata should handle that observation explicitly; the selected artifact's `features()` remains
the authority for compiled features in either mode. The Editor's supervised build continues to
consume full metadata independently of the finite synchronous adapter.

`CargoBuildFailure::Compilation` preserves the actual process exit status, bounded structured
compiler diagnostics and any protocol rejection. Diagnostics include explicit dropped/truncated
counts; raw JSON stdout is not retained in the error. A failed compile therefore remains
actionable without treating a nonzero exit as a successful artifact or discarding its messages.
Tool/version/metadata commands keep their own non-compilation failures, and unconfirmed child
cleanup retains separate custody as described below.

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

For complete cooked content plus a Player whose expectation is compiled as an exact declared
input, use the separate [cooked Player workflow](cooked-player-delivery.md). Content preparation,
code/content binding, package publication and Runtime/native qualification remain distinct.

## Run A Product With Native Process Custody

The `project_tooling::process` module contains the concrete native process implementation shared
by supervised Editor Play and the game's [Rust reconstruction Host](../../samples/two-level-game/README.md#exact-game-reconstruction).
It owns a process and its containment/output resources. The caller owns installation of captured
bytes, source compatibility, game state and selection of an adopted executable generation.

For a product launch, open the immutable installation through filesystem capabilities and mint
`ExactExecutableLaunchAuthority::from_verified_handles` from fresh restrictive executable and
immediate-directory leases. Use the executable-specific handle duplication method, not an ordinary
read lease. The authority is move-only and consumed by `SupervisedProcessOwner::spawn_exact`.
Editor generation admission separately validates its launch manifest and retains that file's
lease with `with_launch_record`; the process module does not interpret the record or its source
meaning. A pathname or process ID is not an alternative exact-launch authority.

`SupervisedProductProcessCommand` selects an absolute working directory, bounded arguments and
either a sealed environment or the reviewed desktop-session projection. It starts with closed
stdin; `with_bootstrap` explicitly selects the inherited duplex child endpoint used by Editor
Play. `SupervisedProcessCommand` is the separate tool path with its existing inherited environment
and explicit overrides. Its `bounded_stdout` gives machine-protocol output a cumulative allowance
independent of truncated diagnostic tails. Parse that exact output rather than a lossy log ring.
Both spawn calls are synchronous; the Editor continues to use its original background spawn
worker. `SupervisedProcessWake` is a coalesced notification, not a control or completion receipt.

Retain the owner and call `advance` or `advance_with_output` until its actual cleanup receipt
exists. The receipt requires the direct child's exit, empty containment, stopped/joined output
readers and drained output. An application-level Close reply alone cannot prove these facts.
Conversely, killing the process cannot prove that the game completed its own Runtime Close.
`request_terminate` is fallback/Stop execution; on Windows its first request can terminate the Job,
so a false `force_killed` escalation flag does not establish a graceful shutdown. The receipt's
`termination_requested()` records every native termination attempt before receipt issuance:
explicit requests, automatic residual-tree termination after direct-child exit, and force-kill
escalation or retries. Attempts remain recorded even if they race with natural exit or fail before
later cleanup succeeds. After direct-child exit, the owner first inspects containment and skips
automatic termination if it is already empty. Live residual descendants still require termination
and cannot be reported as a naturally exited tree. Consumers requiring cleanup without native
termination must check `!receipt.termination_requested()` alongside their own successful Close
protocol. Failed cleanup keeps the same owner for `retry_cleanup`. Drop is a fail-safe and
publishes no clean-close proof.
A transferred bootstrap channel and its consumer worker retain their separate completion owner.

The Windows implementation installs the non-breakaway kill-on-close Job and inherited-handle
allowlist at creation, then verifies the suspended mapped image before resuming its primary
thread. The Linux x86-64 implementation uses descriptor-bound execution and a process group;
its `WNOWAIT` observation keeps the exited leader's PID reserved until containment inspection
finds no other group member, then reaps the direct child. Windows inspects the retained Job after
observing the direct child's exit; drop skips termination for an observed empty Job and retains
its fail-safe for active Jobs or inspection errors. Linux does not provide the same hard-kill
containment guarantee. Other unsupported native targets
refuse execution. Qualification and platform execution evidence belong to the
[current process successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#shared-native-process-custody-successor).
This module does not extend the desktop one-shot READY contract or make native services
checkpointable.
