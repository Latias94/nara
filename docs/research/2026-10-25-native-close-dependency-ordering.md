---
title: Native Close Dependency Ordering
date: 2026-10-25
status: Source research; comparison and implementation not qualified
---

# R4a: Native Close Dependency Ordering

Date: 2026-10-25 (Asia/Shanghai). Nara HEAD `a0abec0780965c5303386a2da9bc087abd4bbf7b`.

Scope: native close dependency ordering only. No repository edits, Cargo, builds, formatting, installation, Git writes, or nested agents. Concurrent changes were observed and preserved. This is source research, not executed qualification.

## Required Contract

The master plan, `docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md:249-258`, requires comparing typed composite close owners with a minimal sealed completion dependency plan. Owner-local participants, separate quiescence and destructive release, and completion-dependent provider release are explicit requirements. Pending and physically incomplete failure must block provider release; failed-but-complete must retain errors.

Here, a consumer depends on a provider: consumer physical completion must precede provider destructive release. Provider quiescence may happen earlier.

## What The Current API Represents

`crates/nara_app/src/runtime.rs:575-665` separates progress from retained evidence:

| Participant result | Physical obligation | Error evidence |
| --- | --- | --- |
| `Ok(Pending)` | Remains unfinished | No new error required |
| `Ok(Complete)` | Complete | Earlier causes remain |
| `Ok(CompleteWithError { code })` | Complete | Terminal error retained |
| `Err(retryable(...))` | Remains unfinished; may be driven again | Retryable cause retained |
| `Err(terminal(...))` | Remains unfinished; no further callbacks | Terminal cause retained |

The implementation confirms these distinctions: `runtime/close.rs:156-230` tracks `complete` separately from `terminal_failure`; `:233-247` reports every unfinished entry, including terminal failures, and requires all entries complete for ledger completion. `CompleteWithError` leaves the pending list while preserving its cause. A terminal error alone never proves physical completion.

Thus pending, failed-but-complete, and failed-physically-incomplete are independently representable. Reading only historical causes would lose that distinction; consumers must also inspect current pending/completion state.

## Concrete Dependent Native Consumer

`editor/src/platform/imgui.rs` coordinates native targets, renderer, platform and GPU ownership:

- `ImguiTarget::retire`, `:309-324`, unregisters a renderer texture before retiring its GPU view.
- `NativeImgui::shutdown_graphics`, `:1333-1369`, waits for GPU idle, retires Scene/Game targets, shuts down the renderer, then disables/shuts down platform viewports. The first error is retained while later stages continue; subsequent errors are discarded.
- `retire_native`, `:1422-1438`, takes the native aggregate, calls graphics shutdown, logs an error if returned, and retains selected frontend/layout state. It exposes no retained pending native-close owner on this path.

This is concrete composite ownership with manually written sequencing. The inspected paths do not establish whether an individual backend error means physical completion; they do establish that the coordinator does not receive that distinction. No backend resource-safety defect is claimed.

## Three Evidenced Current Gaps

1. **The ledger cannot enforce dependency completion.** Registration stores an ID and participant, with duplicate-ID rejection, but no dependency relation (`runtime/close.rs:93-124`). Driving traverses entries in reverse registration order and continues after Pending or error (`:156-230`). Registering provider before consumer orders their callbacks but does not prevent provider release while that consumer remains unfinished. Overall completion is structural; dependency-safe release is caller convention unless one participant privately owns and gates the entire dependent aggregate.

2. **Quiescence and destructive release have no separate contract.** `begin_close`/`poll_close` report whole-participant progress, not release eligibility (`runtime.rs:949-961`). The concrete graphics coordinator continues to renderer/platform teardown after earlier errors (`imgui.rs:1336-1369`). Its `Result<(), EditorRunError>` cannot tell this coordinator whether release is permitted after failure. This is a missing completion boundary, not proof that every such error is physically incomplete.

3. **Non-Send native controllers cannot participate directly.** `RuntimeCloseParticipant: Send + 'static` is required even though its documentation places callbacks on the runtime owner thread (`runtime.rs:949-952`). An owner-local controller containing genuinely non-Send SDK state therefore needs another custody path. Thread affinity alone and permission to move an owner are different contracts.

## Alternatives And Recommendation

| Choice | Enforcement and fit | Cost and limits |
| --- | --- | --- |
| Typed composite owner | Private consumer/provider fields and explicit states gate provider release on consumer completion. Holds local native controllers and Send workers together. Fits the inspected fixed native aggregate. | Concrete coordinator owns each transition and retained error. Composition must retain the entire unfinished aggregate; ordinary Drop is not a wait protocol. Independent participants cannot add dependencies without changing that owner. |
| Minimal sealed dependency plan | Explicit participant IDs and completion prerequisites; reject missing dependencies/cycles before startup. Quiesce separately; release a provider only after all declared consumers are physically complete. | Requires registration/sealing, local callback storage, retained unfinished custody and a small readiness driver. Declarations must bind actual owner custody. Avoid inferred dependencies, task scheduling, or executable discovery. |

Prefer the typed composite as the first candidate for this demonstrated fixed aggregate. Keep dependent owners private and retain the composite until physical completion. Select the sealed plan only if the same public consumer demonstrates independently contributed participants that a composite cannot accommodate without reconstructing central close knowledge. Both candidates must preserve current error/completion independence and support owner-local state; neither is selected merely by callback order.

## One Public Failure Consumer To Select Them

Use one external public-API native-session fixture with a provider and dependent consumer. The consumer still accesses provider-owned state while closing; the provider records its destructive release. Run the same scenario against both candidate APIs, without engine-private inspection.

Exercise three outcomes: consumer Pending, terminal physically incomplete failure, and `CompleteWithError`. Provider release must stay absent for the first two; the third must permit release while retaining the consumer error. Also verify quiescence precedes release and unfinished owner custody survives return to the caller. Use a local non-Send controller to make the owner-local requirement concrete.

Finally express the same dependency through an independently supplied consumer participant. If the typed owner expresses custody and completion cleanly, keep it. If this requires central owner reconstruction, the sealed plan must demonstrate declaration-order independence and pre-start rejection of missing/cyclic dependencies. These are proposed selection checks, not executed results.

## Upstream Limit

The bounded check pinned `repo-ref/godot` to `81ab961f91066a63bcbf3ec340962bb1a1ada558`. `main/main.cpp:5243-5247` explicitly documents order-sensitive cleanup, but the inspected excerpt did not establish a concrete completion-gating mechanism. No upstream mechanism or equivalent guarantee is claimed.

## ADR Reconciliation Before A Replacement

ADR 0084 already distinguishes owner-thread custody from `Send`/`Sync` and explicitly leaves
pollable NonSend SDK controller close to a separately qualified native-close contract. Removing
the trait's `Send` bound alone would not qualify physical retirement or dependent provider release.
ADRs 0003/0010/0042/0082/0084 must be reconciled for the selected native owner and release topology;
0052 remains authoritative for worker return/cancellation and must not be weakened into native
cleanup evidence. ADR 0008's per-Runtime facility policy is a separate R4b comparison.

This note selects no successor API and changes no Accepted contract. The public dependent-close
consumer above is the next evidence needed to decide whether a concrete typed owner suffices or
independent participants require a sealed dependency plan.
