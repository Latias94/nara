---
type: "Verification Evidence"
title: "Image lifecycle contract repairs"
description: "R0 event coalescing, removal recovery, foreground admission and shared authoring revocation."
timestamp: 2026-10-19T02:17:17Z
record_id: "95901fe367a149bc90ec922ad0384a73"
producer_id: "codex-root"
run_id: "image-lifecycle-r0-20261019"
related_plan: "docs/plans/2026-10-18-image-lifecycle-contract-repairs-plan.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Verification

Authorized R0 implementation of A01-A04 from the
[TileSet dependency and Editor loading research](../../../../research/2026-10-18-tileset-dependency-and-editor-loading-research.md).
Tested source: the uncommitted `refactor/foundation-contract-hardening` worktree based on
`0ebc6603ccafc41e89acd0d478658610f7cf19ee`, including the preceding U1-U5 and file-backed TileSet
work. Existing changes were preserved; unrelated `myeasylog.log` and `vendor/` were untouched.
No commit, push or PR was requested or performed.

Cargo build/test commands ran serially with `CARGO_BUILD_JOBS=1`, using the existing root `target`.
Toolchain: `rustc 1.99.0 (b940084d7 2026-09-28)` on Windows 11. Native checks used DX12 and the
existing selected Dear frontend. This is focused development verification, not Rust 1.95 release
qualification, monitor scanout or Linux/macOS evidence.

# Result

A01-A04 are repaired in their owning contracts:

- Source coalescing independently retains latest explicit existence and sticky metadata dirtiness.
  Metadata-only events cannot resurrect removal; later recreation preserves earlier metadata dirtiness.
- Successful removal of a current usable Image can retain one pixel-free receipt per known identity.
  A later source-only recreation re-admits current metadata after exact catalog/type/state/slot
  continuity checks. Failed decode can retry without authorizing first load or identity drift.
- The resolver stamps its actual Loading revision only after queue admission. Spawn rejects an
  intervening state write before capturing an attempt or renewing recovery authority.
- Explicit Import supersedes same-source queued/running hydration and pauses unrelated publication.
  Each accepted last-good change can publish a complete catalog before the worklist finishes.
- Invalid identity cancels ineligible work and revokes the managed Image value, usable state and
  prepared resources. Scene content revision advances; new Scene packets and UI choices agree.
- A failed revocation still installs the completed catalog and prunes unsafe choices. A session-local
  terminal failure stops Scene/Image publication and rejects user requests before metadata writes.
  Valid selected UI content and physical task cleanup remain owned; reopen creates a fresh runtime.

Immutable identity reservations intentionally remain: resolution does not imply residency or pixel
authority. No generic executor, second runtime, database rebinding or compatibility layer was added.

# Evidence

Selections overlap and must not be added as a unique-test count.

| Selection | Result | Nextest run ID |
|---|---|---|
| Initial six selected runtime regressions before I1 implementation | 1 passed, 5 failed as expected | `30397e58-e1d8-4139-a1ea-f5f0d45fbd6f` |
| Initial explicit-priority and managed-revocation regressions | 2 failed as expected: Busy and unchanged content stamp | `b690977a-dc0b-47ba-aa96-4ad8c9f58be5` |
| Initial Asset/Image complete owner libraries after I1 implementation | 155 passed | `e1f1c8cd-0c15-4013-980b-9548b4ef16f2` |
| Editor authoring/preview selection before review-failure correction | 11 passed | `775fa18f-0895-4fb2-9413-11149058a84c` |
| Root all-feature library, including terminal revocation-failure corrections | 213 passed, 1 ignored test skipped | `4f8c9a5c-bcfa-46ef-aeaa-61a841031a21` |
| Review reproducer: state write between Resolve and Spawn, before revision stamp guard | 1 failed as expected: revoked Image incorrectly became present | `6e5bdd2a-cf55-4604-b171-65220830d02b` |
| Final Asset, Asset Watch, Image and root all-feature libraries | 393 passed, 1 ignored test skipped | `a1dee822-3647-4345-9726-b992c2f7345e` |
| Separately locked public Image reconciliation consumer | 2 passed | `1f7bc434-a0ed-4625-ae26-881da4b8978e` |
| Selected ignored native Scene reimport and Canvas save/reopen | 2 passed; 144 outside selection skipped | `b832906f-e5f0-4bc3-8bad-9b43cd27654f` |
| Image with no default features | 68 passed | `87291594-dc77-45c7-a878-19718d38277e` |
| Final terminal-failure selection after making repeated preview drives diagnostically silent | 2 passed; 212 outside selection skipped | `ae1aea59-ea4f-469e-b79a-e302828ed2a8` |

Final owner command:

```text
cargo nextest run --locked -p nara_asset -p nara_asset_watch -p nara_image -p nara --all-features --lib --no-fail-fast --test-threads=1
```

Public consumer command, with `CARGO_TARGET_DIR` set to the existing root `target`:

```text
cargo nextest run --locked --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test image_reconciliation --test-threads=1
```

Native command with `WGPU_BACKEND=dx12`:

```text
cargo nextest run --locked -p nara-editor --lib --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)' --test-threads=1
```

Both native trajectories actually ran. They use production import, presentation and saved-source
paths. The later resolver-revision guard affects source reload, which these explicit-import/Canvas
trajectories do not exercise; the final owner and independent-consumer runs cover the integrated
guard. The complete critical native group was not repeated and no full release claim is made.

Two independent read-only reviews found and closed concrete failures:

1. Consuming a completed catalog then returning on runtime-revocation failure lost the successor
   permanently. Installation/pruning now finishes and a durable session failure blocks new authority.
   Counter-exhaustion tests prove candidate retirement, valid UI retention and no later metadata write.
2. A removed-image recovery could absorb a newer same-version failure written after resolution.
   A failing public scheduling regression preceded the exact Loading-revision stamp guard. A second
   test prevents first removal from minting recovery eligibility after an intervening state write.

Final reviews reported no remaining concrete findings. Nine new Asset/Image owner tests plus the
Editor regressions cover coalescing, recreation, transient decode failure, unusable/empty slots,
identity mismatch, phase injection, priority, cancellation and terminal failure. Existing stale,
first-load, queue-limit, cleanup and last-good tests remain in the complete owner run.

Integrated static checks passed:

```text
cargo check --locked --workspace --all-targets --all-features
cargo check --locked -p nara --no-default-features --features runtime-2d,tooling,serde --lib
cargo fmt --all -- --check
git diff --check
```

The feature-minimal Host check covers authoring without presentation or UI. The final terminal
failure path reports once through the Host transition; later preview drives remain silent and
produce no candidate. Documentation checks resolved 628 local Markdown links and validated the
standalone HTML's IDs and fragment anchors.
Engineering-memory rollups are current and bundle validation passes. Advisory warnings concern
existing legacy metadata/absolute-path records; those historical records were preserved.

Some initial red Image assertions aborted during active Runtime destruction after reporting the
expected assertion failure. New tests now gather owned observations, close their runtime, then
assert; the final run terminates normally. Existing deprecation/dead-code warnings were preserved.

# Follow-up

Metadata-backed recreation requires Image's `serde` feature. Without it, the existing explicit
`MetadataSupportUnavailable` outcome remains; this repair does not create another recipe store.
Live rename/rebinding, acquisition/residency, TileSet reload/hydration, coherent retained dependency
publication, A05 strict tile membership and tile editing remain successors in the research.

The standalone HTML report stays outside `docs/` in the user-requested Downloads location. It marks
A01-A04 repaired and keeps A05/R1-R5 separate. The old U1-U5 goal remains complete; R0 does not
reopen it or claim the unsupported TileSet workflows are implemented.

# Citations

- [R0 plan](../../../../plans/2026-10-18-image-lifecycle-contract-repairs-plan.md)
- [Runtime asset lifetime contract](../../../../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [Editor Image publication contract](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
