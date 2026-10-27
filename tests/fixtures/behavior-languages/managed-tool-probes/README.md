# Managed bootstrap probes

These ordinary .NET 10 projects have no test framework or NuGet package references.
They live outside `tools/` so its SDK compile glob cannot include their entry points.
`Entry/Entry.csproj` references the sibling `Dependency/Dependency.csproj`.

The parent behavior-language runner builds Entry with these global MSBuild properties:

| Variant | DependencyRevision | ProbeMode | Environment variable |
| --- | --- | --- | --- |
| `v1` | `1` (default) | `sync` (default) | `NARA_MANAGED_PROBE_V1_DIR` |
| `v2` | `2` | `sync` | `NARA_MANAGED_PROBE_V2_DIR` |
| `async` | `1` | `async` | `NARA_MANAGED_PROBE_ASYNC_DIR` |
| `sta` | `1` | `sta` | `NARA_MANAGED_PROBE_STA_DIR` |

Each build uses Debug configuration and an absolute `--artifacts-path` of
`<repository>/target/behavior-language-trial/probes/<variant>`. Its environment
variable points to `<artifacts-path>/bin/Entry/debug`, containing both
`Nara.SourceToolProbe.dll` and `Nara.SourceToolProbeDependency.dll`.
The artifact directory uses the project basename `Entry`, not its assembly name.
The dependency identity is `Nara.SourceToolProbeDependency, Version=1.0.0.0` or
`Version=2.0.0.0`. Its nonconstant `Value` property returns 1 or 2, preserving a
real assembly reference in the entry IL. Sync execution prints `managed-probe:1`
or `managed-probe:2` and returns zero. Async and STA variants print the same marker
only if their unsupported entry point executes.

`tests/support/managed_bootstrap.rs` is a standalone support module gated to
Windows x86_64 MSVC. The parent adds it to `tests/languages.rs`. It also requires:

- `NARA_SOURCE_TOOL_DOTNET`: absolute path to the selected `dotnet.exe`.
- `NARA_SOURCE_TOOL_FRAMEWORK`: exact installed `Microsoft.NETCore.App` version.
- `NARA_SOURCE_TOOL_BOOTSTRAP_DIR`: absolute directory containing
  `Nara.SourceToolHost.dll` and its built runtime configuration and deps files.
- `NARA_SOURCE_TOOL_V1_DIR`: absolute directory containing the real V1
  `EnemyStatsTool.dll`, built by the existing runner.

The tests launch the bootstrap directly from its directory using captured DLL
bytes. They cover wire framing, exact dependency identities, duplicate images,
unsupported entry points, and the unchanged JSON tail consumed by EnemyStats.
They do not exercise Session process execution. Each fixture has tiny output;
the sole stdin writer closes before collecting output, with assertions limiting
stdout to 64 KiB and stderr to 16 KiB. Rejections require empty stdout, failure
status, and exactly `Nara managed tool bootstrap failed.\n` on stderr.
