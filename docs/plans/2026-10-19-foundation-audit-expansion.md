# Foundation Audit Expansion

Status: complete; read-only research, independent challenge and review artifacts delivered.

## Objective And Scope

Extend the [Bevy/Godot comparison](../audits/2026-10-19-nara-bevy-godot-architecture-review.md)
beyond its original three research lanes. Inspect complete infrastructure workflows for current
correctness hazards, incomplete extension interfaces and future migration cost. This task does
not authorize implementing every recommendation or changing Accepted ADRs.

Baseline: Nara `d88a96f` (runtime code remains `835c5e4`), branch
`refactor/foundation-contract-hardening`; Bevy `85503941e488152711a4695bbe248e9ed9f453fc`;
Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

## Lane Map

- Mode: `research_spec`.
- Repository: the current shared checkout; no additional worktrees for read-only research.
- Constraints: all researchers are read-only; no Cargo, formatting, commits or external writes.
  Preserve concurrent changes and treat `repo-ref/` as read-only. Existing A01-A09 findings are
  context, not findings to rediscover. Missing future features are not current defects.
- Concurrency: at most three child agents plus the root; dispatch further lanes as slots become free.
- Root ownership: synthesis, evidence adjudication, report/HTML edits and document verification.
  No researcher may edit `AGENTS.md`, ADRs, code or reports.
- Expected output: at most three substantive findings per lane, with exact source locations,
  a concrete trigger, traced failure/cost, contract scope, reference precedent, refactor direction,
  confidence and a proposed acceptance case; include sound choices and rejected hypotheses.
- Stop/escalation: conflicting contracts, unverified counterexamples or missing consumers remain
  explicit questions. Source size and crate count alone do not establish a defect. Do not invent
  compatibility guarantees or a generic framework to make the architecture appear complete.

| Lane | Research ownership | Verification focus |
| --- | --- | --- |
| J01 | Task pools, async handles, cancellation, backpressure and retirement | Admission through terminal publication, overload, reentrancy and physical ownership |
| J02 | ECS components, persistent apply and lifecycle effects | Safe API transactions, hook restrictions, native reconciliation and upgrade coupling |
| J03 | Reflection, schema, migrations and catalog admission | Version/owner lineage, value semantics, bounds and generic authoring consistency |
| J04 | Plugins, groups, product recipes and extension packaging | Configuration identity, dependency closure, lifecycle and external-module usability |
| J05 | Managed runtime, time, scheduling and faults | Pause/step/debt, cleanup, task integration and complete owner transitions |
| J06 | Editor documents, transactions, undo and persistence | Stale actions, unavailable data, save races, multi-document ownership |
| J07 | Input timelines, actions and gameplay commands | Fixed-tick delivery, focus/capture, overflow, replay and pause behavior |
| J08 | Runtime UI, text, fonts and semantics | Projection/interaction identity, layout/cache invalidation and resource ownership |
| J09 | Filesystem capabilities, project admission and export | Source races, byte/identity agreement, cleanup, budget and output authority |
| J10 | Asset storage, reload and watcher integration | State transitions, direct consumers, stale completions and dependency recovery |
| J11 | Rendering, views, materials and backend resources | Snapshot completeness, target/device lifetime, failure behavior and extension cost |
| J12 | Module graph, feature cohorts and external production workflows | Public API closure, packaging/iteration and independently consumed modules |

The root separately checks cross-domain identity/hierarchy/diagnostic integration and challenges
findings across lane boundaries rather than repeating each researcher's search.

## Execution

1. Dispatch focused read-only lanes and retain their source-backed results.
2. Deduplicate against A01-A09 and distinguish current hazards from triggered capabilities.
3. Reconstruct material counterexamples in the owning code; seek independent challenge for
   high-impact or disputed claims. Any execution evidence is recorded with its actual scope.
4. Produce an expanded tracked audit and update the standalone HTML outside `docs/`.
5. Validate references, report structure, HTML filters and staged scope; create a precise
   documentation commit. No runtime implementation or new qualification claim is included.

## Outcome

All twelve lanes completed. The root reconstructed material counterexamples and an independent
review challenged the high-impact findings without requiring a withdrawal. The
[expanded audit](../audits/2026-10-19-foundation-audit-expansion.md) records fourteen new items,
including three P1 recommendations, reviewed non-findings, future triggers and refactor order.
The standalone HTML remains outside the repository. Only dependency metadata and document/HTML
checks ran; no new runtime tests, benchmarks or platform qualification are claimed.

The [foundation correction plan](2026-10-19-1335-refactor-foundation-correction-plan.md) organizes subsequent implementation.
It preserves this completed research record and maps every A/B/M finding to a correction unit or
an explicit follow-up trigger; writing that plan does not implement its recommendations.
