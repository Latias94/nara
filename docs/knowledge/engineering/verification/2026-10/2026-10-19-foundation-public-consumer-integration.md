---
type: Verification Evidence
title: Foundation public consumer and saved-data integration
description: Independent Scene and Editor consumers, candidate doctests and Reference Game workflows pass after fixture corrections.
timestamp: 2026-10-19T09:49:10Z
record_id: d3afc5405dd14f0d951f9b90197d0de2
producer_id: codex-root-foundation-authoring-extension-review
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Verification Scope

The root integration owner executed these commands serially on Windows with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, and the shared repository target directory. This record captures the
reported results against the active correction working tree, not a final committed-source or
platform release qualification. The recording subagent did not rerun Cargo or alter code.

These results extend the earlier focused records without rewriting their then-pending evidence:
[U5 associated Scene candidates](2026-10-19-foundation-u5-scene-candidate.md),
[U11 recursive Inspector](2026-10-19-foundation-u11-recursive-inspector.md),
[U13 stateful panels](2026-10-19-foundation-u13-stateful-panels.md), and
[U14 semantic lifetimes](2026-10-19-foundation-u14-semantic-lifetimes.md).
The [correction plan](../../../../plans/2026-10-19-1335-refactor-foundation-correction-plan.md)
retains the owning acceptance contract.

# Independent Public Consumers

Nextest run `68c7c4d9-caf4-4dfb-b476-7992e761d63a` passed all 12 selected tests across five
binaries using `tests/fixtures/module-consumer/Cargo.toml`:

| Package | Test binaries |
| --- | --- |
| `nara-scene-module-consumer` | `nested_schema`, `durable_identity`, `vec2_authoring`, `manual_component_codec` |
| `nara-editor-panel-module-consumer` | `imgui_contribution` |

This fixture is an independent Cargo workspace using its own lockfile. Sharing compiled artifacts
through the repository target does not make its resolution the root workspace's locked resolution.
The results establish separately compiled public dependency consumption, including associated
Scene materialization, durable references, data-only derived Vec2/scalar/Vec<Vec2> editing,
Undo/Redo and save/reopen, and the existing documented closure-based manual codec interface.

The Editor contribution library and its interaction test compile through the paired panel
composition interface. The interaction test installs a configured frontend into an embedded shell.
This passing result alone does not prove that the separately compiled contribution executes
through its ordinary native runner; that narrow U13 acceptance item remains pending below.

# Candidate Public Rustdoc

The root-lockfile command
`cargo +1.95.0 test -p nara_scene --all-features --doc --locked` passed both compile-fail
doctests: callers cannot substitute the candidate's private document fields, and a borrowed
move-only candidate cannot escape its retained source through cloning. The reported build took
24.5 seconds and the doctest phase 0.37 seconds.

Together with the U5 record's prior owner, public-boundary and shared-retention tests, this
completes the candidate's negative public-interface proof without adding an unchecked
document/provenance route.

# Reference Game And Saved Predecessor

Nextest run `af3ccd7d-0f69-4cef-8587-65113fc5bbbc` used `reference-game/Cargo.toml`, enabled
`desktop`, and passed all 38 selected tests across `authoring`, `vec2_schema_migration`,
`project_content_boot`, `first_wave`, and `desktop_flow`.

The earlier run `10bcf2df-bfc7-442d-86f9-889077b2781f` passed 36 tests and failed two new or
updated fixture expectations. They were corrected to match the admitted production contracts:

- The multi-tick fixture submitted 300 ms although the desktop clock clamps a real frame to
  250 ms. The corrected fixture first accumulates a 50 ms remainder and verifies that no fixed
  tick executes, then drives an unclamped 250 ms frame. It checks three fixed steps, tick 3,
  zero remainder and movement across all three ticks. The production clamp was preserved.
- The content-budget fixture expected five charged file reads; the actual admitted source closure
  charges six. Image metadata discovery and its admission/digest revalidation are distinct reads.
  The corrected fixture also verifies the complete source closure. Schema catalog I/O was not
  added to that loader and was not used to explain the count.

These were fixture corrections, not evidence of an engine production defect. No production
admission, time budget or source closure was weakened to obtain the passing run.

# Acceptance Consequences

- U5's remaining real consumers now exercise authored expansion, startup rejection, retained
  source replacement and Retry. The independent consumers and candidate doctests join the earlier
  shared-document/retention evidence; U5's focused consumer acceptance is satisfied.
- U10 now has saved v1 predecessor migration, strict Check without source rewriting, current-value
  save/reopen and malformed predecessor rejection in the Reference Game. The independent math
  consumer proves data-only editing without its native registry, while the manual codec consumer
  proves the supported external registration interface. This closes the focused saved-data and
  public-consumer evidence for the closed Vec2 shape and private erased codec trait.
- U14 now has a real gameplay consumer for WASD and Arrow bindings, aggregate held movement,
  menu context exit/restoration, release plus fresh press, and zero/multiple fixed-tick command
  delivery through movement intent, velocity and position. This completes its focused consumer
  evidence alongside the earlier Input/Gameplay domain regressions.
- U13's external library and embedded interaction test pass. Its ordinary external native runner
  execution remains a separate item; compilation and manual frontend installation do not close it.

# Remaining Plan-Wide Evidence

The integration owner is still checking U13's separately compiled ordinary native entry path.
The final serialized workspace all-features gate, final integrated review, remaining precise
commits and committed-source evidence also remain pending. This record establishes focused
consumer results and does not close the entire goal or extend native platform claims.
