---
type: "Verification Evidence"
title: "Foundation U1-U13 acceptance closes with actual consumers and completed full review"
description: "Verification Evidence for Foundation U1-U13 acceptance closes with actual consumers and completed full review."
timestamp: 2026-10-15T20:22:28Z
record_id: "b09676d3fa354aa899a795a5dfe9d2ba"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7718a86"
---

# Verification

The U1-U13 implementation is assessed against the plan's finite consumer contract, not an
all-engine feature or all-platform release claim. The verified code checkpoint is `7718a86` in
the integrated worktree. Original overlapping Cargo, filesystem, Host and supervised-Play work
remains preserved outside owned-path commits. A committed-HEAD-only build is not established.

# Independent Review

Completed `ce-code-review` run `20261015-194103-d42bfd69` returns `status: complete`,
`Ready to merge`, and zero actionable findings from ten real reviewer lenses. The late-display
coverage candidate is resolved; the unsupported mandatory CLI-timeout claim was rejected with
an explicit contract/scope rationale. Merge left an empty primary batch, so no independent
validator was launched. Detailed dispositions, the 39 untracked exclusions, static-review
limits and native-platform qualifications are retained in record
`2f0fee3044664b538a2590f95e61131a`. No external-model review or independent rerun of root tests
is claimed. R1-R11 and U1-U13 have source/consumer coverage within the reviewed tracked scope.

# Acceptance Closure

| Acceptance example | Implemented owner and actual evidence |
|---|---|
| AE1: one authored game | One frozen typed composition and managed execution; a fresh custom-component project runs, keeps durable IDs across a Rust type rename, performs compiled-project Check/Repair and exports both products. Record `6a8e83bd2e48414b9573ab094aa2240e` has the actual 198-test checkpoint; later static/Reference gates cover the integrated result. |
| AE2: recoverable resource change | Published image B remains usable after C fails, stale publication is rejected, and the native image-to-WGPU trajectory preserves B. The late display-completion consumer also keeps the successor's pixels and picking authority (`114c2cfd30854192a821c22b8afd8487`). |
| AE3: native state | An independent plugin commits durable configuration on the same entity, reconciles hook-owning transient state, owns replacement/removal cleanup, reports accepted failure revisions unavailable and supports explicit retry. Eleven durable/native consumer tests and 194 reflect/Scene/ECS library tests pass (`330298b054c74303a156f0b61f02f43a`). No universal native rollback is asserted. |
| AE4: ordinary 2D | Sprite/tile depth and palette material have native pixel assertions; two views and affine mapping have actual WGPU coverage. Shared display mapping, integer canvas UI and nearest registration have focused coverage. Opt-in camera/sprite/tile interpolation, including standalone Pause -> Step -> Resume, passes 75 affected tests (`a4de05216dbf4ca286f3e1ef0e0510da`, `114c2cfd30854192a821c22b8afd8487`). |
| AE5: tool and lifecycle recovery | Base-revision multi-field edits commit once with one undo; stale concurrent edits and stale geometry are rejected through native/programmatic actions. Slow generation preparation is owned and cancellable; quiet observations do not redraw endlessly; failed close exposes retry. The real Windows canary verifies saved generations, build failure, Run Last Good, successor generation and physical cleanup (`5aa5d3c1904b478186d78dd1b88ab694`, `320ccd19841f4449b30b7d20e2ee69b0`). |

The audit's F01/F02 structural baseline and member-lifetime defects are corrected in U2. F03's
published-value/attempt confusion is corrected in U3. F04's instance-specific publication preflight
and retained cleanup ownership are corrected in U4, with actual Windows and native Linux filesystem
evidence. F05's same-size presentation configuration identity is corrected with U10. F06-F08's
close retry, quiet redraw and slow-work ownership are corrected in U12. F09/F10's parser-like
Rust proof tests, stale consumers and unsupported evidence claims are corrected in U13 through
real compile/privacy fixtures, maintained independent consumers and current evidence.

# Final Checks

- Workspace all-feature/all-target `cargo check --locked --offline` passes.
- Workspace Clippy with `-D clippy::correctness -D clippy::suspicious` passes; other existing
  style/deprecation warnings remain. The final transform delta passes its affected Clippy gate.
- `cargo fmt --all -- --check` passes. `git diff --check` and memory render/validation are checked
  again at the final documentation closure, all exit 0. Memory validation retains existing legacy
  missing-ID and absolute-path warnings; historical immutable records are preserved. No additional
  Cargo run is needed for documentation-only edits.
- Focused behavioral suites, native Windows Editor/Game View, native multi-view/material/resource
  probes, the migrated Reference journeys, independent native-domain/UI/material consumers and
  actual compiled-project tools have recorded results. This does not claim one final whole-workspace
  nextest invocation; counts from different checkpoints are not added into a fictitious total.
- The original staged binary diff remains byte-identical to the pre-work snapshot. The final local
  documentation commit names only task-owned paths; nothing is pushed or published.

# Concrete Follow-up Triggers

| Trigger | Current limit and next question |
|---|---|
| Linux/macOS desktop release | Native filesystem evidence is not native process/window release coverage. Run each platform's actual process ownership, close/retry and presentation trajectories before claiming that release. |
| Multiple content roots need the same relative locator | The current admitted database rejects collisions and requires distinct source-scoped paths. Design a source-qualified locator with the first real mounted-content workflow; do not silently remap UUIDs or add a package solver now. |
| Rename must update an already-open generic preview | Identity survives readmission through reopen. A live Refresh needs one atomic closure/catalogue replacement policy; a mutable source index alone is insufficient. |
| Generic nested-Prefab geometry editing | Data-only admission preserves structure, but nested preview geometry has an explicit omission. Extend projection with a real editing/picking consumer before advertising that workflow. |
| A native SDK needs old-or-new activation | Configuration publication and native readiness remain separate. Select that SDK's prepare/activate/cleanup protocol rather than promising rollback of arbitrary effects. |
| CLI tooling needs automation cancellation or a service deadline | Current trusted synchronous Cargo/project-tool invocation bounds output bytes, not elapsed time. Establish cancellation/deadline semantics and retained process/reader cleanup with that concrete caller. |
| Runtime reparenting needs interpolation conformance | Parent-history reset is implemented; a dynamic reparent/removal consumer remains a focused coverage improvement, not a proven production defect. |

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- docs/verification/2026-10-15-foundation-correctness-audit.md
- docs/architecture/adr/implementation-status.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T202150Z-full-foundation-review-completes-with-resolved-display-coverage-and-explicit-platform-limits-2f0fee3044664b538a2590f95e61131a.md
- docs/knowledge/engineering/2026-10/2026-10-15T163819Z-compiled-project-tooling-and-host-integration-pass-all-198-focused-tests-6a8e83bd2e48414b9573ab094aa2240e.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T183913Z-native-domain-reconciliation-stable-content-and-toolkit-free-ui-consumers-pass-330298b054c74303a156f0b61f02f43a.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T193836Z-interpolation-native-editor-display-and-migrated-reference-workflows-pass-before-full-review-a4de05216dbf4ca286f3e1ef0e0510da.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T195046Z-final-workspace-compile-formatting-and-correctness-lint-gates-pass-after-focused-fixture-cleanup-2ec69f92ce4145b1bccd7f286981cc15.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T200246Z-paused-presentation-no-longer-rewinds-and-late-display-completion-cannot-replace-its-successor-114c2cfd30854192a821c22b8afd8487.md
