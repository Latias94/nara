---
type: "Verification Evidence"
title: "First actual in-process CSharp and Lua Behavior snapshot workflow"
description: "Qualifies the bounded two-language Scene and Prefab command workflow without selecting a production language or claiming VM reload."
timestamp: 2026-10-27T08:05:53Z
record_id: "81c67c84e2fa47b38b7848f642dae81c"
producer_id: "codex-root"
run_id: "behavior-language-first-slice-2026-10-27"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "experiment/behavior-language-workflows"
---

# Verification

The independently compiled private fixture is based on `535bbd6`, following the envelope/Rows
comparison. This is the first R3b snapshot workflow, not a complete language comparison or a
public Runtime representation. The full foundation Goal remains active and ADR 0120 Proposed.
No production Rust or Editor code changed; the root manifest only excludes the new fixture
workspace. Untracked `vendor/` was neither read nor changed, and nothing was published remotely.

# Result

Both actual in-process adapters run the same edited source and Prefab-projected Mover/Sensor data
with native Name. Source Undo/Redo, next-invocation field visibility, recursive local targets,
managed/Lua callback refusal, an invalid last command, foreign Runtime tokens, native export and
whole-instance retirement pass. The Host retains exact World/entity/Definition authority separately
from the DTO and validates the complete command batch before writes.

The fixture intentionally uses compiled extraction fields/event cases. It does not establish
generic Schema binding, complete author-driven attach/remove, a migration service, actual asset
loading, live domain views, optimal marshalling, a sandbox or a selected language SDK.
Calls are synchronous/serial; reusable/asynchronous snapshot read-set admission is not supported.
Directly loaded managed code lives for the process lifetime; owner Drop is not CLR unload.

# Evidence

Windows 11 x64, Rust 1.95.0, .NET SDK 10.0.400, shared runtime and hostfxr 10.0.11.
Pinned mlua 0.12.2 uses vendored Lua 5.4; the runtime version assertion passes.
Pinned netcorehost 0.22.0 disables default nethost download and loads the actual hostfxr path.
Root `target` is reused, with `CARGO_BUILD_JOBS=1`; managed build and Cargo run serially.

```text
python tests/fixtures/behavior-languages/run.py
```

Final run `a67cd2f4-08c9-4e45-b43c-cb9dbbbe5e68`: **2 passed, 0 skipped**, each exercising direct
Scene and Prefab cases. Managed incremental build: 0.79 seconds, zero warnings/errors.
Rust test build: 4.59 seconds; execution: 0.154 seconds. These are distinct test/build observations,
not a comparative language benchmark. The earlier Lua-only passing run is superseded.

```text
cargo +1.95.0 clippy --manifest-path tests/fixtures/behavior-languages/Cargo.toml --test languages --locked -- -D clippy::correctness
cargo +1.95.0 fmt --manifest-path tests/fixtures/behavior-languages/Cargo.toml --all -- --check
```

Final focused correctness Clippy passed in 1.04 seconds without warnings in this fixture invocation.
Formatting passed, including the reused support modules. Python runner syntax and
`git diff --check` passed. No root workspace/native acceptance was rerun for a private test-only
workspace and documentation change.

Tesla independently reviewed the fixture and report. Its export oracle finding is fixed:
the expected friend is located by native Name effect independently of Sensor's serialized
payload, then the exact exported reference list is asserted. Final read-only review reports
no remaining P1/P2 within the declared slice. The reviewer is closed. All 771 local Markdown
links across the nine affected documents resolve; root locked metadata confirms all 35 members
are unchanged and the fixture is excluded from the workspace.

# Follow-up

Compare real author iteration and Schema-driven bindings before selecting a production language
or representation owner. Test concrete domain access against the snapshot transport rather than
promoting JSON to a universal boundary. Settle canonical data versus VM-local state, complete
instance cleanup, dependent catalogs, retained callbacks/objects and body-replacement custody.
The managed code/rooted-pointer and collectible-context cases remain R3c work; production
language selection remains open.

# Citations

- [First-slice plan](../../../../plans/2026-10-27-behavior-language-workflow-plan.md).
- [Executed comparison](../../../../research/2026-10-27-behavior-language-workflow-comparison.md).
- [Primary-source hosting study](../../../../research/2026-10-27-dotnet-behavior-hosting-feasibility.md).
- [Fixture](../../../../../tests/fixtures/behavior-languages/README.md).
- [Prior representation qualification](2026-10-27T073822Z-private-behavior-representation-comparison-and-revised-language-trial-candidate-0025928de9064bb3b64bfcf4630d397d.md).
