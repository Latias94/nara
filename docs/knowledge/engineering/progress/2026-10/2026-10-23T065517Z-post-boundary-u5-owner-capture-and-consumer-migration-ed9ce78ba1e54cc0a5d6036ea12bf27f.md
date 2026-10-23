---
type: "Work Progress"
title: "Post-boundary U5 owner capture and consumer migration"
description: "Work Progress for Post-boundary U5 owner capture and consumer migration."
timestamp: 2026-10-23T06:55:17Z
record_id: "ed9ce78ba1e54cc0a5d6036ea12bf27f"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
---

# Scope

U5/R5 over `c1c2958`, following
`docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md`.

# Delivered Source

- `PreparedImageSnapshot::capture` derives the usable version, exact typed slot, descriptor and
  retained pixels from actual stores without requiring a managed catalog. The managed content
  projection keeps its original admitted immutable value.
- `PreparedImages` accepts only complete snapshots. The unused generic prepared store, record,
  blanket trait and failure-status layer are removed. UI/Sprite/WGPU and root consumers clone
  complete captures rather than reconstructing identity/pixel pairs.
- Independent UI and Sprite/WGPU workers migrated disjoint paths and added retained old/new
  packet/batch regressions. They did not run Cargo or create commits.
- An independent reviewer found a remaining resource-method call on a snapshot in the isolated
  module-consumer fixture. The call now goes through `resource()`; executable fixture verification
  remains required. No Image capture/store boundary blocker was found in the first review.

# Verified State

- The new compile-fail doctest initially compiled on the old public constructor (red), and now
  passes after removing independent pairing.
- `cargo +1.95.0 nextest run -p nara_image --lib --features render-prepare,serde --locked`:
  92 passed, including code-first direct replacement with unchanged version and descriptor.
- Render/Sprite/UI/WGPU all-feature library cohort: 244 passed, four existing ignored tests.
  Real GPU tests executed, including old/new simultaneous pixel readback, warm-cache no-reupload,
  actual-holder residency, UI pixels and shared Sprite/UI working sets.
- `cargo +1.95.0 check --workspace --all-targets --locked` passed with existing warnings.
- Cargo ran serially with `CARGO_BUILD_JOBS=1` and the existing target. No performance or
  Linux/macOS runtime claim is made.

# Next Action

Finish the isolated module consumer, root Image Host, relevant Windows/DX12 native and independent
Reference Game checks; resolve final independent review and record the qualified U5 commit.
U6-U8 remain required. The user explicitly authorized a higher budget; the active full-scope Goal
was recreated with 100,000,000 tokens. This supersedes the prior handoff's budget-limited stop, not
its implementation or verification facts. Never create or increase another budget without user
instruction.
