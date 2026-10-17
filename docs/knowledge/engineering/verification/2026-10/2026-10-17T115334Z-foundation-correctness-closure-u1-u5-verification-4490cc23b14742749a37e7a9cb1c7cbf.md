---
type: "Verification Evidence"
title: "Foundation correctness closure U1-U5 verification"
description: "Observed U1-U5 behavior regressions, exact renderer cohort and native Windows/DX12 acceptance, with serial checks and bounded platform claims."
timestamp: 2026-10-17T11:53:34Z
record_id: "4490cc23b14742749a37e7a9cb1c7cbf"
status: "completed"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-17-1856-fix-foundation-correctness-closure-plan.md"
git_commit: "4589b5a"
verified_by: "Serial root execution and independent native Codex review"
---

# Verification

This record covers the finite U1-U5 plan, following audit `09b03c6` and activation
`2c3e9ff`. The coordinator runs every Cargo command serially, with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, the existing target and one nextest test thread on Windows.
The final single-case native scheduling run used `--no-capture`; only that one case
was selected, so it did not introduce concurrent tests or builds.
Native qualification uses DX12 and an actual desktop/adapter.

The finite U1-U5 / R1-R7 work is complete at implementation source `4589b5a`.
The additional R4 trajectory, main checks and strict external-codec lint pass.
The later closure commit changes only plan metadata, navigation and evidence.

# Result

Implementation commits are `187abda` (U1), `0b3f569` (U3), `ae4347a` (U4/U5),
and `4589b5a` (U2 including the additional R4 proof). No remote publication is claimed.
The final source tree matches these committed files; subsequent evidence closure
does not widen the observed Windows/DX12 or feature scope.

# Evidence

## Behavior regressions

| Contract | Failing observation before correction | Passing observation after correction |
|---|---|---|
| R1, Removed vs Modified | `0eeafccb-1423-4c8e-8740-ee7f25bfedc3`: Modified control passed; byte-rejected Removed retained publication. Image run `b1e1a331-4943-4c4e-806c-4be79d4e09b8` retained a usable removed version. | `d1a3303f-060c-4b23-bf4d-bc25c93e624d`: 130 Asset/Image tests passed. Item/byte/consumer rejection, quiet successor frame, obsolete image completion and fresh reimport are covered. |
| R2/R4, native frame/cadence | `8a23ac17-2969-4ad6-84da-6943130bd503`: actual same-output resize occurred, but the accompanying selection was rejected; external cadence and browser/presentation deadline regressions also failed. | `0f5d9a81-590e-4846-8cd6-c8947c553ed1`: 130 ordinary Editor tests passed, with 12 ignored native cases excluded. `eb90a823-924f-48eb-b666-e7f45dc03cc8`: all three selected real Windows/DX12 native trajectories passed. |
| R3, Restart admission and widgets | `c135e83a-8383-4550-aab3-71834312c615`: saved-source changes were incorrectly accepted. `6213fb0c-6b79-4378-9a8c-d61fec7a0245`: actual Host rejection was reached, then old widget wording failed. Earlier fixture-parse failures are not behavior-red evidence. | Both saved-source Host regressions passed in `8a23ac17-2969-4ad6-84da-6943130bd503`; the corrected real-widget regression passed in the complete ordinary Editor run `0f5d9a81-590e-4846-8cd6-c8947c553ed1`. |

Removal revokes derivation authority before optional work admission, without requiring
physical CPU/GPU storage destruction. Modified failure still keeps last-good data.
Native input retains exact displayed geometry; the output tail admits actions before
extent reconciliation and a combined drive. Source-diverged Restart emits no retirement
request or attempt and preserves the running generation/output. Scene, referenced
Prefab, Cargo source changes and unavailable saved source are checked against disk.
Unsaved edits retain their separate meaning; unchanged Restart and Stop then Play pass.

## Production-cohort and consumer checks

| Gate | Observed result |
|---|---|
| Root-lock WGPU 30 renderer patch | `300ce01d-bee0-4baa-b7e5-78a1616f4ea8`: 100 unit/integration tests passed with `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`, `--no-default-features --features wgpu-30,multi-viewport-winit`. |
| Root all-features library and retained consumer contracts | `6d0e9d68-e033-40fe-98a6-c355d61b8e26`: 213 passed, one failed, one ignored. The 208 library cases, CI policy and actual independent renamed-root recipe execution passed. The failure was the historical reference-game feature expectation described below. |
| Manual component codec external consumer | `ce636067-ea9a-4351-80fc-a069b106f4b3`: the new independent consumer compiled and passed. Stored Health 42 applies, native Health changes to 17, encoding and reapplication preserve 17. |
| Retained actual reference CLI | `177f5b32-7888-4354-8caf-1c7b2cd3b59e`: six tests passed, including stable summary, tick bound and external-input rejection. |
| Workspace check | `cargo +1.95.0 check --workspace --exclude dear-imgui-wgpu --all-features --all-targets --locked --offline` passed. The separately tested renderer remains a dependency in its valid WGPU 30 configuration. |
| Main lint and formatting | Root `cargo fmt --all -- --check` and independent module-consumer formatting passed. Clippy for `nara`, `nara-editor`, `nara_asset` and `nara_image`, all features/targets, completed without errors. Warnings remain; this is not a zero-warning claim. In-scope test warnings include two redundant terminal returns in the new native proof; they do not alter its assertions or scheduling. |
| Strict manual-codec lint | `cargo +1.95.0 clippy --manifest-path module-consumer/Cargo.toml -p nara-scene-module-consumer --test manual_component_codec --no-deps --locked --offline -- -D warnings` passed. |

The first CI policy run `90cd6b0a-372b-4565-b9c2-5f82f650d160` reproduced its old
unconditional `--exclude` ban conflicting with the renderer's separate cohort gate.
The correction permits only `--exclude dear-imgui-wgpu` in the root job and retains
the ban for other packages, the external jobs, `--no-run` and masked failures.
The subsequent combined root run passed that policy.

The retained reference-game manifest test expected three root features, while
`ca3d803` had already added `project-tooling` for the real compiled-recipe workflow.
Its expected feature list is corrected; the manifest and production dependency
boundary are unchanged. The focused CI/manifest rerun passed in
`a582ad86-f7de-4e37-a200-5ea09986cb94`. Its additional private-surface case reached
all five real compiler checks, but one historical E0603 snapshot contained an extra
diagnostic label no longer emitted. The private-module rejection itself remained
correct. Only those redundant expected label lines were updated; run
`5f2fd713-cd86-4217-927d-107c267369f6` then passed all four CI, manifest and
private-surface tests, including the five compiler cases.

Workspace membership brings the renderer's upstream optional backend families into
the root lock; it does not activate them in Nara's WGPU 30 gate. Blanket all-features
commands exclude this multi-backend package and the dedicated step tests it explicitly.
The existing Tokio 1.53.3 resolution is preserved. The unrelated untracked vendor
lock and INI are neither removed nor used as the root cohort's authority.

## Independent review and simplification

Five simplification lenses were applied in three bounded Codex contexts. Two
redundant production comments were removed. Two optional expression/query
micro-simplifications were left unchanged because they did not justify further
control-flow/signature churn. No proof test was deleted or weakened by simplification.
The CI policy conflict was treated as a defect and corrected, rather than deleting
its guard.

The six-persona code review is `foundation-closure-20261017`, scoped to base
`2c3e9ff` plus the tracked working tree at dispatch HEAD `187abda`.
Correctness, standards, testing, maintainability, reliability and adversarial results
contain no primary findings. Three reviewers identified the same missing integrated
quiet-external maintenance/exit trajectory; the passing supplemental proof below closes it.
Cross-model review was not used, in accordance with the user's Codex-only instruction.
Agreement among these contexts does not claim cross-model confidence promotion.

The completed review receipt is `status: complete`, verdict `Ready with fixes`,
with zero primary/actionable findings and one integrated R4 testing gap. The original
receipt retains its snapshot boundary. A targeted independent test-only review then
found a possible false pass if a due maintenance deadline did not advance; the new
proof now requires strict future advancement, exact production waiting deadline and
no presentation deadline before counting a cycle. The supplemental reviewer confirmed
that correction, then independently reviewed the final fixture and found no blocking
issue. The original full review receipt is preserved rather than retroactively widened.

### R4 complete native scheduling proof

Run `c460e819-cc3d-427f-84fa-06af588378f5` passed the added
`native_external_running_maintenance_stays_quiet_and_presents_child_exit` in 198.398 s.
It opens an ordinary project, admits Play and executes the real Cargo build, source
receipt and READY path. The owner transitions Building -> Starting -> Running,
generation 1. Three due maintenance cycles each advance the exact future process
deadline, retain the same native frame/output sequence, and leave presentation demand
empty. A test-only exit marker in the guarded copy ends that child with code zero;
the production owner observes Idle and the actual native Editor presents it. Normal
graphics and Session/process cleanup complete, and the guarded copy is retired.
This verifies process-exit observation, not graceful Runtime shutdown.

The Editor is visible but initially inactive for this trajectory, because hidden
Windows HWNDs can suppress real redraw delivery. The ordinary child can also show
its window. There is no injected OS pointer/keyboard input or fabricated owner state.

The first added-trajectory attempt `9ad8f293-8d7b-4464-8e14-bbe8829367d6` was stopped
after making no progress before actual child build. It is neither passing evidence
nor a reproduced product defect. The fixture now checks Play admission immediately,
requests its initial successor presentation and has finite phase diagnostics/timeouts.
The interrupted run left no child. Its owned copy is retained under
`target/.nara-supervised-play-canary-177984-1` for diagnosis; a non-destructive move
preserved it after automatic approval rejected recursive deletion. Attempts
`be0b8ce4-0ec8-487e-bcf2-1978a9fcd682` and `c0e92b7e-671e-4147-beda-3d2c42ab4f54`
also stopped during hidden-window startup, before child build. Their copies
`target/.nara-supervised-play-canary-162676-1` and
`target/.nara-supervised-play-canary-185184-1` remain preserved. The final test uses
a visible inactive window, rejects unadmitted Play immediately, and applies its
watchdog even when project Open has not yet produced a frame. These fixture failures
are not product red/green evidence. The passing final run leaves no child or root copy.

# Follow-up

This plan does not claim a real saved/reopened Tilemap workload, large-map cache
performance, an atomic multipart sorting group, or ADR 0092 color correctness.
The audit's measured map/rendering slice remains the next technical consumer.
Linux/macOS native acceptance and a permanently provisioned native CI runner remain
outside the observed Windows/DX12 scope. Local execution does not claim remote CI.

# Citations

- [Completed plan](../../../../plans/2026-10-17-1856-fix-foundation-correctness-closure-plan.md)
- [Audit](../../../../verification/2026-10-17-foundation-and-extension-audit.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
