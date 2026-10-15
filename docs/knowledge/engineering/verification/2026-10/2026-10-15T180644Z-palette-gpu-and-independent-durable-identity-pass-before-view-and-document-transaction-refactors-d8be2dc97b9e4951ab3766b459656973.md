---
type: "Verification Evidence"
title: "Palette GPU and independent durable identity pass before view and document transaction refactors"
description: "Verification Evidence for Palette GPU and independent durable identity pass before view and document transaction refactors."
timestamp: 2026-10-15T18:06:44Z
record_id: "d8be2dc97b9e4951ab3766b459656973"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
status: "verified"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7d7f778"
---

# Verification

All commands were serialized with `CARGO_BUILD_JOBS=1`, reusing `target`.

- Material, sprite, sprite-render, tilemap and WGPU all-feature libraries: run
  `62ab75cc-6200-469d-866d-1c16f03b6904`, 144/144 passed, 19.556s. Both native palette/order pixel
  readback and invalid-shader local rejection passed, together with the retained UI readback tests.
- Independent durable identity consumer: run `4ca8065c-6170-4da1-af28-e865d536e357`, 5/5 passed,
  0.304s, after 1m11s compilation. This used the independent manifest and updated its lockfile.
- Independent material consumer: `cargo check --manifest-path module-consumer/Cargo.toml
  -p nara-material-module-consumer --lib --locked --offline` passed in session `22253`, 26.38s.
- Root content/runtime/admission plus the intended U11 red: run
  `0240fb70-6d39-44ac-9ad9-9df8acd9555a`, 44 tests, 38 passed and 6 failed, 33.194s. Stable image
  and prefab rename, actual headless materialization after rename, generic data-only admission,
  bounded catalogue/duplicate UUID/closure exclusion and actual compiler privacy all passed.
- U10 first red: run `975ff44d-770a-4f48-8526-d63177a3145f` rejects two extracted same-target
  views with `UnsupportedViewCount { actual: 2 }`, as expected before production changes.

# Result

U9's concrete palette behavior and U7's independent identity/migration workflow are verified at
this checkpoint. U9 has a separate focused read-only correctness review with no actionable P1/P2;
the final foundation review is still outstanding. U7 actual generic preview and accepted-import
context updates, Reference consumers, U10/U11 production, U8 and U13 remain unfinished.

# Evidence

The five identity tests exercise actual two-directory metadata/document decoding and scene spawn,
atomic duplicate-UUID rejection, copy-as-new internal references, explicit v1 conversion preserving
unresolved input bytes, and application-owned saved instance binding across changed spawn order.
They add no engine mount/save framework or new public identity API.

Five root failures were stale budget/layout/retained-generation test expectations. The sixth is
the intended U11 counterexample: a successful atomic two-field commit returns revision r1, but
Undo at r1 without another frame returns `FrameAlreadyDispatched`. Corrections are in progress;
this run is not reported as green. Actual compiler errors for private owner access and immutable
document mutation were inspected and saved as trybuild expectations.

The initial root link failed with LNK1318 while F: had about 300 MiB free. The main thread stopped
the next build, then ran `cargo clean -p nara --profile dev --offline`, removing 26.7 GiB of
rebuildable package artifacts only. The root rerun linked and reached all tests. No source,
resource, user index, or user work was discarded. No concurrent Cargo commands ran.

WGPU 30's initial error-scope compilation failure was corrected using its locked local source
and the official ErrorScopeGuard contract: the retained guard is consumed through `pop()`.

# Follow-up

Rerun affected root content budget/retention tests after their behavioral corrections. Finish
generic StableImage preview and accepted Import-to-Sprite behavior. Migrate the existing Reference
bundle through its actual generator and validate Reference selectors. U10 now owns render/WGPU/
sprite projection files; U11 owns ordinary action/history ingress. Do not compile these mutable
dependencies before their freeze points. No native Linux/macOS render or process release claim
follows from this Windows evidence.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- module-consumer/tests/durable_identity.rs
- module-consumer/material-consumer/src/lib.rs
- crates/nara_render_wgpu/src/sprite_probe.rs
- https://wgpu.rs/doc/wgpu/struct.ErrorScopeGuard.html
