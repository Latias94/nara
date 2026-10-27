---
title: Bounded Behavior language workflow comparison
date: 2026-10-27
status: private-comparison
related_plan: 2026-10-23-foundation-architecture-rebaseline-plan.md
---

# Bounded Behavior Language Workflow Comparison

## Question And Constant Inputs

Follow the [representation comparison](../research/2026-10-27-behavior-representation-comparison.md)
with actual in-process C# and Lua execution of the same Mover/Sensor task. Provisionally use the
data-only entity envelope. Schema, source history, declarative Prefab compilation, reference
validation, native Name and runtime export remain owned by the existing fixture and production
modules. A result may reject this representation; neither language is already selected.

Begin with the installed .NET 10 SDK and a pinned vendored Lua 5.4 through mlua. The unrelated
system Lua 5.1 is not the interpreter under test. This fixture does not support a product platform,
require a second language for Rust projects or define a universal ScriptHost.

## First Executable Slice

One authored module contains a Mover callback that consumes the admitted amount and emits a
native Name change, plus a Sensor callback that emits Name changes for its already-resolved local
targets. The Host captures a finite invocation snapshot and validates the complete returned
command batch before applying it. No callback receives a World pointer, borrowed component
reference or authority outside that snapshot. Native command preparation retains exact World,
entity-generation and Definition tokens beside the wire DTO; returned string IDs cannot
reconstruct authority in another Runtime.

C# runs through actual hostfxr in the test process, not one subprocess per gameplay call. The
small authored assembly includes a private unmanaged bridge that copies a JSON snapshot and
returns a bounded JSON command batch. Lua consumes the equivalent DTO through mlua's serde
adapter. These are feasibility transports, not selected production bindings. Their differing
marshalling paths prevent a fair claim about best possible per-call performance.

The private fixture is independently compiled. It reuses the earlier private support rather than
cloning Schema, property, Prefab or reference interpreters. Build products stay under the root
`target`; one small cross-platform Python entry builds the managed library and runs serial Cargo.
It fails on a missing required toolchain rather than silently skipping a language.

Execute both against the same source edit/history and Prefab-derived runtime data. Prove that
changed fields affect the next invocation, a callback error publishes no staged native change,
invalid command targets reject before any write, and runtime export retains the independent
Behavior records and remapped references. Drop all invocation data before retiring the complete
Runtime. Do not treat staged commands as a general rollback mechanism for arbitrary native or VM work.

## Evidence And Limits

Record exact toolchain/package versions, commands, actual results and compile/first-use
observations separately. Do not infer runtime or author-product superiority from one small
callback. Any timings are observations without an invented threshold.

The first slice uses one process and one managed assembly generation. Closing hostfxr does not
prove CLR or assembly unload. Same-process body replacement, collectible AssemblyLoadContext,
held VM objects/delegates, source-driven catalog migration, typed domain calls, asset use,
Editor integration and public representation ownership remain subsequent work. A production
selection requires this additional lifetime and author-workflow evidence; ADR 0120 stays Proposed.
