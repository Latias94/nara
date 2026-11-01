---
type: "Work Progress"
title: "Research reconciliation and registry ownership qualification in progress"
description: "The supplied research matches the integrated archive; domain ownership tests and the corrected rejection oracle pass while public and native qualification remains open."
timestamp: 2026-11-01T02:23:56Z
record_id: "f5649fbb861a414a85096145c166f56f"
producer_id: "codex-foundation-main"
run_id: "registry-ownership-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-11-01-component-registry-ownership-plan.md"
git_branch: "refactor/foundation"
git_commit: "5acab02"
---

# Summary

Continue the complete active foundation Goal on `refactor/foundation`, based on `5acab02`.
The registry construction/admission replacement remains uncommitted and in qualification.
No Cargo command runs concurrently; every build uses jobs1 and the root target. The earlier
[progress](2026-11-01T015531Z-registry-construction-and-immutable-authority-replacement-in-integration-282689551434409ab234964eb2e997ab.md)
describes the implementation and original write ownership.

# Details

- Rechecked the user-supplied research ZIP without extracting its supporting snapshots. SHA-256
  remains `99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`;
  all91 manifest entries and all10 repository studies match. The existing research disposition
  and foundation plan already carry its recommendations. Faraday's independent render/UI/Editor
  review `01a194e1-ee0e-7012-acbc-e9a5e95d4529` found no uncovered expensive contract or new
  plan requirement; it is closed. Dalton's bounded content/runtime rereview remains in progress.
- Root/Editor all-feature library check passed after replacing the final inferred
  `shares_snapshot` call with exact `ptr_eq`. Reflect's all-feature nextest suite passed395/395,
  run `4d446fa5-a20a-44c7-b00c-8c1fe3e77428`. Root and eight affected independent workspaces
  were formatted; the one Reflect unused-mut warning was removed.
- The Scene/Tooling/Transform/Sprite/Tilemap/Render/UI all-feature cohort compiled and ran1219
  tests across52 binaries:1218 passed,1 failed,9 skipped, run
  `09930e63-12bf-49e3-aa1f-92c53a7ee4f4`. The failure was a new test's hardcoded empty-World
  entity count. Current Bevy World initialization itself owns resource entities. The test now
  captures the actual baseline and still requires rejection to leave membership unchanged.
  Its focused Scene-library rerun passed1/1,489 filtered, run
  `f80b393a-099d-460d-86be-bbdb65dd62b3`. No production behavior was relaxed.
- The new real data-record Building/Preloaded test passed in that cohort: actual read/set/remove
  reject before admission and original value7 remains after `App::seal`. Its first compilation
  exposed a mistaken test-only `App::finish` call, corrected to the actual consuming seal entry.
- Updated current ADR0011, affected plugin/Scene/capability/Editor references and the foundation
  overview to the selected Builder/Snapshot model. Removed obsolete successful-freeze retry and
  Frozen-wrapper instructions. Historical superseded ADR0004 and historical0034 evidence retain
  their original names. No compatibility alias or reader was introduced.
- Root/Editor all-feature libraries plus thirteen relevant public integration binaries are now
  building/running in exec58573. TEMP log:`nara-registry-owner-root-editor-tests.log`.
  No result is claimed yet. No new native, independent or minimal-feature qualification exists.

# Next Action

- Poll exec58573 before another Cargo. Resolve real failures without weakening exact native,
  owner-installation, data selection or target-World authority.
- Collect Dalton's actual-data test and content/runtime review; close the read-only lane.
- Run Reflect/Scene doctests, independent module and renamed-root consumers, relevant native
  Editor trajectories from the unit plan, minimal/wasm and affected correctness Clippy. Keep
  relevant feature unions together to avoid unnecessary rebuilds. No broad rerun is required
  solely for current documentation changes or the corrected count assertion.
- Update real evidence/ledger/current plan only after qualification, stage exact intended files,
  Conventional Commit and ff local main. Preserve vendor/references, no push/PR. Full R5 evolution,
  joint motion/execution, durable materials and the editable Nara-drawn Inspector remain open.

# Citations

- [Registry ownership unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [ADR0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
