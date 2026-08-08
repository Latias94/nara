---
type: "Verification Evidence"
title: "UIF-U3 atomic presentation planning and backend closure"
description: "Verifies target-private failure-atomic planning, resource-complete packets, owner-local budgets, bounded rounded rectangles, exact backend retry generations, and one-shot presentation authority."
timestamp: 2026-08-08T18:39:05Z
record_id: "a342f27273bb47dda137107656a35074"
status: "complete"
producer_id: "codex-root"
run_id: "uif-u3"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c4df323"
verified_by: "codex-root"
---

# Verification

- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui_render -p nara_render_wgpu
  --all-features --test-threads=1`: 133 passed, 0 failed on committed code; run ID
  `97541f3d-109f-4b31-b69a-ebeedb0bfa36`.
- `cargo run --locked --example ui_presentation_gallery
  --features presentation-ui,desktop-winit,render-wgpu -j 1`: passed. The public gallery carried an
  ECS candidate through owned solid/image/glyph resources, backend acceptance, exact receipt,
  accessibility action and packet retirement.
- `cargo clippy --locked -p nara_ui_render -p nara_render_wgpu --all-targets --all-features
  --no-deps -j 1 -- -D warnings -A clippy::derivable_impls -A clippy::too_many_arguments`: passed.
  The two allowances name existing WGPU telemetry/function-shape debt outside UIF-U3; a standard
  run reported only those pre-existing categories.
- `cargo fmt --all -- --check`, scoped `git diff --check`, staged-file inspection and precise
  commit review passed.
- Independent adversarial review found a permanently non-progressing instance-budget deferral and
  duplicate accounting for two logical aliases of one image snapshot. Both were corrected before
  the final gate. Its proposed fill-plus-border planner charge was rejected because the Accepted
  owner split counts one renderer-neutral `Rect` packet primitive while WGPU separately admits its
  two device instances; a one-primitive regression now fixes that distinction explicitly.

# Result

UIF-U3 is complete at `c4df323`, including the preceding candidate text-work correction at
`4067967`.

- `UiPresentationPlanner` is the single deep packet owner. Target-private atlas, surface ownership
  and the most recent resource-complete packet commit together only after content preflight,
  staged glyph work, lowering and retention admission all succeed.
- Planner failure is atomic. Missing, ambiguous or mistyped images, capability failures and every
  planner budget rejection leave exact packet and atlas state unchanged. Two logical image aliases
  resolving to one immutable resource revision share one packet index and one retention charge.
- The packet vocabulary is closed to renderer-neutral `Rect` and `Glyph` facts. A `Rect` owns its
  optional solid/image fill, uniform border and bounded physical corner radius without leaking WGPU
  instances or expanding a border into unrelated primitives.
- Kernel candidate work, planner packet/resources and backend upload/instance work have separate
  budgets. Permanent backend hard-limit excess is content rejection; only work that may make
  residency progress is deferred.
- Image and glyph bytes, exact resource revisions and retention storage are packet-owned. WGPU no
  longer reads live asset storage during submission and renders uniform rounded fill, image and
  border through representative GPU readback.
- Successful WGPU submission issues a non-cloneable one-shot commit bound to the exact packet,
  target, backend instance and device epoch. Deferral carries the same exact retry generation;
  neither rejection nor deferral advances receipt authority.
- The former stateful builder was removed. Private `planner`, `packet`, `atlas` and `limits` modules
  now separate planning, immutable transport, staged glyph storage and hard policy while the
  ordinary prelude remains small.

# Evidence

- `crates/nara_ui_kernel/src/compiler.rs`
- `crates/nara_ui_render/src/planner/{mod,images,lowering}.rs`
- `crates/nara_ui_render/src/packet/{mod,outcome}.rs`
- `crates/nara_ui_render/src/{atlas,limits}.rs`
- `crates/nara_ui_render/src/{packet_tests,integration}.rs`
- `crates/nara_ui_render/tests/ordered_hierarchy.rs`
- `crates/nara_render_wgpu/src/{backend,quad,texture,ui}.rs`
- `crates/nara_render_wgpu/src/sprite.wgsl`
- `crates/nara_render_wgpu/src/lib.rs#tests`
- `examples/ui_presentation_gallery.rs`
- `src/ui_presentation.rs`

# Follow-up

- UIF-U4 owns the still-missing Surface Slot generations, exact content transform, endpoint
  retirement and receipt-stamped platform accessibility ingress. UIF-U3 does not fabricate those
  Host identities inside the planner.
- Same-target multi-surface aggregation, generic root selectors, per-surface theme selection,
  project fonts and broader shape vocabulary remain deferred to named consumers.
- Reference Game product acceptance remains outside UIF-U3; no product hash or Cartesian smoke
  matrix was used as Foundation evidence.

# Citations
