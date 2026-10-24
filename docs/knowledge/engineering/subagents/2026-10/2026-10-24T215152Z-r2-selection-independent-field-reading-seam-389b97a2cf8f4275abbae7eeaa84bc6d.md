---
type: "Subagent Finding"
title: "R2 selection independent field reading seam"
description: "Read-only investigation identifies the missing component/root-value observation workflow and bounded source-owner seams without selecting a general query language."
timestamp: 2026-10-24T21:51:52Z
record_id: "389b97a2cf8f4275abbae7eeaa84bc6d"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a16fdf-6f75-7233-b3cc-be52f36296f2"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_commit: "7868d48072a893b5a9f176d93d4597d0a5ecb56a"
---

# Finding

Read-only investigation `01a16fdf-6f75-7233-b3cc-be52f36296f2` is complete and closed. It identifies
R2's next missing consumer: discover an actual component and editable root field on a non-active
document, read its current value, compute an edit and submit through the existing source owner.
Current entity enumeration returns IDs only; existing independent edits supply already-known
values rather than reading them through that route. No implementation or execution claim follows.

# Evidence

The available seams are Scene/Prefab `document()`, `authoring_record_facts()` and
`authoring_component_schema()`. The last is Available-direct-record-only; absence does not prove
a missing component. Keep source interpretation there and Workspace/basis/action/outcome routing
in Tooling. Current entity-query limits bound scanned records and rows, not value bytes/nodes.

`effective_component_value()` can copy a whole component; the full property resolver validates
broader values. Neither is a drop-in bounded root projection. ADR 0047 already rejects eagerly
building a complete Inspector and truncating it afterwards.

# Recommendation

Use a narrow actual-component/root-field projection, not a query DSL or second document store.
Distinguish actual component headers from catalog creation capabilities, Present/Defaulted/Missing
from explicit Null, and unavailable metadata from editable payload access. Retain known readiness
facts without interpreting unknown/future values. Do not copy complete Schemas/shapes.

Bound rejected component/field scans, exact entity lookup, output rows, metadata strings and copied
value nodes/bytes. Borrow and `cost_up_to` before cloning. Do not rebuild complete facts each page,
copy a huge unrequested sibling, or return an unprogressing empty page for an oversized value.

If discovery uses continuation, its private basis must include workspace, document/kind,
incarnation/revision/exact Schema and query semantics (entity/component/projection), with a position
inside components/roots. Save/navigation and unrelated edits do not stale it; target edits, Undo,
Reopen, Close and foreign workspace do. Concrete read limits need their own consumer justification,
not mutation's eight-write ceiling.

# Disposition

Background evidence, not a frozen design. Whether to evolve the existing query family, retain
nominal cursor types or use finite point reads remains an implementation choice. Point reads alone
must not be claimed as complete paged discovery. No new universal-query ADR is justified here.

After construction qualification, build an independent read -> computed edit consumer for Scene
and Prefab. Pair it with large-unrequested-sibling/oversized-target/default/Null/unavailable tests
and exact query/basis fencing. Update ADRs 0047/0118 at the affected boundary. No code, Cargo,
commit or child-agent work was performed by the investigator.

# Citations

- [R2 plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r2-owner-owned-prepared-editing-and-selection-independent-queries)
- [Current query types](../../../../../crates/nara_tooling/src/entity_query.rs)
- [Workspace routing](../../../../../crates/nara_tooling/src/workspace.rs)
- [Source admission](../../../../../crates/nara_scene/src/authoring_decode.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
