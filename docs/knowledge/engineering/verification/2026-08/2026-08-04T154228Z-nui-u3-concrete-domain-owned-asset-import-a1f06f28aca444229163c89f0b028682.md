---
type: "Verification Evidence"
title: "NUI-U3 concrete domain-owned asset import"
description: "Verifies deletion of generic importer execution while preserving the concrete Image transaction, artifact identity, limits, and replacement semantics."
timestamp: 2026-08-04T15:42:28Z
record_id: "a1f06f28aca444229163c89f0b028682"
tags: ["nui-u3", "asset", "image", "import", "verification"]
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3b208568b60da0ad36c9b35510eae32bef885a90"
verified_by: "focused asset and Image nextest suites, root and reference-game safety proofs, affected-target checks, strict changed-target Clippy, rustfmt, stale-symbol inspection, and independent correctness/API review"
---

# Verification

NUI-U3 landed at `3b208568b60da0ad36c9b35510eae32bef885a90`.

- The unused generic `TypedImporter<T>`, erased importer registry, generic
  `ImportedAsset<T>`, registration APIs, and executor-shaped descriptors are deleted.
- `ImageImporter` owns source-extension admission, bounded read/decode, typed publication,
  cancellation, stale-result rejection, and last-good replacement behavior directly.
- Existing Image artifact identity remains exact: importer ID `nara_image_png`, importer version 2,
  output name `nara_image.image`, artifact format version 1, dependency digest, settings hash,
  target profile, and primary label are all covered by a non-default identity proof.
- Root facades and accepted architecture sources contain no generic importer execution claims.

# Result

Passed for the complete NUI-U3 contract.

- Baseline `nara_asset` and `nara_image` suites passed 128 tests before the refactor.
- Complete post-refactor `nara_asset` and `nara_image` all-feature suites passed 132 tests.
- Affected all-target checks passed for both packages; the root library and every root example
  compile with all features.
- Root Image import limits passed 2 tests with `runtime-2d`; reference-game Image safety passed
  its exact integration test.
- The new concrete artifact-identity and extension-admission proofs passed their focused rerun.
- Strict changed-target Clippy passed with only explicitly listed pre-existing Rust 1.97 lints
  allowed. Package-scoped rustfmt, staged diff checks, and stale-publication/API searches passed.
- Independent correctness and API reviews found no unresolved P0, P1, or P2 issue after the
  identity assertions and migration contract were strengthened.

# Concurrent Worktree Gates

- Global rustfmt check reports only the concurrent user-owned
  `crates/nara_ui/src/tests.rs`; that file and `crates/nara_ui_render/src/tests.rs` were neither
  modified nor committed by NUI-U3.
- The default root `image_import_limits` invocation collected no tests because the suite is gated
  by `runtime-2d`; the corrected feature-qualified invocation passed both tests.
- Unqualified Rust 1.97 `-D warnings` exposes pre-existing lints in untouched `nara_app` and
  `nara_asset` code. The changed targets pass with those exact pre-existing lint classes allowed.
- The reference-game feature slice emits pre-existing root dead-code warnings; its Image safety
  behavior still passes.

# Follow-up

NUI-U4 is the sole next unit. Close arbitrary render topology and public technical UI pipeline
stages before introducing the renderer-neutral ordered packet boundary.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u3-remove-the-false-generic-importer-executor`
- `docs/architecture/adr/0103-concrete-domain-owned-asset-import-execution.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u3-concrete-domain-owned-asset-import-execution`
- `crates/nara_image/src/import.rs`
- `crates/nara_image/src/tests.rs`
- `reference-game/tests/image_asset_safety.rs`
