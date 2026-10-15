---
type: "Verification Evidence"
title: "Compiled custom project Check Run Repair Export passes"
description: "One compiled custom schema and migration passes fresh bootstrap and native export; rename extension and remaining U6 consumer checks are pending."
timestamp: 2026-10-15T16:03:15Z
record_id: "0454868035504e1cb6a6b75e6dc841c6"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
---

# Verification

Serial Cargo, one build job, reused target directory. The ignored compiled-project integration
selection ran three actual consumers, including a fresh generated custom-schema project and native
desktop export acceptance. This is evidence for U6/AE1, not completion of U1-U13.

# Result

Nextest run 941e87b9-9c74-434d-a92e-c7ef6ab43064: three passed, four skipped, 1053.815 seconds.

- Reference Game Check succeeds with its registered custom schema without the headless gameplay
  startup marker. This corrects red run 6391a4d1-564f-4066-85ad-e4c8703e7e87 (StartupContent).
- A generated project initially has no Cargo.lock. Check builds its own headless entry, creates
  the lock, and accepts a custom durable component with a v1-to-v2 migration without running
  startup systems. Ordinary headless Run then observes the migrated value 42.
- Renaming its startup Scene and using the existing Repair command succeeds without gameplay.
- Export uses the same definition, builds the selected headless/desktop binaries, and obtains
  two exact product observations, including the native desktop first-present path. The generated
  candidate retains the repaired Scene and both executables; the custom system observes value 42.
- A real legacy executable that ignores tool argv is not launched when the fixed Cargo metadata
  declaration is missing or unsupported. A declared but uncompilable entry fails bootstrap.
  No gameplay marker or candidate destination is published in these failure cases.

# Evidence

The fresh consumer embeds the same Rust contribution used to derive its authoring bundle. It does
not maintain a second schema registration list. Existing closure/build-input/product agreement
remains inside the compiled project process; the parent receives summaries only. Cargo's first
release build accounts for most elapsed time; this was not parallel Cargo or a timed-out process.

The source later gained an explicit Rust type rename between Check and Run. That additional step
was not in the binary verified here and is still pending. Reference Game's complete Export product
observation migration and full U6 focused tests are also pending.

# Follow-up

Recompile/run the complete native-rename trajectory, migrate Reference Game's product observations,
retain real compiler diagnostics on bootstrap failure, run appropriate focused/static checks,
and precisely commit U6-owned changes. Keep original staged SPL work outside those commits.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U6 / AE1
- tests/project_workflow_cli.rs
- tests/support/project_tooling_game.rs
- src/project_tooling/mod.rs
- src/project_tooling/bootstrap.rs
