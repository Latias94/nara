---
title: Production Scene-owned data representation
date: 2026-10-27
status: design
related_plan: 2026-10-23-foundation-architecture-rebaseline-plan.md
---

# Production Scene-Owned Data Representation

## Objective And Contract

Implement the first production non-native data family through the
[Scene representation design](../architecture/scene-component-representations-design.md).
Replace the private preview/post-commit helper in a complete independent consumer; do not promote
its hidden entry points. Native registry cardinality, exact native preparation and strict
retirement remain intact. This is a prerequisite to actual optional-language owner integration,
not proof that R3 or the foundation Goal is complete.

The existing private code-lifetime checkpoint is `65402b9`. Implementation has not started merely
because its insertion/export seams were identified. Durable contract changes require ADR
reconciliation when evidence is ready; ADR 0120 remains Proposed during implementation.

## Delivery Order

### Complete Binding And Compiler Path

Own Reflect metadata admission and Scene representation selection together with the first
consumer. Factor existing catalog/lineage/ownership checks for code-authored use without `serde`;
do not add a second Schema validator or expose an unused abstract extension SPI. Select explicit
native and data-only families, unique owner/type/tombstone claims and exact owner-local snapshots.
The shared Prefab compiler accepts this strict mixed authority and preserves complete source
provenance/actual demand. Do not compile runtime records through native-preview omissions.

### Atomic Production Materialization And Export

Prepare complete per-entity data groups with native prepared values before target allocation.
Insert the concrete pure-data carrier through the existing validated insertion tail, reserving
its type against product overlays. Add exact data-definition capture alongside existing native
World/managed checks. Runtime full-value member replacement/removal validates Schema/references
before mutation and preserves siblings.

Extend the existing current-live export loop and aggregate allowance for both families. Reject
native-only export of encountered data records rather than omitting them. Preserve existing
persistent-reference semantics and strict retirement, including native claims and additional
retirements. No fallible post-native-commit carrier installation or cached initial entity roster.
Data export separately lowers Schema-declared asset references through selected immutable
metadata under `Path` and `StableIdWhenKnown`; it does not inherit policy from native codecs or
the entity-reference rewrite. Test matching native/data output, nested references and unavailable
required conversion metadata with no partial output or asset loading.

### Independent And Managed Consumers

Run a public no-serde code-authored consumer plus a file-backed Scene/Prefab consumer. Cover
two data Schemas on one entity with native Name, source Attach/Edit/Undo/Save/Reopen, repeated
Prefab references, complete export after entity population changes and whole-instance retirement.
The consumer must not import `__private`, forge native bindings, or construct provenance manually.

Then admit the same definition through the actual managed App startup/recipe path and migrate
the selected optional owner away from the private helper. Materialization of plain data is not
language execution readiness: required callback/module contributions still reject when absent.
Select one production optional-language path using domain access and author-workflow evidence;
do not turn both experimental bridges into maintained SDKs for optionality.

## Verification And Removal

Use focused serial nextest with root `target` and `CARGO_BUILD_JOBS=1`, then affected Scene/Reflect,
root and independent consumers. No-default coverage must prove the code-authored family is not
accidentally gated on `serde`. Reuse native identity/retirement and aggregate budget tests rather
than replacing them with positive examples. Add exact last-record, late-hook, wrong-basis,
missing-module, foreign World, native-only export and stale membership counterexamples.

Require independent review of the production diff and public workflow. Actual Editor/runtime
changes need their affected native qualification; a headless family test alone cannot claim it.
After complete replacement, remove the superseded private grouped-materialization/export helper
while retaining only indispensable losing-alternative evidence with a durable report. No old
reader, alias or migration guide is required. Commit locally in coherent verified units without
remote publication. Keep implementation scope and remaining R3/foundation obligations explicit.
