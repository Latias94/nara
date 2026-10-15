---
type: "Verification Evidence"
title: "Toolkit-neutral preview ownership and revision-based editing pass the integrated Host gates"
description: "Verification Evidence for Toolkit-neutral preview ownership and revision-based editing pass the integrated Host gates."
timestamp: 2026-10-15T18:51:08Z
record_id: "5aa5d3c1904b478186d78dd1b88ab694"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2219da8"
---

# Verification

- `cargo check --workspace --all-features --all-targets --locked --offline`: session `87071`,
  passed in 1m17s, including the native Editor, palette example and WGPU test constructors.
- Complete affected Host persistence, workspace Play and CI-policy targets:
  `b19695ea-2a69-4f7b-9d73-23daed528cf3`, 35/35 passed, 20.939s after 38.71s compilation.
- Earlier focused run `5ac2ecf9-66f9-4e91-9958-34ca8770ad71`: 36 run, 34 passed and two failed.
  All 28 render-library tests passed, including new HiDPI/integer-fit/undersized mapping tests.
  The repaired startup-lease and runtime-edit fixtures passed. The new project composition
  privacy consumer passed with the inspected pair of real rustc E0616 diagnostics.

# Result

The actual Host preview now has one backend-neutral content/image/display owner and opaque
presented token. Native registration/sample/present checks stay in the native adapter. A second
CPU toolkit consumer takes real Host candidates, draws their captured quads, completes its own
display operation and uses the same mapping/token/picking API. Submission alone grants no input
authority; an unfinished successor does not replace the displayed frame. Content revision,
mapping, target resize and device namespace independently revoke old geometry.

Document transactions and their no-redraw concurrency behavior remain green together with
physical Play closure, save races, dirty-close and persistence authority privacy.

# Evidence

The two initial failures were fixture drift. The fake invalidation case dispatched a valid
document edit and then read the previous immutable published Editor frame as if dispatch had
redrawn it. It now takes the revision from the next actual Host candidate and explicitly checks
that it changed. Its assertions run with finite physical Editor closure outside unwinding.
The CI dependency allowlist omitted the existing optional Editor canary dependency and the new
independent domain test dependencies; it now admits those exact consumers and keeps the Editor
production dependency optional. No document or presentation check was loosened to pass tests.

U13 replaces 405 lines of project-Host source/AST/method/parameter-name assertions with the
existing typed constructor consumer and one real private-field compiler test. Actual schema-
owning probe rejection and project execution remain covered by behavior consumers. It adds no
Rust parser or public-method inventory in their place.

The owned independent-consumer checkpoint is `2219da8`; the original staged diff remained
byte-identical after that commit. Overlapping Host/Cargo/SPL changes remain in the integrated
worktree, so these checks do not claim an independently buildable committed HEAD.

# Follow-up

Share one actual mapping instance for panel paint and picking, then finish fixed-step extraction
interpolation and the native image-to-window regression. Reference selectors, remaining consumer
and CI checks, simplification and the final full review remain. Current native GPU evidence is
Windows evidence; Linux/macOS release behavior is not inferred.

# Citations

- crates/nara_render/src/presentation.rs
- src/project_host/runtime/editor/presentation.rs
- editor/src/platform/viewport.rs
- tests/support/fake_preview_toolkit.rs
- tests/ui/project_host/private_fields.rs
- tests/ui/project_host/private_fields.stderr
