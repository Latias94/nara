---
type: "Session Handoff"
title: "Post-boundary U1 to U4 delivered U5 red regression next"
description: "Goal budget stopped execution after four qualified owner units; U5 through U8 remain unfinished."
timestamp: 2026-10-23T06:19:00Z
record_id: "595eef43ad4a46f1a6ebcfd1b3c3c540"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "c1c2958"
---

# Summary

Budget-limited handoff, not completion. The user authorized the full eight-unit plan and immediate
implementation. Preserve that scope; U5-U8 remain required. Do not recreate or enlarge a goal budget
without user instruction. The current goal was automatically budget-limited.

## Delivered

- Plan: `7cc3b6f`, `docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md`.
- U3/R3: `53a62bf`, capability-owned logical write cursor and Windows qualification.
- U4/R4: `c48e45c`, post-flush persistent support facts with original asset baseline.
- U1/R1: `7fc9458`, lossless scope output/fault envelope and owned-input entry, all consumers.
- U2/R2: `7502545`, admitted disposal custody and cleanup-failure propagation through Stop/Host.
- Combined evidence/formatting: `c1c2958`. No PR or push.

Actual checks passed: App/FS211; Input/Gameplay/Transform/UI205; full-feature
root/Tasks/Reflect/Scene/Winit670; public Runtime/Editor90; independent Reference Game desktop38;
three actual Windows/DX12 critical Editor trajectories; workspace all-target check; scoped
correctness/suspicious Clippy; formatting. Existing warnings and ignored/platform limits remain.
The U1/U2 evidence shard `3f13b79a544a478b974d062c99662a12` records exact scopes and findings.
Final independent U1/U2 review found no blockers after correcting real Host bookkeeping and
terminal edit-result retention. U3/U4 independent review found no blockers.

## Current Dirty State

Only `crates/nara_image/src/prepare.rs` is modified: a compile-fail doctest for forbidden independent
identity/pixel pairing. It intentionally fails on the old public constructor. Latest command:
`cargo +1.95.0 test -p nara_image --doc --features render-prepare --locked` failed because the test
compiled successfully. This is fresh red evidence for U5, not a broken completed unit. Keep it.
Pre-existing untracked `vendor/` remains untouched. No Cargo process remained at the final poll.
No native subagents launched successfully; installed Codex CLI lanes all terminated. Never delete
unattributed work. Recheck status before resuming.

## Next Work

U5: make PreparedImageSnapshot owner-issued and opaque. Keep managed
ImageContentSnapshot::prepared_snapshot; add a code-first capture taking the actual typed store,
handle and applicable AssetStates, deriving version/slot/descriptor internally. No managed catalog
requirement for code-first Images. Introduce Image-owned complete prepared collection; migrate
Image, Sprite, UI, WGPU, root Editor, public/module fixtures and examples. Remove generic
PreparedRenderResources/record/trait if final inventory agrees: all production uses are Image;
other concrete uses in nara_render/prepare.rs are mock tests. Do not retain a writable pairing
bypass or invent generic sealed preparation framework. Keep neutral render identity vocabulary.
Update invalid-kind tests at the correct remaining seam rather than reopen constructor. Preserve
retained immutable packets, allocation lifetime, equal-capture cache reuse and real GPU readback.

U6: continue selected bounded recovery continuity, not diagnostic-only rejection. Read refined R6
and source. A read-only map is in the machine-local temporary receipt
`nara-post-boundary-u6-transition-map-20261023.md` (if absent, source/design remain authoritative).
Asset owns one per-asset continuity identity using existing revision/store identity; Image owns
pixel-free successful-removal record with exact slot/store/catalog association. Default state writes
invalidate; narrow known owner transitions preserve only from an exact eligible start. Resolver
item/byte rejection still supersedes generation and writes Failed. A Loading A superseded by rejected
B must not strand Loading or lose eligibility. Test eligibility directly with/without retained
DesiredImageRecipe, which can mask a lost receipt. External state writes then a legitimate rejection
must never wash that write; preserve same-version, slot ABA, store replacement, invalid metadata,
post-resolution interference and stale-result negatives. No writer log or second executor.

U7: actual CurrentScene/ProjectScenes owners live in root `src/startup_scene.rs` and
`src/project_host/runtime/scene_requests/api.rs`, not nara_scene. Add borrowed/scoped domain access,
migrate two-level sample; no implicit flush, resource remove/reinsert or unsafe borrowing.

U8: rename full `nara_ecs::upstream` to `nara_ecs::bevy_ecs` and root equivalent with no compatibility
alias; preserve original types, six adapted derives and provenance directory. Update ADR0002/0044,
active docs and direct/root/renamed fixtures. Historical evidence retains its observed names.

## Execution Constraints

Communicate Chinese; project artifacts English. Serial Cargo with jobs=1, Rust1.95.0 and shared
target; nextest preferred. Inspect diffs before formatting/commits, no revert of concurrent edits.
Read-only repo-ref/vendor. No new migration guide, PR, ECS replacement or speculative framework.
Native dispatch rejected arguments before launch; CLI work/review lanes were used instead. Some
CLI resume attempts produced no final output and were not counted as reviews. Bounded fresh
reviewer with a concrete brief succeeded; avoid repeating entire broad research/skill workflows.

# Verified State

# Open Threads

# Next Action

# Citations
