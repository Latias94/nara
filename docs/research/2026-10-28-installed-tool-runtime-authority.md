# Installed Finite-Tool Runtime Authority

**Date:** 2026-10-28. **Status:** Source research and successor design; no installed execution
qualification. **Nara basis:** `e8f3a59`, local `main`, before the shared Windows image primitive.

## Concrete Consumer And Existing Authority

The actual EnemyStats command is a framework-dependent `net10.0` application with
`UseAppHost=false`. Its captured release contains its managed entry, deps/runtimeconfig, catalog
and a content sample. The existing development consumer explicitly selects an absolute `dotnet`
path and tool directory. This is a useful trusted development command, not proof that an
installation receipt determines executable code. The existing Session already probes candidates
while retaining the predecessor, compares the complete catalog/facet, fences proposals and owns
physical child/pipe return through Tasks. Preserve that one owner and executor.

[ADR 0047](../architecture/adr/0047-editor-workspace-and-scene-document-state.md) requires
approval of the same captured candidate, original source observations and exact source-owned
preparation. [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
separates publication, approval and routing. Neither establishes immutable transitive external
runtime bytes. [ADR 0116](../architecture/adr/0116-standalone-supervised-editor-play.md) has its
own Play image authority and explicitly bounds its closure claim. That placement cannot silently
become a generic tool process supervisor.

## Primary Source Findings

| Observation | Evidence and implication |
| --- | --- |
| `dotnet.exe` resolves hostfxr from its selected root and selects the latest version below `host/fxr`. | [.NET runtime v10.0.0 resolver](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/fxr_resolver.cpp): `get_latest_fxr` and `fxr_resolver::try_get_path`. An exact framework option does not independently fix hostfxr. |
| `--fx-version` overrides the first framework reference; `--roll-forward Disable` refuses other versions. | [Official dotnet command reference](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet#options-for-running-an-application). An initial bounded policy can admit one explicit `Microsoft.NETCore.App` reference and exact requested version; patch maintenance then belongs to Host policy. |
| Explicit deps/runtimeconfig paths select those configuration inputs but do not prove their dependencies. | [Official exec options](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet#options-for-running-an-application-with-the-exec-command), [default probing](https://learn.microsoft.com/en-us/dotnet/core/dependency-loading/default-probing). Reject unsupported extra frameworks/probing declarations rather than implementing a resolver. |
| Environment affects startup hooks, profiling, extra dependencies and stores. | [Official environment reference](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables), [v10.0.0 shared-store implementation](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/hostpolicy/shared_store.cpp). Construct an allowlisted environment rather than inheriting a developer's runtime configuration. |
| Windows servicing is an additional machine input. | [v10.0.0 Windows PAL](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/hostmisc/pal.windows.cpp): `get_default_servicing_directory`; [deps resolver](https://github.com/dotnet/runtime/blob/v10.0.0/src/native/corehost/hostpolicy/deps_resolver.cpp): servicing probes. Clearing environment alone does not prove a private runtime closure. |
| Native OS dependency selection remains distinct. | [Windows DLL search rules](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order). An exact launcher image is not an attestation of every library loaded by that process. |

The parent checked the resolver, Windows servicing implementation and command reference online;
the independent runtime researcher also inspected the linked probing/environment sources.
These observations are pinned to v10.0.0 source. The existing fixture records a newer installed
runtime patch; qualification of that actual prerequisite must record its own observed version
and platform. This study is not a new run of the fixture or an official C# SDK selection.

## Selected Trust Partition

Use a Host-approved machine-managed runtime prerequisite for the first framework-dependent tool.
Its maintenance authority belongs to the trusted platform, alongside the explicitly stated OS
baseline. Approval names the launcher/root, requested framework/version and launch policy; finite
inventory/content observations provide admission and drift diagnostics. They do not freeze the
runtime namespace or attest every dynamically selected machine file. A private single-version
runtime deployment may reduce selection ambiguity without changing that distinction.

This decision does not move package-controlled files into the machine trust baseline. Approved
tool bytes must remain the captured candidate, and unknown installation siblings cannot acquire
implicit dependency authority. Managed entry/configuration/member consumption needs its own
concrete proof before installed execution is qualified. Restrictive Windows file leases can deny
ordinary later write/delete opens; directory sharing does not prevent adding files, and existing
writable mappings are a separate limitation. A repeated digest/inventory observation cannot be
called a frozen namespace. Unix descriptor duplication is not an equivalent content guard.

An adversarial contract review found no Accepted requirement for a universal immutable .NET
closure. It retained the harder artifact/input boundary and rejected broadening the platform
trust baseline to installation siblings. The first supported execution policy must state its
actual writer/namespace premise and reject platforms where the required artifact guarantee
cannot be established. No development-path fallback may silently satisfy installed approval.

## Captured Managed Images Instead Of An Installation Namespace

The selected artifact handoff for the next experiment is a concrete Host-owned managed bootstrap,
run in a fresh finite child under the approved runtime baseline. The Session sends a bounded
binary envelope of the already captured managed images, followed by the unchanged finite wire
request on the same stdin. The bootstrap consumes exact unbuffered lengths and loads those images
from memory in a private `AssemblyLoadContext`. It then invokes the supported tool entry point;
the tool's raw standard-input read receives the remaining ordinary request through EOF. Pending
work must retain the original captured object, never refresh it from installed paths.

This is a design inference to be qualified with the actual EnemyStats child. It avoids using
the mutable installed directory as default assembly probing authority. The ordinary tool's
explicit ambient I/O/native calls are still trusted; the adapter is not a sandbox. Runtime and
bootstrap files belong to the explicitly approved Host baseline. Package runtimeconfig/deps are
checked for the admitted subset but cannot configure the trusted bootstrap's own runtime.

The first entry contract is synchronous static `int Main(string[])` with no arguments, including
the actual private EnemyStats entry point. Async/STA entry semantics, original process argv or
`Assembly.Location` identity, package native assets and mixed-mode images are outside that subset.
Bootstrap diagnostics use bounded stderr and never pollute protocol stdout. Invocation failures
become nonzero process outcomes through the existing finite process owner.

Dependency admission must inspect metadata before loading: reject duplicate package simple names,
framework-name collisions and unsupported reference identities. Framework dependencies resolve
only to the explicit framework set in the trusted baseline, not a `System.*` prefix rule. Package
dependencies resolve only to captured images. Unknown identities must stop loading: returning
`null` permits default-context fallback, and `FileNotFoundException` also continues resolution;
another exception such as `FileLoadException` terminates it. The per-context cache precedes the
resolver and accepts an already loaded equal-or-higher version, so checking only resolver callbacks
cannot enforce an exact package graph. These constraints follow the official
[ALC model](https://learn.microsoft.com/en-us/dotnet/core/dependency-loading/understanding-assemblyloadcontext),
[managed loading algorithm](https://learn.microsoft.com/en-us/dotnet/core/dependency-loading/loading-managed)
and [Load API exception contract](https://learn.microsoft.com/en-us/dotnet/api/system.runtime.loader.assemblyloadcontext.load?view=net-10.0),
which the parent checked online. They require real negative loading cases, not only a successful
describe response. The earlier behavior fixture's permissive resolver is not this production owner.

## Alternatives And Costs

1. **Freeze a whole private runtime tree and its discovery namespace.** This would require more
   than file hashes and read handles, including actual namespace control and a servicing/OS
   boundary. No current product requirement justifies an ACL manager or filesystem security
   kernel merely to invoke this explicitly trusted tool. It remains a distinct stronger support
   claim if a concrete deployment requires it.
2. **Trust an explicitly approved machine runtime.** Selected. Keep configuration, observations,
   machine maintenance and artifact byte authority separate. This supports the named consumer
   without pretending to supply reproducible or sandboxed native execution.
3. **Add a native hostfxr bootstrap or self-contained deployment.** Either can reduce some
   framework selection inputs, but introduces a new launcher/publication/support surface and
   still has platform dependencies. Neither is automatically a complete immutable closure.
   Compare it only when the actual tool workflow needs the stronger deployment property.

## Dependency-Ordered Implementation

First move Play's existing Windows exact-image/path evidence to a narrow root Host primitive and
make Play consume it. Keep process acquisition, Job containment, primary-thread resume, bootstrap,
generation publication and cleanup with their existing owners. The primitive has no spawn method,
runtime resolver, package selection or process lifecycle. Test actual matching and same-byte
different-object image checks, plus the existing suspended exact-generation child trajectory.

Then qualify the captured-image bootstrap over the real installation and explicitly selected
runtime policy, using the existing finite child path. Candidate preparation remains code-free;
only explicit Host approval may initiate the describe probe. Bind every actual invocation and its
retirement to that custody, including cancellation before a late spawn and held pipe return.
Preserve V1 on candidate failure; after a successful route switch, retain V1's physical debt.

Durable selection follows readiness and has its own owner-local store. It must not reuse a Scene
Save lane or the artifact ready receipt. Record desired enablement without recovering approval
on Open. A truthful implementation can observe disk selection V2 while routing is still V1 if
post-publication reconciliation refuses the switch. Filesystem detect-only replacement is not a
cross-disk/in-memory atomic commit or strong CAS. Installed dependency closure, adoption, removal
and the complete native UI trajectory remain in the owning lifecycle plan.
