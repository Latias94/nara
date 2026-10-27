---
title: Authored Behavior code iteration and retained execution lifetime
date: 2026-10-27
status: private-comparison
related_plan: 2026-10-23-foundation-architecture-rebaseline-plan.md
---

# Authored Behavior Code Iteration And Retained Execution Lifetime

## Question

The Schema-driven comparison proves data authority and callback availability, not author code
iteration or physical VM/code retirement. Compare those concrete obligations before selecting
a production adapter. Keep the existing complete workflow and remove the old permanently
loaded authored C# module path rather than adding a compatibility dispatcher.

## Concrete Paths

Lua prepares a module in its own actual VM. A retained prepared callback explicitly owns a Lua
VM lease: mlua 0.12.2's Function holds a WeakLua, not a strong runtime owner. A non-replaced
userdata cleanup probe and a weak VM observation distinguish wrapper Drop from actual lifetime.
No asynchronous callbacks or preemptive interruption are claimed.

C# keeps an immutable hostfxr bridge/contract assembly for the process lifetime. Authored code
loads from bytes into its own collectible AssemblyLoadContext sharing that contract. Stream
loading allows an actual rebuild of the same authored output while an older instance stays
alive. A module-owned callback/instance lease with zero executing calls must keep the old
context alive until release; a weak context observation after bounded GC/finalizer polling is
required before reporting collection. CLR unload, forced cancellation and returned RSS are not
implied.

Each path executes revision one, rejects invalid candidates while preserving its active
predecessor, prepares a body-only revision two, validates all callback bindings and switches at
a synchronous owner safe point without replacing native World or Schema. Prepared old results
hold an exact execution-publication receipt and cannot write through the new route, even if the
old callback can still physically execute under its lease. Native Target authority remains exact.

Use the same Mover/Sensor/Label data and actual native Name result. Record compile/load stages
as observations, not cross-language performance claims. Build all products serially under root
`target`. Do not turn a finite runner into a package manager or generic job framework.

## Required Counterexamples And Limits

- Broken source or a missing module refuses readiness without changing current native behavior.
- A candidate with the last required Schema binding mismatched cannot replace the active route.
- A successfully switched successor retains the same native World and admitted data.
- A retained old callback/instance with no executing invocation delays physical completion.
- Old prepared commands reject through the successor route; generation equality alone is not a
  code lifetime proof.
- After explicit lease disposal, actual Lua VM cleanup / collectible ALC disappearance is
  observed; a still-alive weak reference remains pending, never converted into success by timeout.

This is one owner, serial in-process invocation, compatible native layout and unchanged Schema.
It does not qualify live in-flight async work, arbitrary escaping language objects, dependent
owner cohorts, migration, Stop-first, post-publication cleanup failure, a public adapter API or
supported platforms. Full R3 and the foundation Goal remain active; ADR 0120 stays Proposed.
