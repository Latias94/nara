---
type: "Subagent Finding"
title: "Startup preparation facility retry and final Host custody follow-up"
description: "Source-grounded next bounded R4b design, distinguishing activation failure from ordinary pressure and whole Session cleanup from the separate Open Host."
timestamp: 2026-10-25T21:09:30Z
record_id: "8a1cb30c55e442498e0e752712d65439"
producer_id: "codex-root"
run_id: "foundation-editor-private-lazy-20261026"
subagent_id: "01a174d6-1533-7f62-a448-2a0509f38679"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/editor-private-lazy-tasks"
git_commit: "f69ac8785947cd693a25e14e8601f09932ee88f7"
---

# Finding

This is source-grounded design input, not implemented or tested qualification. The persistent
`EditorProjectOpenHost` owns a standalone `StartupPreparationHost` outside every Session. The
current eager pool is allocated on first preparation, then reused for I/O-only startup work.
Merely replacing its constructor with demand activation would retain a permanently failed I/O
kind after `WorkerStartupFailed`; later explicit Open/Retry would reuse it indefinitely.

# Evidence

- `src/project_host/runtime/editor/open/preparation.rs` owns the pool, five capture permits and
  I/O submission. `.into_handle().ok()?` currently erases the rejection reason and input-return
  state. An attempt owns a result handle and may continue through another Open Host; it does not
  acquire thread-bound Runtime authority.
- Actual activation precedes task admission. Failure closes that kind without implicitly retrying;
  partial worker handles still require physical joining. Ordinary pressure, source refusal, body
  panic and cancellation are not activation failure and must not replace a healthy facility.
- Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` initializes global task pools through
  `bevy_app/src/task_pool_plugin.rs`; explicit pool Drop joins workers. Pinned Godot
  `81ab961f91066a63bcbf3ec340962bb1a1ada558` owns singleton/named facilities through
  `core/object/worker_thread_pool.cpp` and `main/main.cpp`. These support explicit physical
  ownership, not a claim that either supplies Nara's responsive retained Open Retry protocol.
- Existing closed-pool and admitted body-panic fixtures do not inject worker creation failure.
  The real worker-spawn injection is currently Tasks-internal test support.

# Recommendation

Keep one reusable owned-lazy facility and at most one failed retirement slot. Refuse the affected
Open immediately with its actual rejection class. On permanent activation failure, stop reuse,
retain the same physical facility and poll its existing close protocol once per maintenance step.
Only observed complete return permits replacement on the next explicit Open; never replace on
queue pressure or migrate already accepted work. A narrow standalone `poll_shutdown` operation
can reuse the existing close owner without a blocking wait loop or another shutdown protocol.

The failed Open and persistent Host may share a private concrete retirement object so either
outer lifetime retains the single physical owner. Keep Tasks reports and unadmitted input-return
failure separate from Runtime evidence; do not invent Runtime participants for a standalone pool.
Application/native sticky errors, Retry admission, maintenance and Exit must consume that evidence.

Qualify actual first-use failure around the first submission, same-Host retry, partial workers,
held physical return, ordinary pressure/healthy reuse and failure propagation. A default-off narrow
test-support seam may expose existing injection without introducing an executor extension API.
Amend ADRs 0008/0052/0118 and the implementation ledger only for the selected, actually tested scope.

# Disposition

Deferred to the next bounded R4b unit. No source changes or Cargo commands were performed by this
sidecar. Also track healthy Open Host final shutdown separately: its pool currently retires through
Drop/reaper, and whole Session close cannot prove physical completion of that external owner.
No facility-generation list, generic recovery registry or automatic retry loop is selected.

# Citations

- [Task deployment and follow-up design](../../../../architecture/task-activity-scope-design.md)
- [Master rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
