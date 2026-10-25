---
type: "Session Handoff"
title: "Demand activation merged with measured task isolation direction"
description: "Pins the qualified lazy/shared comparison and local source commit, records the next concrete Editor deployment and retirement obligations, and preserves the full foundation Goal."
timestamp: 2026-10-25T16:46:58Z
record_id: "b4f47bf5eb9346d092120ce798f90f90"
producer_id: "codex-root"
run_id: "foundation-task-demand-20261026"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "main"
git_commit: "7d8defb"
---

# Summary

Full foundation Goal remains active without a budget or scope reduction. Optional demand
activation and its matched Image comparison are committed as `7d8defb refactor(tasks)!: retain
demand-activated workers through physical close`, source
`7d8defba1643d1f6c969b04c42d1f1172eaca200`, and fast-forwarded into local `main`.
No push/PR. Only pre-existing untracked `vendor/` remains. All three subagents completed and
were closed; no owned Cargo/test process remains. Existing target on F: was reused with jobs1.

# Verified State

- Tasks/Image all-features: **198 passed**, one ignored measurement excluded, nextest
  `6a629678-ca78-4157-9a36-e189a502b9dc`. Tasks-only84 separately passed
  `9928cce2-e1e1-40d0-a891-f118d18a8783`.
- Independent renamed-root/direct Tasks: **2 passed**, nextest
  `ab5297c0-3130-4c6b-b221-7985c0e8ff85`; includes actual public lazy first-use activity.
- One release measurement passed at `acf39ada-7c6d-4f5b-9b50-08a087c1c7e0`, with sixteen complete
  trials per candidate. Same two Runtime/Image consumers, total two I/O workers, eight pending
  slots, seven actual PNG jobs/seven separate probes/two held jobs per trial. All idle kinds stay
  dormant. Actual JSON validates matched active counts `[1,1]` versus `[2]` and successful close.
- Conservative owner observation confirms all 16/16 PNG admissions per candidate preceded
  held-work release, with zero exclusions. Private sibling PNGs complete before release16/16;
  shared0/16. Under pressure, median PNG wall is3.109ms private versus30.625ms shared; separate
  probe admission-to-start5us versus30,238us. Ordinary timings are noisy and not general gains.
  The [report](../../../../benchmarks/2026-10-26-task-facility-locality.md) owns all counts,
  P50/P95, timestamp attribution and limitations. No P99/native/product performance claim.
- Final locked workspace all-features/all-targets check and focused all-target/all-feature
  correctness/suspicious Clippy pass. Formatting, changed Markdown links, whitespace and memory
  validation pass with existing warnings. Logs/raw summary stay ignored in target.

# Open Threads

- `TaskWorkers` private module now owns lazy activation and all worker handles. Activation holds
  its handles mutex, reserves live workers before OS spawn, and retains successful partial handles.
  Physical close atomically prevents more activation and closes queues without waiting for spawn.
  Join polling uses try_lock; abnormal Drop passes the same TaskWorkers Arc to retained reaping,
  not a possibly incomplete handle snapshot.
- Scope admission happens after activation. Scope close may finish when nothing was admitted;
  the late spawn is refused, its captures still belong to the submitter. Startup failure rejects
  as `WorkerStartupFailed`, closes only that kind without implicit retry, and retains physical
  responsibility. Eager construction uses the same code and preserves partial-failure evidence.
- Independent review found no confirmed production bug. Its actual-join and post-reservation
  race test gaps were filled/rechecked; cfg(test) join probes observe real join return, not exit.
  Measurement review required observed admission-before-release; root added and applied that filter.
- The result challenges unconditional shared facilities. Scope custody is correct but not a
  progress reservation against arbitrary blocking closures. Lazy private isolation is the stronger
  next interactive product candidate at matched capacity. Do not blindly proceed with a shared
  default merely because the preceding source study designed its bridge. Do not invent a global
  priority scheduler; priority cannot preempt blocked closures.
- Current public default `TaskPlugin` and product allocation remain eager/unchanged. The optional
  `try_new_lazy` constructors are implemented, not the complete deployment successor. Do not keep
  duplicate names forever for compatibility alone; select placement ergonomically from actual
  required early-capacity versus first-use failure semantics.

## Concrete Product Gaps From Independent Source Trace

The trace used Git source `c62ff32` while root edited Tasks; its full obligations are now in
`docs/architecture/task-activity-scope-design.md`.

- Trusted Editor authoring/preview and same-process Play are the real integration consumer;
  ordinary supervised Play is another process. Reuse
  `trusted_editor_play_travels_b_then_same_a_without_losing_preview_generation_or_request_authority`
  in `src/project_host/runtime/tests/editor_scene_travel.rs` and add Image/Save/held-work progress.
- `EditorAuthoringRuntime::shutdown_tasks` currently consumes only physical `TaskShutdownReport`.
  A shared deployment must separate activity report from Session's final physical join report.
- `content_refresh.rs` currently directly replaces authoring owners and can drop a failed
  replacement. Retain the predecessor/failed replacement and gate another refresh through actual
  retirement; preserve exact `HostCustody::transfer_to` for file custody. Existing quarantine is
  safety fallback, not successful ordinary close evidence.
- Authoring constructors currently map startup failure to `DiagnosticReport`, losing the caller's
  explicit retry handle. Preserve the failure owner through Open/refresh failure where required.
- `EditorProjectSession::advance_close` must cover Preparing/Starting/AwaitingGameTarget/Active/
  failed Play owners using existing trusted-workspace retirement, not only an empty ProjectHost
  slot. Scene-request cancellation itself completes early and relies on Tasks for capture return.
- If shared deployment remains justified, preserve pure replayable PluginPlan configuration.
  `PluginDefinition::exact_eq` does not compare captured facilities Arcs; neither pointer identity
  nor an unrepresented factory capture belongs in canonical plugin config. Prefer fixed typed
  candidate input after pure prepare/before original commit rather than a service registry or an
  arbitrary App callback. Registered scope participant must retain the facilities Arc too.
- A shared Session would choose full exact TaskPoolConfig once, outside the Play slot, refuse
  later mismatch, keep it across Stop/Restart/content refresh, and use nonblocking physical close
  progress after all activities. Existing only-blocking public physical shutdown must not be put
  into a per-frame Editor close poll. These are conditional shared-work obligations, not an
  instruction to add new plumbing if the measured private deployment avoids needing it.

# Next Action

1. Recheck state and preserve vendor. Start the next bounded branch from local main; same target,
   Rust+1.95.0, strict serial Cargo, jobs1. Do not repeat expensive release compilation or old
   cohorts unless subsequent code affects their scope.
2. Select/implement the actual Editor lazy-isolated placement and truthful first-use/startup
   failure semantics using the measured evidence, with real owner/refresh/Stop/Restart/Save/Image
   and required native qualification. First decide placement rather than adding a universal
   startup-policy field by reflex. Explicit eager acquisition is still valid when a named Host
   needs startup failure before publication.
3. Repair the owning authoring refresh/failure retirement contract instead of hiding it under
   scope cancellation or quarantine. Include public close evidence and consumer failure cases.
4. Complete R4b product qualification, then R4c and dependency-ordered remaining waves. R3,
   broader R1/R2/R5, assets/delivery/render and full Goal remain open. Do not infer a total progress
   percentage or reduce Goal success to these implemented Tasks units.

# Citations

- [Verification](../../verification/2026-10/2026-10-25T164306Z-demand-activation-custody-and-matched-lazy-shared-image-comparison-c5d3a638a3884807b489a85f0b7b0b1a.md)
- [Release report](../../../../benchmarks/2026-10-26-task-facility-locality.md)
- [Activity/product design](../../../../architecture/task-activity-scope-design.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
