---
type: "Verification Evidence"
title: "Actual authored code iteration with retained Lua VM and collectible managed code"
description: "Qualifies unchanged-World body routing, rejected candidates, exact old-result fencing and actual physical code/VM retention in the private two-language comparison."
timestamp: 2026-10-27T09:14:00Z
record_id: "060183fe1373441980ece9733ef76f57"
producer_id: "codex-root"
run_id: "behavior-code-lifetime-2026-10-27"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "experiment/behavior-code-lifetime"
---

# Verification

Private code-lifetime successor based on `f1e97b9`, following the Schema-driven authoring fixture.
The full foundation Goal remains active. ADR 0120 stays Proposed; this is not the selected public
production owner or completion of R3c. Production engine/Editor source is unchanged, untracked
`vendor/` is untouched, and no remote operation occurred.

# Result

Lua and C# preserve the exact native World and Definition while one synchronous body route
changes actual Name from `Moved 2` to `Moved 4`. Missing/invalid candidates preserve the old
route. Old prepared commands and commands actually computed through a retained old callback
after switching reject through the new execution-publication receipt.

An actual mlua Function is weak with respect to its VM. The bare-function counterexample observes
expired WeakLua and actual registry-userdata destruction. The callback lease owns Lua explicitly,
keeping the VM/probe alive after Program Drop; releasing it observes physical cleanup.

The permanent C# bridge no longer contains authored callbacks. A separate authored assembly loads
from streams into a collectible context sharing the contract. Its live DLL path rebuilds to the
new body while the predecessor remains loaded and leased. Retirement revokes ordinary invocation
but a retained real module instance with zero active calls keeps its context alive. Releasing the
lease permits observed weak-context disappearance, separately from the `Unload()` request.

Review found a Lua readiness hole: correct bindings but missing/wrong-type invoke could be admitted.
Compile now captures the callable and a copied binding list; actual file candidates setting invoke
to nil and a number reject while the predecessor remains executable. No callback is pre-executed
to guess its semantics. Curie implemented only managed/authored files; the parent integrated and
executed all checks. Schrodinger independently reviewed read-only.
The final review closes the readiness P2 with no new P1/P2 in the declared scope. Both worker
and reviewer are closed; neither independently executed the parent verification commands.

# Evidence

Windows 11 x64 / PowerShell, Rust 1.95.0, .NET SDK 10.0.400, runtime/hostfxr 10.0.11,
mlua 0.12.2 with Lua 5.4, netcorehost 0.22.0. Builds are serial, `CARGO_BUILD_JOBS=1` and root
`target` reused. The runner builds bridge, revision-one, invalid and live initial modules before
the actual test; the test rebuilds the live body variant without another Cargo invocation.

```text
python tests/fixtures/behavior-languages/run.py
```

Final run `e840056a-7acd-4f01-8bc9-19fd2f7e9182`: **6 passed, 0 skipped**. It covers the
complete Schema/source/Prefab workflow as well as actual lifetime probes. Managed builds have
zero warnings/errors. Rust build 5.58 seconds; execution 2.642 seconds. The same-path managed
body rebuild while the predecessor stays loaded is 2.2287652 seconds. These are finite workload
observations without an engine or cross-language performance claim.

```text
cargo +1.95.0 clippy --manifest-path tests/fixtures/behavior-languages/Cargo.toml --test languages --locked -- -D clippy::correctness
cargo +1.95.0 fmt --manifest-path tests/fixtures/behavior-languages/Cargo.toml --all -- --check
```

Final focused correctness Clippy passed in 1.10 seconds without warnings. The shared representation
fixture and production code are unchanged; no root/native Editor acceptance rerun is implied.
Formatting passed. All 786 local Markdown targets across nine affected/new documents resolve;
Python runner syntax, managed project XML and `git diff --check` passed. Memory validation has
no errors, with historical legacy metadata/large-ledger advisories outside this unit.

# Follow-up

Only one owner, unchanged Schema/native layout, no executing invocation at switch, and one Name
domain are covered. Async work, escaped arbitrary language objects, dependency cohorts, catalog
migration, Stop-first, post-publication cleanup failure, actual assets and public owner integration
remain open. GC/finalizer observation is not bounded wall time for arbitrary code; a live weak
context remains pending, not completion by timeout. No CLR unload, RSS reclamation or sandbox
claim is made. Complete native and semantic Rust workflows remain required independently of
optional language support.

# Citations

- [Plan](../../../../plans/2026-10-27-behavior-code-lifetime-plan.md).
- [Report](../../../../research/2026-10-27-behavior-code-iteration-and-lifetime.md).
- [Fixture](../../../../../tests/fixtures/behavior-languages/README.md).
- [Pinned primary-source hosting study](../../../../research/2026-10-27-dotnet-behavior-hosting-feasibility.md).
- [ADR 0120](../../../../architecture/adr/0120-schema-publications-and-execution-representations.md).
