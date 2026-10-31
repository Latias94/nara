# Explicit Cargo Targets And Captured Artifact Custody

Date: 2026-10-31. Source study for the
[R7 selection unit](../plans/2026-10-31-explicit-cargo-build-selection-plan.md).
Nara starting revision: `0c36715`. Reference pins: Bevy
`85503941e488152711a4695bbe248e9ed9f453fc`, Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`.
This is static source evidence, not a build, platform or performance qualification.

## Host Tools And Target Outputs

Bevy's `build-wasm-example` is a Host package that invokes Cargo with a selected example,
profile and wasm target, followed by Host `wasm-bindgen` and optional optimization. Its
browser test branch is separate from compilation. The script does not establish hermetic
inputs or immutable output merely by using a known pathname.
Sources: [build helper](../../repo-ref/bevy/tools/build-wasm-example/src/main.rs),
[workflow](../../repo-ref/bevy/.github/workflows/validation-jobs.yml),
[browser oracle](../../repo-ref/bevy/.github/start-wasm-example/tests/wasm_example.spec.ts).

Godot distinguishes an Editor's compiled platform exporter from whether the current environment
can build that platform's engine. PC export consumes a selected prebuilt template and project
data; platform `run` is another operation. Template architecture validation reads actual PE/ELF
facts without launching the target. This supports Nara's build/qualification separation but is
not a replacement for Rust's Cargo compilation model or complete target ABI evidence.
Sources: [SConstruct](../../repo-ref/godot/SConstruct),
[Editor exporter composition](../../repo-ref/godot/editor/SCsub),
[PC export](../../repo-ref/godot/editor/export/editor_export_platform_pc.cpp),
[Linux export/run](../../repo-ref/godot/platform/linuxbsd/export/export_plugin.cpp),
[Windows template validation](../../repo-ref/godot/platform/windows/export/export_plugin.cpp).

## Effective Selection And Cargo Authority

Bevy separates default-feature policy, requested features, profile and target. Its custom
`wasm-release` profile and `dev`/`debug` mapping show why a product preset name is insufficient.
Godot persists preset name/platform/options separately; export plugins can override effective
options, and debug/release is independently selected. Nara should record the actual invocation,
with Cargo supplying package/target and compiled feature facts, instead of reproducing feature
resolution or inferring flags from `nara.toml` profile names. Workspace metadata features can
include activation from other members or development units. Retain that as a separate metadata
observation; use the selected compiler artifact's features as the actual built set. Cargo also
owns eligibility for dependency-qualified `required-features`. Built-in `dev`/`test` profiles
use `debug` output, `release`/`bench` use `release`, and custom profiles use their own names.
Sources: [Bevy profiles](../../repo-ref/bevy/Cargo.toml),
[example selection](../../repo-ref/bevy/tools/example-showcase/src/main.rs),
[Godot preset behavior](../../repo-ref/godot/editor/export/editor_export_preset.cpp),
[preset persistence](../../repo-ref/godot/editor/export/editor_export.cpp).
Cargo's own [metadata contract](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html),
[compiler-artifact messages](https://doc.rust-lang.org/cargo/reference/external-tools.html#artifact-messages)
and [unit-graph limitation](https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph)
support keeping workspace metadata and actual compiled feature facts separate.

The Nara baseline has three independent fixed selectors: supervised desktop/dev, tool bootstrap
headless/dev, and two-bin source Export/release. The latter reopens original Cargo files during
staging, while the Editor already has a separately retained generation owner. The next change
should share pure explicit facts and preserve each native owner, rather than introduce another
general executor or make compile success authorize activation. Source Export should consume
captured bytes through its original publication owner. The fixed starter remains one explicit
policy adapter, and the Runtime content base remains the original content authority.

## Selected Consumer And Limits

Use an external ordinary workspace with a custom Host tool, a processed-base native Player,
explicit requested/expanded features, a custom profile and a same-bin decoy package. Qualify
one actual foreign WASI build as captured output without a runner. Reuse the existing seven
processed-input process oracles for source/cache/Cargo-inaccessible native execution. A failed
B candidate and overwritten original output must leave captured A unchanged and runnable.

Native Rust/Cargo build code remains trusted. A finite declared input inventory with before/
after file observations is neither a global source snapshot nor hermetic compilation. Selected
actual tools, target-output cooperation, error/cleanup custody and bounded retained payloads
must be explicit. No reference script supplies Nara's missing provenance, stale-result or
immutable-byte evidence; those require the actual selected public consumers.

## Native Integration Counterexample

The first actual native supervised runs exposed a separate transport assumption. Full metadata
overflowed the existing 256 KiB lossy platform-output queue before the Editor could consume it;
the build correctly reported `OutputLoss` for metadata stdout. A synthetic bounded decoder test
and the synchronous CLI reader did not traverse this queue. Preserve the diagnostic tail but
give build stdout an explicit cumulative protocol allowance, independent of stderr and read-chunk
count. Overflow remains a refused build, and cancellation still joins the original readers and
process owner. The unit's native evidence must qualify this correction rather than treating the
first failure as a window/backend prerequisite failure or restoring `--no-deps`.
