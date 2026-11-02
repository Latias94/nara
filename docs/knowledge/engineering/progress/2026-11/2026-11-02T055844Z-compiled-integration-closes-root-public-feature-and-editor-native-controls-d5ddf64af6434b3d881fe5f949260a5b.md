---
type: "Work Progress"
title: "Compiled integration closes Root public feature and Editor native controls"
description: "Root public failures replay green, runtime-2d serde tooling succeeds without project-tooling, and Editor ordinary plus selected native trajectories pass while independent consumers build."
timestamp: 2026-11-02T05:58:44Z
record_id: "d5ddf64af6434b3d881fe5f949260a5b"
producer_id: "codex-foundation-main"
run_id: "compiled-scene-review-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "080852c"
supersedes: "e8f7d20e3e2b49d5b086ce2100f1b9b2"
---

# Summary

The full foundation Goal remains active. Main and refactor/foundation remain080852c; three
reviewed corrections are already committed (349921f bootstrap, d6dccbc window interaction,
080852c upgrade retry). No push/PR. The large compiled/source/export unit, guides, CI and test-
support corrections remain uncommitted until independent/minimal/Clippy qualification. All
agents are closed. Preserve vendor and all WIP; serial Cargo/jobs1/root target/Rust1.95 continue.

# Details

The final core owner/public result remains1629/1629, run9ac76e5b-e122-4f29-9dae-2511252af739,
four preexisting skips. Its complete scope and Root/native20/20 replay are in the predecessor.
No product logic changed after that final core run. Subsequent edits are external fixture tests,
the CI contract assertion, current guides/ledger and this evidence.

Root public integration19841 is terminal100: bf058d6a-37b9-4ca2-9d8f-1802fef02227 runs476,
473 passed/3 failed/6 skipped; build514s, execution270.285s. All three were test contract drift:

- ci_policy retained module-consumer timeout45 after adding the reviewed five-workspace CPU
  lane with timeout90. Its assertion now matches90 and checks the actual new CPU/DX12 commands.
- The independent renamed-root processed-input probe compared CompiledSceneDocument directly
  with SceneDocument. It now checks the exact marker roots/entities/components/hierarchy and
  canonical local provenance, without adding cross-representation equality or a constructor.
- candidate_not_constructible retained from_expansion in its compile-fail source/snapshot. Both
  now use the actual private from_compilation and still prove the owner constructor inaccessible.

All three binaries replay8/8, run d106169c-6570-42db-8033-0fa9e90b0817; session24511 terminal0,
build1.52s and execution33.900s. This includes the actual independent renamed-root workspace tests.
Do not claim the earlier476-test command itself passed. Root fmt and whitespace checks pass after
these fixture-only corrections. No old constructor or expansion API remains in current Rust,
stderr or crate guide text; historical memory still keeps original names.

The review's missing-feature test configuration now actually compiles and runs:

```powershell
cargo +1.95.0 nextest run -p nara --no-default-features --features 'runtime-2d,serde,tooling' --locked --lib -E 'test(source_upgrade_storage::) | test(authored_upgrade::tests::proposal::publication::)' --test-threads=1
```

28/28 pass, run a23519fe-ea17-42d0-94cd-6dbe1711cdb8; session97045 terminal0, build155s,
execution3.060s. project-tooling was not enabled. Private cfg(test,runtime-core) test_host_fs
reuses the existing implementation; production dependencies and adapter owner stay unchanged.
Feature-specific unused warnings remain; there is no production-library feature failure.

Editor qualification used the same frozen source and actual DX12 hardware selection:

- First command75547 selected not-test(native_) plus three native cases with run-ignored all.
  It was overly broad and accidentally included two .NET-artifact tests.
  Run4480545d-cc2f-4673-b4e7-54d9da26adf4 reports282 passed/2 failed/36 excluded,284 run. Both failures are missing
  prerequisite environment/output directories, not product behavior. No attempt was made to
  invent artifacts or mark those optional-language trajectories qualified.
- Correct ordinary all-feature Editor library command12329 is terminal0:284/284 pass,36 existing
  ignored; run62ca9bad-3ef0-4f64-852e-a3c7051007d2. Build0.87s, execution49.984s.
- Exact run-ignored-only three-command trajectory34722 is terminal0:3/3 pass,317 unrelated
  excluded; run2112e267-8550-40b3-85c3-2e566767602c, execution9.578s. The cases are
  native_recursive_inspector_drafts_round_trip_scene_and_prefab,
  native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab, and
  native_retained_scene_receipt_survives_prefab_navigation_and_bound_close.

Those native tests use actual hidden-window production presentation and ordinary source/history/
persistence paths. They are not full critical/native release qualification, custom-drawn editable
frontend parity, monitor scanout or other-platform evidence. The first Editor command's selection
mistake was explained and corrected without modifying product tests or loosening prerequisites.

The Scene crate README and Runtime cooking guide now describe the actual compile/compiled
Interface. The ledger separates final core qualification from unfinished whole-product integration
and complete CP2. Existing source-only observation and actual Prefab declaration meanings remain.
CI YAML and document links are validated in the previous record; remote CI has not run.

# Next Action

First poll Cargo71048. It runs the independent tests/fixtures/module-consumer workspace nextest
with --locked --workspace, jobs1/test-threads1 and absolute CARGO_TARGET_DIR pointing to the root
target. TEMP/nara-compiled-independent-module-consumers-tests.log. It is currently compiling its
fresh feature union; no result yet. All earlier sessions are terminal and there are no open agents.
Other baozi Cargo work may exist and must not be interrupted.

Fix real independent compile/test failures at the appropriate fixture or owner, preserving exact
semantics. Then qualify ReferenceGame and affected independent render/content/behavior consumers,
doc compile-fail tests, minimal Scene/Root without serde and all-target correctness Clippy. Execute
the new CI consumer commands locally where supported; YAML checks are not test execution. Format
independent workspace changes with their own manifests after checking diffs and freezing writers.
Use the same root target and serial Cargo throughout.

Do not rerun unrelated already green broad cohorts without an affected change or unresolved concern.
After the complete compiled/source consumer gates pass, precisely commit and local ff main, with
no push. Root binding, source placements, occurrence correspondence, actual extraction publication/
recovery remain unfinished; full foundation Goal stays active. Do not mistake this prerequisite
or the three reviewed correction commits for complete CP2.

# Citations

- [Current CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Foundation follow-up disposition](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Final core and correction qualification](../../verification/2026-11/2026-11-02T053411Z-compiled-scene-owners-and-reviewed-transition-corrections-pass-current-public-cohort-e8f7d20e3e2b49d5b086ce2100f1b9b2.md)
- [Original supplied review](../../../../audits/2026-11-02-refactor-follow-up-review.md)
