---
type: "Work Progress"
title: "Processed Runtime Scene and long-horizon render design integration"
description: "Records source-independent Scene integration, initial public verification, active root qualification and the graph-first render design refinement."
timestamp: 2026-10-29T05:13:05Z
record_id: "5d66c9ed79cf47238e2c6253c51652c8"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3c62307"
---

# Summary

Continue the complete active foundation Goal from `main@3c62307`; do not reduce it to this Scene
unit or rendering documentation. The processed Scene production replacement is integrated and
under final qualification. Same-base Runtime catalog/startup/later-Scene delivery remains open.
User steering prioritizes a coherent long-term architecture over minimum initial code and keeps
pinned Bevy/Godot as the primary rendering references.

# Details

`RuntimeSceneCandidate::cook` and `RuntimeSceneDocumentCandidate::from_json_bytes(...).admit(...)`
use a distinct v1 flat envelope, complete canonical provenance, deterministic framed projection
IDs, explicit defaults and Product references. Decode/admit execute neither native codecs nor
source migration. The move-only candidate carries an exact definition witness. A private closed
borrowed input reuses existing preparation, native/data publication and advanced claims/product
retry. Immutability plus exact definitions removes a redundant complete pure validation in spawn;
the shared preflight and managed World/commit guards remain. Modeled work limits do not claim to
cover all existing validation/syntax scratch or allocator memory.

Workers Bernoulli/Curie froze their disjoint files before formatting. Independent reviewer Hilbert
found a cumulative asset rewrite budget error: valid exact-size Product values need record/path
work plus decode/encode work. The fix derives that allowance rather than relaxing final value
limits. Primary/subproduct exact 6/7-node and one-short cases pass. A later added shared-prefix
32-leaf assertion still needs its final test rerun at this capture. The reviewer accepted removing
the redundant spawn gate under the immutable/exact-witness invariant. All code workers/reviewer
are closed.

Actual initial evidence: owner 15 tests passed (`18737dd5-771f-4d31-9815-40858a3d69c6`);
full Scene 778 passed, three default skips, 37 binaries (`19e63d87-514d-4bc5-83da-6e9f8d6397ba`,
build 4m16s/run45.514s); independent processed/preparation/data consumers 18 passed across three
binaries (`7537e62f-db7c-4a78-8be5-953cfea6b5a7`, build1m/run1.338s). Earlier processed-only four
also passed. No final native/Clippy/Reference Game result is claimed yet.

Root run `ca265069-224f-4cd3-aaad-8567220b9b85` is live at capture, session `22686`: all-feature
library plus six content/startup/Play integration binaries, 997 selected tests/five default skips,
build4m31s. Its command queues the no-default Scene check afterward; await the original handle,
do not start another Cargo. Logs are temporary `nara-runtime-scene-root-tests.log`,
`nara-runtime-scene-full-tests.log`, `nara-runtime-scene-independent-tests.log`. Cargo stays serial,
`CARGO_BUILD_JOBS=1`, existing root target; the independent workspace uses that same target.

Rendering review `01a185f5-5ff7-7733-b66e-730a4456d340` was dispatched with requested
`gpt-6-astra`/`max`, researched the pinned sources, then performed a bounded long-horizon follow-up
and coherence review. Report and Proposed ADR/plan retain the initial audit separately from the
parent's refinement: explicit finite resource graph + typed Features + runner-owned Host as the
preferred full-workflow design; slots lower into the same graph. Design declaration-order-independent
dependencies, Texture/Buffer access, history imports/advance, independent streams, capabilities,
color/time/display mapping and semantic-vs-GPU material layout now. Add shared/cross-view outputs,
compute statistics/storage-buffer/custom histogram and a clipped Nara-owned drawing tracer as
falsification consumers. Optimizations are phased. Review corrected conditional graph admission,
superseded ordered-only recommendations and an accidental WESL prerequisite. R8/R9 implementation
remains not started; ADR 0121 remains Proposed. This agent is closed.

User authorized branch cleanup and one long-lived refactor branch. All 58 local heads were merged
into main and only this worktree existed. Renamed the current empty feature branch to
`refactor/foundation`; deleted 56 unused merged heads with guarded `git branch -d`, retaining main
and refactor/foundation. No unique commits, remote refs, working files or worktrees were removed.
Future units use the long-lived branch and local periodic ff main. No push/PR. Vendor untouched.
At this capture all implementation/report changes are uncommitted. Nine changed documents and
307 local link targets passed the bounded static check before this memory record.

# Next Action

Await root session, then serially format/recheck the extra nested-budget assertion, run affected
native Editor critical trajectories, correctness Clippy and selected independent/game/minimal
consumer checks. Qualify and commit precise Scene and separate rendering-doc units, capture final
evidence and render memory; ff local main and return to refactor/foundation. Continue verified-base
catalog/source-free startup/later-Scene work. Preserve full Goal, concurrent changes and vendor;
do not mark source-free delivery or rendering implemented from this checkpoint.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Processed Scene contract](../../../../architecture/adr/0006-scene-and-prefab-data-model.md)
- [Public guide](../../../../guides/persistent-components.md)
- [Rendering review](../../../../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
- [Proposed rendering contract](../../../../architecture/adr/0121-composable-render-programs-and-host-execution.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
