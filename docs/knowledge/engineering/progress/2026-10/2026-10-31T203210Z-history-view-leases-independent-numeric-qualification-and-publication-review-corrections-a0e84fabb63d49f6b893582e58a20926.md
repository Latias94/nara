---
type: "Work Progress"
title: "History View leases independent numeric qualification and publication review corrections"
description: "Six independent GPU history cases pass; publication cleanup, program budget and final storage corrections remain in final integration."
timestamp: 2026-10-31T20:32:10Z
record_id: "a0e84fabb63d49f6b893582e58a20926"
producer_id: "codex-foundation-main"
run_id: "render-history-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "9f79d51"
---

# Summary

Continue the complete active foundation Goal and current render unit. All current View/history code
is uncommitted above local `main = refactor/foundation = 9f79d51`. Do not commit/merge until remaining
review/native/consumer checks close. `vendor/` remains the original untouched untracked directory.
No remote push/PR. Serial Cargo/jobs1/shared root target remain mandatory.

# Details

The first [integration record](2026-10-31T200950Z-captured-view-lifetimes-and-production-history-candidates-in-integration-adb17185d2534fb5bd768a08ed6e22dc.md)
describes the neutral View owner, logical declarations, immutable Bindings and Host transaction.
Subsequent concrete corrections are now on disk:

- A shared Feature declaration previously retained rows for every destroyed View. Add explicit
  CPU `WgpuHistoryView` retained in original topology/capture, with its neutral ID and Arc lifetime.
  Slot tokens and cached Bindings keep only Weak leases. Live rows track BOTH View and effect
  declaration; a pending original input/candidate retains the topology, while a disposed View can
  reclaim rows without waiting for a shared static effect to disappear. Foreign live View leases
  for the same key reject in pin and reset. No neutral Copy-ID break, global registry or absence
  inference was added.
- `WgpuPublishedFrame::final_texture` returns the exact selected typed final output for ordinary
  graph imports, alongside exact per-slot texture/buffer outputs and the image receipt.
- Laplace found a P1 where old Bindings Drop could panic after publication and bypass initial
  target storage exit. Old retirement values now release only after target state/result are
  complete, under isolated cleanup. Group `cleanup_panicked` and Host cleanup-fault counter keep
  the successful publication distinct from destructor failure.
- Its follow-up found the same problem in the success exit's implicit candidate Drop. Public
  candidate/UI/one-shot image paths now explicitly preserve success while disposing candidate and
  discarded output metadata. Runtime retains outputs through presentation, then isolates disposal
  of original input/program/candidate/results and returns the original presentation result.
  The test-only `candidate::COMPLETE_BEFORE_RELEASE` ordering flag polls the REAL original prefix
  before candidate disposal; it does not forge completion or change production behavior.
- Oversized caller `GraphLimits` could allocate above the candidate's reservation before final
  rejection. Program builder and required-cost entry now clamp effective metadata to hard64MiB
  before vectors/compile. Capture inline256KiB and staged binding512KiB prechecks precede their
  engine wrappers. Candidate admission reserves full64MiB plus bounded2MiB auxiliary envelope.
  No TLS admission kernel was introduced. Unrelated caller-owned programs/heaps remain outside
  this per-candidate envelope. A focused excessive-access test was added but not yet run.
- A fresh target's baseline texture did not support a valid storage final image. Host now checks
  actual shape/format/usages before reusing that storage, releasing an incompatible unsubmitted
  reservation before normal program allocation. Compatible preflight refusal still preserves it.

Actual additional evidence:

| Scope | Actual result |
| --- | --- |
| Complete WGPU library and public UI |365 passed /42 skipped, run `830a0e39-e8ba-4105-9528-fea20a9439b8`,117.688s. Before the review corrections above. |
| First independent four-chain numeric case |1 passed /4 skipped, run `e81f7082-8238-4393-ad00-050110e4b0c5`,1.093s. |
| Independent initial full history |5 passed /0 skipped, run `635f84cc-bbb7-4c6e-b36b-b80cf1b97ac9`,4.977s. Before View-owner refinement. |
| Independent history with View leases |6 passed /0 skipped, run `321d3a23-7940-47de-b912-945186d3e022`,6.151s. Includes513 actual disposed View incarnations sharing one effect, exact final output, full numeric reorder/replacement/reset/refusal/A-B/retained/recovery paths. Before final fixture window-format generalization only. |
| Minimal WGPU library |No-default check passed3.61s after earlier cleanup/budget corrections, before View-owner field change. |
| Portable WGPU library |No-default wasm32 check passed3.33s at that same earlier scope; compile only. |
| Root neutral library |No WGPU, `project-workflow-cli,tooling,presentation-2d,presentation-ui`, passed19.08s. |
| Editor all-feature library |Passed25.76s after View-owner changes; before final Runtime consumed-value cleanup refinement. |
| Root affected consumer cohort |LIVE exec48978, log `nara-render-history-root-consumers-tests.log`; no terminal result yet. |

All new independent files are under `tests/fixtures/render-policy-consumer`: `tests/history.rs`,
`tests/support/accumulation.rs`, `accumulation.wgsl`, `history_readback.rs`. They use actual production
Host and an independent f64 CPU oracle. Readback uses typed imports and Host image-use custody.
Hypatia migrated topology to retain the actual View owner and added the513-lifetime test; source
is frozen and that agent is closed. Existing scalar policy and all manual identity consumers have
been migrated by Descartes, also closed.

Lovelace's three new native owner regressions in `host/publication_tests.rs` are source-ready and
frozen, agent closed: old Bindings Drop success/initial-storage/pixel preservation; extra final
storage usage on first target; last Capture Drop after original prefix completion. They have not
compiled/run yet. Their readback uses submit_image_use, no raw independent Queue submit.

Boole's actual Runtime temporal window consumer is source-ready and agent closed:
`examples/history_window.rs`, `tests/history_window.rs`. It uses actual two Camera extraction,
normal ECS writer/seal/Runtime/window, three presented receipts, exact original captures and
retirement. The original shared fixture required linear RGBA8 final format; parent just generalized
it to actual RGBA8/BGRA8 linear/sRGB and per-destination-format Composite pipeline state. That
fixture change and the window executable are uncompiled/unexecuted as of this record. No root
production special-case was added.

Gauss `01a193a7-c0c3-7402-b404-499ed947458d` is the only live read-only final reviewer, examining latest
cleanup/budget/View lease/atomic publication sources. Other writers and reviewers are closed.
Final formatting has not run while writers were active; inspect complete diff first.

# Next Action

1. Poll exec48978 before another Cargo. Preserve final result even on observation timeout; do not
   restart a live command. Current compile reached `workspace_play_runtime` at the last read.
2. Run explicit DX12 native owner `host::publication_tests::` plus `host::history_tests::`, focused
   metadata regression and policy/state tests. Fix actual failures without weakening publication
   success or immutable predecessors. New native tests may have fixture API mistakes; no result yet.
3. Build `history_window` using standalone locked manifest/root target, then run its ignored native
   subprocess test. Run independent history6 after final format-state generalization, scalar/image/
   direct-program controls as affected. The window path is the same policy, not a new executor.
4. Collect Gauss final review and owning fixes. Reconcile source/docs if any contract changes.
5. Inspect full diff, format root and affected independent workspaces serially, run appropriate
   correctness Clippy/all-target consumers, affected Editor native trajectories, current minimal/
   wasm checks. Refresh honest qualification, ledger/plan and memory, then precise commit and local
   ff main. Keep full0121 and complete foundation Goal active; no remote publication.

The complete render outcome still includes actual independent streams/two-window/image, complete
source/processed interleaving, exact display/UI/Input joins and Nara-drawn editable Inspector. This
unit is not complete merely because six offscreen numeric tests passed.

# Citations

- `docs/architecture/render-program-production-design.md`
- `docs/architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md`
- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `tests/fixtures/render-policy-consumer/README.md`
- Predecessor progress `adb17185d2534fb5bd768a08ed6e22dc`; registration head at entry
  `5f44bbe538f14c67aaaa5b8d5d5422f2`.
