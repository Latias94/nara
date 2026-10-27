# Schema-Driven Authored Behavior Successor

**Date:** 2026-10-27. **Status:** Private fresh-Runtime comparison; ADR 0120 remains Proposed.

The [code-lifetime successor](2026-10-27-behavior-code-iteration-and-lifetime.md) later adds a
bounded same-World body switch and explicit retained-code probes. This report keeps its original
generation-one data/Schema evidence; later code replacement does not imply catalog migration.

## Why Replace The First Harness

The [first language experiment](2026-10-27-behavior-language-workflow-comparison.md) proves
actual C#/Lua embedding but compiles amount and event cases into the Rust harness. That is
insufficient evidence for independently installed Behavior definitions. The
[successor plan](../plans/2026-10-27-schema-driven-behavior-plan.md) replaces that fixed-field
harness in place rather than keeping a second adapter path for every experiment.

The current [fixture](../../tests/fixtures/behavior-languages/README.md) reads an ordinary
`ComponentSchemaCatalog` JSON file and a Scene JSON file. Existing catalog admission validates
the Schema, then the authoring bundle composes this catalog's one owner with the real native
provider. Definition's executable data view derives from that admitted canonical owner, not a
second unnormalized copy. A collision with native Name refuses composition without inventing
a native binding for an authored Schema.

## Actual Ownership And Dispatch

Both language modules explicitly register callbacks by stable Schema ID and report ID/version
bindings. Before constructing a fresh Runtime, the Host requires exactly one current binding for
each selected Schema. Missing, duplicate and mismatched-version lists refuse construction while
the independent active predecessor remains usable. These checks establish binding availability,
not an ABI compatibility proof, a trusted permission grant or a mutable-publication protocol.

For a selected entity/Schema, capture enumerates admitted root field IDs and calls the existing
property resolver on each. The language receives typed `ComponentValue` data keyed by stable
ID, not by wire path. Missing optional fields remain absent. Declared entity references are
resolved with the existing reference planner and current native Name observations. Exact
World/entity/Definition tokens remain on the Rust side of the DTO, and command proposals cannot
reconstruct those tokens from strings.

The three authored callbacks are Mover, Sensor and Label. Label's stable `text` field is stored
as `wire_text`; both languages receive `text` with no native branch for that Schema. Mover's
optional stable `gain` is stored as `wire_gain`; supplying 3 in source changes amount 2 into
native Name `Moved 6`. Omitting it yields `Moved 2` in both languages. Language-owned scalar
helpers validate a small typed payload; they do not independently resolve Schema paths.

Adding another Behavior in this bounded model changes the external catalog/source and its
language callback map. It does not change Rust field extraction or native type dispatch. This
does not automatically bind arbitrary native domains: only Name commands have an implemented
owner here. Same-Schema attachment multiplicity remains one per entity in this trial.

## Workflows And Counterexamples

- Prepare/Commit edits Mover, while actual Remove, Undo/Redo, Attach from its admitted complete
  creation template and a separate stable-field edit change Label. JSON/RON Save/Reopen use real
  temporary files and retain identical records with a fresh history.
- Both actual language adapters execute all present records generically in a direct Scene and
  two Prefab occurrences. Native Names prove Mover and Label results; Sensor signals each
  occurrence's local friend.
- Export uses native remapping plus complete owner membership. Independent native actor/friend
  markers keyed by the original occurrence identify expected pairs; exact exported Sensor lists
  must point to the matching friend, not merely any existing entity. Label records survive.
- Callback exceptions reject. The invalid-final-command case uses Sensor, whose one reference
  permits two commands under the count cap. It must reach the foreign-target rejection after
  validating the legal first change and preserve that target's prior Name. It cannot pass solely
  because of a smaller command-count limit.
- Foreign Runtime results and results replayed after whole-instance retirement reject through
  the retained native tokens. Missing/duplicate/version-mismatched callback lists do not disable
  a previously prepared Runtime. Invalid Schema kinds and a native ID collision refuse admission.

The source Attach initially rejected because the external Label Schema lacked a complete
creation template. Adding the real template and separating creation from property editing fixed
the consumer; no production validation was relaxed. This is useful workflow evidence, not a
reason to bypass the existing authoring contract for scripted records.

## Executed Evidence

```text
python tests/fixtures/behavior-languages/run.py
```

Final successor run `33cdeebc-b691-4e43-a4e0-2a8b5f02ba86`: **3 passed, 0 skipped**;
each actual language executes all source/Prefab/failure workflows. The separate shared-support
regression `f11f5f00-1e07-4b7f-8e0b-3636b23fd2ed` retains **9 passed, 0 skipped** in the
representation comparison. Focused correctness Clippy and formatting pass. Faraday independently
reviewed the final source/report read-only with no remaining P1/P2 in the declared scope.
Full command, toolchain and evidence limits are in the
[verification record](../knowledge/engineering/verification/2026-10/2026-10-27T084702Z-schema-driven-authored-behavior-dispatch-and-complete-source-workflows-cec09063db0d4d8a85b838302985bb97.md).

## Remaining Decisions

This supersedes only the fixed extraction harness. The shared compiler integration remains
hidden/private, the catalog is a fixed generation-one publication, calls are synchronous and
serial, and pure data stays authoritative in the envelope. There is no authored-module body
replacement, owner-catalog successor/migration, retained asynchronous result protocol, arbitrary
VM-local mutable state, actual asset consumption, public SDK or supported language selection.

The two transports still differ: Lua converts typed values to tables; the C# bridge copies JSON
and uses owned managed DTOs. No latency, allocation or memory superiority follows from these
correctness cases. A concrete domain-access/iteration comparison must decide whether these
copies are appropriate before freezing production interfaces. Direct native Rust systems remain
direct; Editor IPC is not introduced for gameplay calls.

The next responsibility-owned production path must integrate a concrete language/representation
owner with complete author iteration and physical VM cleanup, not lift this fixture's roster,
Name-only command tail or `Describe` list into a universal SDK. Exact binding availability and
World tokens do not replace dependency compatibility, code leases or collectible-context proof.
Keep the full [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
active; implementation coverage and acceptance of ADR 0120 remain separate.
