---
type: "Verification Evidence"
title: "Editor U3 generic composition and U4 trusted panel workspaces pass owner and native regression gates"
description: "Verified uncommitted U3 U4 candidate with full affected owner suites, feature-off UI preservation, trusted panel consumer, focused reviews and Windows generic-open canary."
timestamp: 2026-10-16T08:48:08Z
record_id: "25b42156b4834a40b67d01eb54671485"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a99595d"
---

# Verification

U3 and U4 working candidate based on `a99595d`, before separate local commits.
Root ran Cargo serially with Rust 1.95, one build job, incremental disabled and the existing target directory.

# Result

Generic Scene/Prefab composition and trusted panel workspaces pass their owner and Windows baseline gates.
The full goal remains active; U5-U8 and complete frontend adoption/parity are not established by this record.

# Evidence

| Check | Result |
| --- | --- |
| Scene/tooling/egui adapter all-feature/all-target suites | 246 passed, `8e6d9bdf-a0e3-4e09-bbba-c83a4eedac6b`; after simplification, 246 passed again, `55c8a72a-3168-4a7d-b949-cf5f4ace35f5` |
| Editor library, application, generic recovery, composition, layout, contributions and actual scrolling | 124 passed, `8576e085-c912-4b8b-9444-00fc0f884587` |
| Root all-feature library plus Scene session/component integration | 229 passed, one existing ignored test, `f2466b30-ef6f-4937-96dd-6a92f5babb3f`; includes trybuild consumers |
| Native Prefab Inspector focused regression | Passed, `b0851ae1-e7bf-49ed-a659-67d539e61a96` |
| Native raw-patch UI-domain transition | Passed, `bd7bc001-52fc-4aab-a2ac-fce43936a350` |
| UI preservation without runtime-ui, using tooling/runtime-2d/serde | Passed, `5ea2deba-bcb2-4c32-ba52-88fdf2daacaf` |
| Gesture accepted-owner and real cancelled continuous-draft regressions | Two passed, `968bab33-3cb1-4c3c-ad27-153516aa36ec` |
| Nonzero batch index/whole-batch rejection and three-size presented-texture tessellation | Two passed, `c78cdfa9-9f33-46ce-9f7e-f93f5592c32e` |
| Independently compiled trusted panel consumer | Passed, `f90ea65c-d424-4576-8b9f-495799d78541`; cold build 5m17s, execution 0.175s |
| Independent module-consumer workspace clippy/all-targets with -D warnings | Passed; compile-only workflow forwarding functions explicitly expect the foreign large-error lint to preserve their exact public result type |
| Collection-game scalar creation consumer | Four passed, `512c96ca-f23a-4d84-9865-81cb045b5fc8`; bundle exported by its real compiled recipe |
| Final workspace all-feature/all-target check after source gate and formatting | Passed in 1m14s |
| Workspace/module-consumer/sample cargo fmt and git diff --check | Passed |
| Windows native generic-open v2 canary | Passed with result ok after repairing the Scene/Prefab presentation source gate |

The generic composition integration opens a real data project whose Rust deliberately cannot compile.
Scene and Prefab support empty creation, exact-template custom addition, scalar editing, removal, Undo/Redo, Save/Reopen, no-template/duplicate rejection and complete unavailable-manifest preservation.
A separate trusted compiled trajectory opens an ordinary Prefab through source navigation, edits through the shared Inspector and saves its actual source.
The independent panel consumer observes real selection, emits an existing action, preserves dirty/history facts across close/reopen/duplicate and paints through the same callback registry as built-ins.

## Reproduced findings and dispositions

- Generic creation hit the old field-only transaction (`fbc43276`), then passed through the existing document/history owner (`1f694bcf`).
- Prepending changed unavailable semantic placement without changing the old manifest (`50aa2784`). The canonical manifest now retains direct and nested patch placement using stable First/After and parent/root facts; append and inverse remain valid.
- Native Prefab fields dispatched to the Scene-only owner (`43f6264a`); native routing now uses actual document kind and existing Prefab patch/history.
- A native ordinary Prefab acquired UiRoot without UiNode through raw insertion (`7738f5c4`). The action owner now checks existing UI identity and UI records introduced by commands, drafts and nested patches.
- Without runtime-ui, scalar data admission made a retained UiRoot removable (`fd8fd0a0`). Finite stable UI identity recognition is now feature-independent; an absent specialized implementation still rejects structure changes.
- The old native source-opening path rejected ordinary Prefabs as not UI (`88c26397`). That obsolete restriction was removed; existing UI save/reopen coverage remains.
- Registered painters lost hierarchy/Inspector scrolling (`53fc6d27`), restored Project layouts disagreed with Inspector mode (`450b212a`), a rejected second Begin replaced the accepted owner (`8e46c6d3`), and a cancelled continuous draft reappeared as 40 over source 0 (`e644ad1b`). Per-instance scrolling, layout-derived routing, exact batch outcome confirmation and owner-local draft retirement resolve these findings.
- The native canary exposed old Scene candidates being prepared after the shared active model changed to Prefab. The Host now uses explicit document kind for Scene extent, candidate selection and retirement. Exact GPU sampling checks were preserved; the same canary passed. Temporary diagnostics were removed.
- New tests exposed asynchronous persistence/close helper assumptions; helpers now acknowledge results and drive actual terminal close before assertions. Post-panic platform aborts were not treated as independent memory-corruption findings.

Final focused U3 adversarial and U4 correctness reviews report no remaining confirmed findings.
These are unit-focused reviews, not a full ce-code-review receipt for the goal.
Three simplification reviewers completed: duplicate viewport painting/editing expressions were consolidated, restored-subtree placement borrows records instead of cloning payloads, and stable UI matching no longer allocates temporary IDs.
An additional domain cache was not introduced: this slice retains one bounded scan instead of adding invalidation state before measurement requires it.

# Follow-up

Commit U3 and U4 separately, then implement U5 from the committed panel/view boundary.
U5 still needs independent GPU admission/Host frame clocks and exact multi-image composites from research record `9ab9c93b8d954dd5abb5bef8fe960f3b`.
Layout/off-screen restoration has semantic/JSON evidence only; floating panels remain within one native window.
No detached-window, monitor movement, Dear ImGui, accessibility parity or native Linux/macOS acceptance is claimed.
U8 still requires fresh committed-source external acceptance with strict Check, gameplay, supervised Play repair and export.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `editor/tests/generic_component_composition.rs`
- `editor/tests/registered_panel_interactions.rs`
- `crates/nara_tooling/tests/native_prefab_inspector.rs`
- `tests/editor_ui_domain_feature_off.rs`
- `module-consumer/editor-panel-consumer/tests/contribution.rs`
