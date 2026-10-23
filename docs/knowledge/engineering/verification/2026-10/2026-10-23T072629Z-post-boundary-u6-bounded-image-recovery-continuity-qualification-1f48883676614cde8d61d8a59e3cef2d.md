---
type: "Verification Evidence"
title: "Post-boundary U6 bounded Image recovery continuity qualification"
description: "Verification Evidence for Post-boundary U6 bounded Image recovery continuity qualification."
timestamp: 2026-10-23T07:26:29Z
record_id: "1f48883676614cde8d61d8a59e3cef2d"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "533df48"
---

# Verification

U6/R6 over the qualified U5 commit `533df48`. This record is completed with actual results before
the U6 commit; its existence alone is not qualification.

The first new Image regression reproduced loss of successful-removal eligibility after byte queue
rejection, independently of whether a desired recipe could later mask that loss. The refactor stores
only typed handle, catalog record, removal version, exact empty slot and bounded Asset continuity.
It removes long-lived attempt-snapshot recovery and its obsolete previous-state comparator. Each
actual publication still uses a fresh exact shared mutation baseline and post-resolution revision.

Asset continuity is bound to the state store and registered Image owner. Ordinary state writes
invalidate by default; only narrowly owned resolver Loading/capacity/removal observation and exact
Image failure preserve the current identity. Successful exact repeated Remove can reissue recovery,
but cannot bless a prior external write. No diagnostic parsing, provenance log or executor is added.

The Asset implementation worker edited disjoint paths and added transition tests without Cargo.
Main integrated Image and directly tested eligibility with/without a retained desired recipe. A
first independent review found that unavailable `invalidate_publication` did not advance ordinary
state revision, allowing post-Spawn Remove/success to pass the old baseline. It now always advances
the exact state revision; the new stage-window test covers Remove, success and failure. A separate
main review bound the continuity issuer to the actual registration, rejecting foreign-App tokens.

## Executed Owner Cohorts

Rust 1.95.0, serial Cargo, `CARGO_BUILD_JOBS=1`, existing lockfiles and root target:

- Initial repaired Asset/Image cohort: 195 passed before expanded negative coverage.
- Expanded Asset/Image/TileSet cohort: 248 passed before foreign-owner binding; 249 passed with
  that binding. These intermediate results do not substitute for the final invalidation repair.
- Final current owner cohort after the post-Spawn invalidation correction:
  `cargo +1.95.0 nextest run -p nara_asset -p nara_image -p nara_tilemap --lib --features
  nara_image/render-prepare,nara_image/serde,nara_tilemap/serde --locked --test-threads=1`:
  252 passed, none skipped, final run `8ab38085-7394-41a9-9167-6f6341cd327a`. This includes the later
  exact metadata-acknowledgement revision, wrong stable/path/kind recovery metadata and public
  successful importer supersession controls.
- The no-default-feature Image cohort passed 78 tests, and Asset's two compile-fail doctests
  passed. The no-serde path does not gain implicit metadata support.
- Root Image Host/startup/Scene-request selected integration: 37 passed, 263 excluded, run
  `f228b815-30cd-42c0-8c1f-d82acdb4eb69`, with features
  `desktop-winit,presentation-2d,presentation-ui,render-wgpu,tooling,project-workflow`.
- Asset/Image all-feature library Clippy passed with correctness and suspicious lints denied;
  other advisory lints remain.

The final owner cohort covers repeated item/byte rejection; recovering Loading A superseded by
rejected B; retryable Image failure; direct eligibility with/without recipes; same-version external
Loading/failure/invalidation/metadata before lawful rejection; empty-slot ABA; replaced stores;
foreign consumers; exact stage-window invalidation; metadata/removal ordering; wrong metadata;
public publication supersession; stale task controls and TileSet publication/revocation consumers.

# Result

Final independent read-only review found no U6 blockers after the unavailable-invalidation and
foreign-owner corrections. It checked the source frozen for that review, the original P1, slot/store
association, finite transitions, repeat Remove and explicit external-write controls. The later
metadata acknowledgement additionally advances the exact state revision and has a baseline
rejection assertion; no weaker admission path was introduced. Subsequent final owner execution
includes these changes.

These owner and selected Host results qualify the bounded U6 contract. U5 retains its actual GPU,
native Image/Canvas and independent consumer evidence. U6 introduces no native rendering change;
later final integration can qualify the combined source without broadening earlier platform or
performance claims. U7/U8 and their separate consumer evidence remain required for the overall plan.
