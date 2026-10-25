---
type: "Verification Evidence"
title: "Source local Snap public consumer and installer qualification"
description: "Qualifies the source-built R1 Snap planner, exact panel candidate and independent inactive Session workflow after complete review and direct installer proof correction; child tools and full R1 remain open."
timestamp: 2026-10-25T11:20:59Z
record_id: "f89f21479f7d4c06a81ab5bc63b5ff87"
producer_id: "codex-root"
run_id: "foundation-snap-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "3a3134306a9081ddb5e5e4413da487bf89143d67"
---

# Verification

This qualifies the source-built useful Snap command, a bounded R1 subunit, on `3a31343` plus
the three Rust files below. It does not close full R1, R1b, R2 or the foundation Goal. Existing
ADRs 0015/0026/0047/0118 already own this path; tool-specific grid policy adds no engine contract.

# Result

- One direct Scene entity's local Transform2d position is read through two independently bounded
  scalar observations. The dotted stable root IDs are `translation.x` and `translation.y`, not
  nested member paths. The pure planner and actual Source candidate/inverse are shared by the
  Dear ImGui panel and a separate window-free Session consumer.
- Preview/cancel publish nothing; Apply consumes the same move-only candidate and receives its
  exact originating submission callback. Stale observations/candidates reject. One accepted
  two-axis edit is one Undo/Redo entry, not two independent action writes.
- The Session proof edits an inactive source without changing active navigation, preserves parent,
  other fields and entities, then completes exact attempt-correlated Save/Reopen. Historical query
  observations and prepared edits cannot acquire the reopened source incarnation.
- The tool uses positive finite f64 spacing, local-zero origin and remainder-distance rounding
  with exact binary ties away from zero. Inputs/outputs must fit the actual finite-f32 Transform
  decode range, without an extra coordinate cap or hidden f32 round trip. NoChange creates no
  candidate. Missing/defaulted/non-F64 observations are explicit tool refusals.

# Evidence

## Actual Review And Corrections

Complete CE review `20261025-105156-dceb30b3` ran nine Codex-native lenses, merge, a fresh
one-finding validator and report. All handles were collected and closed. Its original completed
receipt is `Ready with fixes`, with one confirmed P2 registration-proof gap; it is not rewritten
as a zero-finding review. The delegated Spy genuinely used SnapPanel and Application routing,
but its separate descriptor left the public installer's composition unused.

The added `public_snap_installer_opens_and_paints_its_actual_contribution` instead calls
`snap::install`, builds that exact composition, checks its real descriptor/default open instance,
removes obscuring Right-side panels and paints through the returned frontend. The original five
proof tests remain. Independent `correction-review.md` confirms finding #2 is addressed without
claiming native rendering or the advisory paths below. Its first compile used an immutable
Application borrow where `show` requires mutable access; that compile failure is not a behavior red.

Five source-simplification reviewers ran before complete review. Applied reuse is the existing
`ComponentValue::as_f64`; clearer private root-ID naming replaces the explanatory inline comment.
The public SnapPanel lifetime documentation remains useful contract text. The stale proposed-test
module header was corrected without changing proof assertions. A production range-loop lint was
removed; the retained proof still has a non-gating clone-to-slice style warning. No safety checks,
proof tests or history owners were removed. No cross-model corroboration is claimed.

## Serial Final Checks

All commands use Rust 1.95.0, `CARGO_BUILD_JOBS=1`, `RUSTUP_TOOLCHAIN=1.95.0` and shared `target`.

| Check | Actual result |
| --- | --- |
| Package `nextest`, all default tests, final source | Run `0431e670-c503-477a-8b4e-2623bdf60fe6`: 42 passed, 3 ignored native cases skipped |
| Focused Snap proof after installer correction | Run `f10b2904-5423-45b7-b535-092fe000294d`: 6 passed, 0 skipped |
| Package all-target Clippy | Passed with `-D clippy::correctness -D clippy::suspicious`; existing dead-code and the noted test style warning remain |
| Root and independent-consumer `cargo fmt --all -- --check` | Both passed |

Engineering-memory render/check and validation pass with existing historical metadata warnings.
A focused document check resolves 804 local file links across six affected documents; it does not
claim a full Markdown-anchor validator. `git diff --check` passes, and the three normalized Rust
hashes below remain unchanged after documentation closure.

Package command: `cargo +1.95.0 nextest run --locked --manifest-path
tests/fixtures/module-consumer/Cargo.toml --target-dir target -p
nara-editor-panel-module-consumer --no-fail-fast --test-threads=1`. Focused qualification adds
`--test snap_local_position`. The same manifest/package/target selects Clippy with `--all-targets`.

Earlier expanded pre/post-simplification cohorts each pass41/skip3 (`c720efc5-36f5-44aa-98c4-11cdeaefd1f5`
and `f1d6faec-aef9-4973-938d-951193643fe2`); they predate the new installer test and are not final
acceptance for it. Raw logs and review artifacts remain outside tracked memory under the local
temporary directory (`nara-r1-snap-*`); `nara-r1-snap-review.txt` locates the complete review.

Normalized-LF SHA-256 at final qualification:

| Rust path relative to the consumer | SHA-256 |
| --- | --- |
| `src/lib.rs` | `8cc16415846fa5829a76f82c6b42a77bf79695031d1b0db9f0a45f1b046e9ccd` |
| `src/snap_local_position.rs` | `e5b3661759727401dae4b74b348e060b862a9a9059b54f44af73d1a7f009260c` |
| `tests/snap_local_position.rs` | `0a8b8d2fd37216ec92fc51b0e02f11bbcd47bf86418f345fcdb7f6320ed4f55e` |

# Follow-up

- Both panel tests use real Dear ImGui Context/input or composition and public Application
  routing, but no native window/GPU. No new native Snap, Linux/macOS or complete Editor claim is
  made. Root product owners were not changed, so the earlier F2/F3 full qualification was not
  rerun merely for this consumer example.
- Review's supplemental selection/grid-change invalidation, nondispatched-result recovery and
  immediate enqueue-budget refusal are not yet directly exercised by this Snap panel proof.
  Existing handlers/generic frontend tests remain; no product defect was found in those paths.
- The finite child proposal is separately researched, not implemented. Its significant next
  ownership question is who continues physical cleanup after a UI origin retires. R1b inline
  Inspector/placement/install/update/disable/remove, Behavior/Prefab, execution, rendering and
  delivery remain open; this unit does not narrow the active Goal.
  An independent read-only architecture check of that study found no material contradiction;
  it highlighted that the current parent-side planner must not precompute the helper's answer.
  The study now explicitly requires private observation/result separation inside the consumer.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Panel usage](../../../../guides/editor-panels.md#a-source-local-snap-tool)
- [Snap implementation](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/src/snap_local_position.rs)
- [Independent proof](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/snap_local_position.rs)
- [Initial integration and failed fixture attempts](../../sessions/2026-10/2026-10-25T103839Z-compiled-failure-projection-committed-and-real-snap-consumer-initially-passing-b98096de3c6149249a0389e9faacb5d5.md)
- [Finite child study](../../../../research/2026-10-25-finite-snap-child-proposal.md)
