---
type: "Work Progress"
title: "Bounded Input owner replacement and prepared UI integration in progress"
description: "Finite topology, shared terminal credits, read-only projections and migrated adapters pass initial owning tests; UI interaction-basis and public/native integration remain open."
timestamp: 2026-10-30T11:39:20Z
record_id: "ecdeb5811f044c95a0653ac744b58002"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md"
git_branch: "refactor/foundation"
git_commit: "c624673"
---

# Summary

The complete foundation Goal remains active. This is the in-progress R10 Input owner replacement
after research integration `c624673`; local main and the long-lived branch start here. All production
replacement and consumer changes described below remain uncommitted. Do not reduce the Goal to
this slice or infer native, packaged delivery, render-graph or Editor completion.

# Details

The old timeline implementation and its inline tests were replaced, not kept behind aliases.
`timeline.rs` now names private `owner.rs`, `topology.rs`, `capacity.rs`, `types.rs` and tests.
Fixed boxed endpoint/pointer/member slots retain checked private identity. Explicit connected
raw IDs and a once-connected synthetic default feed one ordered log and shared terminal credits.
Every accepted button edge retains its original press sequence and origin. Original press ownership
survives release and physical cancellation. All button edges, including unmapped releases, finish
semantic resolution before member recycling. Frame copies additionally pin their actual records
until projection/observation buffers clear; early log reclamation cannot admit another frame's
worth of copies in the same frame.

`ButtonInput` and PointerState are read-only projections. Direct button writers, independent driver
ports, parallel synthetic action queues, old button errors/constants and route poison are removed.
Semantic contribution sets use admitted member IDs and the actual M*K ceiling. The single-log state
allowance replaces the former three-log accounting; Input event capacity has the existing canonical
4,096 ceiling. Native Winit connects before callbacks; Editor Game View connects once per bridge
Runtime in plugin finish. Source/processed fixtures retain original content owners and use explicit
connection. Root facade stale exports and Gameplay tests are migrated.

Prepared Input tokens retain an immutable declaration plus owner-side invalidation. Any same-endpoint
reservation, including invalidated, blocks a second preparation until settled. Platform revocation
allocates no route generation. PointerLeft invalidates prepared UI state without revoking the
existing window keyboard route. Unused geometry credits refund when their actual demand ends.
Each endpoint remembers its last published binding generation so an old exact stamp cannot retire
a reconnected raw label before the replacement publishes its first route. A private exact read
`prepared_route_invalidation` lets UI check its own state before publishing a new route.

The UI worker first implemented a total `UiPresentationCommit`, then source review exposed an
additional real counterexample: ordinary Release, outside-content movement and accessibility input
can change live UI while prepared authority remains valid in Input. Current candidate invalidation
does not retract that authority. The worker is adding a UI-owned interaction basis and typed
UI invalidation causes, checked before Input route commit, with actual regressions. This is still
active implementation. Root already handles total invalidation by retaining backend pixel acceptance
but skipping new interactive receipt, observation and accessibility success. A root held-backend
test witnesses those separate outcomes; it has not yet compiled or executed.

## Executed Checks

All Cargo is serial with `CARGO_BUILD_JOBS=1`, reused root target. Temporary logs are outside Git.

- Initial `cargo +1.95.0 check -p nara_input --all-features --lib --locked`: passed, 11.72 s.
- Initial timeline test build missed Vec2 in the parent's fixture import; fixed before execution.
- Timeline run `de272b96-f4d7-40be-9206-75e4fec4fd79`: 37/37 passed, 28 unselected, 6.44 s build,
  2.303 s execution. All five predecessor regressions are green; historical churn now performs
  4,100 explicit endpoint/pointer retire/reuse iterations and asserts fixed pool sizes.
- Full Input run `cd7315c1-8af4-453a-b1d3-30193c30a322`: 140/140 passed, zero skipped,
  20.28 s build, 4.788 s execution. This precedes the later exact-reservation review fixes.
- Sartre's independent source review found P1 invalidated reservation overwrite and P2 stranded
  geometry credit. Both owning fixes and actual regression tests are added. Timeline follow-up
  `3b0e4419-04be-4192-afbb-7920ff30a6ac`: 39/39 passed, 28 unselected, 3.34 s build, 2.311 s run.
- Gameplay/Winit all-feature run `a3d337de-8f00-47d6-9c1e-f0b32afe57f5`: 102 passed, four skipped,
  1m22s build, 5.063 s run. This is unit/adapter evidence, not physical-window qualification.
- Root no-default `project-workflow-cli,tooling,presentation-2d` lib check passed, 9.73 s,
  with existing feature-specific dead-code warnings.

The latest meaningful new Input change after that follow-up is the exact read-only prepared-token
invalidation query; actual broad/public acceptance is still required. No all-workspace, final Clippy,
neutral UI, reference-product or actual native successor qualification is claimed yet. No format pass
has run for the production replacement while writers remain active.

## Parallel Ownership

- Raman `01a18c7c-3666-7d50-91cf-b3ef8b27972f`: frozen `nara_input/lib.rs`, `semantic.rs`,
  `projection_tests.rs`; parent integrated frame pin/API and removed facade/state leftovers.
- Epicurus `01a18c7b-fc5e-7a90-8ad8-d92e64ce068a`: active UI interaction-basis successor and
  regressions in Ui runtime/state/adapter files, assigned manual/gallery/reference presenters;
  may migrate interaction-lab and one-time UiTestApp connection. Parent owns root bridge.
- Pasteur `01a18c81-2bb1-75b1-9260-21338262f990`: migrated/froze timeline tests and two Input
  integration tests; now writes only NEW independent `module-consumer/tests/bounded_input_workflow.rs`.
- Sartre `01a18c81-c4ca-70f0-9c8e-1570f9188a05`: native/GameView/source/processed migrations frozen;
  completed read-only topology review with the two corrected findings above.

# Next Action

Poll the actual independent source consumer Cargo session `52185` (root shared target, log
`nara-bounded-input-independent-source-tests.log`) before starting another Cargo command; it was
confirmed live when this draft was written. Never infer termination from silence or timeout.
Collect UI source-ready, review its basis reset coverage and compile/run owning UI tests. Complete
root all-feature checks and root bridge/Editor consumers, the independent bounded gameplay workflow,
existing source/processed fixtures, neutral presenters/reference game and relevant actual native
qualification. Capture exact failures and fix owning contracts; do not weaken new red assertions.
Inspect diff, freeze all writers and format, then independently review and commit the complete unit.
Do not push or alter vendor/reference checkouts. No known permissions blocker exists.

# Citations

- [Endpoint plan](../../../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Previous research/model evidence](2026-10-30T105046Z-engine-research-reconciliation-and-bounded-input-terminal-comparison-84d7a4b9cbdf44419a23d851f92b9a7d.md)
