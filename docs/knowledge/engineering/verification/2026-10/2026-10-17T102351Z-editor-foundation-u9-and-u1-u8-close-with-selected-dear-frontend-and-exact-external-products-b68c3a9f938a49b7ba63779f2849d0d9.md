---
type: "Verification Evidence"
title: "Editor foundation U9 and U1-U8 close with selected Dear frontend and exact external products"
description: "R1-R11 and AE1-AE6 accepted through reviewed single-owner boundaries, complete frontend migration and exact committed-source external authoring Play Check Export on Windows DX12."
timestamp: 2026-10-17T10:23:51Z
record_id: "b68c3a9f938a49b7ba63779f2849d0d9"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f4ffdd18d5dfca70ae204a6b26803ee4962a4523"
verified_by: "Root serial nextest/public products and actual independent review 20261017-081030-69c43aff; both findings independently closed"
---

# Verification

The finite Editor Foundation and Frontends plan (ADR 0118, U9 and U1-U8,
R1-R11 / AE1-AE6) closes at implementation source
`f4ffdd18d5dfca70ae204a6b26803ee4962a4523`. Its App/Input review fix is
`2b26f96`; the reviewed U7 predecessor is `26be74f`.
This accepts the selected Windows/DX12 workflow and reusable ownership boundaries,
not a general cross-platform release or an implemented custom widget toolkit.
All commits are local; no push or PR is part of this record.

# Contract closure

| Contract | Evidence and boundary |
| --- | --- |
| R1 / AE1 | Data-only Scene and Prefab creation, custom component add/edit/remove, selection, exact revision/history, Undo/Redo, Save/result acknowledgement/Reopen pass through ordinary document owners in the external game. Native Scene/Prefab controls have their separate real Host trajectory. Arbitrary entity deletion/reparenting remains excluded. |
| R2-R3 / AE2 | U2/U3 catalog and owner proofs cover absent/invalid/incomplete templates, exact catalog generation, same-version decode independence and complete nonempty unavailable manifests through operations/inverses. External creation compares complete saved JSON and retained manifests, without loading project Rust on Open. This game's manifest is empty; nonempty preservation is proved by the retained fixtures, not inferred from an empty comparison. |
| R4-R5 / AE3 | Application/project state is GUI-free; typed actions, document/history/persistence/Play authority remain single-owned. Built-ins and an independently compiled trusted panel use the same workspace registration. The external panel observes selection, emits an existing action, closes/reopens and retains document/history. |
| R6-R7 / AE4 | Concurrent Scene/Game views have independent identities, targets, displayed mappings and focused input. Native resize, close/reopen, paused recapture, exact Step and retirement pass. External process Play remains distinct from embedded same-device Game rendering. |
| R7-R8 / AE5 | U6 records actual Windows docking/detachment, DPI 144/120 moves, redocking, Chinese IME/clipboard/navigation, ordered secondary close and shutdown. U8 retains the one-Host/device route and actual receipt/recovery tests. Historical physical-input evidence is not relabeled as a fresh U8 physical run. |
| R8-R10 / AE6 | A public-created external game from exact committed source passes ordinary edits/import/Save/Reopen, actual supervised Play failure/repair and strict saved-data gameplay. Public default/desktop Check and two-product development Export run from an unrelated CWD; both exported products run there. |
| R9 | Semantic commands/models, panel/document/view identities and engine render ownership are reusable. Dear widgets, docking, IME display and toolkit layout blobs remain adapter implementation. No universal widget IR, second history owner or public Device/Queue escape is added. |
| R11 / U9 | The coherent Rust 1.95, Bevy ECS/reflection/macros 0.20.0, glam 0.33.12, WGPU/HAL 30.0.1, Winit 0.30.13 and Dear ImGui 0.20.0 cohort is migrated and verified. U8 lock cleanup removes the obsolete GUI closure without upgrading unrelated families. |
| U8 removal / DoD | Ordinary and trusted launch entries select Dear. Old egui crate, root GUI alias/feature, qualification entry points, duplicate Hosts and abandoned probes are removed. Reference Game default/headless normal graphs exclude Editor/Dear/Winit/WGPU. Runtime consumers acquire Winit/WGPU only through selected desktop/render capabilities and do not acquire the Editor or Dear. Existing source/catalog/feature consumers follow the selected graph. |

The earlier U9/U1-U6 evidence remains immutable. The current selected owners and
verification links are indexed in the ADR implementation ledger; this record
supersedes the outstanding U7/U8 delivery gates without rewriting prior receipts.

# Exact source and external project

- `git archive` contains 1,914 source files (2,256 ZIP entries including directories),
  SHA-256 `4c2d5b1bdd6188081c7646fe85a62be86f06fe47a4098ae57e336f328fbb9ef0`.
  Every file was byte-compared with the extracted engine; zero mismatches and no `.git`.
- Engine: `F:/Nara U8 f4ffdd1/engine`; independent game:
  `F:/Nara U8 f4ffdd1/games/collect-game`; command CWD:
  `F:/Nara U8 f4ffdd1/unrelated`; candidate:
  `F:/Nara U8 f4ffdd1/exports/collect-candidate`.
- The public CLI was compiled from that archived engine. Public
  `nara new desktop-2d/v1 collect-game --parent .../games --local-checkout .../engine`
  creates the project. The guide's complete game/plugin sources are then added,
  preserving the generated `nara = { path = "../../engine", ... }` dependency;
  the plugin uses `../../../engine`. The root manifest remains the public scaffold with the documented game/plugin additions.
- Initial generated starter Check passes with three startup members. After adding
  the game/plugin, workspace Check passes; schema Check rejects the stale starter
  bundle, then public schema Write and Check pass. Game workspace nextest
  `bbeaac92-ae45-4c29-b305-3bf03e88eb33` passes 4/4, zero skips, in 0.437s.
- Final game lock SHA-256:
  `7ead79fe7774d12b19da9ff61eee8b38af75e1edd268dca6a5772cac639b687d`.
  Rust 1.95.0 / Windows 11 x86_64, DX12, one Cargo job, incremental compilation
  disabled, offline resolved dependencies and existing shared target apply.
  Editor/game checks use dev/test profiles; Export builds release binaries for
  `x86_64-pc-windows-msvc`. Shared caches and first use of a feature/release cohort
  are not a cold-build or percentile benchmark.

# Ordinary external Editor and saved gameplay

Archive nextest run `ac2b5517-1b61-457f-891f-b050c84c715d` passes its selected
`independent_game_edits_reopens_plays_and_recovers_from_compile_failure` test in
183.758s. Its other ordinary browser test is filtered by `--run-ignored only`;
that browser test already passes in the full selected Editor suite.

The same game proves:

1. Scene and Prefab empty-entity/custom-component creation, editing, removal,
   Undo/Redo, exact Save/Acknowledge/Reopen, selected entity and complete JSON.
   Each strict headless result remains `player.x=108`. Restoring disposable source
   bytes after closing this isolated proof is fixture cleanup, not entity deletion.
2. Player Prefab speed `200 -> 240` reaches strict `108 -> 110.4`; restoring the
   saved speed restores the complete expected source and `108` outcome.
3. Custom radius `14 -> 30.5`, camera editing and image Import/Reimport/assignment
   retain Undo/Redo, Save/Reopen, unchanged sibling data and numeric-domain rejection.
   The saved larger radius reaches strict gameplay `108 -> 92`.
4. Real supervised child READY, deliberate Rust compiler error and structured
   diagnostics, retained last-good executable, source repair/new-generation READY,
   Stop and Editor exit physically clean owned processes.

| Measured cached interval | Observation |
| --- | ---: |
| First supervised Play through READY | 113.8285973s |
| Deliberate compiler failure through diagnostics | 8.9946095s |
| Repaired supervised Play through READY | 17.3314762s |
| Saved default public Check (six members) | 18.4673728s |
| Saved desktop public Check (six members) | 244.9097833s |
| Public release Export and acceptance | 785.8119984s |

The first three intervals are inside the actual Editor workflow. Check/Export
intervals are separate public command observations and include compilation where
needed. The 125.689ms owner edit/Undo/Redo/Save/Reopen interval is not a
save-to-visible-pixel or save-to-READY measurement.

The main workflow uses Dear Context IO for the browser and semantic actions for
other operations; native game children are real. It is not an OS-level Editor
click or gameplay-input claim. Native controls below use separate fixtures and
must not be described as actions in this same external game.

# Public products and independent consumers

Public `nara check <game>` and `nara check <game> --profile desktop` both
pass with six startup-closure members. Public `nara export <game> --parent
.../exports --destination collect-candidate` passes in 785.812s, including a
12m49s first release-cohort build and acceptance of two exact product observations.
The candidate carries eight startup-closure members. Its recorded attempt is
`u8-f4ffdd1-20261017-final` at `F:/Nara U8 f4ffdd1/export-observation.json`.

Both products agree on saved startup content BLAKE3
`38c85754b3b7864139d46e1ca48b26d41e097cb03e9a4362ade953982cb0b5b7`
and authoring schema fingerprint
`fe65ae927381edb802d3975d9aeabc254de1614135abbb09d87756d6ae7b1bc5`.
From the unrelated directory, the actual exported `bin/headless.exe` reports
`tick=62 collected=3 targets=3 won=true resets=0 player=(92.0,0.0)`;
`bin/desktop.exe --verify-first-present` reports
`collect_desktop: ok first_present_verified=true` on the private desktop.
These executables consume their sibling `project/` tree; successful development
source execution is not substituted for exported-product execution.

The archived independently compiled panel consumer passes
`a2ae639e-5391-41fd-be35-3d92c5affeef`: 1/1, zero skips, 0.178s.
Its Dear painting companion uses the ordinary registration/action route and
retains document/selection/history through close/reopen. It uses its named
known-unbound fixture, not the collection-game project.

Archived Reference Game passes `f36df446-dab6-49f4-8e2a-5b735e337fe1`:
9/9, zero skips, 191.907s, covering Game input/focus and GPU recovery, ordinary
generic Open, custom component source receipts, UI Prefab refresh/restart and
actual external Play Restart/Stop/exit retirement (189.073s).

`cargo tree --edges normal --locked --offline --prefix none` for both archived
Reference Game and the independent Collect Game default/headless configurations
contains no `nara-editor`, Dear ImGui, Winit or WGPU package. Desktop products
legitimately select Winit/WGPU. Archived workspace `cargo fmt --all -- --check`
passes. Final re-comparison of all 1,914 extracted archive source files still has
zero mismatches; the independent game lock hash remains unchanged.

Focused archived Editor library run `e0ce16e9-1725-4c9a-9438-68b0ae27132f`
passes all seven selected native tests in 21.322s. The 132 remaining library
cases are filtered by the explicit native subset, not silently treated as run.
Actual Scene/Prefab controls, Canvas picking/move/resize/Save/Reopen, Manager
recovery/replacement/dirty close, Scene image reimport pixels, embedded Game
reopen/refocus/resize/Step, Resume cadence and independent view/device retirement
execute on the owned private desktop. The device-loss diagnostic is an injected
recovery scenario with verified subsequent receipts and cleanup. These are real
Winit/WGPU/ordinary Dear controls with local test IO, not global OS input.

# Integrated checks and independent review

Pre-commit verification of the source committed as `f4ffdd18` passes:

| Scope | Actual nextest run | Result |
| --- | --- | --- |
| Selected Editor, including all 12 native cases | `b2f68604-6d3a-45f1-ae0d-c1f7c8dcee5c` | 167 passed / 2 separate external fixtures skipped, 312.293s |
| App/Input | `125bee6b-0f67-4a17-8b6f-4264c21d3b95` | 145 passed, zero skips |
| Runtime owners | `03f53745-d2e9-49d3-9cfc-70ba5b673bd8` | 83 passed, zero skips |
| UI/gameplay/transform consumers | `3b8d91eb-a54d-437d-ac94-5c1f374a6b42` | 134 passed, zero skips |
| Resume/input/lifecycle targeted native | `f08098e2-0eb9-4557-b380-204977a22661` | 3 passed |

Workspace all-feature/all-target locked/offline Check (4m08s), affected
Editor/App/Input all-target/all-feature Clippy (1m38s) and workspace fmt Check
pass. Existing/advisory warnings remain; this is not a warning-clean claim.
Earlier root feature/module/prelude/project consumers remain recorded in U8
progress; they are not replaced by source-pattern architecture assertions.

Actual nine-persona `ce-code-review` receipt
`20261017-081030-69c43aff` completes against the frozen U8 tracked/staged delta
from `26be74f`; verdict is **Ready with fixes**, two confirmed P2 findings.
Its JSON/metadata remain in the local `compound-engineering-Frankorz/ce-code-review`
temporary directory. This is a real same-family Codex multi-agent review, not an
external-model or inferred review. Both supported findings are fixed and
independently closed:

- Repeated exact Step retained `just_pressed`/`just_released`. Canonical regression
  red `31a317be-a651-4b4b-a7d2-6a2fb0c3adb2` becomes green after App-owned
  `FrameCleanup` runs after the complete ordinary frame or exact-step transaction.
  Held buttons, later consumers and all fixed observers retain their lifetimes;
  ordinary catch-up ticks share one App frame. No full First/Last or per-tick edge
  clearing is introduced. Commit `2b26f96` includes the fix and meaningful proofs.
- Paused idle entered the first resumed simulation delta. Native red
  `837bcd43-a8ae-4065-85cb-769141b29a02` observes a 60-second injected stale baseline
  and excess fixed ticks through the actual Resume/Host callback. The Host now
  rebases at the Running boundary and preserves continuous-running elapsed time,
  remainder and debt. No clock clamp/cap/discard hides the issue. Actual green
  `f08098e2` and the complete Editor suite include the correction.

No review finding or required clean-source product gate remains open.
The retained optional starter-specific `authoring_loop` export fixture is not
selected in this archive batch; the required AE6 gate is the direct same-game
public Check/Export above. This does not claim execution of that alternate
fixture or weaken its assertions/capability rules. Its generated-starter
READY/Stop/exit companion passes in the 167-test suite.

# Limits and follow-up triggers

- Native acceptance is Windows/DX12. The private Windows desktop executor creates
  an owned desktop and never switches the user's desktop or injects global input.
  Exact presented/GPU receipts establish backend progress, not monitor scanout.
- Historical U6 real Chinese input, clipboard, keyboard navigation and multi-DPI
  monitor moves remain valid at their named scope. This final batch does not
  repeat physical IME/monitor operation or prove hot-unplugged monitor recovery.
  The Winit adapter does not supply full inline preedit; Nara's active-input/
  viewport-scoped transient composition/caret overlay is the bounded integration.
- Linux/X11 and macOS are not natively qualified here. Wayland's documented
  capability rejects native detachment while retaining docking; this is not a
  Linux native test result. Complete accessibility trees remain separate work.
- The independently reproduced `dear-imgui-winit` 0.20.0 combined-shutdown
  RefCell conflict is upstream. The Rust wrapper's `NO_INPUTS` omission of
  `NO_NAV_FOCUS` is source-confirmed, without a separately reproduced native focus
  failure. The WGPU patch fills an observability interface gap, not a confirmed
  rendering defect. This machine's Vulkan implicit-layer teardown interaction is
  a separate environment limit. Nara's own routing/layout/frame/cadence defects are not assigned to
  Dear ImGui. Future dependency upgrades must recheck retirement and input bridges.
- Future custom widgets/docking, dynamic native plugins, scripting, Web, generic
  unavailable-record/entity deletion and richer creation expressions remain
  deferred. A real game should supply the next finite workflow/latency pressure;
  this completion does not activate another foundation-refactor lane.

# Citations

- [Plan](../../../../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Current ledger](../../../../architecture/adr/implementation-status.md)
- [U8 chronology](../../../../research/2026-10-16-editor-foundation/u8-migration-progress.md)
- [Adoption evidence](../../../../research/2026-10-16-editor-foundation/adoption-evidence.md)
- [Migration guide](../../../../migrations/2026-10-editor-foundation.md)
- [First game guide](../../../../guides/first-2d-game.md)
