---
type: "Verification Evidence"
title: "Post-boundary U7 borrowed and exclusive Scene consumer qualification"
description: "Verification Evidence for Post-boundary U7 borrowed and exclusive Scene consumer qualification."
timestamp: 2026-10-23T07:41:24Z
record_id: "24bb1ff684fa45c48abb28ebc9aded12"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "ae7a93c"
---

# Verification

U7/E1 adds `CurrentScene::get(&World) -> Option<CurrentSceneView<'_>>` and
`ProjectScenes::scope` in their existing root owners. The borrowed view and scheduled parameter
share private owner reads; the scope safely extracts the same optional parameters and methods.
Neither seam changes resource ownership, flushes, advances, commits or acknowledges implicitly.
The two-level game's travel, Restart and reconstruction construct owned data before mutable commit,
and no longer manually construct `SystemState` in gameplay.

Rust 1.95.0, serial Cargo with one build job, existing lockfiles and root target:

| Scope | Result |
| --- | --- |
| Root Host/startup/Scene requests | 37 passed, 263 excluded, run `f228b815-30cd-42c0-8c1f-d82acdb4eb69`; includes U7 missing-resource, cancelling-slot, owner-lifetime and scheduled/scoped predicate controls |
| Root doctests with desktop-winit,presentation-2d,presentation-ui,render-wgpu,tooling,project-workflow | One positive example and two negative borrow examples passed; a first positive example failed on an incorrect public import and was corrected to `advanced_prelude::commit_prepared_project_scene` before rerunning |
| Independent two-level game | `cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --test-threads=1`: 24 passed, none skipped, run `7d64fb97-3e40-49f2-9259-ac3a0b3346f1` |

The independent consumer actually executes public A/B travel, Restart from retained source,
checkpoint Save and fresh-process restore of two logical instances, and rejection of changed
destinations or invalid checkpoint references. The tests do not claim native presentation.

The implementation worker edited disjoint root/sample paths without Cargo. A separate read-only
adversarial reviewer found no blockers in owner privacy, weak retention, borrow escape, optional
availability, cancellation/acknowledgement or real consumer choreography. Main executed the tests.
ADRs and the product recipe guide now document borrowed/scoped access without moving ownership to
`nara_scene` or expanding the ordinary ECS prelude.

# Result

U7 is qualified within these scopes. U8 naming and the final combined integration remain required.
