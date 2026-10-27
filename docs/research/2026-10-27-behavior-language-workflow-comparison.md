# First In-Process Behavior Language Workflow

**Date:** 2026-10-27. **Status:** Executed private C#/Lua slice; full language comparison and
production selection remain open.

## What Was Executed

The [plan](../plans/2026-10-27-behavior-language-workflow-plan.md) follows the
[representation comparison](2026-10-27-behavior-representation-comparison.md).
The independently compiled [fixture](../../tests/fixtures/behavior-languages/README.md) reuses
its data-only entity envelope, admitted Mover/Sensor Schemas and existing hidden authoring
compiler. No production runtime, registry or Editor code changes.

C# executes the actual built assembly inside the Rust test process through hostfxr.
Lua executes through the vendored mlua Lua 5.4 backend; the test asserts its `_VERSION`.
Neither path uses one subprocess per gameplay call.

Both receive the same finite invocation DTO and return proposed native Name commands. Lua
uses serde-to-table conversion; the C# bridge copies JSON into managed DTOs and writes bounded
JSON output. This compares feasibility, not equally optimized bindings. The Host retains exact
World/entity/Definition tokens separately from the wire strings, checks every returned command
before the first native write and applies accepted commands under exclusive World access.
No World/component borrow or raw host callback is given to authored code.

Each language runs the same assertions against direct Scene data and a real Prefab projection:

| Case | Required observed behavior |
| --- | --- |
| Source edit, Undo and Redo | The accepted amount 7 reaches the callback and actual native Name becomes `Moved 7` |
| Recursive Sensor reference | Existing declared-reference traversal resolves the local target; language code changes that native Name |
| Runtime data edit | Replacing Mover's amount with 9 leaves Sensor present; the next invocation produces `Moved 9` |
| Callback exception | Lua error / managed exception produces no accepted command batch; Name remains unchanged |
| Invalid final command | A valid changing first command followed by a foreign target rejects the entire batch before any native write |
| Another Runtime with identical source IDs | Original invocation tokens reject; the other Runtime's Name remains unchanged |
| Export | Actual native export plus owner records retains amount 9 and Sensor; the exported reference must point to the friend independently identified by its native Name effect |
| Whole-instance retirement | Scene retirement succeeds and envelope records are physically gone |

The invocation is synchronous and commands are applied before capturing the next invocation.
There is no retained asynchronous result path, snapshot revision/read-set protocol or arbitrary
native rollback. Lua compilation refusal is also exercised; this slice does not test a failing
C# rebuild against a retained running assembly.

## Actual Verification

Windows 11 x64, Rust 1.95.0, installed .NET SDK **10.0.400**, shared runtime **10.0.11**,
selected **hostfxr 10.0.11**. Pins: **mlua 0.12.2** with `lua54/vendored/serde`,
**netcorehost 0.22.0** with defaults disabled and `net10_0`; exact resolution is in the fixture
lockfile. nethost binary download is disabled.
The selected Lua backend and serde features are defined by the
[pinned mlua manifest](https://github.com/mlua-rs/mlua/blob/v0.12.2/Cargo.toml).

```text
python tests/fixtures/behavior-languages/run.py
```

Final run `a67cd2f4-08c9-4e45-b43c-cb9dbbbe5e68`: **2 passed, 0 skipped**. Each test executes both
source cases, including the independently identified exported reference target. The managed
incremental build reports 0.79 seconds and zero warnings/errors;
Rust test build reports 4.59 seconds and test execution 0.154 seconds. These include different
stages and cache states; they are not a C# versus Lua timing comparison or an edit-latency metric.
The earlier Lua-only run is superseded by this complete two-language run.

The runner serializes managed build and Cargo, fixes `CARGO_BUILD_JOBS=1`, reuses the root
`target`, disables managed build servers and supplies explicit artifact/host paths. The workspace
is excluded from ordinary root builds; a missing required SDK fails this dedicated comparison
rather than silently skipping C#.

## Design Consequences And Open Work

This removes a feasibility uncertainty: the fixed two-Behavior task can execute in-process in
either language without weakening native binding cardinality or borrowing the World across
the language call. It does not select C#, Lua, a public SDK or a universal snapshot interface.
The fixed task's extraction fields and event cases are compiled into the private harness;
arbitrary Schema registration and evolution without native harness edits are not proved.

The [hosting study](2026-10-27-dotnet-behavior-hosting-feasibility.md) traces exact hostfxr and
pinned Godot mechanisms. Directly loaded managed components and their entry pointers live for
the process lifetime. Keeping/dropping hostfxr context owners is not CLR or assembly unload.
Lua's Program owns its VM/function handles; this experiment does not retain escaping callbacks,
exercise asynchronous invocations or prove a production retirement owner for either language.

The next comparison must address actual author iteration, Schema-driven binding and concrete
domain access costs, including whether copied DTOs are appropriate for that domain. Keep native
Rust gameplay direct; do not route every gameplay call through Editor IPC or promote this JSON
bridge to a public interface. A candidate production owner still needs complete attach/remove,
asset use, data/VM state authority, safe-point instance cleanup, Schema migration, target evidence
and independent public consumption. Body replacement, collectible managed contexts, cross-owner
compatibility and physical code lifetime retain their separate R3c qualification.

[ADR 0120](../architecture/adr/0120-schema-publications-and-execution-representations.md)
remains Proposed. The complete foundation Goal remains active.
