---
type: "Decision"
title: "Use one runtime World and orthogonal typed gameplay state domains"
description: "User-approved architecture direction for runtime World ownership, scene-instance lifetime, typed state domains, and safe-point transitions."
timestamp: 2026-07-21T02:18:02Z
record_id: "7cb3aa4b7c094f5388865ca3b6c2fe64"
tags: ["runtime", "world", "scene-instance", "gameplay-state", "lifecycle", "schedule"]
status: "discussed"
producer_id: "codex-root"
run_id: "20260721-runtime-world-gameplay-state-direction"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3d23c40bd1ed26cc972d2d6d12858633a676e3db"
---

# Decision

Nara adopts the following product and architecture direction:

1. One Runtime Generation owns exactly one authoritative ECS `World` by default. A simulation that
   needs true isolation uses another Runtime Instance and generation rather than an implicit
   scene-owned World or default subworld graph.
2. A Runtime Scene Instance is a lifecycle and provenance owner inside that World. A runtime entity
   has at most one Scene Instance lifecycle owner.
3. Parent/child hierarchy, prefab provenance, region residency, and Gameplay State scope are
   orthogonal relations. None implicitly owns or derives the others.
4. Gameplay State is expressed as multiple game- or plugin-owned typed domains. Each domain is flat
   by default; hierarchy, stacks, or graph behavior require evidence within that domain rather than
   a universal engine state tree.
5. State changes enter as typed transition requests and commit only at an explicit safe point.
   Requests are resolved and validated before transition side effects. An accepted transition runs
   Exit behavior, cleans explicitly scoped data, switches the active state, and runs Enter behavior.
   Validation or conflict failure leaves the old state unchanged.

(session-settled: user-approved - chosen over scene-per-World and default subworld models,
multi-owner or hierarchy-derived entity lifetime, a universal hierarchical state tree, and
immediate mid-tick state mutation.)

This record establishes direction and vocabulary only. It does not accept Proposed ADR 0084 or ADR
0089, authorize implementation, create a state crate or public scheduler API, define streaming or
subworld support, or prove the exact cleanup and fault behavior of every scoped carrier.

# Context

The discussion tested whether Nara should model scenes, worlds, runtime isolation, and gameplay
states as one hierarchy. That model initially looks convenient, but it combines four independent
questions: simulation isolation, scene lifecycle, spatial residency, and gameplay-mode gating. It
also makes common cases such as a persistent player crossing scenes, a pause overlay over gameplay,
or a streamed region shared by several systems depend on hidden tree ownership.

Nara already has stronger boundaries to build on: a Runtime Instance owns one `App`, stable runtime
identity is scoped to one World identity domain, scene documents project into runtime entities, and
Play Mode uses an isolated runtime fork. The simplest coherent direction is therefore one
authoritative World per Runtime Generation, with explicit relations inside it and a separate
Runtime Instance for genuine isolation.

Gameplay states have similar pressure. Boot flow, menus, play mode, pause, combat phases, and
plugin-specific modes do not naturally form one universal tree. Typed owner-defined domains retain
compile-time meaning and let unrelated domains remain orthogonal. A safe-point transition protocol
keeps systems from observing a half-applied state change while leaving detailed failure policy open
for evidence.

# Alternatives

- **One ECS World per scene.** Rejected as the default because cross-scene entities, persistent
  services, travel, streaming, and cross-scene references would become cross-World coordination.
- **A default World/subworld graph.** Rejected because it adds identity, query, scheduling,
  extraction, physics, and tooling boundaries before a production workload proves the need.
- **Derive lifetime from hierarchy or allow several Scene Instance owners.** Rejected because
  reparenting and provenance changes would become implicit destructive lifecycle operations and
  cleanup authority would be ambiguous.
- **One global hierarchical or stacked state machine.** Rejected because independent game and
  plugin concerns would contend for one topology and one ordering policy.
- **Apply state changes immediately when requested.** Rejected because system ordering could expose
  partially exited, cleaned, or entered state during the same schedule execution.
- **One World with explicit orthogonal relations and typed state domains.** Selected as the default
  because it keeps the common execution path direct while preserving explicit isolation when a
  second runtime is actually required.

# Consequences

- Scene load, unload, and travel operate on Scene Instance lifecycle inside the Runtime World; they
  do not imply World creation or replacement.
- A runtime entity may be scene-owned or runtime-only, but it cannot have competing Scene Instance
  lifecycle owners. Hierarchy, prefab, region, and state indices remain independently queryable.
- Large-world residency remains OQ-035 work inside the same World by default. A future subworld or
  shard mechanism requires workload evidence and an explicit identity/schedule/tooling contract.
- Games and plugins may own typed state domains without waiting for a universal Nara state graph.
  This direction does not yet choose concrete component/resource types, transition queues, or run
  condition APIs.
- The eventual transition contract must name the safe point, conflict resolution, re-entrant
  requests, Exit/cleanup/Enter faults, and cleanup behavior for each supported scoped carrier.
- The default is performance-friendly because ordinary ECS queries, schedule execution, extraction,
  and cross-scene references stay in one World. This is a direction, not a performance guarantee;
  measurement still governs region streaming, indexing, and any future partition mechanism.
- OQ-034 remains open for topology and fault details. Proposed ADR 0084 and ADR 0089 retain their
  existing authority state and must be accepted separately before implementation relies on them.

# Citations

- `CONTEXT.md`, Runtime Topology and Gameplay State.
- `docs/architecture/open-questions.md`, OQ-034 and OQ-035.
- `docs/architecture/adr/0058-stable-runtime-identity-and-entity-references.md`.
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md` (Proposed).
- `docs/architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md`
  (Proposed).
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`.
