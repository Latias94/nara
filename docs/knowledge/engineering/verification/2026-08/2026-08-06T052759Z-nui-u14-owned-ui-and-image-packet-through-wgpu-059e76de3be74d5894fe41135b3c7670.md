---
type: "Verification Evidence"
title: "NUI-U14 owned UI and image packet through WGPU"
description: "Verifies immutable resource-complete UI/image packets, stable render ordering, closed content/backend outcomes, captured image and glyph ownership, and desktop GPU readback."
timestamp: 2026-08-06T05:27:59Z
record_id: "059e76de3be74d5894fe41135b3c7670"
producer_id: "codex-root"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commits: "c1226ec, 4a79e00"
verified_by: "codex-root"
---

# Verification

NUI-U14 closes the transfer boundary after NUI-U13 and NUI-U5B. The render backend accepts only
owned immutable packet resources and returns the closed presentation outcome/receipt path. The
reference product state matrix remains an NUI-U15 responsibility.

# Result

Passed. UI packets retain exact frame/surface/environment revisions, immutable prepared image
snapshots, and bounded glyph-atlas page snapshots. Sprite and tilemap extraction consume completed
structural order rather than Entity/query order. Content rejection and upload deferral remain
bounded outcomes while permanent backend unavailability is a typed backend fault. Fully clipped
primitives are omitted instead of degrading to an unclipped draw, Sprite batches have one image
identity authority, and a rejected candidate keeps the prior last-good paint packet. Real WGPU
tests submit captured image and glyph pixels and produce nonblank readback.

# Evidence

- `cargo nextest run --locked -p nara_ui_render -p nara_render_wgpu --all-features --test-threads=1`: 51 passed, 0 failed.
- `cargo nextest run --locked -p nara_image -p nara_sprite_render -p nara_ui_render -p nara_render_wgpu --all-features --test-threads=1`: 141 passed, 0 failed.
- `cargo nextest run --locked -p nara_ui_render -p nara_render_wgpu -p nara_sprite_render --all-features --test-threads=1`: 76 passed, 0 failed after the architecture/correctness review fixes.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --features desktop --test desktop_render --test-threads=1`: 6 passed, 0 failed, including the real product Host WGPU submit/present probe.
- `crates/nara_ui_render/src/{packet,atlas,compiler,integration}.rs` own the immutable packet, bounded atlas, compiler, and presentation-route integration; the deleted extract/queue/types surface has no compiled consumer.
- `crates/nara_image/src/{asset,prepare,reload}.rs` publishes captured prepared-image snapshots; WGPU UI/image paths do not recover bytes from live `Assets<ImageAsset>` during submission.
- `crates/nara_sprite_render/src/{extract,queue,types}.rs` and `crates/nara_render_wgpu/src/{sprite,ui,backend}.rs` preserve stable structural order and consume captured packet resources.
- Readback evidence is direct: `glyph_packet_renders_nonblank_gpu_pixels`, `image_packet_renders_the_captured_snapshot_pixels`, and `desktop_product_host_prepares_and_submits_the_committed_texture` all passed.
- Failure containment evidence is direct: `rejected_candidate_keeps_the_last_good_packet`, `packet_clip_outside_the_viewport_culls_the_primitive`, `unavailable_backend_rejects_an_admitted_frame`, `rejected_content_skips_one_frame_without_poisoning_the_backend`, `upload_deferral_skips_one_frame_without_poisoning_the_backend`, stale-generation admission, atlas saturation/deferral, and device/surface retirement tests all passed.

# Follow-up

NUI-U17 must first deepen runtime UI lifecycle ownership: bind the exact behavior snapshot to the
presented frame/receipt, make arbitration fail-closed, publish pause-safe bounded intents, and make
route ownership explicit. NUI-U15 remains downstream product proof. Do not widen U14 with a
generic render graph, public batch stage, or speculative upload scheduler.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u14-publish-owned-ui-and-image-packets-through-wgpu`
- `docs/architecture/reference-game-ui-product-slice.md#unit-boundary-decision`
- `docs/architecture/adr/0105-closed-first-party-render-plan.md`
- `docs/architecture/adr/implementation-status.md`
