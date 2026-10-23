---
type: "Verification Evidence"
title: "Post-boundary U1 U2 lossless outcomes and disposal-close qualification"
description: "Current-source regression, Host bookkeeping, task disposal custody and exact native scope for post-boundary owner repairs."
timestamp: 2026-10-23T05:58:44Z
record_id: "3f13b79a544a478b974d062c99662a12"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "c48e45c"
---

# Verification

U1/R1 and U2/R2 under the post-boundary plan, integrated over `c48e45c`. The implementation
commits own the subsequent source deltas; this baseline is not claimed to include those repairs.
U1 is `7fc9458`; U2 is `7502545`. Their independently reviewed changes preserve the same source
state exercised by the combined owner/Host and native cohorts below.

## Repairs and Counterexamples

- Runtime scopes return a normally produced output plus post-operation fault. Only entry errors
  mean not invoked; typed owned-port entry retains the original input until admission succeeds.
  All driver/admission consumers migrate without a general lossy output conversion.
- Real Game View completion is reconciled before the Session observes a sticky fault. Successful
  completion clears its in-flight slot once; domain/entry rejection returns the original completion
  and UI outcome allocation. These tests call the actual port and Session, not a fabricated result.
- Image transaction and hydration register the actual v2 publication as last-good after a fault;
  task admission returns its live owner. Runtime edit acceptance is recorded before fault handling.
  A new real-consumer test additionally exposed repeated fault cancellation discarding an already
  terminal edit result; that result now remains until acknowledgement.
- Task disposal custody begins under the queue lock before admitted work leaves it. Destructors
  run outside the mutex. Physical close waits for caller disposal after worker joining and retains
  bounded cleanup-failure evidence independently of live custody.
- Close snapshot last-result destruction and reaper failures propagate through the typed report,
  CompleteWithError, failed Stop and actual Host diagnostics. Normal business Err stays distinct.
- Winit records a post-retirement Runtime fault without treating successful physical retirement
  as failure; it still waits for native destruction and pending Runtime close.

## Executed Evidence

All Cargo commands were serial with `CARGO_BUILD_JOBS=1`, Rust 1.95.0, the lockfile and the existing
target directory. No cached-artifact probe substitutes for these results.

| Scope | Result |
| --- | --- |
| New owned-output regression, before implementation | Failed on current source because a normal return became Err; rerun with explicit fixture close reproduced that failure without a quarantine-abort artifact |
| App/FS package nextest | 211 passed, three existing privileged FS cases ignored |
| Tasks/Reflect/Scene/Winit library cohort | 312 passed, four ignored before the final snapshot regression was added |
| Input/Gameplay/Transform/UI libraries | 205 passed, none skipped |
| Full-feature root/Tasks/Reflect/Scene/Winit libraries | 670 passed, five ignored; includes the final snapshot regression and real Host propagation |
| New Game View/edit/Image/Host targeted cohort | 7 passed after correcting terminal edit-result retention |
| Root runtime_instance/runtime_driver_boundary/workspace_play_runtime/image_import_limits | 90 passed, none skipped, including compile-fail scope-escape cases |
| Independent Reference Game desktop consumer | 38 passed across first_wave, asset_task_flow, desktop_flow and desktop_render, none skipped |
| Windows/DX12 critical native Editor group | All three selected tests executed and passed; 197 other tests excluded |
| Workspace all-target check | `cargo +1.95.0 check --workspace --all-targets --locked` passed with existing warnings |
| Scoped correctness/suspicious Clippy | App/Tasks/Reflect/Winit all targets passed; advisory large-error/type-complexity and other existing warnings remain |

The root feature combination was `desktop-winit,presentation-2d,presentation-ui,
project-workflow-cargo-facts,render-wgpu,runtime-ui,tooling`. The 670-test command selected these as
qualified `nara/...` features together with `-p nara -p nara_tasks -p nara_reflect -p nara_scene
-p nara_winit --lib --locked --test-threads=2 --no-fail-fast`.

The native command used `WGPU_BACKEND=dx12`, `-p nara-editor --lib --locked --run-ignored only`
and one test thread. Its exact selected names were:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`

These run real hidden windows and presentation. Fault injection itself is proven by the
deterministic owner/Host tests; healthy native trajectories do not claim an actual GPU-driver fault.
The Reference Game used its own locked workspace with `--features desktop` and the shared target
directory. Its independent dependency versions compiled from source; this is not an all-Reference
suite claim. Final formatting covered both the root workspace and that separate consumer workspace.

## Independent Review and Limits

Independent read-only Codex reviews identified the production assertion, incomplete Image
bookkeeping, Winit early-close classification and missing decisive completion/snapshot regressions.
All were corrected. A fresh bounded final reviewer found no remaining blocker and confirmed the
real-owner test seams. Native multi-agent dispatch rejected arguments before launch, so the reviews
used the installed Codex CLI; some follow-up CLI runs returned no final message and are not counted
as completed reviews. No outside model was used.

No new Linux/macOS native execution, comparative benchmark or arbitrary panic recovery is claimed.
Negative Game View fixtures deliberately close the lower Runtime to clean up an impossible Session
state; they prove input retention, not automatic recovery of that state. Cleanup failure remains
conservative at the existing FnOnce unwind boundary. Escaped result ownership and process abort
remain outside physical-close completion guarantees.

## Sources

- [Plan](../../../../plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md)
- [ADR 0052](../../../../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
- [ADR 0076](../../../../architecture/adr/0076-play-runtime-debug-control-and-observation.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Runtime owner](../../../../../crates/nara_app/src/runtime/owner.rs)
- [Task close tests](../../../../../crates/nara_tasks/src/tests/close.rs)
- [Real completion tests](../../../../../src/project_host/runtime/tests/game_view_outcomes.rs)
