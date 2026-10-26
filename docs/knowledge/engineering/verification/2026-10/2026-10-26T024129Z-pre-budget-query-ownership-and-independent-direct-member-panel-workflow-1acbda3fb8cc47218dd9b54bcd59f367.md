---
type: "Verification Evidence"
title: "Pre-budget query ownership and independent direct-member panel workflow"
description: "Removed caller-ID cloning before query budget admission and qualified a separately compiled non-active Scene direct-member panel through retained edits, history and divergent-memory Save/Reopen."
timestamp: 2026-10-26T02:41:29Z
record_id: "1acbda3fb8cc47218dd9b54bcd59f367"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "fix/bounded-authoring-query-ownership"
git_commit: "1404d59"
---

# Verification

Qualifies the bounded ownership correction and actual independent consumer after `1404d59`.
The Members API and source-owned direct views remain the contract adopted at `a0d64ef`; no new
recursive query, document store, transaction type or native qualification is introduced.

# Result

- `query_document_authoring` borrows `target.projection` while collecting borrowed rows. Complete
  page/continuation metadata admission precedes ID copying. Rows become owned before moving
  `target` into the result, avoiding the earlier pre-budget clone used to satisfy a borrow error.
- Rustdoc now distinguishes inspected candidates from returned rows: wire-only members and
  non-direct tombstones consume scan work without a direct durable row.
- The separately compiled `editor-panel-consumer` discovers entity/component/root/direct-member
  targets in a non-active Scene through actual public panel queries. Its selected scalar read
  fits an eight-byte value budget while a whole-root read under that budget refuses.
- A registered panel cancels one prepared edit, retains two candidates across paints and receives
  distinct exact accepted/stale submission results. One accepted edit creates one history entry.
  Undo/Redo preserve the selected scalar and complete known root, including its large sibling.
- Save compares complete JSON, including the untouched unknown component. Then a public Undo
  deliberately makes memory differ from saved bytes; Reopen restores the saved scalar, clean
  state and empty history. Old query/read bases refuse both revision drift and fresh incarnation.
  Active document, selection, preview and the other source remain unchanged.

# Evidence

Cargo ran serially with `CARGO_BUILD_JOBS=1` and the checkout `target`:

| Scope | Actual result |
| --- | --- |
| Initial Scene/Tooling cohort | `5b6723a8-c734-4399-b212-989d57609824`: 426 passed, 4 skipped |
| Initial independent prepared-authoring binary | `e8943d1a-550c-4be1-9b00-10036e90ef43`: 8 passed, 1 skipped |
| Final Scene/Tooling after proof corrections | `7bad9f62-09d0-4381-a90a-aa909c5358f2`: 426 passed, 4 skipped |
| Final independent prepared-authoring binary | `3d3fd76e-9742-44b0-85a1-66c9c17ddad9`: 8 passed, 1 skipped |
| Scene/Tooling all-target correctness Clippy | Passed with existing ordinary warnings |

The 426-test Scene/Tooling cohort includes six separate private Prefab semantic-comparison tests.
Those do not expand this query unit's production scope or establish production Prefab overlays.
The independent eight-test binary includes the new direct-member test and seven existing public
workflows. The skipped cost probe is not new performance evidence.

Independent reviewer `01a17610-4474-7e50-8e18-0636c8a19800` found no query-ownership defect but
identified a P2 persistence proof gap: Save and Reopen initially had equal memory/disk values.
The test now changes memory via Undo before Reopen and checks restored value/clean/history state.
The optional known-root sibling proof is also added on Undo/Redo. Its separate equal-value-pin
finding belongs to the private Prefab comparison, not this API correction.
The same reviewer completed a targeted follow-up after the corrections, with no remaining
confirmed findings. Its review is static; the final execution results above are separate evidence.

# Follow-up

- This is the actual independent successor to the internal panel proof mislabeled in the
  [earlier record](2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md),
  as first corrected by the [Exit-driver record](2026-10-26T022556Z-public-exit-drivers-retain-maintenance-after-workspace-retirement-2c1f097eeed04d499d81adc33d3232dd.md).
- The consumer qualifies Scene direct Record scalar discovery, not Prefab overlays, recursive or
  collection discovery, native window/GPU interaction, throughput or full R2.
- Source inspection establishes the copy ordering; no allocator-isolation or RSS bound is claimed.
  Existing query limits model logical owned metadata and work.

# Citations

- [Read-projection design](../../../../architecture/authoring-read-projection-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- `crates/nara_tooling/src/workspace/authoring_query.rs`
- `tests/fixtures/module-consumer/editor-panel-consumer/tests/support/direct_member_authoring.rs`
