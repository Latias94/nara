---
type: "Verification Evidence"
title: "Editor U5 independent Scene and Game views pass native concurrent presentation close reopen and paused refocus"
description: "Verification Evidence for Editor U5 independent Scene and Game views pass native concurrent presentation close reopen and paused refocus."
timestamp: 2026-10-16T09:41:18Z
record_id: "ad10a59ba57e4fe7b45a5176d3311a33"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "527a135"
---

# Verification

U5 under ADR 0118, integrated after committed U1-U4 (`527a135`). Root owned all Cargo,
formatting and integration; workers owned disjoint GPU, frontend and native Host paths.
Rust 1.95, serial Cargo, `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`, shared `target`.

# Result

Independent Authoring and admitted embedded Game views pass semantic, native Windows presentation
and focused review gates. Each view has its own non-reused identity, target, producer admission,
texture registration, displayed mapping, focus and final retirement. One Host clock owns shared
cache/budget age and frame accounting. External process Play remains a separate native window.

This closes U5 only. Dear ImGui qualification, delegated secondary windows, complete replacement
parity and committed-source external acceptance remain U6-U8. No native Linux/macOS, detached
monitor, IME or accessibility acceptance is inferred from this result.

# Evidence

| Gate | Observed result |
|---|---|
| Initial GPU behavioral proof | `85f233a2-c42c-4792-8307-500c42c61ab4`: two intended failures, cross-producer stale admission and reset submission count |
| Initial actual dual-texture proof | `f9e0fa05-03b0-4121-9dda-b3561d69ebae`: Scene sampled, Game omitted at 720 by 520 |
| GPU execution focused / full | `bfe54d6b-35f1-4abc-a88b-9154bcc809f4`: 12 passed; `6707516a-40a0-4c29-ab32-2e3275565aef`: 100 passed, including existing real GPU readback |
| Final view metadata cleanup | `7e3b6608-fc56-408f-9a9b-127fa2e356c4`: 1025 retained records instead of one; fix verified in 13 focused passes, `cb0d3690-a333-4ade-a4f5-6abdff7d236f` |
| Tooling frontend | `480d6628-45ad-4957-a2cd-779141c8e6b4`: 35 passed; final floating-window hit tests included in the final owner suite |
| Editor and existing product contracts | `50a7a548-a9d8-468c-a6cb-5d8d4cdc7f7c`: 125 passed across library and six affected integration targets |
| ProjectHost real producers | `6f5b3c54-be88-4bf4-bebe-de9a3bbac24c`: exact native activation test strengthened with simultaneous Running Scene/Game candidates, unchanged Scene extent after Game resize and preview after Game hiding; passed without production changes |
| Final three-owner libraries | `a5169ebb-251a-4e2e-81b7-75b9364b4aee`: 237 passed, zero skipped |
| Independent panel consumer | `63744f49-a0d6-4608-b7ac-9f8ae82f7b10`: real separately compiled contribution selection/action/close/reopen test passed |
| Workspace all-feature/all-target compilation | Passed again after final focus follow-up (exec `5242`); workspace, Reference Game and module-consumer formatting checks also passed |
| Lint | Scoped `cargo clippy -p nara-editor -p nara_render_wgpu -p nara_tooling_egui --all-features --all-targets --no-deps` completed with warnings. Workspace `-D warnings` failed in unchanged `nara_app` large retained errors/type complexity/collapsible branches; no lint-clean claim |

Native command:

`cargo +1.95 run --manifest-path reference-game/Cargo.toml --locked --features editor-game-view-canary --bin editor_game_view_canary`

Final Windows run (exec session `14881`) exited zero and published
`nara-reference-game.editor-game-view-v3`: nine exact concurrent native window frames,
Scene view ID 1, closed Game view ID 2, reopened Game view ID 3, resized Game geometry 3,
1120 by 403 target, 130 sprite instances, Runtime UI present, positive input and Step deltas.
The existing trajectory also completed UI selection, continuous preview/commit, structure creation
and physical Host shutdown. Three hidden redraws checked that only Game's target/registration and
capture retired while Running continued and Scene retained its view identity. Game resize preserved
Scene's target and target generation.

The native paused-refocus regression failed before the fix (exec `8914`): releasing to the editor
cleared still-valid paused image input authority. The owner now releases capture while preserving
that valid authority; a later ordinary paused redraw can recapture without a fabricated Runtime
frame. Both exec `63178` and final exec `14881` passed the expanded trajectory.

The first expanded canary exposed an invalid test assumption: after correct close/release, a fresh
paused key cannot change velocity in an exact FixedUpdate-only Step. Source tracing confirmed
input/action translation belongs to First/PreUpdate. The canary now observes fresh Running input
after reopening before Pause/resize/Step. Production exact-step semantics were not changed, and
strict positive rendered displacement assertions remain intact. Temporary diagnostics were removed.

Three behavior-preserving simplification reviews completed: reuse and quality each supplied one
applied finding (canonical edit gating and redundant Game notice); efficiency found no worthwhile
change for a two-view workload. Focused GPU correctness found none. Host correctness findings
(paused recapture and new image authority while editor widgets hold focus) and the adversarial
Floating Inspector hit-through finding were fixed and independently rechecked as resolved.
The actual egui Floating Window characterization `dfe68661-a332-4fbc-8ca8-85a17b518c4e` proves a
point can be inside Game geometry but owned by an upper layer; final `owns_pointer` checks both.
These are focused reviews, not the full-goal `ce-code-review` completion receipt.

# Follow-up

Proceed to U6 only after committing this integrated unit. Use the actual Nara textures and the
same GPU lineage for Dear ImGui qualification; retain exact per-image/window samples and explicit
delegated shutdown. Preserve the existing production frontend until the adoption gate passes.
Native Inspector text entry is not separately claimed here: toolkit tests exercise actual TextEdit
focus/typing, while this native canary exercises input ownership, recapture and presentation.

# Citations

- [Active plan](../../../../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md), U5, R6-R7 and AE4.
- [Accepted ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md).
- [Initial U5 behavioral failures](2026-10-16T090440Z-editor-u5-confirms-singleton-admission-clocks-and-exclusive-viewport-painting-regressions-before-implementation-9768f10fccbc427f8433518ba9500a0c.md).
- `editor/src/platform/{views,viewport,game_input,canary}.rs`, `crates/nara_render_wgpu/src/editor_execution.rs`.
