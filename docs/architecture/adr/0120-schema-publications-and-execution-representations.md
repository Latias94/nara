# ADR 0120: Schema Publications And Execution Representations

**Status**: Proposed
**Date**: 2026-10-23
**Owner**: Schema, Scene materialization, optional gameplay adapters and executable Hosts
**Would Refine**: ADR 0006, ADR 0081, ADR 0090, ADR 0098
**Related**: ADR 0046, ADR 0082, ADR 0084, ADR 0093, ADR 0119

## Context

The user requires optional runtime-defined gameplay Behaviors and installable tools without
rebuilding Editor for every change. The native executable registry currently has a valid but
narrower contract: each admitted persistent component has an exact Rust type and World binding.
Mapping many Behavior Schemas to one native carrier would violate its cardinality, encoding and
absence assumptions even if duplicate registration were permitted.

An immutable registry is valuable. Freezing the entire future Schema universe for every change
is not the only way to preserve that value. The target is immutable admitted publications, not
unrestricted mutation of a live global registry.

## Proposed Direction

### Complete Representation Families

Keep native exact-`T` preparation and stable-ID/World-`ComponentId` bijection. A non-native
representation, if selected, must independently prove:

- Schema-complete enumeration/read and explicit Absent, BindingUnavailable and EncodeFailure;
- owner-grouped per-entity preparation/commit, not repeated replacement of a shared carrier;
- complete Schema/version/reference validation and exact World/entity-incarnation association;
- prepared-data custody bound to immutable Schema and execution generation;
- exact row/membership retirement and export, without deleting unrelated rows in one carrier;
- native/VM reconciliation as an explicit fallible lifecycle, not arbitrary reversible World work.

The [executed private comparison](../../research/2026-10-27-behavior-representation-comparison.md)
uses the same authored records and compiler for an attachment envelope and owner-managed rows.
Use the data-only entity envelope as the initial candidate for the language workflow trial,
revising the earlier Rows preference. Independent semantic Schema identity does not require
independent physical storage. Rows inspect fewer records for a selective query, but ordinary
entity/Scene destruction leaves owner rows unless a separate cleanup path participates.
This is a bounded candidate choice, not a selected public representation or a speed claim.

An envelope must expose typed inner Schema/reference/migration semantics, not an opaque
properties blob. Its data cleanup does not prove VM instance, callback or code lifetime.
Actual language custody and dispatch costs may reverse the candidate choice. Compare dynamic
Bevy descriptors against a named requirement for first-class per-Behavior query presence or
independent scheduler access; their implementation must own layout/drop/by-ID safety, not
synthesize arbitrary Rust layouts from file data. No duplicate Rust `TypeId` exception is an
adequate design.

The [first in-process language slice](../../research/2026-10-27-behavior-language-workflow-comparison.md)
executes the same fixed Mover/Sensor task through actual C#/.NET and Lua calls. Exact native
tokens guard returned Name batches; no World borrow crosses the call. This qualifies that
private snapshot workflow only. Compiled field extraction/event cases, copied DTO costs,
language iteration, generic Schema binding and VM instance/code custody still need evidence.
Neither this JSON bridge nor either language is selected as the production interface.

### Publication Scope

Native executable/layout changes continue to create a fresh native Runtime generation. A
managed body-only edit may replace a module execution publication. A managed Schema change
requires an owner-catalog candidate, explicit data migration and matching executable candidate.
These are distinct identities, not one universal generation counter.

The Host selects an exact immutable set of owner snapshots and required contributions. A safe
point stops affected admissions, validates/migrates candidates and publishes that required cohort
as one routing decision. Old publications remain immutable and leased by in-flight work. Stale
completion is rejected; physical callbacks, code pointers and drop glue must finish before code
unload. If that proof fails, process replacement or explicit restart remains the fallback.

The selected adapter declares a bounded dependency/read-set over the Schema/interfaces and
executable bindings it consumes. Validate the transitive affected closure, including unchanged
source owners with cached access/query/codec plans. An incompatible dependency update rejects or
reprepares/migrates/rebinds those consumers; simultaneous routing alone is insufficient.
Independent owners need not change. This is not a universal Package/Cargo/Schema resolver;
missing or unsupported relationships fail explicitly.

Code lifetime covers retained executable snapshots, prepared closures, binding/query plans,
instances, vtables and destructors even with zero active invocations. They either keep an exact
old-code lease until disposal or are invalidated and safely disposed before unload. Stale objects
cannot submit through the new route. Data-only Schema captures need not pin code; a container
also storing callbacks is not data-only. Untracked escaped language objects require process
replacement/restart rather than a fictional unload proof.

Coexist-and-switch can preserve old active routing until a complete candidate is Ready, retiring
old owners after publication. Exclusive-authority Stop-first instead keeps a compatible
restartable last-good executable/data/recipe, exposes interruption/recovery and publishes only
when all required members are Ready. Reversible admission quiesce is not physical retirement.
Post-publication failure retains cleanup obligations, not automatic routing/external-effect rollback.

This proposal does not relax today's native registry checks before the new owner proof exists.
It does not require all changes to replace all projects, all tools or every Runtime.

## Alternatives And Tradeoffs

| Design | Advantages | Costs |
| --- | --- | --- |
| Whole Runtime reconstruction for all changes | Strong isolation, simple fallback | Disruptive managed iteration; broad lifetime coupling |
| Mutable global registry | Simple-looking incremental registration | Invalidates admitted snapshots and spreads stale-state checks |
| Immutable owner publications | Bounded replacement, explicit authority, independent projects | Composite compatibility, migration and retirement must be implemented |

Prefer owner publications as the evolution target with whole-Runtime reconstruction retained.
This is not a universal ScriptHost or a language-neutral binding dispatcher. Domain operations
may be synchronous queries, callback-scoped views, commands, asynchronous requests or retained
handles with different semantics.

## Admission Experiments

The [rebaseline plan](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md) owns the
concrete work. Acceptance requires:

1. Two stable Behavior Schemas coexist on one entity without per-Behavior Rust types, with
   independent edits, Undo, Prefab/source persistence and mixed native materialization.
2. Nested asset/entity references are traversed; one migration does not mutate the other record.
   Missing module/language/binding round-trips bounded authored data and refuses new execution.
3. Compare envelope and owner-managed rows on the same complete workflow; use a dynamic
   descriptor counterexample only to test a named presence/query/storage requirement.
4. Select same-type attachment cardinality explicitly; multiple instances require stable
   attachment identity. Trial simplification is not a permanent product constraint.
5. Different projects use different catalog/module generations. Old drafts, tasks and prepared
   data fail closed. Failed activation preserves old executable plus compatible data.
6. A module-body change with unchanged native layout avoids whole-native-Runtime reconstruction
   without mutating its native registry. Old callbacks retain their code until physical return.
7. A depends on B while C is independent. An incompatible B upgrade rejects or jointly
   migrates/reprepares A; C stays usable. Repeat with zero callbacks but retained old executable
   snapshots/prepared/binding objects: they prevent unload until disposal and cannot use new routing.
8. For each supported replacement mode, fail the last of two required candidate members, hold old
   retirement Pending, and fail retirement after successful publication. Check active routing,
   last-good restartability and physical custody separately; Stop-first must not claim old-active.

A working bridge alone does not freeze an SDK, advertise platform support or prove performance.
