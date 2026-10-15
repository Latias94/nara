---
type: "Verification Evidence"
title: "Foundation U3 published image availability and identity revocation"
description: "Retain usable publications through ordinary failures, revoke invalid identity and settings authority, and exercise the real desktop WGPU path."
timestamp: 2026-10-15T09:41:34Z
record_id: "868d1a8a5a8e4218b4d52b2d02f5dc7b"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Verification

Behavior changed: AssetState now records usable publication independently of the latest attempt.
Version, hashes, state revision, slot revision, and load generation remain the existing owners.
Ordinary failure/loading preserves availability; removal or identity revocation removes it.

# Result

- `cargo nextest run -p nara_asset -p nara_image --lib --features nara_image/serde,nara_image/render-prepare --locked --offline --no-fail-fast --test-threads=1`: 128 passed after the final identity-recovery correction.
- With the shared `target`, one build job, and `NARA_WGPU_FORCE_FALLBACK=1`, the Reference Game
  `desktop_render` test `desktop_product_host_prepares_and_submits_the_committed_texture` passed
  on Windows. Its startup probe fails a retry before the first Image Prepare, then requires actual
  prepared images, textured batches, GPU submission, UI glyphs, and restart continuity.
- `cargo fmt --all --check` passed. Focused rustfmt of the changed Reference Game probe passed;
  that excluded workspace has unrelated pre-existing formatting differences, which were not changed.
- Focused Clippy for hierarchy/asset/image/scene libraries passed with existing warnings; no warning
  suppression or unrelated warning cleanup was added.

# Evidence

The new retained-publication test failed before the state change because no prepared resource
existed (Nextest run `a6631f3e-14a8-493b-8b1b-1be2da1f2789`). It now covers both Loading and Failed
before first Prepare. Additional cases cover newer accepted content followed by failure, invalidation
before/after preparation, removed content, first-load failure, and existing stale/slot-ABA paths.

Read-only review found one reachable omission: after metadata identity revocation, ordinary PNG
modification could reuse retained settings and republish without fixing metadata. The strengthened
real-event test failed with Some(AssetVersion(3)) instead of None in run
`3c97f946-7c38-410f-9981-8ef1fb1600d3`. Only a usable publication now authorizes settings reuse;
MetaModified still performs fresh identity validation and can restore availability. The final
128-test run includes that failure/recovery chain. The reviewer confirmed the specific fix.

The three simplification lenses retained three small behavior-preserving corrections: reuse the
existing identity-invalidation routine, avoid cloning failed diagnostic strings when only revoking
publication, and remove duplicate affected-entity pushes in Scene retirement. Focused resolution
(3 tests) and retirement (5 tests) passed; the resource/image suite was rerun after the state change.

Malformed metadata remains an ordinary failed attempt under the existing runtime last-good policy;
identity mismatch is explicit revocation. No extra generation, artifact-cache mechanism, or universal
reload recovery framework was added. Existing CPU retention/budget accounting is unchanged.

# Follow-up

Continue U4 effect ownership and U5 execution consolidation. AssetState serialization now includes
published availability; no persisted AssetState consumer was found, and no compatibility decoder
was added for an unused runtime-state wire format. Native evidence here is Windows only.

# Citations

- `docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md` U3 / R5 / AE2
- `crates/nara_asset/src/{state,storage}.rs` and `reload/{resolution,tests_resolution}.rs`
- `crates/nara_image/src/{prepare,reload,tests}.rs` and `import/publication.rs`
- `reference-game/src/bin/desktop_render_probe.rs`
