---
type: "Verification Evidence"
title: "One bounded Application no-action pass through Open and physical retirement"
description: "Replace public frame/no-frame maintenance choreography with one bounded Application pass while preserving action evidence, quiet wakes and physical Exit completion."
timestamp: 2026-10-26T06:32:46Z
record_id: "f8c6241efa2b48f8af54f1603482cd39"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/effective-candidate-and-editor-progress"
base_commit: "3b1cd87"
---

# Verification

Implementation after the qualified typed-origin/fault-delivery checkpoint.
The earlier Reference Game Exit correction exposed that ordinary consumers needed internal
frame/no-frame choreography. This unit provides an ordinary no-action pass and migrates native
maintenance plus the Reference Game helpers. Owner/public, complete ordinary Editor, Reference
Game and selected actual native regressions now pass within the separately recorded scopes.
Workspace static checks also pass. The attempted formal orchestration returned degraded;
completed independent native fallback coverage and its limits are recorded below.

# Result

- `EditorApplication::advance(real_delta)` selects one bounded owner pass for initial Open,
  replacement phases, ordinary Session work or physical retirement without an artificial empty
  action batch. It offers delta to at most one Session drive and does not recursively drive a
  successor published by that pass.
- Native ordinary maintenance reuses the internal pass with its existing Runtime cadence
  policy. Effects-only supervised wakes stay distinct. Real action/commit/frontend paths retain
  their own drive, original-slot outcomes, prepared evidence and deferred commit boundary.
- Busy retirement retries dispatch without a second Session drive. Replacement Save failure
  likewise dispatches Cancel after observing failure, leaving its published failure available
  until the following pass publishes cancellation and restores the incumbent.
- Preparing/Refreshing and the initial Close pass maintain the retained supervised owner.
  External observation publication shares any persistence drive; Refreshing rejection uses its
  existing AbortPrepared drive instead of adding another one.
- Source actions and dirty decisions retain their exact admission. Frame disappearance is not
  completion; ordinary progress continues until `EditorApplicationUpdate::exit_ready()` proves
  the Session/Open Host physical close. Existing ADRs 0047/0118/0084 already own these boundaries;
  the implementation/rustdoc/guide correction introduces no new state machine or ADR.

# Evidence

All Cargo execution remains serial in the checkout with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, root target.

- Initial Editor run `ee32e83e-24ff-48c3-af5a-784150eb0e44`: 259 passed, 2 failed,
  34 skipped in 381.666 seconds. Two added preview
  tests fail because they call the ordinary-Workspace preview accessor during replacement,
  where it intentionally returns `None`. Their assertion unwind aborts while live owners remain.
  Parent correction borrows the actual retained Session's candidate, captures the optional
  geometry observation, completes retirement, then asserts the exact single-drive value. No
  production preview admission is broadened and no count assertion is weakened.
- Corrected focused Application owner, frontend submission and public Application contract run
  `a7ba5d31-c208-4751-847d-1d498b22774b`: 70 passed, 200 excluded/skipped by selection,
  in 17.316 seconds. This includes single-delta, retained completion, exact successor frame,
  public failed-Save next-pass cancellation, held physical Open cleanup and prepared outcome
  regressions.
- Final complete ordinary Editor run `e7fb1955-fd8d-411c-82f0-0d69d5955b86`: 262 passed,
  34 skipped in 381.458 seconds across 11 binaries. The slow consumer performs an actual build;
  it is included in the passing run. The initial two failed tests are not omitted from this run.
- Initial seven-case Windows/DX12 run `c46b8b1d-c003-4ca7-b7d6-2103a5bb43b3`: 6 passed,
  quiet Open Host Exit failed. Isolated unchanged reproduction
  `da67e431-1193-4f94-ab44-55a0dd73efd7` also failed. Temporary diagnostic probes found the
  actual report's first `join_timed_out` change caused an intentional observed redraw after
  three unchanged actual maintenance wakes. The fixture waited for a fourth wake before
  releasing its held worker, crossing the real join deadline. Probe runs
  `8b6a7089-a421-4bb2-9b23-ff15c41be486`, `881bbc31-a59f-40fb-ac68-bcef146dc287` and
  `d7d73bb1-6cca-4241-9a72-3cf018bd0f72` retain that RED. A first diagnostic edit briefly
  failed compilation because `check` needs `&str`, not an owned formatted String; this was
  corrected before collecting probe output.
- The fixture now releases at the end of its third actual unchanged maintenance wake, not
  the start of a fourth. It still checks exact redraw/frame/presentation equality, exact held
  native identity and false physical readiness, then real worker join and graphics completion
  before Exit. Production report changes/timeout policy are not suppressed. All temporary
  `[DEBUG-advance-quiet]` probes were removed. Isolated corrected run
  `8cc392f8-f72b-486f-8677-a39fada23d7f`: 1 passed, 247 excluded/skipped.
- Final complete selected Windows/DX12 group `591e1e1f-a266-4ba4-bb61-43d662361c94`:
  7 passed, 241 excluded/skipped in 26.825 seconds. Executed the resume-rebase, real Game
  input/reopen/pause/resize/Step, held Open Host quiet Exit, dirty Exit Cancel, dirty Exit during
  replacement, manager recovery/replacement/Close and real external-process quiet maintenance/
  child-exit presentation trajectories. The external trajectory includes an actual supervised
  build/READY handshake and actual maintenance wakes, not a stub or an empty filter result.
  This is selected affected development qualification, not the complete native/platform matrix.
- Final complete Reference Game all-feature run `3986f116-c065-436c-8f98-0329d0a1151a`:
  170 passed, 0 skipped across 34 binaries in 62.246 seconds, following a 7m15s build. This
  includes actual external Play/Restart/Stop/Exit helpers using the ordinary public pass, strict
  host parity and source/UI workflows. It is a fresh whole run, not reuse of the origin result.
- Final `cargo fmt --all -- --check` and workspace all-feature/all-target Clippy with
  `-D clippy::correctness` passed. Existing lint warnings remain; zero-warning status is not
  claimed. The final native test-only timing correction does not change production logic from
  the passing ordinary Editor run.

## Independent Challenge

Worker `01a176ce-fbed-77c2-bdc7-3d979b6f0860` owned the seven Editor/caller/guide files, made
no Cargo/fmt/commit, and froze its write set on parent request. Its handoff explicitly retained
the supervised-maintenance gap and missing per-pass failure assertion; the parent owns fixes.

Reviewer `01a176e0-81b2-79b0-9cfa-b81a53783dce` confirmed P1 starvation of retained supervised
Play during Preparing/Refreshing through the new ordinary pass. Parent added actual owner
polling and a completion-consumption regression across both phases. The test uses a real owner
rejection of Pause without a child, not a process or fabricated Session observation; actual
running process/cadence evidence still requires the separate consumer/native trajectories.
The reviewer also required public per-pass failed-Save cancellation observations and public
pending-persistence Close/Exit driving. Parent strengthened those cases without loosening
physical completion or original submission evidence. Completed targeted post-fix review confirms
P1 fixed, no new confirmed findings, at most one Session drive and no observed delta/publication
loss in Refreshing rejection. Its remaining non-blocking test limit is the exact conjunction of
a matching Session attempt's changed external observation and a Refreshing rejection in the
same pass; completion consumption has concrete coverage, but that conjunction was reviewed
statically rather than injected. The reviewer ran no Cargo, made no changes and is closed.

Separate reviewer `01a176fe-4e38-7b93-8f34-17b22c54fe3c` checked only the quiet Exit fixture
correction against its actual report/default-policy/callback owners. It found no new confirmed
defect or weakened three-wake/identity/physical-completion assertion. Timing remains bounded by
the real 100ms join timeout, which may precede three wakes under slower convergence; a genuine
report change then legitimately permits a redraw and is not evidence of an unchanged-state
production failure. The result proves delivered redraw/frame/present equality before release,
not absence of every queued future OS redraw. The final repeated `request_exit` is mutating
maintenance, not a read-only report query; its pre-existing ignored `actions_observed` also
limits any report-stability claim across that call. No universal quiet guarantee is inferred.
The reviewer ran no Cargo, made no changes and is closed; the parent owns the final seven-pass
native execution evidence.

## Final Review Route

Code review: harness-native fallback. The actual `ce-code-review` attempt
`20261026-150310-250944e3` returned `status: degraded`: its coordinator completed a source scan
but could not dispatch its independent leaves. It is not a completed formal multi-agent receipt,
and earlier receipts are not relabeled as that run's reviewers. No external model/tool route
was started. The parent closed the coordinator and directly dispatched two native Codex lanes.

API/testing/standards reviewer `01a1770f-26af-7a62-8895-36007c2f1215` completed with no confirmed
findings. It checked actual public/native driving, exact submission/prepared/physical contracts,
changed tests, source validation and guide claims, including both frozen production hashes.
Maintainability/simplicity reviewer `01a1770f-c57a-7ed3-836d-db8f69a25bb1` found no blocking issue
and one P3: the private Scene compiler retained a complete operation log used only for its length
after switching to a whole candidate. Parent replaces it with a plain operation count, preserving
execution order and diagnostic indexes. No proof test is deleted or weakened. This last change
is test-only code, not a new Editor production delta. Focused rerun results are recorded below.
Both lanes were read-only, ran no Cargo and are closed. This is truthful independent native
fallback coverage, not retroactive completion of the degraded orchestration.

Final private comparison/complete-value nextest `2adc1c06-d6b0-4d7d-a6b5-a7ba53425435`:
22 passed, 201 excluded/skipped. Format check and final Scene all-feature/all-target correctness
Clippy also passed with existing warnings. The count-only correction is committed
as `babbdef`; it touches no production code or proof test. The complete-value extraction is
`59c10a8`, and Proposed mixed-origin target design is `3ac5041`.
Final engineering memory validation and deterministic rollups pass with existing legacy advisory
warnings. The five affected/new documents and derived views have 885 local links with no missing
target; `git diff --check` passes. No historical record was rewritten for these results.

# Follow-up

Create the precise Editor checkpoint commit and ff merge locally. Preserve actual failures and
counts.
Neither this ergonomic API nor the separate full-value compiler comparison completes the
foundation Goal, the durable overlay replacement or the whole native platform matrix.

# Citations

- [Fault/Exit predecessor](2026-10-26T052536Z-retained-runtime-fault-delivery-and-reference-game-exit-driver-correction-19b2541631464d8798e3fa47ace8bfca.md)
- [Bounded next-owner design](../../progress/2026-10/2026-10-26T060135Z-typed-prefab-origin-merged-and-next-owner-refactors-bounded-4fb257d72ad2415ea82c1412961c36f9.md)
- [Editor panel guide](../../../../guides/editor-panels.md)
- [Native qualification](../../../../verification/native-editor-qualification.md)
- `editor/src/app.rs`, `editor/src/platform/imgui.rs`, `editor/src/app/persistence_tests.rs`
- `editor/src/app/startup_retirement_tests.rs`, `editor/tests/application_contract.rs`
- `reference-game/tests/editor_supervised_play.rs`
