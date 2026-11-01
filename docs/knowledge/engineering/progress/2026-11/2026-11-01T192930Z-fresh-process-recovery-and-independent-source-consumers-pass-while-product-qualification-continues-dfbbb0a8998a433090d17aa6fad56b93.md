---
type: "Work Progress"
title: "Fresh-process recovery and independent source consumers pass while product qualification continues"
description: "Normal and six subprocess upgrade controls pass22, Root public consumers100 and independent module consumers49; Reference Game159 is running with its actual desktop build."
timestamp: 2026-11-01T19:29:30Z
record_id: "dfbbb0a8998a433090d17aa6fad56b93"
producer_id: "codex-foundation-main"
run_id: "authored-upgrade-final-integration-20261102"
related_plan: "docs/plans/2026-11-01-authored-schema-upgrade-plan.md"
supersedes: "95571bffbb114d509693a5c6cab03243"
---

# Summary

The full foundation Goal remains active. The complete authored Schema upgrade is still WIP on
`refactor/foundation` at `ed21b230`, with no commit/ff/push. Source code remains the frozen version
that passed final Root392/Application35 after the three independently reviewed handoff fixes.
Subsequent changes are research/plan/memory documentation only. Actual process and independent
source consumer qualification now passes; Reference, feature/lint and affected native remain.

# Details

The independently built CLI uses only project-workflow-cli. Its normal dependency graph contains
no nara_tooling or Editor. Final build session6332 succeeded in40.94s from root target. Independent
behavior-languages run `a2111ce4-52a2-44f8-8996-59077ad2f759` passes22/22, one intentionally excluded
child_entry. It executes16 normal conversion/live/v2 controls and all six real subprocess recovery
tests with NARA_AUTHORED_UPGRADE_RECOVERY_EXE set to that freshly built CLI. The test subprocesses
invoke the excluded entry themselves. No recovery case was skipped for a missing executable.
All-old, mixed, all-new, external/metadata/recovery conflicts, Ready-before-ack and recovered-v2
Check/Runtime/Play therefore have final-source process evidence. This is process-interruption
qualification, not power-loss or cross-file atomicity.

Root public run `f1216b03-2c9e-4c48-b86d-2cd379d6cf08` passes100/100 across eight binaries, with
three existing ignored nested-Cargo trajectories excluded. It covers plugin/project composition,
runtime configuration, source boot, UI-feature-off, workflow Check/CLI/creation. The excluded
cases are fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay,
compiled_project_check_uses_custom_schema_without_starting_gameplay, and
legacy_tool_entry_and_failed_bootstrap_never_launch_or_publish. They are rejection/bootstrap
controls, not compatibility readers. A first command had mistakenly named a support file as a
test target and exited before compilation; the corrected selection above is the actual result.

Independent module run `43318acf-cc69-4ead-ba26-67353dbc48bc` passes49/49 across ten binaries,
one existing ignored control excluded. It covers component inventory, Scene Data components,
property/nested Schema, Editor Prefab conversion, source context/Scene references/Input,
manual Native codecs and complete conversion. Its4m30 build reused root target with jobs1 and
its own locked dependency graph; test execution took4.579s. Legitimate standalone Native Session
APIs remain supported. No source change followed these checks.

All subagents are closed. Bacon's separate CP2 study is recorded in
docs/research/2026-11-02-prefab-extraction-and-occurrence-continuity.md, with26 checked local links.
The foundation and authored-upgrade plans link it only as a separate successor. It identifies
the instance-root and cross-boundary SceneLocal representation gates and forbids fabricating a
Schema successor to reuse CP1 topology-blind authority. Anchor-preserving child extraction is
an experiment, not a frozen restriction on the ordinary branch-to-Prefab workflow. No CP2 code
or adopted representation is claimed. Pinned Godot/Bevy remain unchanged/read-only.

Local branches are already only main and refactor/foundation, both at ed21b230. One worktree is
attached to refactor/foundation. No branch deletion was needed. Original untracked vendor remains
untouched. No extra target, artifact removal or compression helper was introduced.

# Next Action

Poll serial Cargo session50577, log TEMP/nara-authored-upgrade-final-reference-game-tests.log.
It builds/runs reference-game with features editor, --lib --tests, locked/root target/jobs1.
Compilation completed in10m30; run `7828fdff-22cb-48cf-8232-39afd7da6f16` selected159 tests across32
binaries. It is still executing, including its actual nested desktop build with explicit target
x86_64-pc-windows-msvc; do not launch another Cargo while that consumer is active. An unrelated
Baozi tools/consumers/assets Cargo may also run and must not be stopped.

Then finish affected Root+Editor all-feature/all-target correctness Clippy, root minimal check,
independent formatting, and selected hidden Windows/DX12 qualification under
docs/verification/native-editor-qualification.md. Planned native selection: the three critical
trajectories, retained Scene/Prefab navigation, UI Canvas retained-close, complete Prefab-to-local
Save/Reopen and stock Scene effects. Record real platform limits. Consider the one ignored public
fresh-custom-project Check/Run/Repair/Export consumer if needed for final source/bootstrap coverage.

Only after those final results: create honest complete-unit verification evidence, update ledger,
guide/selected plan status, inspect exact intended changes, Conventional Commit and local ff main.
Do not push/PR or narrow/complete the full foundation Goal. CP2, joint motion/execution, material
generations and editable Nara-drawn Editor remain unfinished. No permission question is pending.

# Citations

- [Previous final handoffs](2026-11-01T190332Z-exact-captured-predecessors-and-retained-task-inputs-pass-final-root-and-application-handoffs-95571bffbb114d509693a5c6cab03243.md)
- [Complete unit](../../../../plans/2026-11-01-authored-schema-upgrade-plan.md)
- [CP2 study](../../../../research/2026-11-02-prefab-extraction-and-occurrence-continuity.md)
- TEMP logs: nara-authored-upgrade-final-independent-process-tests.log,
  nara-authored-upgrade-final-root-public-consumers.log,
  nara-authored-upgrade-final-module-consumers.log,
  nara-authored-upgrade-final-reference-game-tests.log.
