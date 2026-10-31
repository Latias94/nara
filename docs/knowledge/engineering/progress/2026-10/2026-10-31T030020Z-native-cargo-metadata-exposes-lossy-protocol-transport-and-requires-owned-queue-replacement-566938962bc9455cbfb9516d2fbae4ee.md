---
type: "Work Progress"
title: "Native Cargo metadata exposes lossy protocol transport and requires owned queue replacement"
description: "Work Progress for Native Cargo metadata exposes lossy protocol transport and requires owned queue replacement."
timestamp: 2026-10-31T03:00:20Z
record_id: "566938962bc9455cbfb9516d2fbae4ee"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-explicit-cargo-build-selection-plan.md"
git_branch: "refactor/foundation"
git_commit: "0c36715"
---

# Summary

The full foundation Goal remains active and the explicit Cargo unit remains uncommitted at
`0c36715` on `refactor/foundation`. Source CLI and final native/foreign build consumers pass,
but actual native Editor preparation failed. The confirmed owner defect is full metadata stdout
crossing a lossy 256 KiB diagnostic queue. Production protocol capture replacement is in progress;
do not submit, merge or claim this unit qualified before final owner/native acceptance.

# Details

New actual results after the predecessor progress:

- Original runtime-runner processed Input extraction: run `a8772a76-5a81-4632-8225-48f8e78922db`,
  1 passed, including all seven original process cases.
- Real source CLI Check/Run/Repair/Export and relocated products: run
  `bb1e1fea-6c43-4299-ba5b-33b54114fbd9`, 1 passed, 7 skipped, 1178.403s. This includes real
  release builds and native product agreements. The later tool/output budget separation preserves
  its default 512 MiB observations; it adds independently adjustable fields rather than changing
  this default policy.
- Final native/foreign build consumer after `tool_bytes` separation: run
  `471a2420-4501-4047-af91-513582390446`, 2 passed, 104.590s. A 1 MiB target allowance is below
  actual compiler sizes; exact tool-file allowance passes and one-short rejects. Relative target
  directory has its own error. Prior compiled feature/capture/physical process oracles remain.
- Actual native Editor run `eeb0e7bc-1f2f-428a-bd4f-27cc8301e7b9`: 0/2 passed. Both selected
  supervised/cadence trajectories failed Preparation before READY. A temporary test-only terminal
  diagnostic reproduced `Failed(OutputLoss { phase: Metadata, stream: Stdout })`; that temporary
  diagnostic is now removed. This is a confirmed integration defect, not unavailable graphics.

The new complete metadata burst exceeds the old platform queue's 256 KiB lossy tail before polling.
Restore neither `--no-deps` nor metadata/actual-feature equality, and do not merely enlarge logs.
Accepted ADR 0116 now separates finite complete machine stdout from bounded lossy diagnostics.
Explicit cumulative capture limits are 16 KiB for identity, 16 MiB metadata and 64 MiB Cargo JSON.
Coalesce small OS reads independently of stderr/record-count pressure, reject overflow/cancelled
capture and preserve existing cancellation, reader joins, native custody and 64-record poll budget.
Only stdout loss invalidates the build protocol; stderr loss remains explicit diagnostic evidence.

Current disjoint writers/reviewer:

- Plato `01a18f4b-c76b-7ef0-833d-0af12e280bab` writes only
  `editor/src/supervised_play/process/{mod.rs,output.rs,windows.rs,unix.rs}` and their owning tests.
  It is not source-ready. Parent must not Cargo/fmt or overwrite these files until frozen.
- Parent wrote build.rs phase selection, removed temporary owner diagnostics, and added a real
  `protocol-burst` helper/consumer in `supervised_play/tests.rs`: 1 MiB stdout plus 1 MiB stderr,
  child flush marker before first owner poll, exact protocol bytes, explicit stderr loss,
  bounded logs and physical cleanup. These tests are not yet compiled.
- Banach `01a18fda-c5ce-7ce2-a2d2-146f36484cf2` independently reviews queue/lifecycle changes,
  read-only. It has been asked to re-review the actual source-ready delta.

No Cargo is running now; last native diagnostic exec session `26898` has a failure summary and
must be polled if the runtime still reports it open. No build/artifact cleanup was performed.
Vendor and refs remain untouched. The optional P3 captured executable Debug is now bounded to
digest, byte count, provenance digest and retention; it no longer prints payload/native path.

The supplied ZIP was revalidated again: all 91 manifest entries and ten byte-identical repository
studies match. Next R7 work is now recorded in `2026-10-31-cooked-player-delivery-plan.md` and
linked from both parent plans. Independent review corrected its overclaim: complete candidate
deep validation before publication, startup validation of demanded data, later Deferred corruption
refusal at acquisition with current Scene preserved. No package implementation/Ready or wholesale
0088 adoption is claimed. Fourteen changed Markdown documents and their links/anchors passed the
independent review before the latest protocol amendment; rerun document checks at integration.

# Next Action

Finish the process queue replacement, collect independent concrete review, inspect diff and fmt
after writer freeze. Run focused Root/Editor supervised owner tests, actual burst/cancel cases and
the same two native trajectories; repair real failures. Then appropriate minimal features,
workspace correctness and strict consumer Clippy, formatting and document checks. Capture final
honest evidence, commit precise units and locally fast-forward main. Preserve full Goal active.

# Citations

- [Previous progress](2026-10-31T020011Z-explicit-cargo-contracts-independently-reviewed-with-actual-native-and-foreign-consumers-ca2e1780a12140d6acee32ad39c35481.md)
- [Unit plan](../../../../plans/2026-10-31-explicit-cargo-build-selection-plan.md)
- [Supervised contract](../../../../architecture/adr/0116-standalone-supervised-editor-play.md)
- [Next R7 plan](../../../../plans/2026-10-31-cooked-player-delivery-plan.md)
