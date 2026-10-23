---
type: "Verification Evidence"
title: "Post-boundary U5 Image-owned prepared capture qualification"
description: "Verification Evidence for Post-boundary U5 Image-owned prepared capture qualification."
timestamp: 2026-10-23T07:03:52Z
record_id: "a532b06bf3d94666bc5f1c2748052ef5"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "c1c2958"
---

# Scope

U5/R5 over `c1c2958`. The implementation commit containing this record owns the subsequent Image,
Render, UI/Sprite/WGPU, root, example and independent-consumer deltas. U6-U8 remain separate work.

# Contract and Review

Image now captures the exact identity and immutable pixels together. Managed content projects its
original admitted value; code-first capture reads actual typed storage and applicable usable state
without a managed catalog. `PreparedImages` stores complete opaque snapshots. The generic writable
prepared store/record/blanket trait/failure-status abstraction is removed after the inventory found
only Image production users. Neutral render identities and existing backend cache policy remain.

Independent UI and Sprite/WGPU workers migrated disjoint paths. An adversarial reviewer found one
missed resource accessor in the independent module fixture; it was repaired and the real fixture
subsequently passed, including a renamed-root capture. Final read-only review found no remaining
blocker in capture association, immutable packet lifetime, complete-identity batching, cache reuse
or allocation retention. Review alone did not claim executable or platform qualification.

# Executed Evidence

All commands used Rust 1.95.0, the existing lockfiles, serial Cargo and `CARGO_BUILD_JOBS=1`.
Independent workspaces explicitly reused the root target. Existing advisory warnings remain.

| Scope | Result and identity |
| --- | --- |
| Compile-fail wrong pairing | `cargo +1.95.0 test -p nara_image --doc --features render-prepare --locked`: old constructor first reproduced red because it compiled; after removal, one compile-fail passed |
| Image owner | `cargo +1.95.0 nextest run -p nara_image --lib --features render-prepare,serde --locked`: 92 passed, run `5781ad6e-bc53-471a-8426-29de3315b5a7` |
| Render/Sprite/UI/WGPU | `cargo +1.95.0 nextest run -p nara_render -p nara_sprite_render -p nara_ui_render -p nara_render_wgpu --lib --all-features --locked --test-threads=1`: 244 passed, four existing ignored, run `98d851ba-d231-40e0-9b0a-ae9c894b6a2e` |
| Independent direct and renamed-root Image consumer | `cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked -p nara-scene-module-consumer --test image_reconciliation --test-threads=1`: two passed, final run `515ae5dd-7f57-4da7-bcb0-1916adad020c` |
| Root Image Host | `cargo +1.95.0 nextest run -p nara --lib --features desktop-winit,presentation-2d,presentation-ui,render-wgpu,tooling,project-workflow --locked -E 'test(authoring_images)' --test-threads=1`: 13 passed, 283 excluded, run `093a95fb-bbf1-4311-b0a5-e033be8b0f9e` |
| Windows/DX12 Image/Canvas | `cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) \| test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)' --test-threads=1`: both actual native tests executed and passed, run `8860589c-d43b-4d8f-bcb0-1c74e751d972` |
| Independent Reference Game desktop | `cargo +1.95.0 nextest run --manifest-path reference-game/Cargo.toml --features desktop --locked --test first_wave --test asset_task_flow --test desktop_flow --test desktop_render --test-threads=1`: 38 passed, none skipped |
| Workspace compilation | `cargo +1.95.0 check --workspace --all-targets --locked`: passed |
| Scoped Clippy | Image/Render/Sprite/UI/WGPU all-feature libraries passed with `-D clippy::correctness -D clippy::suspicious`; other advisory lints remain |

The render cohort actually exercised old/new/old simultaneous pixel readback and repeated drawing
without reupload, actual GPU-holder residency, Sprite/UI shared working-set admission, UI captured
pixels and material paths. New code-first tests hold version and descriptor constant during direct
slot replacement, preserve different pixels, and retain the old capture after store/cache drop.
New UI and Sprite tests retain old packets/batches across successor publication or cache removal.
No per-frame full-pixel hashing was introduced.

# Limits

The two Image/Canvas native trajectories qualify their actual Windows/DX12 scope, not every native
Editor path, monitor scanout, accessibility, or Linux/macOS. The Reference Game cohort is selected,
not its entire suite. This unit neither qualifies recovery continuity nor claims performance gains.
U1-U4 retain their separate evidence; U6-U8 must still complete the plan.
