---
type: "Work Progress"
title: "Complete compile interface and follow-up owner transition corrections in integration"
description: "Coherent compile names and closed Scene storage are integrated; contextual conversion repro is corrected and independent retry presentation and bootstrap fixes await serial qualification."
timestamp: 2026-11-02T04:43:48Z
record_id: "84710703b83743ad9ebc067159d8b714"
producer_id: "codex-foundation-main"
run_id: "compiled-scene-review-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "a2640a10973b6d1724176a506237bc458aa5f3bc"
supersedes: "f8d1c94059364a21ac62e8b27226845a"
---

# Summary

The complete foundation Goal stays active. Main and refactor/foundation remain at a2640a1;
the compiled production replacement, naming and the new reviewed corrections are uncommitted.
The user explicitly permits handling all current changes. Preserve vendor and all WIP, do not
push or create a PR. Windows/pwsh, Rust1.95, serial Cargo/jobs1 and the root target still apply.
F has roughly342GiB free; no cleanup is needed.

# Details

The public operation is now compile, not expand_prefabs. Scene/PrefabDocument use compile and
try_compile_with_loader; mixed bindings use compile_scene and try_compile_scene_with_loader.
SceneCompilation types replace PrefabExpansion types; AuthoringSceneCompilation report/parts/
omission names and try_compile_authoring_scene_with_loader are coherent. Private observation
helpers use from_compilation. No aliases or unchecked ExpandedSceneCandidate remain. Four
previously missed fixture families (stock composition, cargo build, runtime runner and render
composition) now compile real source with actual bindings/assets before bind_source. Pauli's
five-file migration is frozen. All earlier writers and Harvey/Kepler are closed.

Harvey's independent final Native/Runtime/custody review found no assigned P1/P2 after naming.
Kepler's prior pure-core result remains scoped. Reference-work exact testing now sums all five
Reflect proof operations, preserving independent node/byte exact and one-short controls;
69937f0e-da7f-455f-a707-0ec0192d3277 passes. The latest pre-naming library607 plus this correction
covered that earlier608-test snapshot, not the new F1 code below.

The user's new external review is preserved at docs/audits/2026-11-02-refactor-follow-up-review.md.
It reviews committed a2640a1, not this WIP. The foundation plan now gives its four P2 owner
transitions explicit acceptance and corrects stale inline-reference, policy and stock-composition
status. It keeps CP2 extraction, editable Nara UI, motion/execution and rebuild/resume open.

F1 is reproduced by the new nested/masked contextual-default conversion test:
da26aaf7-232c-4787-a3dd-d88bbb9f690e fails because the prepared source lost its default map.
The correction separates source-only snapshots from flat values in private StagedSceneOutput.
Only selected conversion plans hidden defaults through the existing lexical reference/compiler
kernel; normal activation retains dormant demand semantics. ComponentReferencePrograms pairs
effective and default plans. Flat validation finishes before snapshots return to the selected
source fragment. Localization remaps both through the same source ID map. Reflect adds a
payload-relative admitted entity-reference planner using its existing selected visitor. Native
compiled data retains no contextual declarations. The new nested/same-name/masked source test
and Persistent control pass2/2, run25087441-1d66-4ff6-8ca0-2cff2d97fd5f. JSON admission, RON
reopen, classification equality and exact Undo/Redo are checked. ADR0038 and child plans describe
this source-producing contract and specific refusal for unrepresentable defaults.

F2's publication owner now uses digest plus pre-admitted random128-bit attempt identity. A new
operation has a distinct durable destination; same-operation retry keeps its original path.
The real-task test extends cancellation after recovery/before Pending into identical-proposal
successful retry, and storage still rejects even identical preexisting bytes. F3 centralizes
same-window successful replacement revocation across Runtime and present_window. An ignored
native regression covers failed paint, successful clear, another window and retained image.
F4 preserves bounded CargoBuildDecoder diagnostics through status, push and finish failures via
existing CargoBuildFailure, including caller-facing Display. Five owner tests and an ignored
real compile_error bootstrap consumer were added. Averroes/Goodall/Kant wrote disjoint paths,
froze and are closed. Banach independently found no assigned P1/P2 in F2-F4, static only, then
closed. None of these three corrections has runtime qualification yet.

Fermat01a19a8a-3ac4-7610-9917-dae83002a78d is the only open agent: read-only F1 review. It was
asked especially about hidden-default asset inventory versus fresh classification. No edits,
Cargo or fmt permission. Do not declare that review closed before its result.

CI now explicitly runs the five independent render workspaces' CPU tests, warning for a GPU-only
empty selection, and selects two window-free DX12 program/output cases on Windows with the
explicit software adapter. This does not qualify hardware/desktop cases. CI YAML parses; no
remote workflow executed. Classification representation costs remain the documented P3 owner-
locality follow-up; no unsubstantiated budget bug or universal framework is claimed.

Scene public test migration initially stopped at source export getters and then source-record
versus compiled-carrier assertions. These are corrected with actual field comparisons, without
weakening behavior. Final all-feature Scene library plus all public-test typecheck passes17.18s.
Root/Editor all-feature test check had two Editor data-carrier getter errors; they were corrected
but not rerun after F2-F4. Full root/public/external/native/minimal/Clippy qualification remains.
Root fmt was run after all writers froze; later test cleanups still need final fmt.

# Next Action

First poll Cargo session24386; do not start another Cargo while it runs. It executes all-feature
nara_scene --lib --tests nextest, jobs1/test-threads1, with log TEMP/nara-compiled-final-scene-tests.log.
Prior session14886 is terminal0 (Scene test typecheck); earlier13153 is terminal101 (fixed test
type errors). No other known task Cargo or compaction helper is active.

Collect Fermat and fix substantiated F1 findings. Then qualify Scene public results, Root/Editor
test builds, actual F2 retry and F4 owner/real bootstrap, actual F3 native presentation, owning
Tooling/UI and independent/module/Reference consumers. Run affected Editor DX12 conversion/
ordinary editing/retained Scene trajectories, minimal and all-target correctness Clippy. Do not
commit incomplete or failing cohorts; separate logical P2 fixes from the complete compiled unit
where practical, then local ff main. No push. Finish current compiled prerequisite before root/
placement/extraction implementation; full CP2 and foundation remain active.

# Citations

- [Current CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [External follow-up review](../../../../audits/2026-11-02-refactor-follow-up-review.md)
- [Prior compiled integration](2026-11-02T040920Z-compiled-scene-production-storage-and-native-runtime-integration-in-progress-f8d1c94059364a21ac62e8b27226845a.md)
