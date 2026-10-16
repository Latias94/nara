---
type: "Verification Evidence"
title: "Editor frontend separation and pure creation templates pass semantic and native workflow gates"
description: "U1 and U2 verified through focused regressions, complete reflect and builtin suites, Editor lifecycle, external Play repair, strict starter Check and native Export; independent findings resolved."
timestamp: 2026-10-16T07:11:48Z
record_id: "8863dd8fc56d4cad847b7eb255cbd321"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "63ffe42"
---

# Verification

U1 (R4/R9/R10, application/frontend ownership) and U2 (R2/R3, pure creation metadata) on source based on `63ffe42`.
This evidence accompanies their local implementation commits. U3-U8 remain pending; no Dear ImGui, panel extension or concurrent-view acceptance is claimed.

# Result

Application and project lifecycle state no longer contains selected-toolkit types, widgets or drafts.
The retained frontend consumes semantic projections and immediately observes its own dispatch result, without a second session, history or feedback queue.
Exact data catalogs carry optional complete creation templates; missing templates preserve prior serialization/fingerprints and do not change stored-record decoding.
Transform2d, Sprite, Name and Visibility have complete templates, three owner generation-2 catalogs and generation-1 predecessors, with wire version 1 preserved.

# Evidence

All Cargo commands ran serially with Rust 1.95, one build job, incremental disabled and the existing target directory.

| Verification | Result |
| --- | --- |
| Before U1, Editor lib/application_contract/generic_open_recovery characterization | 108 passed, `ace54b7f-d559-420a-a969-c1f1ae6b0aff` |
| U1 all-target check before regression repair | Passed |
| U1 direct-feedback regression | Passed, `20582ff8-43ff-41c1-a3d9-befacf0a8fa6` |
| U2 focused metadata and scalar derive | 22 passed, `04dd33cd-44d7-4d56-91b1-2f2fe83c91bb` |
| Complete nara_reflect, all features/targets including derive UI and old catalog fixtures | 135 passed, `fbf088d0-90da-4d49-a327-e517ec2b59a9` |
| Transform/sprite/Scene built-in libraries, all features | 145 passed, `1ce26a31-3a78-4815-809f-66b00bd4fd5a` |
| Editor lib/application_contract/generic_open_recovery after migration | 109 passed, `8b5cb0f5-d34a-4860-abc5-8b220b0669c0` |
| Ordinary authoring_loop selection | One passed; two existing long tests explicitly skipped, `908eae78-ec70-48e5-bde1-ecf631a44f49` |
| Explicit ignored starter edit/Undo/Redo/Save/Reopen/strict Check/generated game/native Export | Passed in 515.453s, `afa3eb05-a024-4d95-a30b-60dafe220c18` |
| Explicit ignored first_game_workflow on a disposable external collect-game copy | Passed in 295.560s, `85a7ce7c-0ab9-4fcd-b7d5-4c37bd1fcaa6`; custom scalar edit, image reimport/assignment, Save/Reopen, native supervised Play, injected compiler error and repair |
| Updated independently locked renamed runner | Five passed, `9ded6493-10a2-4bb8-96a3-8cb79e1f214f` |
| Final workspace all-feature/all-target check after formatting | Passed in 2m11s |
| cargo fmt --all and git diff --check | Passed |

The native Windows generic-open v2 canary passed after the frontend separation and catalog refresh: Project Manager, Recovery, degraded editing, Scene/Prefab browser, generation refresh, last-good retention and successful exit.
Native Linux/macOS behavior is not established by these Windows runs.
The disposable external copy used this working source and is U1/U2 integration evidence; U8 still requires its separate committed-source acceptance.

## Reproduced review findings and dispositions

- U1 correctness reviewer found a pending latest-only action-feedback slot could overwrite A with B before redraw. Root observed the precise originating-revision mismatch (`bc0a7361-3ca9-4e01-97a6-27603c20fe10`). The slot was removed: each dispatch returns its own semantic report and the frontend observes it before driving the application. The corrected regression cleans up the session before assertions. Final reviewer findings are empty.
- U2 adversarial reviewer found float range inference from type names disagreed with actual aliases. Root observed the valid wide alias being wrongly rejected; generation now emits an inline const finite assertion using the actual Rust field type.
- The reviewer found integer-shaped float suffixes bypassed that assertion. Root observed the negative case unexpectedly compiling (`cd8746d3-dad5-4a14-b1cf-829cf0e41a1a`). Literal syntax classification now covers f32/f64 suffixes; the real compiler E0080 snapshots prove both overflow paths reject before catalog construction. Final adversarial findings are empty.
- New template API and opt-in derive tests were written and observed red before production changes. Canonical full-field validation is an explicit mode within the existing owner; ordinary decoding retains its previous mode. No project callback, native Default invocation or alternative validator was introduced.
- The ignored starter test exposed its pre-existing version-1 observation assertion against the current version-2 product protocol. The test now verifies version 2 plus the checked schema fingerprint, authoring-bundle digest and closure-plan digest. Production protocol behavior was unchanged.
- Reuse, quality and efficiency simplification reviewers all completed with no worthwhile behavior-preserving changes. They did not run Cargo or edit files.

## Fixture conversion

Reference-game and collect-game bundles were exported through their own compiled recipes.
The three canonical built-in owner records were copied into image-import and renamed-runner fixtures; every other owner was asserted unchanged, including the renamed custom owner.
The known-unbound fixture also refreshes only its native Name/Visibility owner so the existing exact trusted adapter remains usable; its unsupported studio.signal owner and source records are unchanged.
No catalog equality, source preservation, strict Runtime agreement or unknown-record assertion was weakened.
Old-format and no-template fingerprint fixtures remain in the reflection suites.

# Follow-up

Commit U2 and U1 separately, then activate U3 generic Scene/Prefab composition and U4 panel/workspace ownership from their committed prerequisites.
The full goal remains active. Native multi-view/window and Dear ImGui qualification are later gates.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `docs/migrations/2026-10-editor-foundation.md`
- `crates/nara_reflect/tests/creation_templates.rs`
- `editor/tests/authoring_loop.rs`
- `editor/tests/first_game_workflow.rs`
