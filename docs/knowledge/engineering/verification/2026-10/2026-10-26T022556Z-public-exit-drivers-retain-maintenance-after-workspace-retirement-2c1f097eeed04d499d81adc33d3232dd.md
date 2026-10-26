---
type: "Verification Evidence"
title: "Public Exit drivers retain maintenance after Workspace retirement"
description: "Corrected public and independent Exit callers, a deterministic held-worker regression, and successor clarification of earlier lifecycle and panel evidence."
timestamp: 2026-10-26T02:25:56Z
record_id: "2c1f097eeed04d499d81adc33d3232dd"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "fix/editor-public-exit-drivers"
git_commit: "a0d64ef"
---

# Verification

Qualifies the public Exit-driver correction after `a0d64ef`. Production Open Host custody from
`90a92e5` remains unchanged: Workspace/Session retirement can precede the final physical facility
join. Callers must wait for `exit_ready`, continuing maintenance even without a Workspace frame.

# Result

- Fixed public Application, ordinary authoring, Recovery, panel and generated-project test
  drivers that prematurely returned, asserted immediate completion or tried to sample a retired
  frame. Independent panel/import consumers follow the same contract. Dirty Save/Discard/Cancel
  decisions, deadlines and business assertions remain; poll-only helpers do not reissue Exit.
- Added `final_exit_maintenance_outlives_the_workspace_frame`, holding an actual startup worker
  exit. The Workspace retires first; repeated maintenance and Exit cannot report ready while the
  worker remains held. Releasing that worker permits final join and ready; later maintenance
  does not manufacture observed actions.
- Public rustdoc and the panel guide distinguish process completion from frame absence and
  explain the existing maintenance entry. No new lifetime framework or production logic is added.

# Evidence

All Cargo operations ran serially with `CARGO_BUILD_JOBS=1`, using this checkout's `target`.
The generated starter's isolated build is also under that directory and retires with its fixture.

| Scope | Actual result |
| --- | --- |
| Reproduce four original public integration binaries | `d2f20551-6908-4e7d-a439-6e45c29ab1a7`: 19 passed, 8 failed, 2 skipped |
| Held-worker public regression before correcting its poll helper | `da4fa011-00ba-45c2-829d-03849e6cbe59`: failed at the old helper's frame-absence return |
| Complete Editor package after initial driver fixes | `989516e5-8224-4a3c-bf1f-c8d675e77e17`: 258 passed, 1 failed, 34 skipped |
| Remaining generated-starter Play/Stop/Exit driver after correction | `3280a25e-7773-4697-b050-0fd00a8431ab`: 1 passed, 0 skipped |
| Independent dialogue, ImGui, source import and Snap consumers | `a0008956-c13d-4dfe-a487-fa466ecedbb2`: 17 passed, 0 skipped |

The remaining full-package failure occurred at the clean-room test's forced frame read during
final Exit, after Play and Stop. Its successor run uses the corrected no-frame maintenance path.
The two Editor runs provide cumulative coverage, not a fabricated single all-green full run.
Builds emit existing ordinary warnings; this unit does not claim warning-free compilation.

Independent reviewer `01a17601-b63a-7881-8f67-aa2f8d6a8460` completed a read-only correctness review
of the originally specified ten code/test files against `a0d64ef`, including Exit admission,
dirty decisions, timeout behavior and the held-worker proof. No confirmed findings. The later
clean-room helper was parent-reviewed and executed separately; do not expand the independent
reviewer's scope to that later file. Formatting and final whitespace checks are integration gates.

# Corrections To Earlier Evidence

The [direct-member record](2026-10-26T004155Z-bounded-direct-member-discovery-and-panel-query-workflow-7003450c49204d8eba92ba90db765dc0.md)
called seven failed Editor tests pre-existing/unrelated without establishing a cause. The
reproduction above exposed eight affected public callers, and the full package found another
driver in the generated-project test. They derive from callers not following `90a92e5`'s physical
retirement contract; the earlier attribution must not be used as an exemption from verification.

That record also calls the new direct-member panel proof independently compiled. Its new proof
was actually an internal headless `submission_tests.rs` unit test. Existing independently compiled
prepared-edit consumers do not by themselves establish the new Members discovery chain. A true
independent Members consumer has a separate pending successor and is not qualified by this record.
The historical record stays immutable; these corrections supersede the two claims.

# Follow-up

- Full foundation rebaseline remains active. This is one caller-contract repair, not R4/R2 or a
  delivery-wave completion.
- No new native-window/GPU, other-platform or performance qualification is claimed for these
  test-driver/rustdoc changes. Earlier evidence retains its own scope.
- The existing public maintenance method is named `advance_project_open` and hidden from normal
  generated docs, despite also retiring the final Open Host. That is an interface-ergonomics
  follow-up, not evidence that frame absence proves Exit or permission to undo physical custody.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Panel guide](../../../../guides/editor-panels.md)
- `editor/src/app.rs`, `editor/tests/application_contract.rs`
- `editor/tests/supervised_play_clean_room.rs`
