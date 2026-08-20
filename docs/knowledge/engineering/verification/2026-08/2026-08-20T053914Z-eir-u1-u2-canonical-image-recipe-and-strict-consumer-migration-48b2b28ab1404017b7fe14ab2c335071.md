---
type: "Verification Evidence"
title: "EIR-U1/U2 canonical image recipe and strict consumer migration"
description: "Verifies the atomic typed metadata reset across image owners, Generic Authoring, strict ProjectContent, reload, fixtures, and consumers."
timestamp: 2026-08-20T05:39:14Z
record_id: "48b2b28ab1404017b7fe14ab2c335071"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "88f4a7a"
verified_by: "codex-root"
---

# Verification

Verified implementation commits `334f946` and `88f4a7a` against the EIR-U1/EIR-U2 atomic migration.
The first commit replaces the unreleased flat Image metadata contract with one typed recipe and
moves every existing strict consumer in the same build-green commit; the review correction makes a
`.meta` source change re-admit that persisted recipe rather than silently reusing last-good settings.
No compatibility reader or caller-authored recipe hash remains.

# Result

`nara_asset` now owns the nested identity/settings envelope and fallible random stable-ID
generation. `nara_image` owns the closed sRGB/Linear recipe, canonical settings hash, importer
version, and concrete byte/file request construction. Generic Authoring retains the bounded
identity-first candidate without depending on the Image domain, while a concrete Image consumer
can later admit the exact retained recipe. Strict ProjectContent, trusted products, runtime reload,
examples, Reference Game sidecars, and render consumers use the same typed settings.

The migration is intentionally breaking and unreleased. Flat metadata, missing-metadata identity
generation, global image color-space configuration, and arbitrary settings-hash/profile request
arguments were removed. The migration guide records the required manual sidecar rewrite and image
artifact rebuild.

# Evidence

- Owner-level tests passed 127/127, serially:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_asset -p nara_image --test-threads=1
  ```

  This includes strict nested metadata/golden rejection, stable identity/database invariants,
  deterministic and color-sensitive settings hashes, both recipe roundtrips, concrete import,
  Linear reload with the accepted recipe, failure-atomic last-good retention, and bounded decode.
- Strict ProjectContent consumed the persisted Linear recipe and matched the artifact settings hash:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run --features runtime-2d,serde --test project_content_boot -E 'test(strict_project_content_load_uses_the_persisted_linear_recipe)' --test-threads=1
  ```

  The focused test passed 1/1.
- Invalid persisted Image settings failed strict loading before snapshot publication and released
  all reservations:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run --features runtime-2d,serde --test project_content_boundary -E 'test(image_metadata_must_match_the_referenced_path_kind_and_typed_recipe)' --test-threads=1
  ```

  The focused test passed 1/1.
- Generic Authoring preserved Image metadata and source bytes without eager decode. The same test
  passed under `runtime-core,serde` and under `runtime-2d,tooling,serde`; the latter re-admitted the
  retained candidate as the real `ImageImportSettings::Linear` value.
- Focused Reference Game image safety and project boot tests passed, and the WGPU, Sprite, and UI
  render consumers compiled against the typed request API. All repository examples compiled.
- The adversarial review found that `MetaModified` and source `Modified` requests still shared the
  old reload-settings path. Commit `88f4a7a` corrected the owning Image reload boundary: metadata is
  read through the same assets-root capability, bounded by `AssetMetaFileLimits`, admitted as the
  concrete typed recipe, and checked against the complete target identity. One trajectory proves an
  invalid sidecar preserves the exact last-good image/version and a corrected Linear sidecar advances
  version, color space, and canonical settings hash:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_image --features serde -E 'test(image_plugin_meta_reload_admits_changed_recipe_and_preserves_last_good_on_invalid_meta)' --test-threads=1
  ```

  The focused test passed 1/1. The existing source-reload and diagnostic-matrix tests passed 2/2
  under default features, which also compiles the no-Serde fail-closed branch.
- Boundary searches found no `MissingMetaPolicy`, `insert_source_with_meta_policy`, or
  `with_color_space` production symbol. Image decode/file requests now accept typed settings rather
  than independent hashes or profiles.
- `cargo fmt --all -- --check` and `git diff --check` passed. User-owned `.codex-research/` content
  remained untouched and unstaged.
- Simplification removed duplicated metadata facts, unused request getters, repeated full-metadata
  decoding, and a leaked random-source error type. Multi-reviewer code review found a migration-doc
  gap, two focused evidence gaps, and the late metadata-reload correctness defect; all were fixed and
  validated. Reliability, maintainability, performance, API-contract, and project-standards reviews
  found no further defect. A cross-model pass was skipped because the only available `bash.exe` path
  would invoke WSL, which this session explicitly forbids.

# Follow-up

EIR-U3 is the sole active successor. It owns the bounded Host metadata catalog, exact
create-versus-replace metadata publication, one shared Editor authoring runtime, and one concrete
last-good Image transaction. EIR-U4 through EIR-U6 remain inactive; no Inspector, Canvas
publication, referenced hydration, watcher, generic executor, or native trajectory is admitted by
this record.

# Citations

- `334f946` (`refactor(image): canonicalize import recipes`)
- `88f4a7a` (`fix(image): admit metadata reload recipes (U2)`)
- `docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md`
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`
- `docs/migrations/2026-07-engine-foundation.md#eir-u1-1-typed-image-metadata-and-canonical-import-recipes`
- `crates/nara_asset/tests/meta_format.rs`
- `crates/nara_image/tests/image_settings.rs`
- `crates/nara_image/src/tests.rs`
- `tests/project_content_boot.rs`
- `tests/project_content_boundary.rs`
- `src/project_host/authoring/mod.rs`
- `reference-game/tests/image_asset_safety.rs`
