---
type: "Verification Evidence"
title: "First author workflow U1-U6 closes with clean committed external products and measured iteration"
description: "Verification Evidence for First author workflow U1-U6 closes with clean committed external products and measured iteration."
timestamp: 2026-10-16T02:50:31Z
record_id: "7a2024c6678b43a48a069406d6eb729d"
status: "verified"
producer_id: "codex-root"
run_id: "nara-author-workflow-20261016"
related_plan: "docs/plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "78faa7c"
---

# Verification

The First 2D Author Workflow U1-U6 is complete within its finite Windows product
scope. Root verified the implementation, committed it, extracted an exact Git
archive of 78faa7ce58001365b407c8de6f6323099cea20a2 into a fresh sibling engine
directory, and followed the external-project workflow from those committed sources.
The final closure changes only documentation and engineering-memory records.

# Result

| Requirement / unit | Concrete result |
|---|---|
| R1 / U2 / AE1 / F1 | CLI creates an independent workspace under a sibling `games with spaces` directory. Generated root and nested plugin use relative coordinates to the same selected engine. A second creation and Check from an unrelated temporary CWD pass. |
| R2 / U3 / AE2 | After adding the Cargo plugin and bootstrapping Cargo.lock, schema check reports stale bytes; explicit schema write and subsequent schema check pass. Strict Check uses the compiled recipe. Ordinary Editor Open remains data-only. |
| R3 / U4 / AE3 | The public-API game/plugin builds independently. Three outcome tests pass; bounded execution collects all three authored targets, changes victory state, and resets to three visible targets at (-120,0). Desktop first presentation passes. The committed content variant also passes Check and gameplay. |
| R4 / U5 / F2-F3 | Ordinary generic Editor actions exercise PNG selection/reimport, StableId assignment, Undo/Redo, Camera edit, Save/Reopen, preserved read-only custom records, native child READY/Stop, actual compiler failure/diagnostics, repaired successor READY and physical cleanup. |
| R5 / U6 / F4 | Both baseline and Editor-edited candidates export with eight startup-closure members and two exact accepted product observations. Both exported binaries execute successfully from an unrelated CWD. |
| R6 / U5-U6 | The maintained sample and first-game guide cover normal commands. Actual first-build, data and Rust observations and concrete ergonomic obstacles are recorded below. |
| R7 / U1-U6 | All 38 previously excluded implementation paths receive bounded supplemental review; new contracts have owning-boundary tests and eight review lenses. The clean committed-source external trajectory passes. |

This closes the sole plan-completeness finding #1 from review
20261016-101126-0d3e71ba. Its original **Not ready** receipt remains historical:
at that snapshot final evidence was pending. No remaining product-source defect
or unfinished acceptance obligation within this finite scope is asserted. No
independent validator or external-model review is claimed.

# Evidence

## Implementation and focused gates

- c6e0cb6: common-ancestor local dependency relation, ADR 0110 amendment, external
  creation and exact relative Cargo coordinate.
- e3e8cab: Linux DirBuilderExt import and headless feature-specific test import.
- ddb45f7: explicit compiled-recipe schema check/write and shared file publication.
- 35bfc19: real built-in generic Inspector fields and typed AssetRef edit/history.
- 99f159a: Cargo-selected target directory, safe child-output projection and
  cleanup timing measured from actual cleanup.
- 7869ecd: independent collection game/plugin and authored content.
- 78faa7c: first-game guide and ordinary Editor integration acceptance.

Root's serial gates passed:

- 36 focused workflow/protocol/publication/CLI tests, nextest
  34f13215-7984-4174-8895-c86b5d9497dc, including actual independent project creation.
- 196 Scene/tooling/egui library tests and 93 Editor library tests. The latter
  includes the output-projection panic regression, first reproduced red in
  f56f04be-98dd-4648-9cc9-6e5da2829587, then green after the owning fix.
- Three independent sample gameplay tests with all features, nextest
  bbeb97eb-7438-4055-87c3-ae5a2d4e8321.
- `cargo check --workspace --all-features --all-targets --locked --offline`.
- `cargo fmt --all --check`, the sample's explicit-manifest fmt check, and
  `git diff --check`.

These are focused gates, not a claim that the entire repository nextest suite was
run once at the final revision. Existing deprecation/unused warnings remain.

## Clean committed-source trajectory

Archive SHA-256:
`49bead5e83aad2d1cab93ed525ddc37089f5099bb368191872ed9114d548addf`.
No uncommitted engine source or repo-ref tree was copied into the clean source.
The fresh source archive and the external game are siblings under an owned
temporary directory. The maintained game's ordinary files were copied into the
CLI-generated workspace, retaining its generated dependency coordinate and
adjusting the plugin's coordinate by one directory level.

The actual sequence was:

1. Build nara CLI with project-workflow-cli and nara-editor from the archive.
2. Create desktop-2d/v1 externally; Check; run headless and desktop first-present.
3. Add the ordinary sample source/plugin/tests and update Cargo.lock with an
   unlocked workspace check. Observe stale schema, write it, check it, strict
   Check, nextest, headless win/reset and desktop first-present.
4. Apply the committed content variant; Check and headless gameplay pass. Restore
   the owned original. Change one Rust movement expression, rebuild/run, observe
   the changed outcome, and restore the original source bytes.
5. Export into an absent destination; run both accepted products from an unrelated
   CWD. Repeat independent creation and Check from that unrelated CWD.
6. Change authored speed to 320, observe changed headless outcome, restore it;
   run the ordinary Editor acceptance from the clean source; strict Check the
   saved project, export into another absent destination, and run both products.

The clean-source ordinary Editor nextest run is
3b669c4e-9928-4254-935b-4e21d937c656: one passed in 83.062 seconds, including real
native external windows. Its enclosing build/test command took 99.308 seconds.
The earlier working-tree run 614e8f92-52fc-4d88-8b51-efb55e963a03 also passed;
it is not substituted for the clean-source result.

The action test does not drive native Editor controls or WASD keyboard gameplay.
It uses the ordinary EditorApplication and egui action path without a trusted
game adapter; actual native game children supply accepted first-presentation and
process cleanup evidence. Semantic movement/win/reset are independently checked
by the game's public command/outcome tests. No manual gameplay observation is
claimed.

## Measured observations and context

Host: Windows 11, x86_64-pc-windows-msvc, Intel Core i9-13900KF (32 logical CPUs).
Rust 1.99.0 b940084d7 (2026-09-28), LLVM 23.1.1. All substantial Cargo work was
serial with CARGO_BUILD_JOBS=1 and CARGO_INCREMENTAL=0, using the checkout's
existing shared target and cached registry, with offline network resolution.
This is **not a cold-cache benchmark**. Default desktop features are selected
through the project's desktop feature; Editor Play also builds its explicit host
target triple. Dev/test and release observations must not be mixed.

Engine Cargo.lock SHA-256:
`af2472e7a5f44d3d5c65593922d3a99f1dcdc0346e765d112fe6341121429c66`.
Bootstrapped external game Cargo.lock SHA-256:
`3046e74c90aa32b05d15b3ca2ce8168165d5d84a4ef95a694b6c79750fd7abda`.

| Observation | Monotonic wall time | Meaning |
|---|---:|---|
| CLI build from relocated committed source | 51.894 s | Dev, existing shared dependency cache |
| Editor build from that source | 93.084 s | Dev, same cache |
| First external starter Check | 56.939 s | Includes first compiled headless tooling build |
| First starter desktop build/run | 64.266 s | Dev desktop features, actual first presentation |
| Data save to bounded observed result | 0.113 s | Speed 200 -> 320, existing headless executable, final x 108.0 -> 110.4 |
| Rust function-body save to observed result | 7.843 s | Movement multiplier 1.0 -> 1.1, rebuild/run, final x 108.0 -> 108.8 |
| Editor Camera edit/Undo/Redo/Save/Reopen | 0.148 s | Saved height is reread; excludes native child startup |
| First Editor Play through READY | 67.229 s | First explicit-triple build for relocated consumer; cached dependencies |
| Deliberate compile error through diagnostics | 7.486 s | Actual compiler error reaches Editor while authoring remains available |
| Repaired Play through READY | 7.553 s | New executable generation and accepted child presentation |
| First development Export | 190.444 s | Includes release build and two product observations |
| Export after Editor save | 10.802 s | Warm release path, eight members and two observations |
| Edited exported headless / desktop run | 0.105 / 1.461 s | Unrelated CWD; desktop is first-present-only |

Data/headless result latency, editor-only persistence, and external READY are
different intervals. The measurements do not establish a continuous manual data
save-to-visible-pixel latency, percentile, target SLA, or in-place hot reload.

Session-local command logs and monotonic observations were retained without
committing raw machine paths. Two disposable-harness errors were corrected:
speed 200 and 300 both stop at x=108 after winning, so different final positions
were an invalid assertion for that variant; speed 320 supplies the distinct
data observation. A Python write initially doubled CRLF during the Rust edit;
the original bytes were restored, the harness switched to byte-preserving writes,
and the successful Rust result above was then observed. Neither error required
an engine-source workaround.

# Follow-up

These are bounded future triggers, not automatically activated implementation:

- The persistent-component derive admitted fewer ordinary scalar types than a
  game author expects. The first f32 speed prototype did not compile; this sample
  uses natural u64 speed/radius. Extend the existing derive's finite scalar
  mappings when the next authored gameplay configuration needs them, rather than
  building a second schema language or custom type-resolution framework.
- Generic custom component records remain KnownUnbound/read-only. Before claiming
  visual authoring for third-party Rust plugins, provide one exact installed
  authoring adapter through the established schema/readiness contract and prove
  its edit/Undo/Save path. Do not load arbitrary game-native code during Open.
- Initial Prefab creation still uses ordinary source files. A visual creator
  should be driven by the next real editing workflow, not by a speculative editor
  extension registry.
- First explicit-triple builds remain material; warm Rust/READY observations
  identify a concrete iteration target. Measure representative edits before
  introducing dynamic-library ABI, code patching or automatic watchers.
- Optional focused coverage from the review remains recorded in 680bdf8c:
  stable-ID/invalid-ID panel text entry, keyboard adapter wiring and Cargo
  configuration target-dir selection. These are not asserted product defects.
- Native Linux/macOS process/window/export trajectories remain unverified. This
  Windows result does not establish cross-volume local creation, final release
  packaging, a mod manager or a dynamic native plugin ABI.

# Citations

- docs/plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md
- docs/guides/first-2d-game.md
- samples/collect-game/README.md
- docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md
- docs/knowledge/engineering/verification/2026-10/2026-10-16T023314Z-first-author-workflow-closes-the-prior-excluded-source-review-scope-136220880e154191a83573e2687f217f.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-16T024040Z-first-author-workflow-review-has-no-remaining-product-defects-and-one-evidence-gate-680bdf8c0e7a4532a30d512fcd0e251e.md
