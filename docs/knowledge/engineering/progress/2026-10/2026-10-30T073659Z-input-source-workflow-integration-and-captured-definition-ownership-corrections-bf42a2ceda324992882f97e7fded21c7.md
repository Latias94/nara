---
type: "Work Progress"
title: "Input source workflow integration and captured definition ownership corrections"
description: "Source13 and predecessor root1124 pass; original owner projection and Authoring read admission corrected; final physical consumer and native qualification pending."
timestamp: 2026-10-30T07:36:59Z
record_id: "bf42a2ceda324992882f97e7fded21c7"
status: "in-progress"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-definition-and-source-admission-plan.md"
git_branch: "refactor/foundation"
git_commit: "6ef3cb0"
---

# Summary

All Input Rust changes remain uncommitted on `refactor/foundation`, based on local main `6ef3cb0`.
The full foundation Goal remains active. All worker/reviewer agents for this unit are closed;
there are no remaining write reservations. Final integration is not qualified or ready to commit.
Preserve the original untracked `vendor/` and unrelated source changes. Do not push or create a PR.

# Details

This succeeds [the initial Input capture](2026-10-30T063815Z-input-definition-ownership-replacement-and-source-admission-in-progress-041a86ea9f3743889211e31710ef740b.md).
The initial plain-data owner design was tightened after a concrete safe-Rust counterexample:
an `AsRef<ActionMapDefinition>` owner can select between two definitions using an atomic flag.
Calling that projection on every borrow could change installed context cardinality without a
mapping replacement. The final design captures the private compiled allocation once, retaining
the actual owner alongside it. `ActionMapDefinition` is a move-only wrapper around private
`Arc<CompiledActionMap>`; its Arc and sharing operations are not public. `ActionMapBindings`
alone clones that private view and original owner. `shares_owner_with` checks both allocation and
owner identity. No arbitrary keepalive accompanies a borrowed-definition sharing API. Cost
admission includes the compiled allocation, wrapper, bindings and control metadata.

`replace_bindings` now returns `()`: a compiled definition is already within the hard context
maximum and every state has a fixed maximum-context allowance. Rejected candidate compilation
leaves current state/lifetimes untouched; no impossible installation error is retained. Live
`ButtonInput` and `SemanticActions` also lose Clone. No actual consumer required duplicating
their private queued/held lifetimes. Copy observations/outcomes explicitly instead.

Source capture/Host integration now includes actual files, primary identity, optional sidecar
agreement, independent original reservations, pre-Startup selection, input closure receipts and
travel reuse of the original owner. Actual Source Check and relocation Export tests are present.
The independent module consumer adds five normalized physical keyboard/mouse, semantic gameplay,
context cancellation, A-to-B-to-A, rejection/restart, Ready capture, code-first and explicit-empty
conflict controls. Its source is complete but has not executed yet.

Authoring startup capture now includes Input and optional metadata facts before the global
catalogue/source-fact freeze. Ordinary Scene/Prefab capture does not activate global Input.
It temporarily compiles through the same format owner and drops the definition, retaining only
actual source observations and identity. Editor external Play maps the new explicit source kind
to content digest and wire tag 9; existing child checks consume these facts.

Independent Authoring review exposed a late read-memory reservation. Full decoder/buffer and
source fact/path storage now admit before copy/open/read, with matching sidecar handling and
explicit record scratch admission. Preserving the original file ceiling in the shared reader
also preserves aggregate-budget versus single-file-too-large diagnostics. New regressions test
no encoded/file counter growth on low memory and one-short aggregate reads for file and sidecar.
The bounded rereview closed both P2s. Root all-features compilation also exposed the new error
variants missing from Editor Scene-capture diagnostics; explicit mappings have been added.

Actual verification:

| Scope | Result | Evidence |
| --- | --- | --- |
| Initial Input owner | 108 passed | `b528a85d-8641-46c7-8011-92650a7fa5f0` |
| Project projection | 79 passed | `b8628feb-f7b8-43b5-8838-287e15133ea8` |
| Predecessor Source/Host/projection | 76 passed | `9b215ba8-9fc3-4238-8e6f-3fbbe91e5a03` |
| Input/Gameplay/UI/Winit after infallible replacement | 313 passed, 4 existing skipped | `e755ea71-0810-421c-8fab-8f822ab2cd6c` |
| Same after live-resource Clone removal | 313 passed, 4 existing skipped | `d61642b4-06d9-421d-9d56-5ba460191fc7` |
| Actual Input files and Check | 13 passed | `4ac88b46-7cb6-46a0-88f1-ec56e2634d14` |
| Broad root library plus four integration binaries | 1124 passed, 1 failed, 4 existing skipped | `7f457b0a-417d-4a60-899e-c183dcd148cb` |
| Input minimal all-target check | passed | `nara-input-minimal-check.log` |
| Owner all-feature/all-target correctness Clippy | passed, existing warnings | `nara-input-owner-clippy.log` |

The broad root failure is one new test comparing session-bound FileIdentity values from two
separately created root capabilities. The fixture now opens the expected file through the
snapshot's retained original root; production identity checks stay strict. The broad run also
precedes the last aggregate-attribution test and the captured-definition correction. Final
coverage must rerun the affected scopes. No complete green root claim is made.

The first Source13 compile sampled an active writer's missing module and was retried only after
source-ready; this transitional failure is not a product verdict. Format was run after writers
froze. Root/source, Reference Game, samples, module consumer, behavior-language and runtime-runner
locked dependencies now include Input's actual serde Asset/JSON dependencies where selected.
Only root target is used with one Cargo job. Other machine Cargo processes belong to other
workspaces and must not be killed.

# Next Action

1. Poll the only current parent Cargo session `35566`, Input/Gameplay/UI/Winit after captured
   definition ownership; log `nara-input-captured-owner-tests.log` in TEMP. No root Cargo is active.
2. Run final all-feature root focused new Input/Authoring source facts, actual Source13,
   Source/travel and source Export checks. Fix meaningful failures; no need to repeat unaffected
   broad root cases merely for documentation closure. Keep scope/evidence honest.
3. Run independent `source_input_workflow` from module-consumer with `--locked` and explicit root
   target. Verify Reference Game, samples, relevant independent feature/public consumers and
   workspace all-feature/all-target check. Run the affected native DX12 trajectories explicitly.
4. Final code/ADR/guide/diff review, format, memory/ledger update and exact Conventional Commit;
   ff local main only after complete source qualification. Then processed Input remains ordered
   work. Do not mark the full foundation Goal complete.

The existing cross-frame endpoint/contribution growth finding now has concrete R10 admission and
regression criteria in the Input plan. The fixed context/revision allowance does not claim whole
physical-pipeline boundedness. Render graph, target build and the custom-drawn Editor remain open.

# Citations

- [Input source plan](../../../../plans/2026-10-30-input-definition-and-source-admission-plan.md)
- [Input guide](../../../../guides/input-actions.md)
- [Owning Input ADR](../../../../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
- [Delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
