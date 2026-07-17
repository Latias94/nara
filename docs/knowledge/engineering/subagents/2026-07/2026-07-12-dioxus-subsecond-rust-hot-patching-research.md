---
type: "Subagent Finding"
title: "Dioxus hot reload and Subsecond Rust hot-patching boundaries"
description: "Primary-source review of Dioxus RSX and asset reload, Subsecond runtime patching, current limitations, failure behavior, and realistic use with a Rust-first bevy_ecs game runtime."
timestamp: 2026-07-12T01:43:53Z
record_id: "d2457d96-0855-4043-8356-b593dc11882e"
tags: ["nara", "rust", "hot-reload", "hot-patching", "dioxus", "subsecond", "bevy-ecs"]
status: "complete"
producer_id: "subsecond-research"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
verified_by: "Dioxus and Subsecond first-party documentation, source, and release notes"
---

# Finding

## Executive conclusion

Subsecond is credible evidence that Rust gameplay iteration does not always have to mean restarting
the whole process. It is an experimental development-time function hot-patching system, not a general
Rust state-migration or dynamic-ABI solution.

The useful product distinction is:

1. **Dioxus RSX hot reload** reparses and diffs supported `rsx!` template changes without compiling
   Rust. It is specific to Dioxus UI templates.
2. **Dioxus asset hot reload** watches and refreshes CSS, images, and other static assets. This is a
   data/asset pipeline, not machine-code replacement.
3. **Subsecond hot-patching** incrementally compiles Rust, produces a patch dynamic library or WASM
   side module, and redirects explicitly integrated calls through a new jump table while the process
   remains alive.

For Nara, Subsecond is worth a bounded prototype behind an optional development plugin. The strongest
candidate boundary is a complete gameplay tick, schedule, state, or system callback invoked at a
quiescent main-thread point. Nara must own pause/quiescence, compatibility checks, world retention or
rebuild, diagnostics, and fallback. It must not promise that arbitrary Rust edits, component layout
changes, dependency changes, active parallel systems, or native resources can be patched safely.

An optional Luau package solves a different problem. It can provide fast reload for gameplay written
against a stable script host, while Subsecond can reduce iteration latency for Rust function bodies.
Neither needs to be mandatory, and neither should redefine Nara's Rust-first core.

## Version and evidence baseline

This note was checked on **2026-07-12** against:

- Dioxus `v0.7.9`, the latest stable GitHub release visible on the check date.
- Dioxus `v0.8.0-alpha.0`, published on 2026-05-19.
- Dioxus `main` commit
  [`f717a8e184a522d078b70bb4b4d62a5f9a99ddfc`](https://github.com/DioxusLabs/dioxus/tree/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc).
- Dioxus docsite commit
  [`22a93ecf49e68754d8c821eb55443fb04746246b`](https://github.com/DioxusLabs/docsite/tree/22a93ecf49e68754d8c821eb55443fb04746246b).

There is important version drift. The stable 0.7 guide and the `subsecond` crate-level documentation
say only the tip/binary crate is patched. Current `main` has newer workspace-crate replay code. The
workspace path is therefore an active implementation direction, not a stable 0.7 contract.

Sources:

- [Dioxus 0.7 hot-reload guide](https://github.com/DioxusLabs/docsite/blob/22a93ecf49e68754d8c821eb55443fb04746246b/docs-src/0.7/src/essentials/ui/hotreload.md)
- [Dioxus v0.7.9 release](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.9)
- [Dioxus v0.8.0-alpha.0 release](https://github.com/DioxusLabs/dioxus/releases/tag/v0.8.0-alpha.0)
- [`subsecond` crate documentation and implementation](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs)

## What Dioxus hot reloads

### RSX template hot reload

The 0.7 guide says the devtools parser reparses `rsx!` independently of Rust compilation. Supported
edits include many changes to element structure, styling, text, string attributes, movement of
already-compiled formatted expressions, simple literal component properties, and markup inside
existing loops and conditional branches.

It does **not** compile new Rust semantics. The guide lists these as requiring a Rust rebuild or
Subsecond patch:

- new variables or expressions absent from the last compilation;
- logic changes outside `rsx!`;
- component signature changes;
- imports or module-structure changes;
- complex attribute expressions involving function calls.

The implementation is deliberately conservative. It parses old and new files, calls `diff_rsx`, and
sets `needs_rust_rebuild` when the file shape or template cannot be represented by the old compiled
dynamic pools. `HotReloadResult::new` returns `None` for an unsupported diff; it does not attempt an
unsafe partial template update.

Sources:

- [0.7 RSX hot-reload capabilities and rebuild cases](https://github.com/DioxusLabs/docsite/blob/22a93ecf49e68754d8c821eb55443fb04746246b/docs-src/0.7/src/essentials/ui/hotreload.md#rsx-hot-reload)
- [RSX diff implementation and documented structural limits](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/rsx-hotreload/src/diff.rs#L1-L16)
- [CLI fallback from an unsupported RSX diff to a Rust rebuild](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/serve/runner.rs#L466-L529)

### Asset hot reload

Dioxus separately watches CSS, images, and other files referenced through its asset pipeline. CSS can
be refreshed and SCSS/Tailwind can be rebuilt by their dedicated tooling; static asset URLs are
invalidated or recopied. This preserves the running Rust program because it replaces data consumed by
the renderer, not Rust code.

This pattern is directly transferable to Nara at the architectural level: source change detection,
asset reimport, generation-stamped preparation, and last-good runtime values are lower-risk and should
remain independent of native code patching.

Source: [0.7 asset hot reload](https://github.com/DioxusLabs/docsite/blob/22a93ecf49e68754d8c821eb55443fb04746246b/docs-src/0.7/src/essentials/ui/hotreload.md#asset-hot-reload).

## What Subsecond hot-patches

Subsecond replaces compiled Rust function implementations reached through explicit integration
points. `subsecond::call` wraps a repeatable closure, and `HotFn::current` exposes a callable whose
implementation address is looked up in the current jump table. In release builds without
`debug_assertions`, `call` directly invokes the original closure, so the runtime indirection is a
development-mode behavior.

The external build side performs an incremental Rust compile, links changed objects into a patch
dynamic library (or relocatable WASM module), constructs old-to-new function-address mappings, and
sends a `JumpTable` to the running process. Native `apply_patch` loads the library, adjusts addresses
for ASLR, and atomically replaces the global jump-table pointer. The original executable is not
rewritten in place.

The initial "fat" build preserves symbols and caches link metadata. Later "thin" builds reuse
captured rustc arguments and saved object files. Current source uses normal `rustc`/Cargo outputs plus
platform linkers and flags such as `-Csave-temps=true` and `-Clink-dead-code`; ThinLink remains
integrated into the Dioxus CLI rather than published as a standalone tool.

Sources:

- [Subsecond mechanism and explicit `call` boundary](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L13-L65)
- [`HotFn` lookup and invocation](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L350-L481)
- [Native/WASM patch application](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L484-L690)
- [Fat/thin compiler flags](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/request.rs#L1818-L1889)
- [Thin patch linking](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/link.rs#L135-L337)
- [ThinLink availability statement](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L168-L183)

## State retention is integration-dependent

The process stays alive, so state not discarded by the framework can remain resident. That is not a
blanket guarantee that all state remains valid.

Subsecond explicitly says struct layout and alignment changes are unsupported: new code using an old
allocation with a changed layout can crash. It asks framework integrations to dispose, rebuild, or
re-instance affected state. Its own documentation describes Dioxus as rebuilding incompatible state.

Dioxus has framework-specific machinery rather than automatic Rust migration:

- component functions are invoked through `HotFn`;
- a successful patch marks all virtual-DOM scopes dirty;
- hook slots retain a value only when the requested concrete type still downcasts successfully;
- in debug mode, an incompatible hook type can replace the old slot;
- a changed component render-function identity causes that component scope to be replaced and its
  resources dropped.

Therefore, "preserves app state" should be read as "can preserve compatible state selected by the
framework without a process restart," not "migrates arbitrary Rust object graphs."

Sources:

- [Struct layout limitation and framework-owned re-instancing](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L94-L109)
- [Dioxus component `HotFn` integration](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/core/src/properties.rs#L140-L174)
- [Patch handler marks the virtual DOM dirty](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/core/src/virtual_dom.rs#L770-L775)
- [Hook type retention/replacement behavior](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/core/src/scope_context.rs#L330-L395)
- [Changed component functions replace and destroy the old scope](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/core/src/diff/component.rs#L117-L170)
- [Dioxus 0.7 release explanation of synchronization points and state migration](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0#rust-hot-patching-with-subsecond)

## Detailed limitation matrix

| Change | Verified behavior | Confidence and consequence |
|---|---|---|
| Ordinary function/closure body | Subsecond is designed to compile changed Rust and route an explicit hot call to the latest mapped implementation. | High for integrated calls; not a guarantee for every call in the program. |
| Function signature | `HotFn::try_call_with_ptr` requires unchanged argument layouts; malformed/mismatched pointers can crash. | High. Treat signature changes as incompatible unless the enclosing state is rebuilt. |
| Capturing closure | `HotFunction` is implemented for repeatable `FnMut`; a closure's captured environment is passed to the patched `call_it`. Non-zero-sized/trait-object handling has documented vtable caveats. | High. Captured field layout changes are the same class of risk as struct layout changes. |
| `FnOnce` closure | Explicitly unsupported because a patch boundary may invoke the function repeatedly. | High. |
| Generic function | Rust monomorphizes generic code into concrete instances. Subsecond documentation warns that forwarded generics can cause cascading codegen changes; current main recompiles changed workspace crates and their dependent chain to catch generic instantiations. | High for the limitation; do not promise isolated generic patches. |
| Inline function | Rust's `#[inline]` is only a hint, but an already inlined body exists in the caller and cannot be changed merely by redirecting the callee symbol. Dioxus has special WASM logic to retain/promote indirect functions, but no first-party blanket guarantee was found for arbitrary native inlining. | High as a risk, deliberately not stated as an absolute failure rule. |
| Struct/enum/component/resource layout | Struct size/alignment hot reload is unsupported unless the framework discards or migrates old instances. | High. This includes ECS component/resource values and closure environments. |
| Existing global/static value | Globals are tracked across patches and intentionally persist. Renaming is treated as a new global. | High. |
| Static initializer edit | The new initializer is not observed for an already existing static. | High. A full rebuild/restart is required for normal initialization semantics. |
| New global | Can be added, but its destructor is never called. Patch libraries are intentionally leaked. | High. |
| Tip-crate thread local | Current crate docs warn that it may appear reset and complex use can crash or segfault. | High. Avoid relying on tip-crate TLS at patch boundaries. |
| Workspace crate source | Stable 0.7 docs say unsupported. Current main replays modified workspace crates in dependency order and recompiles dependents. | High, but version-sensitive and not a stable 0.7 promise. |
| Cargo dependency/features/profile change | Current main classifies compilation-affecting `Cargo.toml` edits as a full fat rebuild, not a thin patch. Newly added workspace members require restarting `dx serve` before their files are watched. | High for current main. |
| External dependency implementation | No first-party guarantee of patching arbitrary registry/git dependencies was found. Current workspace replay code explicitly traverses workspace members. | High that it is unpromised; exact future behavior is unknown. |
| Active async work | Dioxus's non-Dioxus helper drops the current future and constructs a new one after a patch. | High for that helper; not automatic for arbitrary executors or tasks. |
| Active parallel work | The runtime uses a relaxed atomic jump-table pointer and contains a source comment that heavily multithreaded frameworks such as Bevy may need stronger synchronization. | High as an implementation risk. No stop-the-world safety contract exists. |
| WASM stack rewind | Crate docs say Rust/WASM does not support the `call` unwind strategy; a framework can instead drop and recreate a future. | High. |

Supporting sources:

- [`HotFunction` supports `FnMut`, not `FnOnce`](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L847-L874)
- [Trait-object/vtable caveat and argument-layout safety condition](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L416-L478)
- [Statics and thread-local limitations](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L77-L92)
- [Generic cascade tracking in current main](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/builder.rs#L347-L427)
- [Workspace crate replay](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/link.rs#L135-L221)
- [Cargo configuration changes force a full build](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/serve/runner.rs#L580-L640)
- [Rust Reference: `inline` is a suggestion](https://doc.rust-lang.org/reference/attributes/codegen.html#the-inline-attribute)
- [Rust compiler guide: monomorphization](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html)
- [WASM indirect-function/inlining handling](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/patch.rs#L520-L539)
- [Multithreaded jump-table synchronization concern](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L273-L280)
- [WASM unwind limitation](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L241-L270)
- [Async helper drops and recreates the future](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/devtools/src/lib.rs#L112-L217)

## Platform and toolchain support

Current crate documentation lists:

- Android: `arm64-v8a`, `armeabi-v7a`;
- iOS: arm64 simulator use, with physical iOS devices explicitly excluded because of code signing;
- Linux: x86_64 and aarch64;
- macOS: x86_64 and aarch64;
- Windows: x86_64 and arm64;
- WebAssembly: wasm32.

This is a declared support list, not equal evidence of maturity on every target. The source contains
platform-specific loaders and linkers, WASM relocation and table rewriting, Android executable memfd
loading, and explicit warnings that some paths are experimental. The v0.8 alpha release includes
additional hotpatch fixes, which is evidence that the platform matrix remains active engineering.

The library is Rust 2024 in current main. The build engine is coupled to rustc/Cargo invocation
details, linker flavors, saved object files, symbol tables, ASLR, dynamic loading, and target-specific
post-processing. It is not simply a portable function-pointer crate.

Sources:

- [Declared platform support](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L193-L208)
- [`subsecond` target dependencies and Rust 2024 edition](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/Cargo.toml)
- [Platform linker selection](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/link.rs#L340-L514)
- [v0.8 alpha hotpatch changes](https://github.com/DioxusLabs/dioxus/releases/tag/v0.8.0-alpha.0)

## Failure, fallback, and rollback behavior

### Before a patch is committed

- A Rust compile or link failure produces diagnostics and no new `JumpTable`; the running process has
  not yet installed that patch.
- Unsupported RSX edits are escalated to a Rust rebuild/hotpatch attempt.
- Changes to compilation configuration or dependency topology are classified as full rebuilds.
- On native targets, a dynamic-library load failure returns `PatchError::Dlopen` before
  `commit_patch`, so the previous global jump table remains installed.

This provides an important last-good-code property for ordinary build/load failures, but it is not a
complete transaction across asset copying, patch delivery, runtime quiescence, and state validation.

### After or during application

- `apply_patch` is `unsafe`; its documentation says malformed pointers or changed signatures can
  crash the program.
- Native patch libraries and jump-table boxes are intentionally leaked because unloading code still
  referenced by objects or destructors is unsafe.
- Current Subsecond has one current jump-table pointer, no documented pointer-version history, no
  compatibility manifest, and no public rollback API.
- Dioxus CLI keeps a patch list so a newly connected client can replay patches. That is session
  synchronization, not a verified rollback mechanism.
- Some convenience integrations call `unwrap()` on patch application. For example,
  `connect_subsecond` applies a patch in the devtools connection thread and unwraps the result.
- The WASM loader performs asynchronous fetch/compile and then mutates shared memory/table state; its
  error paths include panics and it returns `Ok(())` before the spawned task completes. This is not a
  synchronous success acknowledgement suitable for a transactional engine contract.

Sources:

- [`apply_patch` safety warning and commit order](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L484-L546)
- [WASM asynchronous application path](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L549-L690)
- [No function-pointer versioning](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L111-L122)
- [Patch history is committed for client replay](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/build/builder.rs#L759-L803)
- [`connect_subsecond` error handling](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/devtools/src/lib.rs#L69-L109)

## License and independent integration

The published `subsecond` and `subsecond-types` manifests declare
`MIT OR Apache-2.0`. The Dioxus repository contains both license texts. The crate-level prose still
says MIT only, so consumers should rely on the Cargo manifest/SPDX declaration for the crate license.

Independent runtime integration is explicitly supported at the API level:

- `subsecond::call`, `HotFn`, `register_handler`, and unsafe `apply_patch` are public;
- `dioxus-devtools::connect_subsecond` is intended for non-Dioxus projects;
- the 0.7 release shows non-Dioxus use and says the CLI removes Dioxus branding for such projects.

Independent **toolchain** integration is not turnkey:

- the crate docs require a third-party compiler/protocol implementation and recommend Dioxus CLI;
- `JumpTable` is public, but producing a valid table requires the Dioxus build/link pipeline;
- ThinLink is explicitly not available as a standalone tool;
- the 0.7 release says the complex engine is, for now, shipped inside Dioxus CLI.

Nara could legally prototype with the crates and invoke or adapt the CLI, but adopting Subsecond as a
product feature would create a real toolchain integration and maintenance obligation. A small runtime
dependency alone is insufficient.

Sources:

- [`subsecond` Cargo license](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/Cargo.toml)
- [Repository MIT license](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/LICENSE-MIT)
- [Repository Apache-2.0 license](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/LICENSE-APACHE)
- [External compiler/protocol requirement and Dioxus CLI recommendation](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L26-L44)
- [Public patch integration](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/subsecond/subsecond/src/lib.rs#L150-L166)
- [Dioxus 0.7 release on non-Dioxus use and CLI packaging](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0#rust-hot-patching-with-subsecond)

## Realistic Nara + `bevy_ecs` integration

The following is an engineering assessment from the verified constraints above, not a claim that
Dioxus officially supports Nara or guarantees these behaviors.

### Candidate integration boundary

Use an optional development plugin around a coarse, explicit, quiescent call:

```text
watch/build patch
      |
      v
request pause at a complete fixed-tick boundary
      |
      v
finish current schedule and join task/system work
      |
      v
validate patch class and apply jump table
      |
      +--> compatible function-only patch: retain World, rebuild schedules if required
      |
      +--> structural/unknown patch: reject and restart from last-good document/checkpoint
      v
resume on the next complete tick
```

Strong candidates for `HotFn` entrypoints are:

- the application gameplay callback registered by the game crate;
- a whole fixed gameplay schedule invoked once per tick;
- coarse behavior/system adapters that can be re-registered at a safe boundary.

Wrapping arbitrary inner `bevy_ecs` system calls is weaker. System functions are transformed into
system state, may be generic, may hold cached access metadata/local state, and may execute in parallel.
Nara would still need to decide whether to preserve, recreate, or reject every affected system state.

### State policy

The safest first prototype is:

- retain `World` only for verified function-body-only patches;
- rebuild schedule/system objects at the boundary rather than assuming their captured layouts are
  unchanged;
- reject component/resource/event type layout changes;
- keep editor documents and asset state outside the native patch lifetime;
- preserve a last-good executable and use restart plus document/checkpoint restoration as the reliable
  fallback.

Even retaining `World` is conditional. A component's Rust layout is embodied in both stored values and
ECS registration metadata. Subsecond does not provide the schema comparison or migration needed to
make a changed component safe. Nara's reflection/schema catalog can help classify changes, but that is
Nara-owned validation, not Subsecond functionality.

### Threading and quiescence

Do not apply patches concurrently with a running parallel schedule, background callback, render
extraction, or service callback. Current Subsecond uses a relaxed atomic jump-table swap and itself
notes the potential issue for Bevy-like multithreading. A Nara experiment should apply only on the
main execution authority after all engine-owned work for the tick has reached a declared safe point.

### Success criteria for a bounded prototype

Measure before making any strategy promise:

- Windows and Linux patch success for a tip-crate gameplay function body;
- workspace gameplay-crate behavior separately on a pinned 0.8/main revision;
- P50/P95 edit-to-running-code latency versus incremental build plus runtime restart;
- `World` preservation for explicitly compatible edits;
- clean rejection of component/resource layout, function signature, static initializer, Cargo feature,
  and dependency changes;
- behavior with parallel `bevy_ecs` schedules and outstanding tasks;
- memory growth across hundreds of patches, since libraries and jump tables are leaked;
- debugger quality, because current Dioxus CLI warns that symbols may not work properly in hotpatch
  mode;
- deterministic recovery after compiler, linker, loader, and runtime validation failures.

Source for debugger warning: [Dioxus CLI debugger handling](https://github.com/DioxusLabs/dioxus/blob/f717a8e184a522d078b70bb4b4d62a5f9a99ddfc/packages/cli/src/serve/runner.rs#L1523-L1528).

## Recommended product wording

Nara should describe the layers without making any one of them mandatory:

> Nara is Rust-first. Asset and scene data reload are core development workflows. Faster Rust
> iteration may be provided through optional development-time hot-patching where a change is proven
> compatible; otherwise Nara rebuilds and restarts from the last good runtime state. Script runtimes
> such as Luau can be installed as packages when a project wants a separately reloadable gameplay
> layer.

Do not promise:

- "all Rust code hot reloads";
- "state is always preserved";
- "component changes migrate automatically";
- "dependency changes never restart";
- "hotpatch works safely during parallel ECS execution";
- "Subsecond is a stable standalone toolchain";
- "hotpatch failure always rolls back atomically."

This layered position keeps the official production path Rust-only while leaving two optional latency
tools: Subsecond-like native function patching for compatible Rust edits, and a Luau package for teams
that deliberately choose script-defined gameplay. It also preserves a reliable baseline: asset/data
reload plus incremental compile, last-good build, runtime restart, and state restoration.

## High-confidence answers

1. **Can Subsecond materially reduce Rust edit latency?** Yes, for compatible, explicitly integrated
   function changes; Dioxus demonstrates a working compiler/linker/runtime system across multiple
   targets.
2. **Does it eliminate Rust rebuilds?** No. It still compiles Rust, and structural/configuration changes
   require a full build or framework-owned reconstruction.
3. **Can it preserve a running ECS world?** Potentially for compatible code-only edits, because the
   process need not restart. Subsecond does not prove that a `bevy_ecs::World` is safe after arbitrary
   patches; Nara must enforce the boundary.
4. **Can it replace a scripting runtime?** No. It patches native Rust code and inherits native layout,
   linker, platform, and toolchain constraints. A Luau adapter provides a different reload and sandbox
   model.
5. **Should Nara adopt it now as a product guarantee?** No. Prototype it as an optional development
   plugin after the ordinary rebuild/restart loop is measurable and reliable.
6. **Should Nara remain extensible for it?** Yes. Explicit quiescent callback/schedule boundaries,
   inspectable schema compatibility, runtime isolation, and last-good recovery are useful regardless
   of which hotpatch or scripting implementation is eventually chosen.
