---
type: "Work Progress"
title: "R5a scoped property identity implementation and open qualification"
description: "Scoped member lineage and keyed selectors replace the rejected component-global design; owner edits and canonical v3 metadata are implemented but final verification is pending."
timestamp: 2026-10-24T00:41:12Z
record_id: "beb7daa155f8431ebb46c3d5e3082f32"
producer_id: "codex"
run_id: "session-01a1629b-r5a"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "bddb0509947e2e40c9cf161c299b5c96ba821f5f"
---

# Summary

The foundation goal remains Active and broader than R5a. HEAD is still `bddb050`; the current
uncommitted unit implements Schema-relative property selectors, not complete source-object/
Prefab-occurrence addressing or declarative overlays. Preserve the pre-existing untracked vendor
directory. All other changed source/tests/docs in this snapshot belong to this coordinated unit.

# Details

- Independent review rejected component-global nested IDs: one identified Vec2 shape could not
  be reused under both position and velocity. The selected replacement uses sibling Record IDs
  plus the complete stable parent path. Root field IDs remain component-scoped.
- Each root field retains declaration-path member tombstones with Member/ListItem/MapEntry
  schema segments, not actual collection keys. A retired parent prefix protects descendants and
  survives intermediate Record-to-scalar changes. Registry lineage compares complete tagged paths.
- Keyed-item selectors include both `by` member ID and exact String/I64/U64 key. Changing key
  policy refuses old selectors; reusing the same historical item/map key explicitly asserts
  identity continuity, with no invented arbitrary value history.
- Reflect owns current-version read/atomic replacement. Scene owns ordered property intent,
  raw-input work admission, root coalescing and the existing prepared candidate/inverse/history.
  Tooling, project-session and Application wrappers delegate to that owner and preserve closing,
  exact document revision and Schema checks. Omitted defaults are not synthesized by the selector.
- Catalog/bundle envelopes and the fingerprint domain reset to v3. Fifteen JSON files and one
  RON golden were regenerated; a structural comparison confirms component definitions/histories
  and authored metadata are unchanged except envelope versions and predecessor fingerprint strings.
  The temporary fixture-refresh example was removed, not committed as another maintenance script.
- A correctness review found one P2: property interval failure lost safe numeric-range/root-field
  diagnostics. Reflect now reuses the registry-owned numeric-range error and Scene reuses its
  existing safe codec bridge. Scene/Prefab canary tests preserve root identity without map keys.
- The adjacent UI source-scope collision remains source-level evidence, not a runtime reproduction
  or a completed fix. It requires a separate source-provenance owner unit and ADR 0102 refreeze.

Observed verification so far:

- `cargo check --locked -p nara_reflect -p nara_scene --features serde` passed an earlier source
  snapshot; subsequent wrappers/diagnostic changes require their current checks.
- The first new 30-case property cohort passed 26 before a lineage test failed because it used an
  unkeyed list for a durable-item expectation. The fixture now uses an explicit keyed list.
- The first full serial Reflect/Scene run executed 529 tests: 528 passed, one failed, one skipped.
  The sole failure was the old hardcoded composition fingerprint after the v3 reset; its golden
  is updated, but the final rerun is still required. New scoped-lineage/property/budget/Prefab/
  numeric-diagnostic cases passed in that run. Do not cite the cohort as green yet.
- The independently compiled public `property_authoring` consumer is being verified serially in
  the shared target directory. It uses no private APIs, covers Scene/Prefab, same-root multiwrite,
  key changes, last-operation failure, stale bases, non-active editing, real Save/Reopen and
  preservation of unknown/future component records. Its source is not evidence of execution.

# Next Action

Collect the public-consumer execution and formal read-only review. Fix justified findings; run
current focused and broader cohorts, Clippy correctness and formatting serially, update owning
ADR implementation evidence, then commit precise local units. No push or full-goal completion.
Original broader plan obligations, preparation-cost comparison, field queries, source/occurrence
identity, declarative Prefab overlays and later delivery waves remain open.

# Citations

- [Design](../../../../architecture/durable-property-addressing-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- Source session: `01a1629b-6c28-7203-87c5-85328214df4a`.
- Completed read-only design lanes: `01a16b05-6640-7172-a41b-514b135d8651`,
  `01a16b1a-abed-7c80-b2e3-145d631c0386`, `01a16b3a-2493-7523-9f6a-ce4d1b2d2e5d`.
- Implementation worker `01a16b3b-72e8-76e1-823a-ddedee27c0c7` and consumer worker
  `01a16b53-1f94-7433-b6c3-b972d0774d69` completed and were closed; neither ran Cargo or committed.
- Correctness follow-up: `01a16b50-7e07-70b3-852b-1364fe2a3784`; formal review:
  `01a16b5d-24a1-7731-9ea5-8b5a8939c966`. Their launch is not a completed-review claim.
