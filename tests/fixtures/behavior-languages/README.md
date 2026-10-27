# Experimental Behavior Languages And Finite Source Tools

This independently compiled fixture exercises in-process C#/Lua gameplay over public Scene
storage and a separate standalone C# source tool through the production Project Session. It
admits owner-local catalogs and uses public authoring, preparation, data access, persistence and
Scene retirement. The gameplay bridges and their execution-publication owner remain
experimental. Neither workflow establishes a language SDK, sandbox or production gameplay reload.

From the repository root:

```text
python tests/fixtures/behavior-languages/run.py
```

Requires the .NET 10 SDK, Python, Rust 1.95.0 and cargo-nextest. The runner builds the managed
gameplay bridge/modules and the independent EnemyStats executable, supplies their paths to the
tests and runs Cargo serially. It selects hostfxr for in-process gameplay and exports the resolved
dotnet executable as `NARA_SOURCE_TOOL_DOTNET` for direct child launches. Tool output directories
are supplied through `NARA_SOURCE_TOOL_V1_DIR`, `NARA_SOURCE_TOOL_V2_DIR` and
`NARA_SOURCE_TOOL_INVALID_DIR`. All language and tool consumers must pass; missing tools fail
the command. All build products use the root `target`. The runner does not use the system Lua
executable or download nethost binaries.

The exact Rust dependency pins are in `Cargo.toml` and `Cargo.lock`. Lua uses the vendored
Lua 5.4 backend of mlua 0.12.2. In-process C# gameplay uses netcorehost 0.22.0 and the installed
.NET 10 framework-dependent runtime; the standalone tool uses that runtime through dotnet.

Each callback receives an owned data observation and returns proposed Name commands. The
native Host retains exact entity/publication tokens, validates the complete batch and then
applies it. Calls are synchronous and serial. No borrowed World data crosses the language call.
The transport does not support asynchronous results or validate a reusable snapshot read-set.
The immutable managed bridge remains loaded for the test process lifetime. Authored modules use
separate collectible contexts; explicit tests distinguish requested unload, retained roots and
observed collection. Drop alone does not prove collection or CLR unload.

The consumer loads `data/behaviors.catalog.json` through the existing catalog decoder and
`data/game.scene.json` through bounded source decoding and the shared mixed compiler. Source
Attach, stable-property editing, Undo/Redo, Save and fresh-Host Reopen run through a real
`EditorProjectSession` in both JSON and RON projects with canonical image metadata. No private
authoring receipt, preview projection, post-commit carrier installation or cached export roster
is used. Callback registration lives in
`authored/Module.cs` and `lua/schema_behaviors.lua`. Stable root fields are resolved
generically through the existing property resolver; declared references use the existing walker.
The Host does not switch over authored Schema IDs to extract different fields.

To add a Behavior for this trial, declare its Schema and source record, then register its
language callback. A complete authoring creation template is required for Attach. Native Name
is the behavior callback command domain; a new native runtime domain requires its own binding.
Callback IDs and versions are checked against the complete catalog before language activation.
Separate actual managed candidates omit Sensor or report its wrong version; rejection preserves
the active predecessor and observes candidate context retirement. Plain Scene data admission does
not grant callback readiness. A readmitted equivalent Schema selection cannot authorize language
observation of another Runtime. Repeated Prefabs execute only their own referenced targets, and
flattened live exports are replayed through the same public storage path before another callback.
The fixed generation-one comparison is not a catalog migration or production owner protocol.

The lifetime tests edit Lua source and rebuild the conditional C# body variant to the same live
DLL path while its predecessor remains loaded. They retain the native World, reject old execution
results and hold an actual callback/instance lease with zero executing callbacks. Lua leases own
the VM explicitly; a bare mlua Function does not. A still-live managed weak context remains pending,
never success by timeout. The finite probes do not cover async work, escaping arbitrary language
objects, dependent owners, Stop-first or Schema migration.

The concrete `EnemyStats` tool is an independent .NET 10 console project in `tools/`. It has no
gameplay or hostfxr bridge assembly dependency. One `enemy-stats.catalog.json` declaration is
embedded in the executable and copied beside its DLL, runtime configuration and dependency
manifest. Rust admits that metadata with the existing bounded catalog decoder. A trusted Host
explicitly supplies the absolute dotnet command, DLL argument, working directory, timeout,
selected owner and component to the production `EditorProjectSession` source-tool API.
Ordinary project Open admits metadata without executing the tool.

Each finite invocation receives copied component fields and parameters through bounded JSON
stdin and returns a catalog description or property writes through stdout. The production
Session owns activation, newest request, original query basis, retained review and acceptance.
The consumer only borrows the review's title, patch, inverse and fixed created ID, then accepts
the same request. Spawn's name and placement remain Host intent. Existing source owners retain
preparation, generated IDs, history and persistence; the fixture has no second tool publication,
transaction or retirement authority.

The view reports probe failures in `attempt_error` and proposal/request failures in
`request_error`. Starting a probe clears only its attempt error; starting a request clears only
its request error. A rejected probe remains observable while the active predecessor handles
requests, and both failures can be retained together. Successful replacement clears the old
request, request error and review. Disable clears request state while retaining the last attempt
observation.

The JSON/RON cases cover Attach, edit, additive Spawn, preview without mutation, exact inverse
and created-ID receipts, single-transaction Undo/Redo, Save, physical Close and fresh-Host Reopen.
Public Rust Scene bindings materialize and read the saved owner-tagged values. Negative cases
cover late invalid writes including Spawn, excessive writes, checked arithmetic failure, failed
reads superseding retained requests, changed document bases, exact Schema admission, owner and
declaration mismatches, disabled in-flight work and disabled review. Failed candidate probes
exercise incompatible catalogs, malformed JSON, stdout/stderr overflow, nonzero exit and a
300 ms hold against a 60 ms command timeout while preserving the active predecessor and review.
A real V2 child doubles the bonus and invalidates predecessor review. Polls correlate exact
attempt/request IDs and use a 30-second test deadline; disable and Close drain actual child/pipe
custody through the production Tasks owner. No drawing toolkit or collectible tool context is
involved. See the [finite source-tool guide](../../../docs/guides/finite-source-tools.md) and
[owner plan](../../../docs/plans/2026-10-28-finite-source-tool-owner-plan.md).

On Windows the runner also selects the ignored window-free Application test against the same
actual C# executable. It exercises Application review, exact successful receipts and source/history
delegation. This separate test requires the runner's artifact environment; ordinary Editor tests
do not silently substitute another child or count it as native UI qualification.

The old grouped-materialization/export prototype is superseded. The original representation
comparison remains fixed-commit research; the remaining raw storage probes measure logical query
inspections and orphan retention only. They are not a second Scene Runtime or a throughput claim.

See the [lifetime plan](../../../docs/plans/2026-10-27-behavior-code-lifetime-plan.md),
[Schema successor plan](../../../docs/plans/2026-10-27-schema-driven-behavior-plan.md),
[initial plan](../../../docs/plans/2026-10-27-behavior-language-workflow-plan.md) and
[hosting study](../../../docs/research/2026-10-27-dotnet-behavior-hosting-feasibility.md).
