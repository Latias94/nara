# .NET In-Process Behavior Hosting Feasibility

Date: 2026-10-27. Nara basis: `535bbd677bbad6563e0dd9f8436336b97e84038d`,
branch `experiment/behavior-language-workflows`. Research only; no production changes,
builds, package installation, downloads to disk, or commits were performed in this lane.
The parent reports SDK **10.0.400**, `Microsoft.NETCore.App` **10.0.11**, a successful
managed bridge build with zero warnings/errors, and a passing first Lua slice.
Those execution results were not independently repeated here.

## Immediate Binding Recommendation

**Use the existing managed library directly through hostfxr for the initial R3b slice.**
This is actual in-process execution. JSON buffers do not require process IPC. Keep one
fresh private Runtime and the loaded CLR for the fixture process lifetime. Do not claim
assembly unload, body reload, direct domain-call latency, or production hosting support.
Microsoft documents the initialize/get-delegate/load-managed-entry route in its
[native hosting tutorial](https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting).

The exact package pin verified against crates.io metadata is **`netcorehost = "=0.22.0"`**,
published **2026-07-26**, not yanked. Its packaged `.cargo_vcs_info.json` identifies
`85358e26871765b029ef56a15d2fd0d249794b1d`. This is an inspected release, not an
assumed version. Sources:
[version metadata](https://crates.io/api/v1/crates/netcorehost/0.22.0),
[packaged VCS metadata](https://docs.rs/crate/netcorehost/0.22.0/source/.cargo_vcs_info.json).

Suggested dependency declaration for the parent to apply:

```toml
netcorehost = { version = "=0.22.0", default-features = false, features = ["net10_0"] }
```

Disabling defaults matters: this release defaults to
`["nethost-download", "net10_0", "utils"]`; `nethost-download` enables NuGet acquisition
through `nethost-sys`. The direct `Hostfxr::load_from_path` route needs no nethost
discovery/linking dependency. See the
[pinned manifest](https://github.com/OpenByteDev/netcorehost/blob/85358e26871765b029ef56a15d2fd0d249794b1d/Cargo.toml#L44)
and [library API](https://docs.rs/crate/netcorehost/0.22.0/source/src/hostfxr/library.rs).
This does not promise that Cargo has all ordinary Rust dependencies cached.

### Minimal unmanaged entry

The integrated private method is:

```csharp
[UnmanagedCallersOnly]
public static unsafe int Invoke(byte* input, int len, byte* output, int capacity)
```

Its assembly-qualified declaring type is
`Nara.BehaviorTrial.Bridge, Nara.BehaviorTrial`.
The exact netcorehost method name is
**`get_function_with_unmanaged_callers_only`**; there is no need to invent a
`load_unmanaged_callers_only` wrapper.

Inside a fallible Rust runner, with `hostfxr_path` an absolute native-library path
and `runtime_config`/`assembly_path` owned `PdCString` values:

```rust
use netcorehost::{hostfxr::Hostfxr, pdcstr};

type Invoke = unsafe extern "system" fn(*const u8, i32, *mut u8, i32) -> i32;

let hostfxr = Hostfxr::load_from_path(hostfxr_path)?;
let context = hostfxr.initialize_for_runtime_config(&runtime_config)?;
let loader = context.get_delegate_loader_for_assembly(assembly_path)?;
let invoke = loader.get_function_with_unmanaged_callers_only::<Invoke>(
    pdcstr!("Nara.BehaviorTrial.Bridge, Nara.BehaviorTrial"),
    pdcstr!("Invoke"),
)?;

// input_len and output_capacity are checked i32 conversions.
// Both Rust allocations remain valid and distinct throughout this call.
let written = unsafe {
    invoke(input.as_ptr(), input_len, output.as_mut_ptr(), output_capacity)
};
```

Convert filesystem paths with
`netcorehost::pdcstring::PdCString::from_os_str(path.as_os_str())?`.
Keep the hostfxr, context and loader owners alive through the fixture calls.
This is a source-verified API sketch, **not a compiled Rust probe**.
The published [delegate-loader source](https://docs.rs/crate/netcorehost/0.22.0/source/src/hostfxr/delegate_loader.rs)
implements this lookup using `UNMANAGED_CALLERS_ONLY_METHOD`; its
[upstream example](https://docs.rs/crate/netcorehost/0.22.0/source/examples/call-managed-function/main.rs)
uses the same API sequence.

**Calling convention:** the wrapper returns `ManagedFunction<<F as WithAbi<System>>::F>`,
thus converts the requested function type to `extern "system"` while preserving its
argument types and safety. The parent removed its initial explicit Cdecl annotation
to match this platform-default entry. Windows x86 would distinguish Cdecl and
System/Stdcall; compatibility must not be inferred from a passing Windows x64 call.
See the source above and Microsoft's
[calling-convention rules](https://learn.microsoft.com/en-us/dotnet/standard/native-interop/calling-conventions).
No cross-architecture probe was run.

The managed entry must be static, non-generic, with blittable arguments/results;
the attribute does not make buffers safe. The Rust wrapper cannot verify that its
chosen signature matches the actual managed method. Keep pointers call-scoped,
catch managed exceptions inside the bridge, and validate the returned byte count
against the allocated output capacity before parsing. Use the parent's explicit
negative-error convention and parse only the accepted output prefix.
[UnmanagedCallersOnly requirements](https://learn.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.unmanagedcallersonlyattribute?view=net-10.0).

### Installed hostfxr discovery

Reuse the parent's recorded .NET installation root. On Windows the direct load
target is `<dotnet-root>/host/fxr/<actual-host-version>/hostfxr.dll`;
Linux/macOS use `libhostfxr.so`/`libhostfxr.dylib`. Record the selected absolute path
and architecture; do not infer the hostfxr directory version from the SDK number
`10.0.400`, or assume that runtime inventory alone proved that file's location.
The generated `.runtimeconfig.json` then selects a compatible shared runtime;
loading the hostfxr library and selecting the CLR are separate steps.
[Hosting tutorial](https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting).

If automatic discovery is later needed, this release exposes
`nethost::load_hostfxr()` and `nethost::load_hostfxr_with_dotnet_root(...)`
behind the `nethost` feature. The native `get_hostfxr_path` API accepts an
explicit `dotnet_root`; this is not a reason to rebuild an SDK locator for R3b.
[Pinned Rust discovery source](https://docs.rs/crate/netcorehost/0.22.0/source/src/nethost.rs),
[official nethost header, .NET v10.0.0](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/nethost/nethost.h).

## What The Snapshot Slice Establishes

The recommended private flow is:

1. Rust captures admitted Behavior records and the native observations needed by
   this invocation, with exact Schema/publication and entity identities.
2. One managed entry dispatches callbacks by runtime Schema identity, consuming
   copied DTO data and returning bounded staged Name commands.
3. Rust decodes and validates the complete response, rejects unknown/stale targets
   or publications and invalid commands, then applies the accepted command data
   through the actual native owner.

This is a proposed experiment boundary, not a new public Nara Host contract.
It fits the [existing representation comparison](2026-10-27-behavior-representation-comparison.md):
runtime-defined Behavior records remain independent of the native Name binding.
A managed dictionary from Schema keys to callbacks over one common JSON/value
shape supports additional runtime-defined Schemas without one Rust type per Behavior.
Per-Schema managed DTO classes are optional, not required by hostfxr.
Merely hard-coding two cases would establish those cases, not general callback registration.

The DTO is an owned observation at a particular invocation basis, **not live World
authority**. Returned commands are proposals until native validation. Copied state
must not become a second authoritative persistent record. State the exact snapshot
read and command-application order, including whether one callback can observe an
earlier callback's commands. Align those semantics with the parent's Lua typed
snapshots. JSON parse/serialize and allocation costs belong in any eventual timing;
this slice cannot estimate fine-grained domain-call overhead.

A future host-scoped accessor could provide synchronous native reads and staged
writes using a validated invocation token and a small function table. That requires
separate aliasing, reentrancy, thread-affinity, stale-token and retirement evidence.
Neither direct World pointers nor such callbacks are needed to establish this slice.
Without them, arbitrary managed code still retains the process's ordinary privileges;
DTO access discipline is not a .NET sandbox.
[AssemblyLoadContext security scope](https://learn.microsoft.com/en-us/dotnet/fundamentals/runtime-libraries/system-runtime-loader-assemblyloadcontext).

## Lifetime And R3c Boundary

The pinned .NET native-hosting design states that only one runtime can be loaded
per process; native hosting is limited to framework-dependent deployments;
components loaded by `load_assembly_and_get_function_pointer` remain for process
lifetime, and those returned pointers are valid for that lifetime. The
`UnmanagedCallersOnly` sentinel avoids creating a managed delegate wrapper; it does
not make the component collectible.
[Native-hosting restrictions, v10.0.0](https://github.com/dotnet/runtime/blob/v10.0.0/docs/design/features/native-hosting.md).

Consequently, directly loading the parent's authored library is appropriate for
R3b, but not evidence for unloading/replacing it. `hostfxr_close` closes a host
context, not the CLR. Initialization alone does not load CoreCLR; obtaining a
runtime delegate starts it.
[Official hostfxr API contract, v10.0.0](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/hostfxr.h).

For R3c, retain a permanent managed bridge and load authored assemblies into a
separate collectible `AssemblyLoadContext`, sharing the bridge's contract assembly.
Keep native entry pointers in the permanent bridge; it owns and revokes managed
callback registrations. `Unload()` starts **cooperative** unloading. Completion
requires returning from executing methods and releasing external strong roots,
including instances, delegates, reflection objects, strong/pinned GCHandles, host
registries/event subscriptions and running thread stacks. A cycle wholly inside
the collectible context is not itself an external root.
[Microsoft unloadability guidance](https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability).

An ALC weak reference becoming dead after bounded GC/finalizer observation is
assembly-context evidence, not proof of CLR unload, complete native-resource
retirement, or RSS returning to baseline. A still-live weak reference means
unloading is incomplete; stop advertising reload success. R3b intentionally leaves
all of this untested. A hanging callback cannot be safely killed by checking an
elapsed deadline after its synchronous return.

## Pinned Godot Precedent

Read-only local source is `repo-ref/godot`, verified at
**`81ab961f91066a63bcbf3ec340962bb1a1ada558`**. The directory name `modules/mono`
does not imply that its desktop Editor route bypasses hostfxr.

| Source path at that commit | Observed mechanism and limit |
| --- | --- |
| [`modules/mono/mono_gd/gd_mono.cpp`, lines 365-383 and 432-459](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/mono_gd/gd_mono.cpp#L365) | Initializes from `GodotPlugins.runtimeconfig.json`, obtains the load-assembly delegate, closes the host context, then resolves `GodotPlugins.Main.InitializeFromEngine` using `UNMANAGEDCALLERSONLY_METHOD`. This corroborates an in-process bridge. |
| [`modules/mono/glue/GodotSharp/GodotPlugins/Main.cs`, lines 88-151](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs#L88) | Exchanges native/managed callback tables. Project assemblies are collectible only with the Editor hint; tools assemblies use a non-collectible context. |
| [`modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs`, lines 17-79](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotPlugins/PluginLoadContext.cs#L17) | Shares selected API assemblies with the main context, resolves dependencies, and loads assemblies/PDBs from streams to avoid locking the managed files. Native dependencies have a separate loader. |
| [`modules/mono/glue/GodotSharp/GodotPlugins/Main.cs`, lines 15-72 and 234-284](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotPlugins/Main.cs#L234) | Uses non-inlined helpers, a weak reference, forced collections/finalizer waits, a 200 ms warning and a 1,000 ms failed-unload result. These are Godot policies, not guaranteed unload deadlines. |
| [`modules/mono/csharp_script.cpp`, lines 625-639](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/csharp_script.cpp#L625); [`mono_gd/gd_mono.cpp`, lines 789-823](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/mono_gd/gd_mono.cpp#L789) | C# assembly reload is a hard reload; player collectible assemblies are disabled. Unload failure is reported, and repeated failures can require an Editor restart. This does not demonstrate atomic replacement or CLR unloading. |

## Acceptance And Remaining Failures

**The parent's current approach is an honest initial R3b partial comparison** once
the actual Rust-to-managed call and native Name effect pass. It must retain the
existing data/Schema oracle rather than interpreting successful JSON exchange as
proof of production Runtime integration. The managed build has passed according
to the parent; the native loading, ABI call and returned Name effect remain
unverified in this research lane.

Keep the initial check narrow: actual hostfxr path, generated runtimeconfig,
existing four-argument entry, same two-Behavior/native-Name workflow, and unchanged
native state on rejected responses. Record failures for missing/incompatible runtime
or architecture, missing type/method, managed exception, malformed/oversized output,
unknown Schema and stale/foreign native identity. Define large integer/reference
encoding and numeric conversion consistently across JSON and Lua. Do not repeat
the parent's toolchain inventory or expand this into a universal transport layer.

Deferred failure cases include unbounded managed execution, fatal native/process
faults, GC pauses/allocation pressure, retained callbacks/objects/threads, dependent
assembly/native-library resolution, Windows rebuild locks, repeated-reload memory,
state migration, and native Linux/macOS qualification. Fresh-process rebuild and
rerun is the initial edit-iteration boundary; it measures neither compatible body
reload nor collectible unload.

Starting with NativeAOT would change the question: it compiles for a particular
target, has no dynamic assembly loading or runtime code generation, and cannot
establish the JIT/collectible authored-assembly route.
[Official NativeAOT limitations](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/).
A subprocess would add an address-space boundary, IPC and process termination
semantics; it can be a separate isolation control, but does not answer this
in-process workflow comparison. Neither alternative is required to unblock R3b.
