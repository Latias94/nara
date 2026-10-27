---
title: Runtime-Defined Behavior Representation Comparison
date: 2026-10-27
status: private-comparison
related_plan: 2026-10-23-foundation-architecture-rebaseline-plan.md
---

# Runtime-Defined Behavior Representation Comparison

## Decision To Establish

Compare an explicit per-entity attachment envelope with owner-managed per-Schema rows for two
runtime-defined Behavior Schemas on the same entity. Both consume the same admitted source
records and execute the same bounded Rust feasibility adapter. Neither registers a Rust type
per Behavior, weakens the native stable-ID/type bijection or defines a language SDK.

This is the representation prerequisite under Proposed ADR 0120. Complete source addressing,
declarative Prefab compilation and direct-instance conversion now have qualified predecessors.
The full foundation Goal remains active. A private comparison does not qualify ordinary Runtime
execution, installable languages or module unload.

## Competing Storage

- Envelope: one actual ECS component owns a map of typed Schema records for an entity. Prepare
  all members together; replacing a whole carrier per Schema would overwrite siblings.
- Rows: one owner resource indexes each Schema's exact entity membership and complete records.
  Per-Schema enumeration avoids scanning unrelated native entities, but entity destruction does
  not automatically remove resource-owned rows. Exact retirement is part of this alternative.

Use the same source representation for both. A separate opaque serialized envelope would compare
different source models and duplicate inner property/reference/migration dispatch before storage
has been selected. This comparison varies execution storage, not the already admitted document
grammar. Same-Schema multiplicity is one record per entity in this trial; multiple attachments
would require a distinct stable attachment identity and remains an explicit future decision.

## Consumer And Failure Matrix

Two runtime-defined Schemas, Mover and Sensor, coexist with a real native Name on one entity.
Mover has an editable numeric amount; Sensor has recursive local/entity and asset references.
Both have independent source properties and owner claims. Execute:

1. Read/prepare/commit source edits, one Undo/Redo, actual JSON/RON Save/Reopen.
2. Declare a base Prefab and a selected leaf override; compile through the existing semantic
   compiler and retain actual typed provenance and reference rewriting.
3. Prepare every Behavior and native value before publishing a fresh private Runtime. Refuse
   an invalid last record or missing execution binding without replacing an active predecessor.
4. Read/enumerate each Schema independently; distinguish absent membership, unavailable binding
   and invalid encoding. Remove one member while its sibling survives.
5. Export the mixed native/dynamic state through the existing native export plus complete owner
   membership, rewriting declared references through the export's actual identity remap.
6. Retire exact members and the Scene; no stale rows or old entity/publication tokens may alias.
7. Migrate Mover only into a new Schema publication; preserve Sensor exactly. Missing module
   retains source data and refuses fresh execution. Separate projects/generations cannot use
   one another's prepared values.

The fixture uses bounded finite inputs. Do not introduce a second Schema/property/reference
interpreter, fake native registry or generic callback transaction. Fresh private Runtime
construction allows candidate failure to discard the entire unpublished World. It does not
establish atomic in-place native/VM reconciliation or hot code replacement.

## Comparison Entry And Measurements

Freeze correctness before implementation. At experiment entry use two Schemas, two Behavior
members per selected entity, and a second unselected entity. A separate 256-native-entity control
measures inspected entries for per-Schema enumeration. Report records, grouped carrier writes,
queries, exact retirement obligations and author-visible concepts. Optional timings are
observations without a target latency or RSS claim; no performance threshold is invented.

Keep prototypes private to the independently compiled module-consumer test workspace. It may use
the existing hidden semantic compiler for this representation experiment; that does not turn it
into public Runtime support. A selected production representation needs a responsibility-owned
integration and a later independent public workflow before ADR 0120 adoption.

## External Evidence

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` supplies native replacement semantics,
runtime `ComponentDescriptor` registration and dynamic query builders. Pinned Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` stores GDScript members through one instance type,
but an Object has only one direct ScriptInstance. These establish alternatives and cardinality
constraints, not Nara workflow qualification.

Dynamic descriptors are a separate counterfactual for a named requirement: first-class per-Behavior
Bevy query presence/absence and independent scheduler access alongside native components.
Owner queries do not provide that exact contract. Unless a concrete consumer requires it, unsafe
layout/drop/by-ID handling is not justified by two Schemas coexisting alone.

## Delivery

Execute the comparison serially with the root target, independently review the actual failures
and lifecycle model, record measured results, then select or revise the storage direction.
Leave language comparison, code replacement, cross-owner publication cohorts and native Editor
integration explicitly open. Remove losing experiments when their evidence has a durable home;
do not preserve several production SDKs for optionality.
