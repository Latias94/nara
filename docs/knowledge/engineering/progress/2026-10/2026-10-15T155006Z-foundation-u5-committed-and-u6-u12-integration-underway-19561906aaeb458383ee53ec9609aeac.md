---
type: "Work Progress"
title: "Foundation U5 committed and U6 U12 integration underway"
description: "U5 consumer closure is verified; compiled project tools and responsive supervised Play are under implementation."
timestamp: 2026-10-15T15:50:06Z
record_id: "19561906aaeb458383ee53ec9609aeac"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
---

# Summary

U5 owned paths are committed at 25c29de after independent-consumer/native-example verification.
Original staged SPL changes remain byte-identical to the prework snapshot. U6 and U12 are being
implemented against the integrated worktree; neither is complete.

# Details

- U6's real Reference Game Check failed before production changes with StartupContent, run
  6391a4d1-564f-4066-85ad-e4c8703e7e87. The CLI used an empty recipe. Compiled project tools now reuse
  the project's definition and retain strict Check/Export/Repair authority in its process. Root
  all-features/all-targets check passed; real consumer run 941e87b9-9c74-434d-a92e-c7ef6ab43064 is
  still active at capture time, including a real first build of a fresh custom-schema starter.
- U6 includes a fixed Cargo metadata protocol declaration because an old headless entry can ignore
  unfamiliar argv and start gameplay. Missing/unsupported declaration is rejected before launch;
  trusted native code is not sandboxed. No source parser is introduced.
- U12 Host observation regression failed as intended (0fe7038c-afa7-4ff7-b847-0f7b026f0bc6); identical
  output/state must not report a visible change or erase the last operation result. The source fix
  is pending integrated recheck; an attempted run encountered U6's then-incomplete module edits.
- U12's locked-UI regression reached the intended failure: clicking Retry close emitted no action,
  run recorded in the session tool output and the temporary close-ui log. The test initially had
  an incorrect test output accessor, corrected before observing this behavior failure.
- Current U12 edits make close retry reachable while ordinary authoring stays locked, suppress
  unchanged owner/Host observations, preserve frames on quiet effect wakes, move generation
  publication into a private cancellable owned thread, and move launch revalidation into the
  existing process spawn worker. The new publication worker has not yet been compiled or tested.
  Its controlled held-worker test must prove cancellation/close retain the physical store until
  join; it adds no general filesystem job service. Cancellation is cooperative across bounded
  copy chunks and filesystem phases; a blocking OS sync cannot be forcefully cancelled.
- Normal closure polls/collects incrementally. An abandoned worker during error/unwind uses
  synchronous fail-safe cancellation/join instead of detaching work; that exceptional Drop path
  supplies no physical-close receipt. This is a limitation to retain in final verification.

# Next Action

Collect the live U6 consumer run before any other Cargo command. Fix its real failures, then
compile and verify U12 Editor/Host/UI changes serially. U6's worker owns project_tooling/product/
CLI/starter/Reference game files; root owns Editor supervised Play and its Host/UI consumers.
Preserve the original index. U7-U11/U13 and the full review still remain.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T120302Z-foundation-u5-independent-consumers-and-native-examples-verified-dd39a41fb70c462bacd772d8fd26b752.md
- tests/project_workflow_cli.rs
- editor/src/supervised_play/preparation.rs
- editor/src/supervised_play/owner.rs
- src/project_host/runtime/tests.rs
