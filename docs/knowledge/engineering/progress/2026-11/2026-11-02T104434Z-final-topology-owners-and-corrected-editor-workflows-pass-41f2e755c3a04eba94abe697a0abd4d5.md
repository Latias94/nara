---
type: "Work Progress"
title: "Final topology owners and corrected Editor workflows pass"
description: "Work Progress for Final topology owners and corrected Editor workflows pass."
timestamp: 2026-11-02T10:44:34Z
record_id: "41f2e755c3a04eba94abe697a0abd4d5"
producer_id: "codex-foundation-main"
run_id: "root-binding-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "dcbbb93"
---

# Summary

Continue the active complete foundation Goal. Local main and `refactor/foundation` remain at
`dcbbb93`; the topology prerequisite is still uncommitted. This successor corrects the earlier
Application triage and records fresh passing owners. Complete extraction, correspondence and
project publication/recovery remain unfinished.

# Details

- Final Scene/Tooling/UI all-feature owner/public run **1,270 passed / four existing skips**,
  run `49674beb-1f0f-4524-b743-3d7d9ff92cfb`, build6m44s/run63.620s. This includes the final
  `KnownParent` category correction, both mixed Root/projected membership controls, borrowed
  root lookup allocation observation and positive namespace/unknown-parent controls.
- Corrected Editor cohort **16/16**, with337 tests filtered out, run
  `50fd9818-9158-456f-ab1c-e51e6abc6f12`, build2m03s/run5.423s. It covers both complete manifest
  frontend corrections, generic Scene/Prefab composition, the real construction/preview/Play
  lifecycle, initial quiescent repaint and exact startup-Host retirement.
- The preceding broad Root/Editor result remains **2,271/2,280 with nine failures and50 skips**,
  run `9b867669-9e4f-4e6f-9385-5f92749e97f6`. Do not relabel it a passing broad result. Its actual
  Transform2d and clean-room supervised Play paths passed. Corrected failure-path results and
  their source revisions supply separate scope.

The follow-up three-Application diagnostic run `02c89c65-81c0-48fa-8437-81925b5442ed` initially
failed all three. It established:

1. The initial source capture is finite background work. The repaint test now admits an actual
   preview extent, waits for an accepted candidate and retirement to settle, and proves source
   revision/dirty/selection did not change during initialization before measuring an empty
   repaint. It explicitly closes the Application.
2. The old authoring-loop caller attempted raw component creation inside a Published generic
   document. The refusal is correct. It now uses the public construction owner, admitted
   Transform/Sprite templates and six declared property writes, one prepare/commit and the
   actual generated ID throughout preview, selection and Save/Reopen. Geometry/tint remain
   independently asserted; fixed raw payload helpers were removed.
3. The broad worker gate was not specific to Open Host. Three observed I/O workers included
   Session workers; the gate is taken by the first exiting worker, not the first created one.
   It could correctly prevent Session physical Complete. A read-only lifecycle review ruled
   out treating the guard in `finish_session_retirement` as this failure's root cause.
   The faulty duplicate public test was consolidated into the existing exact startup-owner
   test. Its helper injects only the startup submission, outside subsequent Session creation.
   Explicit no-frame/held-Host assertions and stable completed-advance assertions now run for
   manager/recovery/opening/authoring. Actual exit and `joined_workers == 1` remain required.
   Independent rereview confirmed equivalent/stronger coverage and no unique invariant loss.
   Production Application close ordering was not changed.

All agents are closed and writers frozen. Root plus four independent workspaces were formatted:
module consumer, Reference Game, Collect Game and Two Level Game. Further owner/API/native
qualification remains outstanding.

Current serial Cargo session **78642** is the Root correction cohort: all features, lib plus
`editor_persistence`, `scene_product_public_surface`, `project_content_boundary` and
`root_bound_scene`, filtered to those binaries and `independent_capture::`.
Log: `TEMP/nara-root-placement-root-corrections.log`. Poll it before another Cargo.
The first compile found the parent fixture incorrectly reading private Session workspace; the
next found that public tab models do not contain detailed selection/Inspector state. The final
fixture now uses actual public ComposeScene/CreateEntity, obtains its ID through the active
Scene model, and validates that accepted ID in saved source and reopened Inspector. No new
public raw read or bypass was introduced. Its current compile/execution result is pending.

Final core and Editor logs use `nara-root-placement-final-owner-tests.log` and
`nara-root-placement-editor-corrections.log`. Last F: free space was about96 GiB. No cleanup,
push, PR, vendor access or reference-checkout change was performed.

# Next Action

Finish the Root correction cohort, then the independent module workspace (including the new
coherent mixed-owner edit/history/JSON-RON/native-spawn consumer), Reference Game, both samples,
actual behavior-language/cooked delivery and affected Windows/DX12 structure/conversion/Canvas
paths. Complete minimal/no-serde, rustdoc negatives, correctness Clippy and final formatting.
Retain exact tests rather than weakening capacity, complete manifest or lifetime assertions.
Record fresh results/limits, commit the complete topology prerequisite precisely, fast-forward
local main and continue full CP2 extraction/publication/recovery. The complete Goal stays active.

# Citations

- [Complete CP2 unit](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Previous triage](2026-11-02T101534Z-topology-independent-review-corrections-and-root-editor-qualification-triage-56612e614a7c4a768d3d322bd4fe6361.md)
- [Infrastructure disposition](../../../../research/2026-11-02-infrastructure-comparison-reconciliation.md)
