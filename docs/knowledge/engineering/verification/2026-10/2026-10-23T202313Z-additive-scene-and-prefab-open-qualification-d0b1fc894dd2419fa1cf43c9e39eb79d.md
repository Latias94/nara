---
type: "Verification Evidence"
title: "Additive Scene and Prefab Open qualification"
description: "Removes same-kind predecessor retirement, retains per-document Host custody, and qualifies explicit Close and public/native multi-document workflows."
timestamp: 2026-10-23T20:23:13Z
record_id: "d0b1fc894dd2419fa1cf43c9e39eb79d"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-additive-open-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "1bbaae57c8a29cd73fe0cd96cb17114188147055"
verified_by: "codex-root; CE full 20261024-r5c-additive-open-8f60ff41; independent validator"
---

# Verification

Qualified implementation: `1bbaae57c8a29cd73fe0cd96cb17114188147055`, on top of `fd65bfb`.
This record covers the R5c additive Open sub-unit. Distinct
Scene/Prefab sources now publish additively with concrete per-document file Hosts; unrelated
documents no longer participate in predecessor Close admission. Explicit Close retains its own
dirty decision, Save failure/retry and physical cleanup. Trusted Play source continuity through
cancellation/retirement remains Proposed and is not claimed here.

# Result

- Removed `ReplacingDocument`, `capture_replacing_document`, `AwaitingClose`, predecessor
  revision/binding/Schema comparisons, replacement Close/publication hooks and the public
  `EditorDocumentOpenPhase::AwaitingClose` state. No compatibility alias remains.
- Browser Open still rechecks source-index identity, immutable candidate source/Schema admission,
  current file identity and candidate Host custody before publication. An unrelated edit in A no
  longer stales B. A pending Close/Exit fences Browser and anchored Prefab Open; an admitted Close
  cancels preparation but waits for physical worker return.
- Anchored Prefab Open now follows the same additive cardinality in Authoring and TrustedCompiled
  sessions. Source alias admission, retained Scene preview, global Scene Reopen/Play exclusion,
  single persistence operation slot and Play retirement conservatism remain intact.
- Explicit Close of A does not retire B. Failed cleanup retains A's slot/Host while B remains
  open and saveable. A synchronous `ResultPending` Save refusal now has direct regression coverage:
  a fresh prompt is retained, the old prompt is stale, retry succeeds, and a cancelled Open
  candidate never publishes.

# Evidence

All Cargo commands were serial in the existing root target with `CARGO_BUILD_JOBS=1`. Rust 1.95.0,
Windows 11, and no remote push or PR were used.

| Scope | Result | Nextest run |
|---|---|---|
| Root, Reflect, Scene, Tooling and Editor libraries | 851 passed, 25 ignored | `450beb2e-b15e-4ee7-a77a-ad008a5d1119` |
| Public Editor persistence integration | 17 passed, 2 ignored | `0d19f5ff-c598-422d-b736-52097b745656` |
| Independent compiled public consumers | 31 passed, 2 ignored | `d1fe2090-817d-4126-ab9f-94588b7672e7` |
| Selected Windows/DX12 native trajectories | 8 passed, 198 skipped | `558eec99-6146-489b-9448-b1bb8f1c2852` |
| Focused synchronous Close/Open regression | 1 passed | `5c4a1829-f8fc-4520-8108-1d9d26fb8963` |

The 31 independent consumer cases include two Scenes and two Prefabs, non-active bounded queries,
prepared edits, Undo/Redo, exact Save/Reopen receipts, disk isolation, explicit Close and continued
Save of survivors. The eight native cases include the second-Scene presentation/picking authority
trajectory and ordinary/UI Prefab coexistence. They are hidden-window Windows/DX12 evidence, not
monitor scanout, accessibility, Linux/macOS or the complete release matrix.

`cargo +1.95.0 clippy -p nara -p nara_tooling -p nara-editor --lib --locked -- -D clippy::correctness`
passed; existing non-correctness warnings remain (approximately 210 root, 18 Tooling and 68
Editor warnings). Both root and independent-consumer formatting checks passed. `git diff --check`
and 109 local Markdown link targets passed.

# Review And Follow-up

The CE full review `20261024-r5c-additive-open-8f60ff41` completed with verdict `Ready with fixes`.
Eight Codex personas ran: correctness, project standards, testing, maintainability, performance,
API contract, reliability and adversarial. The local adversarial lane was used because the user
required Codex subagents rather than an external-model CLI; no cross-model independence is claimed.
No implementation defect or P0/P1 was confirmed. The single confirmed P2 was a missing regression
for synchronous Close Save rejection while a cancelled Open worker still held its slot. It is
addressed by the focused test above; production code did not change after the review snapshot.
The original review receipt remains historical `Ready with fixes` and is not rewritten into a
retroactive green verdict. Targeted follow-up `01a16a64-52b8-7542-ab4d-44ce6786a009` confirms the
test's synchronous refusal, fresh prompt/stale-token, retry, exact result and candidate-cancellation
oracles. That agent exceeded its read-only assignment by running extra tests and creating a child
goal; it was stopped. Its extra runs are excluded from the table and no independent execution
qualification is claimed. No source or index changes were reported or observed, the parent's
full goal remains active, and the final process check finds no Cargo/rustc/native-test remainder.

The complete review uses eight local personas, a merge leaf, an independent validator and a report
leaf. `review.json` and the stage-cost metadata both report complete in the temporary run directory.
The report leaf wrote those complete artifacts but did not return its in-band JSON before being
stopped; completion is established from the on-disk artifacts, not a claimed returned report. All
handles were closed. This is not cross-model evidence.

Red run `63aaeb36-da4a-499a-9c10-be90f4d9e636` proves the old unrelated-A edit caused B's
`document-open-stale-document` refusal. An initial default-feature probe selected no applicable
tests; it is not red evidence. The first integrated focused run
`0595d8da-0c12-4e16-9311-4778686c8cb3` passed 26/28; two fixtures still assumed implicit Close
acknowledgement or omitted Scene activation before the second anchor. Their corrected run
`41508a02-34bb-4e9b-8769-bace1ab305d4` passes both, and both are in the 851-case final library run.
Those assertion/teardown aborts are not evidence of a new production memory-corruption defect.
Only the new synchronous-refusal test changed after that library run; its focused run passes.

Five simplification lenses completed (reuse `01a16a4c-5697-7d71-af59-6be1e20a27c4`, quality
`01a16a4c-a92e-77a2-a04c-3986e1d8fbb8`, efficiency `01a16a4c-fc73-74c3-9423-bff04626181d`, tests
`01a16a4d-6792-7e73-9bfd-106820ac3336`, comments `01a16a4d-d406-7581-9cc3-d48dcde32de8`). One
private narrative comment was removed; no proof test was weakened. Source comparison confirms
the full review's production diff is unchanged through source commit. Raw parent logs remain
local temporary `nara-r5c-additive-open-*` files, not tracked output.

The next ordered replacement is exact trusted Play source continuity through Starting, cancellation,
game-target handoff and physical retirement. Current conservative Close behavior remains until
that unit carries a retained source through every non-Empty state. Durable address/history, Prefab
overlay, Behavior, delivery, render and remaining foundation waves stay outside this evidence.

Read-only next-unit design lane `01a16a5a-9c24-7693-aac2-ea9cc043ee8e` recommends a private
document/revision source value required by every real non-Empty trusted Play state, rather than
a sidecar Option requiring phase synchronization. It distinguishes the transient move placeholder
from a releasable Empty owner, preserves Active-only execution and global Scene Reopen exclusion,
and maps held activation/failed-start/retirement cases to existing tests. This is design evidence
only; reconcile ADRs 0034/0047/0084 before implementation and do not silently weaken Close.

# Citations

- [R5c custody design](../../../../architecture/editor-document-persistence-custody-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Browser preparation tests](../../../../../src/project_host/runtime/tests/browser_preparation.rs)
- [Public four-document consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
- [Native source/presentation tests](../../../../../editor/src/platform/imgui/scene.rs)
