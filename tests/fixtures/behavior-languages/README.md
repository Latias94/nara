# Private Behavior Language Trial

This independently compiled fixture compares one finite in-process C#/Lua workflow. It reuses
the private representation fixture's data-only entity envelope and hidden authoring compiler.
It is not a public Runtime consumer, language SDK, sandbox or reload implementation.

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
Managed code remains loaded for the test process lifetime; Drop does not prove CLR unload.

See the [plan](../../../docs/plans/2026-10-27-behavior-language-workflow-plan.md) and
[hosting study](../../../docs/research/2026-10-27-dotnet-behavior-hosting-feasibility.md).
