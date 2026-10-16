---
type: "Verification Evidence"
title: "Custom scalar authoring closes with clean committed-source Editor and strict gameplay"
description: "R1-R5 and U1-U4 accepted through finite scalar contracts, exact data authoring, completed review and an external consumer of committed source."
timestamp: 2026-10-16T04:08:28Z
record_id: "27663c95797a48c58c16188165a86f97"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-16-1101-feat-custom-component-authoring-plan.md"
git_commit: "25f5c8bb6640c5ee4e809238bbb0586a91695db6"
---

# Verification

The Custom Component Authoring plan closes R1-R5 and U1-U4 at reviewed source
`25f5c8bb6640c5ee4e809238bbb0586a91695db6`. All implementation changes are local
Conventional Commits; no push, PR, formal release or cross-platform acceptance is
part of this record.

# Result

| Contract | Accepted evidence |
| --- | --- |
| R1 / U1 | Common sealed scalar codecs, checked narrowing, finite floating values, numeric-domain defaults, exact catalog identity and same-version lineage; derive and compiler fixtures pass. |
| R2 / U2 | The ordinary Editor installs exact owned scalar-data receipts without loading project Rust. Whole-component unsupported shapes, empty schemas, stale receipts and future records remain protected. |
| R3 / U2 | Data availability is separate from native preview. `NativeBindingUnavailable` omits records only from the temporary typed projection, retaining complete source records and strict Runtime bindings. |
| R4 / U2 | Scene/Prefab field transactions preserve unknown records and reject invalid values without revision/history publication. Optional Null Undo/Redo is covered. Open and external Play re-admission share Adapter construction. |
| R5 / U3 | An external collection game is edited through the ordinary Editor, undone/redone, saved and reopened. Radius 14 to 30.5 reaches compiled strict gameplay: victory player.x changes from 108 to 92. |
| U4 | Seven completed code-review lenses, independently confirmed fix, serial checks and a clean committed-source external Editor/game consumer pass. This record closes the review's intentionally pending post-commit delivery gate. |

# Evidence

## Serial repository and consumer checks

- `cargo check --workspace --all-features --all-targets --locked --offline` passed
  after the review fix; final run completed in 1m09s.
- `cargo nextest run -p nara_reflect --features serde --locked --offline` passed
  125 tests, including trybuild. Lazy Vec2 diagnostic formatting then passed all
  13 `derive_schema` tests again.
- `cargo nextest run -p nara_scene -p nara_tooling -p nara_tooling_egui --all-features
  --locked --offline --test-threads 2` passed 240 tests. The subsequently added
  finite-coverage and optional-null regressions passed all four `data_authoring`
  tests under run `f5afe5f7-6849-406a-8db7-de65aa42438c`.
- Ordinary Editor library, application and generic-open suites passed 108 tests.
  Root all-feature library tests passed 203, with one existing native smoke test
  ignored. Independent sample nextest passed three gameplay tests.
- Four dependency fixtures passed under run
  `a4edde43-98b1-4548-acdf-fd12cc6934a3`, including renamed-root and renamed-ecs.
  The stale reference-game metadata assertion now measures required runtime
  dependencies, independently of optional/development canaries; the renamed-root
  lock was refreshed offline and the fixture reuses the selected target directory.
- Root and sample `cargo fmt --all -- --check`, `git diff --check`, and engineering
  memory render/check/validate passed. Legacy memory warnings remain historical.
- Public `nara schema samples/collect-game --write` generated the committed bundle;
  an external `schema --check` and strict `nara check` passed with six startup members.
  External desktop `--verify-first-present` reported `first_present_verified=true`.

## Real ordinary Editor trajectories

The pre-commit external test passed in 69.355s under run
`1a1fbb93-97be-42b6-841d-7c7e4d4caeab`. The final proof extracts only
`git archive 25f5c8b` into a fresh engine directory and copies its maintained sample
to a separate path containing spaces. Only the external consumer's two Cargo path
coordinates are adjusted to this archived engine. Its ordinary Editor test binary
is compiled from that archive, sharing the existing target directory.

`cargo nextest run -p nara-editor --test first_game_workflow --run-ignored all
--locked --offline --test-threads 1 --success-output immediate` passed under run
`84bc9f61-2bc7-4422-8493-28624a2c1854` in 148.196s after 1m42s compilation. It proves:

- Custom scalar inspection/editing, finite f32 overflow rejection, unchanged source
  bytes on failed edit, Undo/Redo, Save/Reopen, image assignment and camera editing.
- Both baseline and edited strict compiled headless outcomes; player.x 108 to 92.
- Native supervised Play through READY, Stop and physical cleanup; actual compiler
  failure reaches diagnostics, retained authoring remains usable, source repair
  starts a fresh successful child and closes cleanly.

Measured archive-consumer timings: custom radius/camera edit through Save/Reopen
89.383ms; first Play through READY 69.261s; deliberate compiler failure 8.733s;
repaired Play through READY 7.079s. These share an existing Cargo cache and are not
cold-build or distribution latency claims. All 1,751 archived engine files remained
byte-equal to the commit after the test.

## Review and disposition

Completed `ce-code-review` receipt `20261016-114129-dd0e4796` has `status: complete`,
`Ready to merge`, seven actual lenses and zero remaining actionable findings.
The single initial P2 exposed optional Null inverse edits blocked by a redundant
kind comparison. Root removed that comparison while retaining capability and
canonical value checks; a separate validator inspected the current source and
Scene/Prefab regressions and confirmed closure. No cross-model review or reviewer
Cargo rerun is claimed. Invalid acknowledgement returns were recovered before
final coverage; every selected lens has a valid terminal result.

The preceding six-lens document review is captured by `f408016c706f46d9b0855ea906e87c85`.
Simplification applied one efficiency suggestion: defer Vec2 error-path formatting.
Local detailed review artifacts remain under ignored `docs/reviews/`; this immutable
record retains the outcome, scope and defect disposition.

The session scratch directory named `nara-custom-authoring-9k6u5l_i` retains the
commands' logs, consumer coordinates, source-identity receipt and local commit list.

# Follow-up

No required work remains in this finite lane. Collection/map/enum authoring,
project-specific validation descriptors and native custom-component preview need
their own real consumers. A fractional wire-kind change explicitly moves sample
component versions to 2; legacy integer sample content must be migrated rather
than silently reinterpreted.

Native evidence is Windows action-driven integration and first presentation, not
manual keyboard acceptance or a Linux/macOS release trajectory. Browser and other
deferred platforms retain their existing strategy boundaries. No successor starts
automatically.

# Citations

- `docs/plans/2026-10-16-1101-feat-custom-component-authoring-plan.md`
- ADRs 0090 and 0114 (2026-10-16 schema-only refinement).
- `docs/guides/first-2d-game.md`, `editor/tests/first_game_workflow.rs`.
- `crates/nara_reflect/tests/numeric_domains.rs`, `crates/nara_scene/tests/data_authoring.rs`.
- Local commits `162eeda`, `029e32f`, `4e79a7b`, `31967ea`, `25f5c8b`.
