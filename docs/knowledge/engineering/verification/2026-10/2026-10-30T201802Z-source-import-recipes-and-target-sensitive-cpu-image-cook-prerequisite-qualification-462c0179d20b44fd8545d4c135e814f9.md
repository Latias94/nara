---
type: "Verification Evidence"
title: "Source import recipes and target-sensitive CPU Image cook prerequisite qualification"
description: "Verification Evidence for Source import recipes and target-sensitive CPU Image cook prerequisite qualification."
timestamp: 2026-10-30T20:18:02Z
record_id: "462c0179d20b44fd8545d4c135e814f9"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-local-artifact-build-plan.md"
git_branch: "refactor/foundation"
git_commit: "96f4519"
---

# Verification

Qualified the source recipe replacement and concrete CPU Image cook prerequisites of R6b from
baseline `96f4519` on Windows 11, Rust +1.95.0. CARGO_BUILD_JOBS=1, root target, serial Cargo.
The full local artifact store/build group, R6b and the complete foundation Goal remain open.

The normalized Git source state of the 39 changed present Rust files is
`88349fb25d80ba269c2473b281aa812b8ee77149c902f003e076178379b9d0ff`.
This is SHA-256 of sorted `path + NUL + git hash-object blob + newline` entries. The superseded
`crates/nara_asset/src/artifact.rs` is deleted and replaced by the source recipe module; that
deletion is tracked separately, not included as a present blob in this digest.

# Result

- Source provenance now has one six-field `ImportRecipe`: source ID/hash, dependency digest,
  importer ID/revision and effective settings hash. `ImportRecipeDigest` uses its own v1 hash
  domain. Synthetic artifact record/path and unused profile/output/label/format fields are
  removed with no compatibility alias or reader. Image source metadata stores the recipe directly.
- Asset state, storage, dependency graph, reload requests, Editor capture/hydration and native
  publication name recipe digests accurately. Actual `ImageContentHash` and prepared resource
  hashes remain independent. Source-only Settings still requires only DeserializeOwned.
- The real PNG importer still captures actual primary bytes/settings and preserves output,
  publication, physical cancellation and escaped-pixel budget custody. Existing exact budget
  tests were retained; no limit was widened to accommodate the replacement.
- Image has a pure CPU target cook with explicit Reject or proportional FitNearest. It preserves
  complete primary/auxiliary Product, format, color space and exact selected RGBA texels, owns
  fresh output and strips source provenance/old retention. It does not implicitly resize atlases.
- The cook shares Runtime format dimensional/pixel/RGBA constraints with the codec, including
  non-serde builds. Retained output and conservative two-buffer allocation overlap are checked
  before allocation. Caller-owned aggregate input/output/encoding custody remains explicit.
- A valid 4096-by-1 PNG goes through the actual ImageImporter and produces distinct valid 4K/2K
  extents, pixels and Runtime bytes under the same Product. Reject refuses the 2K target. This
  is real conversion, not target-label-only hashing or admission-only evidence.

# Evidence

| Check | Actual result |
| --- | --- |
| Asset/Image all-feature owning cohort | 328 passed, 1 existing skipped; dabee5f3-2cba-42f1-bf6b-55573dfea767 |
| Render/SpriteRender/Tilemap/UiRender all-feature cohort | 219/219 passed; 42e4334f-1df4-4eb3-95ec-03bd18053825 |
| Root lib + actual project_content_boot, project-workflow-cli/tooling/presentation-2d | 1039 passed, 4 existing skipped; 242ebabf-ce0c-4d55-8006-1ec619919d17 |
| Independent Runtime Image/TileSet delivery binaries | 22/22 passed; f12b6c68-8a1d-45f4-a917-667ac32c98fb |
| Selected actual Windows/DX12 Scene Reimport and retained Canvas trajectories | 2/2 passed; d8e79961-a5d4-485e-a11d-5b5d24ddbb44 |
| Workspace all-feature/all-target correctness Clippy | Passed; non-correctness warnings retained |
| Asset/Image non-serde minimal lib check | Passed |
| Root runtime-2d/serde minimal lib check | Passed |
| Render compile-fail doctest | 1/1 passed; recipe digest cannot be a render content hash |
| Root/independent fmt, whitespace and selected documentation links | Passed |

Image cook has seven integration oracles plus a no-allocation Runtime extent unit oracle. They
cover exact nearest-coordinate selection on both axes, one-texel long-axis fitting, no upscale,
color/alpha and RGB-under-zero-alpha preservation, anonymous refusal, primary/auxiliary IDs,
fresh pixel ownership, discarded external retention, exact/one-short costs and actual Runtime
roundtrip. The complete cohorts protect reload/state and downstream prepared data.

The Root boot consumer checks actual persisted linear settings, importer identity/revision,
source/metadata ID, primary hash, empty dependency digest and effective settings hash. Independent
consumers use real capability-backed member capture/pack/Runtime publication, Scene retirement
and escaped pixels. No private fake loader or fabricated output pathname supplies that evidence.

The two selected native tests execute production submission/presentation through hidden windows:
`native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes` and
`native_ui_canvas_priority_survives_closing_retained_scene`. They do not establish framebuffer
readback, monitor scanout, every native trajectory, Linux/macOS or GPU cook/performance.

Representative commands:

```text
cargo +1.95.0 nextest run -p nara_asset -p nara_image --all-features --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run -p nara_render -p nara_sprite_render -p nara_tilemap -p nara_ui_render --all-features --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run -p nara --no-default-features --features project-workflow-cli,tooling,presentation-2d --locked --lib --test project_content_boot --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --locked --test runtime_image_delivery --test runtime_tileset_delivery --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_ui_canvas_priority_survives_closing_retained_scene)' --test-threads=1
cargo +1.95.0 clippy --workspace --all-features --all-targets --locked -- -D clippy::correctness
cargo +1.95.0 check -p nara_asset -p nara_image --no-default-features --lib --locked
cargo +1.95.0 check -p nara --no-default-features --features runtime-2d,serde --lib --locked
cargo +1.95.0 test -p nara_render --all-features --doc --locked
```

WGPU_BACKEND=dx12 was selected for native verification; independent workspace also used root
CARGO_TARGET_DIR. An NTFS transparent compression pass on target/debug/deps recovered space
without deleting artifacts. Its exact parent-owned process was stopped after free space rose
above 30 GiB, before subsequent Cargo; its interrupted exit is not a product failure.

Independent read-only recipe review by Popper found no confirmed P1/P2. Jason reviewed the cook
and identified the moved constant's test import; parent had reproduced/fixed that P2, and Jason
re-read the current fully qualified access and closed it. The first owning build failed on that
private import, the next tried an unavailable unqualified name, then the final actual owner
cohort passed. The evidence does not hide those compile attempts. No worker ran Cargo/fmt/commit.
Mencius migrated disjoint downstream consumers; parent inspected, formatted and verified them.

# Follow-up

This is an explicitly qualified prerequisite, not completed R6b. There is no implemented durable
recipe index/CAS, tracked optional/processed build execution, whole-group commit/reopen protocol,
determinism-conflict history, target executable selection or complete packaged Player in this
change. CPU cooking alone does not establish any of those guarantees.

The bounded next-design review by Peirce was adopted into the existing design: normalize before
lookup; bind actual implementation/output authority; replay required/optional/processed facts;
separate seed hints from successful recipe associations; capture verified group bytes; and
serialize complete blobs/manifest/index publication with actual committed/rejected/uncertain
and filesystem durability evidence. That review is design evidence, not implementation.

No throughput/RSS, arbitrary native allocation interception, filtered texture conversion,
atlas transformation, GPU cooking or unexecuted platform claim is made. Historical memory and
migration documents retain their original vocabulary; all current Rust APIs and consumers drop
the superseded names. Reference Game's separate suite was not rerun for this prerequisite;
affected public consumers and the selected native workflows are recorded above.

# Citations

- [Design](../../../../architecture/local-artifact-build-design.md)
- [Plan](../../../../plans/2026-10-31-local-artifact-build-plan.md)
- [Source study](../../../../research/2026-10-31-local-build-recipes-and-publication.md)
- [Runtime Image guide](../../../../guides/runtime-images.md)
- [In-progress capture](../../progress/2026-10/2026-10-30T193104Z-durable-build-recipe-successor-and-source-provenance-replacement-in-progress-d866a6d48d1c440e802557c7954186ec.md)
