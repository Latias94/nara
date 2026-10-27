# Authored Behavior Code Iteration And Physical Lifetime

**Date:** 2026-10-27. **Status:** Private one-owner comparison with unchanged Schema/native
layout. Not a public adapter, language selection or complete adoption of ADR 0120.

## Owning Evidence

This follows the [Schema-driven successor](2026-10-27-schema-driven-behavior-comparison.md)
under the [code-lifetime plan](../plans/2026-10-27-behavior-code-lifetime-plan.md). The current
[fixture](../../tests/fixtures/behavior-languages/README.md) retains complete source/history,
external catalog, actual language callbacks and repeated Prefab/export tests. It replaces the
permanently loaded authored C# implementation; there is no compatibility entry retaining it.

Nara baseline is `f1e97b9`. Godot source observations use
`81ab961f91066a63bcbf3ec340962bb1a1ada558`; mlua 0.12.2's published VCS revision is
`a186d461c7772aed4cb0a9fa1d1899121c6450e7`. No production engine/Editor source changes.

## Two Different Retention Obligations

### Lua Needs An Explicit VM Owner

Inspection of pinned [ValueRef](https://github.com/mlua-rs/mlua/blob/a186d461c7772aed4cb0a9fa1d1899121c6450e7/src/types/value_ref.rs)
shows that Function's underlying value stores WeakLua. Merely retaining the Function does not
establish physical VM custody. The actual test keeps a bare function, drops its program and
observes both a dead WeakLua and destruction of a registry-rooted Rust userdata. It deliberately
does not call the expired function.

The prepared-callback lease therefore holds both an actual Lua clone and the callable Function.
After the owner switches to another VM and drops the old program, the explicit lease keeps the
old VM alive and executes its original body. Releasing the last strong lease makes WeakLua
unupgradable and destroys the registry probe. The
[pinned RawLua destructor](https://github.com/mlua-rs/mlua/blob/a186d461c7772aed4cb0a9fa1d1899121c6450e7/src/state/raw.rs)
calls `lua_close`; the probe is not a wrapper-local boolean set by Program Drop.

This is one VM per candidate with synchronous calls. It does not prove coroutine cancellation,
escaped native callbacks, arbitrary finalizer safety, shared-VM selective reload or an OS sandbox.

### C# Roots Delay Collectible Context Retirement

The permanent hostfxr-loaded bridge contains only the contract, DTOs, routing/control and its
bookkeeping. Actual authored callbacks live in `Nara.AuthoredBehaviors`, loaded into a distinct
collectible AssemblyLoadContext. The module and dependent assemblies load from streams; the
contract resolves to the permanent bridge assembly. This keeps authored file handles out of
the live instance, rather than relying on copies at a different output path.

Retirement removes ordinary `describe`/`invoke` admission, clears module/context strong roots
and requests `Unload`. A retained instance lease independently roots real authored code even
with zero executing invocations. Polling while that lease exists reports incomplete, and the
test-only lease probe can still execute the old body. After explicit release, the weak context
must disappear during GC/finalizer observation before the test reports collection.

This follows the distinction in the [primary hosting study](2026-10-27-dotnet-behavior-hosting-feasibility.md).
Godot's pinned
[PluginLoadContext](../../repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs)
uses stream loading and shared contract assemblies; its
[Main](../../repo-ref/godot/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs) separates non-inlined
strong-root release from weak-reference observation. Nara copies neither Godot's timeout policy
nor a claim that `Unload()` is completion.

The fixture polls at most sixteen collection observations and fails if the context remains
alive. One observation calls collect/wait-for-finalizers/collect; that is not a bounded wall-time
operation for arbitrary authored code. No timeout is converted into success. This establishes
collection of these authored contexts, not CLR unload, immediate native resource release, RSS
return, crash isolation or universal ability to unload arbitrary packages.

## Actual Body Iteration And Routing

Lua writes module source to a real temporary file. A syntax error and then a candidate whose
last required binding has the wrong version both refuse replacement while the predecessor
remains executable. A body edit prepares another real VM, validates every callback binding and
switches at the synchronous owner boundary.

Independent review exposed a missing-readiness case: valid bindings with a deleted or non-callable
`invoke` could otherwise replace the old route and fail only on first execution. Compile now
captures the actual Function and owned binding list before readiness. Missing and wrong-type
exports reject without executing authored callbacks or replacing the predecessor.

C# prepares a missing-module failure and a compiled candidate with only the last required Sensor
binding mismatched. Its rejected context is explicitly retired and observed collected. The
test then rebuilds `TrialRevision=2` to the **same live DLL path while revision one is still
loaded and leased**, loads its new bytes and admits the complete binding list. This is a real
managed build of a conditional body variant, not a measured human edit session or arbitrary
source-migration experiment. It builds no Rust source in the running test process.

Both preserve the exact native World and Definition and change actual Name from `Moved 2` to
`Moved 4`. A prepared response carries a separate exact execution-publication receipt; the
native entity token alone would not detect this same-World, same-Schema code change. Old
prepared results, and old results computed under a retained callback after the switch, reject
through the successor route while its Name remains unchanged.

The old response is pure owned data and need not pin executable code. The callback/instance
lease is the actual physical root. This is why using one global generation for Schema, native
World and managed body lifetime would conflate different obligations.

## Executed Qualification

```text
python tests/fixtures/behavior-languages/run.py
```

Final run `e840056a-7acd-4f01-8bc9-19fd2f7e9182`: **6 passed, 0 skipped**, including the
complete prior Schema/source workflow and the three lifetime tests. Managed builds report zero
warnings/errors. Rust build is 5.58 seconds and the suite 2.642 seconds; the controlled same-path
managed body rebuild inside its test is 2.2287652 seconds. These are cache- and fixture-specific
observations, not a comparative language latency target or engine benchmark. Lua test durations
include different setup/work and cannot be divided into the managed build timing.

Focused correctness Clippy and formatting pass. The
[verification record](../knowledge/engineering/verification/2026-10/2026-10-27T091400Z-actual-authored-code-iteration-with-retained-lua-vm-and-collectible-managed-code-060183fe1373441980ece9733ef76f57.md)
owns exact commands, review disposition and qualification limits.

## Scope And Consequences

The actual result strengthens the existing proposal for immutable owner execution publications:
body-only replacement can keep native layout, data and World while changing an owner-local
execution route. It does not justify weakening the native registry or turning all domain calls
into JSON. It also does not establish that this particular private control protocol belongs in
the engine core. The concrete Lua owner needs explicit VM custody; the concrete C# owner needs
controlled roots and a permanent bridge. Their internals are not interchangeable merely because
both expose an invocation.

The experiment has no active callback at the switching safe point, one module/owner, unchanged
Schema, no arbitrary VM-local state and one Name command domain. It does not qualify parallel
in-flight callbacks, dependency cohorts, catalog migration, Stop-first, post-publication cleanup
failure or a public representation owner. Contract-name matching and current callback IDs/
versions are not dependency/ABI compatibility proofs. The fixture's private retained weak-entry
bookkeeping is finite test custody, not a production cache/eviction design.

Next select an actual responsibility-owned production path using complete author workflow and
concrete domain access evidence, rather than growing both trial adapters into two permanent
SDKs. Public Scene representation ownership, actual assets, data/VM state authority and the
remaining foundation delivery waves are still open. ADR 0120 remains Proposed and the full
foundation Goal active.
