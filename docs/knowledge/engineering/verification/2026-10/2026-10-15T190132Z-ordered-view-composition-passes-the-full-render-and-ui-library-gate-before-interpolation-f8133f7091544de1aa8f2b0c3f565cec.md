---
type: "Verification Evidence"
title: "Ordered view composition passes the full render and UI library gate before interpolation"
description: "Verification Evidence for Ordered view composition passes the full render and UI library gate before interpolation."
timestamp: 2026-10-15T19:01:32Z
record_id: "f8133f7091544de1aa8f2b0c3f565cec"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "80426d9"
---

# Verification

- `cargo nextest run -p nara_render -p nara_sprite_render -p nara_render_wgpu
  -p nara_tooling_egui --all-features --lib --locked --offline --test-threads=1`:
  run `39b0c602-34cd-4b69-b3e3-86b578cb3e3b`, 180/180 passed in 23.732s after 17.20s compilation.
  This includes actual Windows WGPU split-view pixels, clear/load composition, one UI overlay,
  the material/palette probe and shared display-mapping library tests.
- `cargo nextest run -p nara_render -p nara_sprite_render --all-features
  --test presentation_interpolation --locked --offline --test-threads=1`:
  run `defce173-051c-4115-adaa-abd4a4c84db6`, one intended failure. Fixed simulation reaches x=8;
  half-tick extraction still returns x=8 where the new presentation consumer expects x=4.

# Result

Ordered view/target composition and toolkit-neutral display mapping are green. Interpolation
is not implemented at this capture. Its real extraction test retires the managed Runtime
before asserting, so the observed failure is the midpoint mismatch rather than a shutdown abort.

# Evidence

The stable-content owned checkpoint is `80426d9`, following independent native-domain and
durable-identity consumer checkpoint `2219da8`. Both commits preserved the original staged diff
byte-for-byte. Integrated worktree verification does not imply standalone committed-HEAD closure.

One attempt to build the library matrix was stopped before tests when disk space became low.
Cargo's package-scoped `cargo clean -p nara --profile dev --offline` reclaimed 12.2 GiB of
rebuildable root-package artifacts; the same render/UI matrix then completed successfully.
No source, user change or unrelated cache was removed.

# Follow-up

Implement explicit opt-in fixed-step presentation interpolation in the existing transform owner,
then verify the native Editor display path, migrated Reference selectors and remaining public
consumer/CI boundaries. Finish simplification, full code review and precise owned commits.
Linux/macOS native process and presentation release evidence remain outside this Windows run.

# Citations

- crates/nara_render/src/view.rs
- crates/nara_render/src/presentation.rs
- crates/nara_render_wgpu/src/view_probe.rs
- crates/nara_sprite_render/tests/presentation_interpolation.rs
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
