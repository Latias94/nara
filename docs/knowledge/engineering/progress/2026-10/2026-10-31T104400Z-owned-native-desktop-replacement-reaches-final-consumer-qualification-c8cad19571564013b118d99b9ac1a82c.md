---
type: "Work Progress"
title: "Owned native desktop replacement reaches final consumer qualification"
description: "Work Progress for Owned native desktop replacement reaches final consumer qualification."
timestamp: 2026-10-31T10:44:00Z
record_id: "c8cad19571564013b118d99b9ac1a82c"
producer_id: "codex-foundation-main"
run_id: "render-owned-native-session-2026-10-31"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "cd35da0"
---

# Summary

The entire foundation Goal stays active. The complete owned native desktop replacement remains
uncommitted on `refactor/foundation`, based on `cd35da0` shared with main. All workers/reviewers are
closed, source is integrated and formatted, and final native Editor/consumer qualification is in
progress. Do not reset authorized changes or touch unrelated `vendor/`; no remote publication.

# Details

Implementation now includes the actual owning Winit session, retained original pump/registry/
windows, original CPU WGPU capture/result and close authority, outside-World GPU execution,
post-Host UI/first-present/READY, direct startup observation, finite telemetry seal and separate
Runtime/native budgets. One common `project_host/desktop_run.rs` owner supports minimal code-first
products; Source/Processed construction conditionally adds original content/configuration custody.
Reference candidate/render probes and native examples use it. No borrowed runner, GPU Resource,
scheduled submit, legacy wrapper or source-required code-first fallback remains.

The final owner cleanup deletes `WindowSurfaceRetirementDriver/Error` and the permanently refusing
World operation. `WindowPresentationRequirement` is only CPU product intent for duplicate/backend
admission; concrete physical retirement remains with the session. ADRs0032/0042/0078 and the selected
design/current guide were reconciled. The old ADR0078 baseline is explicitly historical.

Review corrections are implemented and independently rereviewed: original UI permit Drop before
the transfer barrier; original authority before collection; no invented native state on cancel;
provider-phase-based unexpected Destroyed; Wait maintenance without gameplay/Last/clock consumption;
final observer veto for Exit and repeated/final SealTelemetryAndExit. Real cross-thread permit Drop
and actual post-Host replacement/missing-port/consumed-postfault tests cover the boundaries.

Actual verification, with source timing:

- Root minimal desktop-winit+render-wgpu library/examples check passed without serde. Independently
  locked UI-only desktop check passed; independent 2D check initially required the newly requested
  Winit lock graph. Offline resolution added 103 package versions, removed/upgraded none. Its check
  passed; final locked confirmation is still needed.
- Final WGPU transport/Drop/actual no-telemetry original-queue drain: 50 passed, run
  `a0cad60a-a870-4bbc-b59a-fe99676b3f60` (before deleting obsolete World marker operation).
- Final owning Window/Winit/WGPU libraries after marker removal: 227 passed, 17 ignored, run
  `4b6cc6d1-4f86-40fd-9d21-6352fc83713c`, build59.41s/run74.578s.
- Root final post-Host/UI/action/Game: 58 passed, run
  `12b768ab-b084-4734-9b44-4b800372d61f`, build1m07s/run7.217s (before marker-only removal).
- Root all-feature library/examples/tests check passed before final added seven regression tests;
  those all compiled and passed in the final Root58 result. No full workspace test claim.
- Actual Windows retained-pump six: all passed, run
  `b363aed0-a031-4259-b184-8fb6ba789408`, build6.26s/run2.675s. Early Exit, budget retry/Destroyed,
  construction failure, Drop fail-stop, ordinary window and maintenance-only native wake are covered.
  This precedes marker-only removal. Earlier53 had one new maintenance observation failure: it
  counted only ResumeTimeReached, missing a valid WaitCancelled maintenance exit. The corrected
  test checks every no-demand boundary, still requires six timer wakes and actual redraw/delta.
- Reference desktop/measurement/source cohorts: 57 passed across eight binaries, zero skipped,
  run `49475908-5749-44bd-9957-4c869f978686`, build4m26s/run13.488s; includes actual product
  desktop_render_probe. This precedes marker-only removal. The initial command named a nonexistent
  startup_materialization target and never built/tests; corrected to prefab_startup/content_boot.
- Final minimal native surface smoke compiled and passed baseline plus all four actual mutation
  modes: remove ingress, replace ingress, replace CPU observation, replace World window registry.
  All require original successful presentation and resize before mutation, then actual original
  surface/provider retirement and NativeDestroyed. Executable is root target/debug/examples/
  window_surface_retirement_smoke.exe. There is no fake two-window support.
- Root/reference/independent consumer formatting completed; git diff whitespace/local architecture
  links pass. Current code has existing non-correctness cfg warnings. No final Clippy yet.

At capture, the only live Cargo is native Editor qualification session **93018**, temporary log
`nara-owned-desktop-native-editor-tests.log`. It selects the critical three plus actual external
running-maintenance/child-exit. Explicit `CARGO_TARGET_DIR` is root target, jobs1, DX12 and required
real adapter. Earlier setup session48223 omitted inherited target-dir configuration; its exact
nextest process tree was deliberately stopped while compiling, before tests/child builds, then
restarted with the correct shared target. That cancellation is not a product failure or test result.

# Next Action

Poll session93018 to terminal; handle real failures. Keep Cargo strictly serial with explicit root
target for all independent/nested consumers. Then run final native owner7 (Winit6 + original GPU
drain), Root boundary/public product tests as appropriate, independent locked2D/UI confirmation,
actual Reference candidate telemetry seal/startup as needed, correctness Clippy and final fmt checks.
No need to repeat unmodified broad suites merely for document closure. Inspect final diff, exact
Conventional Commit for code/owning ADR changes, capture real qualification referencing its hash,
update current ledger/plan/design/guide links and registration, locally fast-forward main. Do not
push. Full immutable output/producer-prefix/external Feature/custom UI workflows stay open.

# Citations

- Previous progress: `37208eb577264f2788ff41b8155d9908`.
- Registration head before this capture: `e7abfc525324458e8050789384a762ac`.
- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-host-ownership-design.md`
- `docs/research/2026-10-30-engine-direction-reconciliation.md`
- `docs/verification/native-editor-qualification.md`
