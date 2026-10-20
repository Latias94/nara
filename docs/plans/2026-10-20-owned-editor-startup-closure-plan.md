# Owned Editor Startup-Closure Preparation

Status: Completed in `eb1ee98` within the
[qualification scope](../knowledge/engineering/verification/2026-10/2026-10-20T032446Z-owned-startup-closure-publication-and-responsiveness-qualification-f187b61104d4443187cea8b9bc392460.md).
Follows the qualified Host lifetime correction at `fe3ad96`.

## Scope And Contract

The v1 ordinary Open measurement places startup-closure owner calls above the private 8 ms
investigation trigger at 4,096 empty entities. Move that concrete capability-bound read, decode,
dependency traversal and pure schema classification to existing task workers. Preserve the
`EditorProjectOpenHost` begin/advance interface and its linear attempt, source budgets and safe
diagnostics. Adapter admission, native registry/Plugin construction, Workspace publication and
session/runtime creation stay on the owner thread.

Keep prepared and in-flight startup states under the existing StartupClosure phase. Polling
cannot block for a filesystem call. Only an actual Completed result belonging to the live attempt
may advance to session publication. Dropping/cancelling an attempt revokes publication, without
claiming OS-call preemption; existing task retirement keeps running captures alive. Do not put an
Editor session in a task payload or add a generic operation service or async trait.

Open precedes the authoring runtime. Reuse a lazy, explicitly small standalone TaskPools owner
inside the Open Host rather than creating an App or a pool per attempt. Existing task rejection,
standalone abandonment and physical reaping contracts remain authoritative. Limit admission so
cancelled uncooperative work cannot create an unbounded queue. Measure cancelled completed-result
disposal separately; no blanket caller-destructor latency guarantee is implied.

The application preserves retained replacement sessions, persistence-before-preparation and
Save-before-refresh ordering. Unchanged pending polls do not request repaint. The existing native
Opening maintenance deadline observes progress; no new wake/executor framework is required.

## Lane Map

Mode: execute_direct. Shared branch `refactor/foundation-contract-hardening`.
Root alone runs Cargo/fmt/stage/commit, serially with shared target and CARGO_BUILD_JOBS=1.
Preserve unrelated vendor/ and myeasylog.log. No push or PR.

| Lane | Writable scope | Responsibility |
| --- | --- | --- |
| Lower worker | `src/project_host/runtime/editor/open.rs` and private `open/` children | Owned startup preparation, bounded worker use, cancellation and deterministic fault tests |
| Root | `editor/src/app.rs`, root runtime tests, measurement, plan/contracts | Actual progress observation, owner integration, measurements and serialized verification |
| Consumers | `editor/` tests and test helpers except root-owned app.rs; root tests except measurement; reference/module consumers | Replace fixed phase-count assumptions with bounded terminal waits and qualify Cancel/replacement |
| Review | Read-only | Attempt affinity, cancellation/disposal, task pressure and retained-project counterexamples |

## Acceptance

1. Deterministically hold startup preparation on a real worker and prove owner advance returns.
   Drop/cancel, rejection, panic and delayed completion cannot publish a partial or late session.
2. Keep schema/source/dependency preservation and limit diagnostics. Native admission remains owner
   work. No new cross-file atomic snapshot or strong CAS claim.
3. Exercise ordinary initial Open/Cancel/retry, replacement/Cancel, Close/Exit and quiet pending polls.
   Run affected root/public, Editor, independent module/reference and selected native consumers.
4. Record v4 first/max startup owner calls, full completion observation and completed-candidate
   abandonment separately in three optimized processes. Keep earlier measurement data frozen.
5. Independently review, update owning contracts and standalone HTML reports, capture actual
   verification/limits and commit. Reconcile the remaining registered correctness findings again.

## Disposition

Final engine/public 131, Editor/panel 195, selected actual Windows/DX12 native 4, independent Scene
module 3, independent panel module 2, reference-game 4 and runtime-ui-off 1 tests pass. The reference
cohort includes actual supervised Play/Restart/Stop/Exit. Workspace all-features all-targets check,
formatting and diff checks pass. Three optimized processes retain 864 v4 rows each; independent
recomputation agrees with all 162 summary groups and report cells.

A final same-owner sweep confirms a separate Image metadata cleanup abandonment gap. The
[Image publication custody plan](2026-10-20-editor-image-publication-custody-plan.md) prioritizes
that correctness repair next. This qualification does not repair or claim coverage of that owner.
