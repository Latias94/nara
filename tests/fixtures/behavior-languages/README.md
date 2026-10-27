# Experimental Behavior Languages With Public Scene Storage

This independently compiled fixture exercises one finite in-process C#/Lua workflow over public
Scene storage and window-free source authoring. It admits an owner-local catalog, selects real
native/data bindings and uses the production compiler, preparation, guarded data access, live
export and Scene retirement. Its language bridges and execution-publication owner remain
experimental; it does not establish a language SDK, sandbox or production reload implementation.

From the repository root:

```text
python tests/fixtures/behavior-languages/run.py
```

Requires the .NET 10 SDK, Python, Rust 1.95.0 and cargo-nextest. The runner builds the managed
library, selects hostfxr from that dotnet installation, supplies its paths to the tests and runs
Cargo serially. Both languages must pass; missing tools fail the command. All build products
use the root `target`. It does not use the system Lua executable or download nethost binaries.

The exact Rust dependency pins are in `Cargo.toml` and `Cargo.lock`. Lua uses the vendored
Lua 5.4 backend of mlua 0.12.2. C# uses netcorehost 0.22.0 and the actual installed .NET 10
framework-dependent runtime.

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

The concrete `EnemyStats` tool adds a separate source-authoring domain. Its C# module describes
the complete existing-format Schema and creation template; Rust admits that metadata and reads
the same saved data through public Scene bindings. Copied scalar inputs produce named property
write proposals. The Host retains the target/command, original query basis, exact code publication
and newest request. It bounds accepted output, prepares through the existing Session, exposes
that candidate's patch/inverse for review and consumes the same candidate on acceptance. Source
IDs and document authority never come from the managed result. No drawing toolkit is involved.

The actual JSON/RON workflow covers Attach, edit, additive Spawn, one Undo/Redo each, exact commit
and created-ID receipts, correlated Save and physically closed fresh-Host Reopen. Late invalid
writes, including a Spawn with a final invalid field, leave source/history/navigation/disk intact.
Older requests, changed document bases, replaced code publications and disabled tools reject at
preparation and final acceptance. A wrong-Schema candidate preserves the working predecessor;
a real second C# body changes the proposed health increment. Rejected, replaced and disabled
collectible contexts are explicitly retired and observed collected. This is synchronous retained
result testing, not pending-task or installed-package qualification. Tool and behavior interfaces
currently share the experimental authored assembly/loader; no standalone tool package is shipped.
See the [concrete tool plan](../../../docs/plans/2026-10-27-enemy-stats-semantic-tool-plan.md).

The old grouped-materialization/export prototype is superseded. The original representation
comparison remains fixed-commit research; the remaining raw storage probes measure logical query
inspections and orphan retention only. They are not a second Scene Runtime or a throughput claim.

See the [lifetime plan](../../../docs/plans/2026-10-27-behavior-code-lifetime-plan.md),
[Schema successor plan](../../../docs/plans/2026-10-27-schema-driven-behavior-plan.md),
[initial plan](../../../docs/plans/2026-10-27-behavior-language-workflow-plan.md) and
[hosting study](../../../docs/research/2026-10-27-dotnet-behavior-hosting-feasibility.md).
