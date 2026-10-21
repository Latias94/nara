# Rust Authoring, Reload, And ECS Direction

Date: 2026-10-21. Nara basis: `6bb1a355c2e09d60979b063569b1de2237e25163`.
This is an architecture investigation and recommendation, not an adopted contract, implementation
plan, performance result, or qualification of a hot-reload library.

## Recommended Direction

Keep Rust as the complete game-authoring language and Bevy ECS as the substrate. Invest in the
iteration experience around those choices: data reload, measured rebuild-and-resume, and a bounded
development-only function-patching experiment. An optional scripting adapter becomes worthwhile
when a real game/team needs its authoring workflow. Neither script adoption nor ECS replacement
is a prerequisite for investigating Rust hot reload.

This recommendation agrees with [Strategy](../../STRATEGY.md),
[ADR 0002](../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md), and
[ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md).
Agreement with existing decisions is not the evidence for it; the mechanisms, consumer costs,
and remaining uncertainty are examined below.

## Separate The Decisions

| Decision | Relevant question | Current recommendation |
| --- | --- | --- |
| Authoring language | What lets the target team implement, debug, package, and maintain a complete game? | Complete Rust workflow; optional script package for a demonstrated authoring need |
| ECS substrate | Which implementation supplies the required query, scheduling, lifecycle, and tool semantics at acceptable cost? | Retain Bevy ECS and measure concrete costs before changing the substrate |
| Development iteration | How does each class of edit reach a useful running test situation? | Distinct data, compatible-function, and structural-change routes |
| Shipped updates | How are compatible content and executable versions delivered to players? | A separate delivery/update design; development DLL reload does not establish it |

Changing the ECS cannot remove Rust's native-code compatibility requirements. Conversely, a
script VM can run beside Bevy ECS; its script representation and bindings own the reload contract.
These are architectural deductions, not comparative performance measurements.

## What The Supplied Hot-Reload Article Adds

The author's [Hotreload in Rust](https://alordash.github.io/alordash/blog/hotreload-in-rust/)
describes a useful candidate distinct from whole-process restart: build a Rust library, redirect
annotated function calls into a copied dynamic-library generation, atomically replace its handle,
and keep an old generation alive through a per-call guard. It also describes symbol caching and
the `rlib`/`dylib` split. Those mechanisms reduce manual loader work; they do not establish ECS
state migration or coordinated engine-wide activation.

The implementation was inspected at `alordash/hotcode`
[`d125836c7ea80d6ee70067675e8c0a49aa6d3bd0`](https://github.com/alordash/hotcode/tree/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0).
Its workspace declares version **1.0.3**. The article links version **1.0.2** documentation;
these are different evidence baselines, not interchangeable release claims.
[Workspace manifest](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/Cargo.toml#L9-L12),
[versioned documentation](https://docs.rs/hotcode/1.0.2/hotcode/).

### Verified Mechanism

- The macro adds a function entry branch and an exported symbol. It synthesizes a native function
  pointer from the annotated argument/return types. Symbol lookup then transmutes a raw pointer;
  this path does not compare old/new function signatures or component schemas.
  [Macro](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode_proc_macro/src/hotreload_syntax.rs#L29-L114),
  [lookup](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode_core/src/library_wrapper.rs#L38-L66).
- The watcher debounces file events, constructs a replacement library, and publishes it through
  `ArcSwap`. It does not consult Nara's schedule or task owner before publication.
  [Watcher](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode_core/src/library_watcher.rs#L9-L53).
- `FnGuard` owns an `Arc<LibraryWrapper>` beside the function pointer. Copying the library before
  loading avoids holding the Cargo output file open. Those are useful generation-lifetime and
  build-output mechanisms.
  [Guard](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode_core/src/fn_guard.rs#L5-L24),
  [copy/load](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode_core/src/library_wrapper.rs#L14-L29).

### Limits That Matter For Nara

| Boundary | Evidence or inference | Consequence |
| --- | --- | --- |
| ABI/signature changes | Upstream explicitly documents undefined behavior when a caller retains the old ABI | Successful Cargo compilation of the library is insufficient compatibility admission |
| Escaping references | The inspected 1.0.3 source documents a returned static reference dangling after old-library unload | Keeping a library alive during the call does not keep all returned values valid afterward |
| Retained native behavior | Nara inference: a callback, observer, task closure, trait-object vtable, or destructor may refer to library code after the entry call ends | Every participating retained code reference needs generation custody or retirement; a per-call guard alone is insufficient |
| World and system state | No ECS migration or Nara schedule admission appears in the inspected loader/macro path | A constant `fn(&mut World)` signature does not prove compatible component layouts, registrations, or stored system state |
| Publication | Watcher stores each library independently as file events arrive | Atomic publication of one pointer is not coherent activation of every function/library used by a frame |
| Failure | Copy/load/symbol paths can panic; replacement construction precedes pointer publication | This ordering is useful but does not establish structured recovery or engine transaction rollback |

The first two limits are explicit in
[upstream limitations](https://github.com/alordash/hotcode/blob/d125836c7ea80d6ee70067675e8c0a49aa6d3bd0/src/hotcode/src/lib.rs#L94-L131).
The later rows are deductions from the cited implementation and Nara's retained ECS/task owners.
The loader's dependency also requires the caller to supply the correct symbol type and uphold
library initialization/termination safety;
[libloading safety contract](https://docs.rs/libloading/0.9.0/libloading/struct.Library.html#safety).
Using `cdylib` or `extern "C"` would not by itself make arbitrary `World` references, native
layouts, callbacks, or destructors a stable boundary.

This makes `hotcode` a credible **experiment candidate**, not an integration rejected merely
because Rust lacks a universal reload contract. Its library-crate model is relevant to Nara's
[two-level game](../../samples/two-level-game/Cargo.toml), whose logic is in a library consumed
by small desktop/headless binaries. Actual dependency invalidation, binary/DLL size, debugging,
and useful-result latency still need a matched experiment. Upstream function-call microbenchmarks
do not answer those product questions.

## Lessons From Other Engines

The dedicated studies carry the source anchors and version limits:

- [Bevy ECS and Rust hot-patch boundaries](2026-10-21-bevy-ecs-and-rust-hotpatch-boundaries.md):
  Nara exposes real Bevy-shaped semantics. Bevy's optional patch route refreshes function pointers
  while retaining World/system state; it does not supply Nara-wide structural admission.
- [Godot language and reload boundaries](2026-10-21-godot-language-and-reload-boundaries.md):
  VM member remapping, managed assembly reconstruction, and native-extension reconstruction are
  distinct mechanisms with different state and restart limits.
- [Unity, Unreal, and Defold](2026-10-21-mature-engine-iteration-boundaries.md): serialization,
  reflected-object reinstancing, and script/resource reload depend on explicit runtime contracts.

The transferable pattern is to own representation, retained state, and failure behavior at an
explicit boundary. No comparison establishes that an arbitrary Rust World can survive every edit.
Godot's SceneTree-facing authoring model also does not establish that Nara needs a custom ECS.

## Language Choice

| Approach | Main benefit | Product cost and decision |
| --- | --- | --- |
| Rust as the complete language | Direct public systems, ordinary Rust tooling, one complete production path | Own compile/activation latency as product work; recommended baseline |
| Rust plus optional scripting | Fast rule/quest/AI iteration or a useful workflow for other team members | Adapter owns VM, bindings, reload state, debugger/editor integration, package and target support; add for a real consumer |
| Scripting as the primary language | Can optimize the whole editor around a controlled dynamic state representation | Changes the primary audience/workflow and creates a second complete runtime/tooling product; requires an explicit Strategy decision |

For a script consumer, start with a limited gameplay domain and batched reads/commands, not an
ambient mutable World. Script state must have a declared reload policy. The choice between Luau,
Lua, C#, or another runtime remains open until its workflow, debugging, distribution, and boundary
costs are compared. A VM being fast to reload is not enough to choose its language for Nara.

## ECS Choice

The [Bevy study](2026-10-21-bevy-ecs-and-rust-hotpatch-boundaries.md) identifies concrete Nara
coupling in queries, derives, scheduling, reflection, observers, and lifecycle-safe persistence.
An in-house ECS would need to replace or intentionally revise those consumers as well as storage.
It would not automatically solve code-generation lifetimes, layouts, or state restoration.

The completed [feature trial](2026-10-21-task-execution-baseline.md#bevy-ecs-feature-trial) retained
the default feature selection after no binary-size gain was demonstrated. This study does not
reuse an earlier static suggestion to remove `async_executor` as an implementation recommendation.

Reopen the substrate decision for a reproducible blocking semantic requirement, unacceptable
measured memory/frame/build cost attributable to ECS, or an upstream constraint that prevents
a required consumer. Compare an upstream correction, bounded fork, and alternative ECS against
the full consumer acceptance suite before selecting an in-house implementation. Dependency names
and source-file count do not establish those triggers.

## Next Experiments And Their Order

1. **Measure one useful rebuild-and-resume loop.** Use the two-level game, its explicit versioned
   checkpoint, and a declared test situation. Extend development state only for a demonstrated
   need such as a live test position; existing checkpoint semantics reconstruct position/input/tick.
   Keep authoring documents distinct from live runtime memory.
2. **Run one isolated function-patching comparison.** Compare `hotcode`'s library model with
   Subsecond's actual patched-crate coverage. Begin with a coarse, non-generic calculation using
   host-owned fixed inputs/outputs and no escaping library-owned references or callbacks. Prove
   the intended game function actually changes. This experiment can proceed independently of
   completing a general Editor integration; it must not become the correctness baseline.
3. **Consider an ECS-aware adapter only if the small experiment earns it.** Define explicit
   owner-controlled activation and retained-code custody. Unsupported signature/layout/closure,
   registration/plugin, dependency, static/TLS, or unknown changes rebuild and restore. Do not
   implement an incomplete Rust compiler in a verification script to infer compatibility.
4. **Evaluate a script adapter with its first game consumer.** Measure the complete edit/debug/
   package workflow and binding cost before turning it into a default language decision.

Record edit completion, Cargo start/end, generation publication, process activation, state
admission, and first useful presentation separately. The existing
[Rust Play measurements](../benchmarks/2026-10-20-rust-play.md#final-public-consumer)
end at accepted first presentation; their five-sample cohorts are not a baseline for restored test
state or stable tail latency. No new latency target is selected here.

Any later integration should cover compiler failure and repair, incompatible change rejection,
repeated updates, old callback/task retirement, Stop, fresh-start fallback, and long-run resource
growth. Keep current known-good executable custody truthful: a retained executable does not mean
its old process remained alive.

## Recovery And Evidence Limits

The source session `01a14cc1-8f06-7820-abf1-5e70e9984557` is historical evidence after recovery into
this session. Its persisted child edges do not establish live agent status. The mature-engine
note was recovered as completed output; the missing Bevy/Godot reports were researched anew.
The current receiving session and later compactions own subsequent continuity.

This work reads source and documentation only. It does not install a reload dependency, change
runtime/ADR contracts, run Cargo, migrate a live World, benchmark another engine, or qualify a
toolchain/platform. The web gateway failed; primary pages were retrieved directly. GitHub's
unauthenticated API returned a rate limit, so `git ls-remote` resolved the hotcode commit and raw
files were read at that commit. Guessed incorrect source paths returned 404 and are not evidence.
