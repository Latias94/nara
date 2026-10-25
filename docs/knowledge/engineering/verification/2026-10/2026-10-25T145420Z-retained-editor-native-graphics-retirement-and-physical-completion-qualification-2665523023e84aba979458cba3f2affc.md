---
type: "Verification Evidence"
title: "Retained Editor native graphics retirement and physical completion qualification"
description: "Records exact GPU drain custody, partial construction return, phased native retirement, exit decision continuity and scoped Windows DX12 evidence; full foundation and remaining R4 work stay open."
timestamp: 2026-10-25T14:54:20Z
record_id: "2665523023e84aba979458cba3f2affc"
producer_id: "codex-root"
run_id: "foundation-native-retirement-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/editor-native-retirement"
git_commit: "ccc1318"
---

# Verification

Concrete R4a Editor Host graphics retirement, descended from `ccc1318`, on branch
`refactor/editor-native-retirement`. Source and tests implement the fixed typed aggregate selected
by the preceding local-close comparison. This record does not close the full foundation Goal.

# Result

The Host retains an incomplete predecessor rather than dropping it after one best-effort close.
It blocks installation until the exact device/queue drain, renderer, Scene/Game view and platform
phases complete. Partial initialization returns the remaining native aggregate; failed SDK native
attach returns its original docked renderer instead of cleaning it up inside an error conversion.
Renderer success is idempotent for its original Context, rejects foreign Contexts, and releases
Nara binding clones and secondary observations after actual SDK success.

The drain captures device/queue before construction, flushes pending uploads once on normal close
and polls without GPU waits. Invalidated-device discard explicitly destroys the captured device
and skips unsubmitted uploads, but still requires actual `QueueEmpty` for accepted work. An
interrupted destroy/flush leaves a poisoned token; no arbitrary unwind recovery is promised.

Semantic Exit retains live UI for dirty decisions and maintenance without unrelated events. Only
observed `exit_ready` permits final native retirement/exit. Cancellation reads the Application's
current custody, preserves dirty data and returns to a live frontend; a GPU reconstruction can
retain a still-needed exit decision. Renderer refusal retains Context/provider custody; SDK errors
reported after physical renderer release remain observable through a subsequent successful phase.

ADRs 0078/0118 adopt this concrete Editor Host contract. It does not move Editor graphics into a
Play Runtime or add a universal native close graph, new executor, compatibility reader or migration
guide. Existing surface/provider leases retain their resource-owner release/acknowledgment rules.

# Evidence

Environment: Windows, actual DX12 adapter, Rust `+1.95.0`, `RUSTUP_TOOLCHAIN=1.95.0`,
`CARGO_BUILD_JOBS=1`, `CARGO_TARGET_DIR=F:/SourceCodes/Rust/nara/target`. All Cargo was serial.
GPU runs set `WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.

| Check | Actual result |
| --- | --- |
| Initial Editor tests compile | Passed; later affected source recompiled by native runs |
| Adapter-required `nara_render_wgpu --features editor-imgui --lib` | 67 passed, 2 native tests skipped; initial `aa1e7e23-21ce-46ad-9fbc-856c311f9875`, final adapter source `c86dcd73-a668-429a-bee6-55ce23d216eb` |
| Ordinary Editor library | 198 passed, 25 native/helper tests skipped at that source; `6c2b31d1-5b9e-4ac5-979c-a7b50c73b79d` |
| Initial focused native four | 3 passed, one failed before lifecycle because the fixture used Published authoring API on a native Scene; `dbd81a95-9707-493f-86e2-6cde0e9507c6` |
| Final new native four plus critical existing three | 7 passed, 220 unselected/ignored tests skipped; `fc7c3802-aaa0-4b66-bbef-a8db0a8769d9` |
| Final crossing-state correction and migrated native callers | 11 passed, 217 unselected/ignored tests skipped; `e1df9a64-33cb-4dfb-8241-a5b85b03fa26` |
| Actual secondary presentation bridge | 1 passed, 375 unselected/ignored tests skipped across two binaries; `409a40b0-b36e-4728-9b67-67467db9679b` |
| Final ordinary Editor before caller migration | 198 passed, 29 native/helper tests skipped; `938cbe29-81ec-4a66-b533-ebf79c1df18e` |
| Final workspace locked all-feature/all-target check | Passed after caller migration, 15.58s; existing warnings remain |
| Final Editor/WGPU all-target correctness/suspicious Clippy | Passed after caller migration, 17.28s; existing style/large-error/type-complexity warnings remain |
| Root formatting, changed local document links and diff whitespace | Passed |

The final seven actual trajectories are:

- `native_close_before_first_presentation_retires_before_exit`: actual `CloseRequested`, prepared
  semantic Exit, successive maintenance callbacks without redraw, initialization drain, Context
  destruction and no native owner when the event loop exits.
- `native_dirty_exit_cancel_keeps_live_owner_and_returns_to_hold`: public native Scene AddEntity
  patch creates a real dirty revision. A retained Host error requests Exit; the actual decision
  remains operable. Public Cancel preserves the same Context/backend and dirty document over three
  production maintenance callbacks without re-requesting Exit or rendering.
- `native_callback_drift_retains_custody_until_retry_then_replaces`: clearing the exact installed
  renderer callback produces real SDK teardown error after renderer-resource release. The Host
  retains the adapter/Context, rejects installation, retries to success, destroys the predecessor
  before replacement, and keeps the original callback diagnostic.
- `native_partial_initialization_preserves_fresh_and_recovered_frontends`: a test-only omission of
  native platform enablement exercises actual SDK native attach refusal after renderer creation.
  The Host retains the returned renderer/drain/Context, rejects another install, retires it, and
  preserves both Fresh and Recovered frontend/layout identities across later explicit installation.
- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`.
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`.
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`: actual secondary
  presentation, independent view lifetime, device destruction, paused recovery and new presentation.

The final eleven-case run retains all seven and adds
`native_dirty_exit_during_replacement_rebuilds_decision_frontend`,
`native_composed_panels_keep_independent_drafts_through_gpu_reconstruction`,
`native_manager_widgets_recover_cancel_replacement_and_resolve_dirty_project_close` and
`native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source`. The crossing-state
case uses real SDK callback drift to keep retirement observable even on a fast adapter; it retains
the queued successor, proves predecessor destruction, preserves the exact unsaved source revision
and Cancels through the public decision before returning to stable maintenance.

The adapter's new GPU tests include pending initialization-write readback, returned renderer on
native attach failure, constructor failure with no renderer but retained drain, normal flush racing
device destruction, explicit discard with accepted-work callback, foreign-Context refusal, binding
release and same-Context successor-safe repeated shutdown. There is no pointer/keyboard injection,
foreground activation or visible native test window.

## Review And Corrected Findings

- Correctness review `01a17373-c048-72e2-9472-5cf87e335b18` found two P2 lifecycle issues: retiring
  UI before a dirty close decision, and losing maintenance wake between `CommitPrepared` and the
  subsequent observed `Applied`. The successor distinguishes `AwaitingExitReady` from final `Exit`,
  retains UI, uses Application-owned pending-exit state, and maintains wake deadlines. Ordinary
  CloseRequested uses the same entry. Independent source recheck closes both; the final clean
  exit and dirty Cancel native trajectories execute their discriminating cases.
- API review `01a17382-54ea-7e91-994b-b7fcd041180e` found a P3: a closed adapter returned success
  for a foreign Context. Initialization now stores `ContextId` and checks it before idempotence;
  the actual GPU test proves rejection and harmless repeat shutdown with a same-Context successor.
  The review also narrowed ownership wording to owners actually returned by the SDK and checked
  the conservative poison state. Independent correction review found no new issue in that scope.
- Adversarial review `01a1738d-1ba4-7af0-83c0-4c8c004d9373` found two further P2s before commit:
  dirty Exit could overwrite an already queued rebuild and lose its decision frontend; older
  native test callers projected retained construction failures to strings or cleaned only the live
  slot. The exit transition now preserves queued reconstruction as `InstallForExit`, and failed
  reconstruction preserves the decision while awaiting an explicit retry. Shared test cleanup
  consumes retained constructor owners and completes both Host slots before Session disposal;
  incomplete cleanup fails stop. Existing helper migration and the crossing-state native regression
  pass the final eleven-case execution above. The same reviewer independently rechecked both P2
  corrections and all 14 migrated callers, closing both within their scoped source follow-up.
- Worker `01a17366-f6da-7c73-98e0-77e846dd940f` implemented the adapter slice; worker
  `01a17365-3024-7913-881e-fcc6121140d7` implemented the focused native fixture. Parent integration
  corrected a WGPU 30 mapped-range `Result` access before execution and replaced the unsupported
  Published-authoring fixture preparation with the native public Scene patch. Those initial
  compile/fixture failures are not product regressions or omitted qualification.
- Worker `01a17395-31e5-7343-87d4-acc85b1b81aa` migrated the 14 existing native callers to the
  shared custody-aware helpers and a finite asynchronous replacement wait, retaining their
  original content, receipt, object/draft and close assertions. Parent compiled all targets and
  ran the relevant eleven native workflows afterward. No full nine-persona review receipt is
  claimed; the actual scoped reviewers and their findings above remain the review record.

# Follow-up

This qualifies the named Windows/DX12 native workflows, not arbitrary SDK faults, native Linux/macOS,
OS window destruction, IME/accessibility, foreign sidecar recovery or detached outstanding-epoch
shutdown. Production synchronous renderer frames exclusively borrow Context; no unsafe Context
alias or leaked frame is introduced to manufacture that test. SDK-caught callbacks that return
errors differ from an arbitrary panic escaping through the Host; no exception-containment claim.

Normal close retains the event loop. A terminal event-loop failure has one finite local cleanup
attempt and fails stop if SDK owners remain incomplete rather than destroying them or reporting
clean completion. There is no generic native quarantine. Bounded phase errors expose truncation
and overflow, but cannot restore secondary errors already discarded by SDK internal first-error
retention.
The existing device-loss lifecycle explicitly drains before its controlled `Device::destroy`.
Its passing result does not qualify arbitrary physical-device failure with in-flight work; the
adapter separately checks accepted-work completion across explicit destruction without converting
loss itself into completion.

R4b activity/facility sharing, R4c execution phases, dynamic-code retirement, broader Behavior/Prefab,
assets/delivery/rendering and the full foundation rebaseline remain open. No completion percentage
is inferred from this test count.

# Citations

- [Selected close design](../../../../architecture/native-close-ownership-design.md)
- [Native SDK outcome inventory](../../../../research/2026-10-25-native-graphics-retirement-outcomes.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
