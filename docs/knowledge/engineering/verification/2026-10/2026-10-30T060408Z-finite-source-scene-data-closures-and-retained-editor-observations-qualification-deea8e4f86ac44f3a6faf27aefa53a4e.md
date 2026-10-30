---
type: "Verification Evidence"
title: "Finite source Scene data closures and retained Editor observations qualification"
description: "Qualifies finite authored Scene dependencies, primary Product travel, open working copies, exact retained observations and affected Windows/DX12 workflows at f96951a."
timestamp: 2026-10-30T06:04:08Z
record_id: "deea8e4f86ac44f3a6faf27aefa53a4e"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "f96951a"
---

# Verification

Source Scene references and retained Editor observations are implemented as `f96951a`
(`feat(content)!: retain finite source Scene dependency closures`). The complete foundation Goal
remains active. This closes the bounded source successor, not input/target delivery, render
composition or the future custom-drawn Editor. No remote push or PR occurred.

Verification ran on Windows 11, Rust 1.95.0, one Cargo job and the shared root target. Native
trajectories used production Windows/DX12 with actual adapter admission and hidden windows.
The final source commit contains the source/tests/Accepted contract state used below; later
documentation closure introduces no product changes. Preserve untracked `vendor/`.

# Result

- `ProjectSourceContent` owns the actual flat selected/dependent Scene data and reference context.
  `ProjectSceneContent` separates raw authored documents, effective compiled candidates, canonical
  locations and source identities. Escaped `SceneSource` values retain actual documents and
  original reservations, not only an accounting lease.
- The production lazy compiler serves the finite effective Scene closure. Canonical self/cycle/
  diamond sources are captured once; structural Prefab cycles keep their own rejection rules.
  Only the requested Scene is instantiated. Source primary Product requests discover actual
  metadata in the worker, separately require startup/declared-root membership and preserve original
  selection. Ready/publication consumes exact captured facts, original plan/context and current
  Scene receipt without another dependency discoverer.
- Reader aliases remain separate from canonical identity. Stable metadata can declare an outer
  scope while a narrow overlapping capability reads the same source. Path-first/StableId-first
  and primary Product selection agree. Wrong sidecar paths/kinds, duplicate UUIDs and ambiguous
  observations reject. A ProjectLocation Path does not invent sidecar knowledge. Stable lookup
  observations name a global UUID/target independently of fallback alias scope.
- Authoring capture selects only demanded open Scene/Prefab working copies under the root's exact
  nominal Schema basis and accepted file/persistence observations. Unavailable or incompatible
  selections refuse disk fallback. Actual dependency Edit, Save, Reopen and Close invalidate the
  dependent stamp; unrelated edits do not cancel pending work or revoke acceptance. Save A writes
  A only. Schema successor capture passes its prepared Scene snapshots through the same path.
  Accepted captures retain dependency data after preview publication.
- Host observations and worker snapshots have separate admitted retention. One offered selection
  moves through Compose/Candidate. Output overlap is admitted before copies; unused entries and
  vector storage retire before settlement. Acceptance keeps root and actual participants only.
  Atomic Host replacement cannot refund source/compiled data; startup checks complete project
  occupancy before its first copied root observation.
- Cancelled capture remains pending until physical return with no selected root or hidden viewport.
  Public frame state preserves native maintenance scheduling. Return clears custody/deadlines.
  Failed input preflight keeps a small current-root retry observation and respects the retry delay.
- Check/delivery and source Export consume saved-source receipts. The cyclic-source Export fixture
  copies exact A/B sources/metadata, rejects changed dependencies before publication, preserves
  last-good output, relocates it and checks after damaging the originals. Its synthetic executable
  fixture does not claim execution of those bytes.

# Evidence

| Cohort | Actual result | Nextest run or log |
| --- | --- | --- |
| Final root library after retention replacement | 1020 passed, 4 existing skipped | `6ea70902-e26a-46ad-bb01-14750c8e883c` |
| Final startup/capture after complete-project guard and boundary fixture | 20 passed, 1005 unselected | `0286c2cf-3ce7-4235-88ff-7e58bb52c581` |
| Retention/capture/Schema/conversion owner cohort | 84 passed, 940 unselected | `736e3278-3e2a-48a0-a5ff-52b1830aafab` |
| Independent public Source Scene consumer | 12 passed, 0 skipped | `73a2ba0c-05d8-4d71-bac8-803632542ecd` |
| Eleven root integration binaries | 129 passed, 2 diagnostic fixture failures, 1 existing skipped | `40a72494-7dc4-4ea9-b68f-5b659c0bce75` |
| Corrected integration diagnostic assertions | 2 passed, 26 unselected | `2c309b79-84ac-4233-a3a5-8a2c1e096c56` |
| Final native Windows/DX12 six | 6 passed, 280 unselected | `42f8b58d-2c35-4750-92f5-0fa37449cadf` |
| Reference Game ordinary cohort before final handoff correction | 179 passed, 0 skipped across 34 binaries | `bfa7c27c-5b63-4766-b148-7c47a87a68d6` |
| Final affected Reference Game Editor/Authoring | 18 passed, 0 skipped across 7 binaries | `624178d3-98ff-48a2-909d-e0c0f4940595` |
| Final workspace all-feature/all-target check | passed, existing warnings | `nara-source-scene-final-workspace-check.log` |
| Final minimal source/tooling library check | passed, existing feature warnings | `nara-source-scene-final-minimal-check.log` |
| Minimal runtime-ui library check | passed before handoff-only correction | `nara-source-scene-minimal-ui-check.log` |
| Final root/Editor correctness Clippy, all features/targets | passed, existing warnings | `nara-source-scene-final-clippy.log` |

Named logs are temporary local files, not repository artifacts. The broad root run precedes the
last startup first-copy ordering line; the subsequent twenty-test capture run covers that line
and its exact/one-short fixture. Independent Source Scene and the integration binaries cover
unchanged Runtime paths; later edits affect Authoring handoff and its final root/native/game
scopes. Existing skipped tests and filter-unselected tests are distinct.

Representative commands, all serial with `CARGO_BUILD_JOBS=1`:

```text
cargo +1.95.0 nextest run -p nara --all-features --locked --lib --test-threads=1
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --locked --test source_scene_references --test-threads=1
cargo +1.95.0 check --workspace --all-features --all-targets --locked
cargo +1.95.0 check -p nara --no-default-features --features project-workflow-cli,tooling --lib --locked
cargo +1.95.0 clippy -p nara -p nara-editor --all-features --all-targets --locked -- -D clippy::correctness
```

Native selection used `--run-ignored only` for all three critical trajectories, retained Scene
navigation, Canvas priority and complete Prefab conversion in the native qualification procedure.
All six executed. They prove those presented/composite and physical lifecycle paths, not monitor
scanout, OS IME delivery, Linux/macOS qualification or the future custom-drawn frontend.

Final Reference selection: `editor_generic_open`, `editor_journey`, `editor_ui_refresh`,
`editor_game_input`, `editor_supervised_play`, `authoring` and `ui_authoring_roundtrip`, using its
own locked workspace and the root target. Final formatting and staged whitespace checks passed.

Independent Runtime review closed reader/cache, containing-key work and selected Scene metadata
alias findings. Authoring review closed decode memory/work conflation, double admission, Path
self-metadata and mixed loading order. Editor review closed unrelated cancellation, premature
discard, no-root drain, retry binding and hidden pending. A later independent review exposed
Host retention refunds; the move-only replacement supersedes it. A separate reviewer checked
the new token/replacement/validation protocol without implementing it. Final bounded reviews
have no confirmed remaining P1/P2; source review is not executed acceptance evidence.

Preliminary failures/corrections remain in the progress shards. One broad-root fixture expected
Product knowledge from an undiscovered Path; the corrected combination passed as
`e218c131-852a-4753-ad96-77ebd0729d4c` and in the later broad run. Two integration fixtures
expected generic diagnostics; current strict rejection reports `scene.unknown-component` with
the actual component and `project.content.asset-identity-conflict`. No production limit or
activation rule was weakened to satisfy a fixture.

# Follow-up

Reconcile current plan/ledger and guides with this bounded result, commit documentation and ff
local main. Continue the full Goal with source input ownership, processed input and target-build
delivery in dependency order. Rendering and Nara's own Editor drawing/runtime remain open. The
new input review is proposed design evidence, not implemented support.

# Citations

- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md`
- `docs/verification/native-editor-qualification.md`
- `progress/2026-10/2026-10-30T054311Z-source-scene-accepted-observation-custody-replaced-after-retained-allowance-review-8f842ab1a411446791af1ed8f90bf6f1.md`
