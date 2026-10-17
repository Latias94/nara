# Engineering Memory

Use this bundle for session recovery, cross-machine continuity, non-obvious engineering reasoning,
and verification evidence. It supplements the [architecture documents](../../architecture/README.md);
records and registration states do not authorize work or add acceptance requirements.

## Read and Capture

On resume, use the derived Current State to locate relevant records, then read their source
shards. Registration lineage describes work contexts; it does not reserve files or replace live
agent coordination. Old task lists and platform gaps retain their recorded scope.

Capture meaningful context at handoff, decision, verification or commit boundaries: failed
approaches, rationale absent from the final code, results and limits, or the next recovery step.
Combine related facts where useful and cite existing artifacts rather than repeating their
contents. A boundary needs a record when it contributes recoverable information, not a series
of duplicate progress and closure receipts.

Keep normal writes immutable and regenerate derived views during integration. If a complex task
uses an external workstream manifest, link it from registration instead of mirroring its live
state here. Local diagnostics and raw tool output belong outside tracked memory.

## Core

* [Current State](current-state.md) - Derived registration heads and recent evidence, not a complete product status report.
* [Update Log](log.md) - Derived chronological view of memory updates.

## Concepts

* [Decisions](decisions/) - Engineering rationale and context; durable architecture contracts belong in ADRs.
* [Progress](progress/) - Work progress tied to plans, branches, or commits.
* [Registry](registry/) - Registration history and development contexts; follow successor records for current state.
* [Sessions](sessions/) - Compaction, interruption, and handoff summaries.
* [Subagents](subagents/) - Distilled findings from spawned agents.
* [Verification](verification/) - Test, build, lint, benchmark, and manual evidence.
* [Logs](logs/) - Append-only chronological event concepts.
* [Conventions](conventions/) - Local repo rules and reusable agent contracts.
