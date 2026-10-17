# Selected frontend and superseded-path removal

Status: **Complete: U9 and U1-U8 accepted at committed source `f4ffdd18`.**

[Final acceptance](../../knowledge/engineering/verification/2026-10/2026-10-17T102351Z-editor-foundation-u9-and-u1-u8-close-with-selected-dear-frontend-and-exact-external-products-b68c3a9f938a49b7ba63779f2849d0d9.md)
closes the exact committed-source external Editor/Play/Check/Export gate, the
independent panel and Reference Game consumers, and the selected frontend removal.
The final archive tests pass 7 native controls and all 9 affected Reference Game
cases; the same game's public Export accepts both products, whose actual runs
confirm saved `player.x=92` and desktop first presentation. Default/headless normal
graphs exclude Editor/Dear/Winit/WGPU. Windows/DX12 is the native profile.

The chronology below preserves intermediate pending/red/green checkpoints.
Those earlier pending statements are superseded by this final result; they are
not current remaining work. No successor implementation lane starts automatically.

The reviewed U7 candidate is committed at `26be74f493fcff04d677de83d16b0ef1286931d8`.
Its 149 Editor tests include nine Windows/DX12 native tests; the root owner suite passes
207 tests with one separate native test skipped. The actual nine-persona review and
bounded independent finding closure are recorded in [U7 progress](u7-parity-progress.md).
These are the earlier characterization baseline for removal; U8 was still unfinished at that checkpoint.

## Source migration

- The normal `run` and `run_trusted_compiled` entries select one Dear ImGui Host.
  Semantic application/document owners stay unchanged. Dear ImGui is a private Editor
  dependency; the root facade has no replacement GUI feature.
- The old egui crate, root aliases and feature are removed. Feature-boundary and CI
  consumers follow the selected graph rather than retaining unused adapters.
- Neutral Game presentation identity moves out of the old viewport implementation
  before deletion. Supervised Play's guarded child-project fixture and the native
  multi-view lifecycle proof become private test support.
- Qualification launch APIs, environment-driven probes, metrics and duplicate native
  Hosts are removed. Their useful behavior assertions remain in ordinary or ignored
  native tests; successful old probe runs remain historical evidence.
- Ordinary application and game-workflow tests retain owner, disk and process assertions
  while replacing their egui driver. Existing U7 private Context/native proofs preserve
  actual widget coverage; an action-driven integration test is not described as physical
  input or an end-to-end native click trajectory.
- The independent panel consumer uses its Dear painting companion by default. Reference
  Game Editor support is explicit; default runtime and headless dependency trees must
  exclude the Editor and selected GUI packages.

## Delivery-gate chronology

At the first checkpoint, source edits above were still being integrated. No U8 check, review or external acceptance
is claimed complete merely because the old files are absent.

The complete selected frontend/removal source is now committed at
`f4ffdd18d5dfca70ae204a6b26803ee4962a4523`, after the review and verification
results recorded below. Exact clean-source acceptance starts from its `git archive`
ZIP (SHA-256 `4c2d5b1bdd6188081c7646fe85a62be86f06fe47a4098ae57e336f328fbb9ef0`).
The separate engine directory contains that archive's source; its ignored target
is a junction to the existing shared cache. This is not a cold-build measurement.
The public CLI is being built from the archived engine before creating the new
external game. The accepted outcome is still pending.

The root all-feature/all-target check passes after removal. Root nextest run
`fec584d7-06ec-42f6-b762-7101cfbafc4c` passes 234 tests in 70.280 seconds across
the runtime owner library, CI policy, project composition, product capabilities and
module-boundary suites. Three tests are skipped: the separate native library test,
the independent locked module metadata check pending its lock refresh, and the
public-prelude compile consumer pending its dedicated run. The existing feature-tree
tests pass, including exclusion of Editor and Dear ImGui from runtime product graphs.
The two deferred compile consumers subsequently pass together in run
`6832fcca-5edc-4987-854b-63fa4c4694d1` (268.444 seconds). Its other 15 tests are
filtered out; they are not new skips in the preceding broader run.

Cargo refreshed the root, module-consumer and Reference Game locks offline. Relative
to `26be74f`, their package sets add no packages and remove 50, 61 and 51 respectively;
this cleanup does not upgrade a dependency cohort. The default independent panel
consumer passes `98fa8da9-7396-43ae-8cf9-4c178e19dcc7`: one real Dear ImGui companion
test, zero skipped. The selected Editor all-target check passes in 1 minute 37 seconds,
including the migrated integration and native test compilation. Subsequent source
cleanup and the complete Editor test suites still require fresh verification below.

The migration also found that the old Host was the only publisher of semantic panel
render-view identities. The selected Host now publishes the actual live Scene/Game
view IDs and clears them on retirement; the preserved native lifecycle test checks
independent close/reopen identities and final cleanup. That new check is not yet a
passing result.

The removed Game View probe also contained behavior not covered by the preserved
eight-phase lifecycle proof: actual keyboard-driven motion, Paused recapture,
independent Game resize and a fixed-tick Step reflected in the presented game image.
Those assertions are being migrated into a separate private native test. Routing and
coordinate unit tests alone are not acceptance of this complete owner/presentation chain.

Reference Game's default library check passes in 57.63 seconds. Its actual Cargo normal
dependency tree excludes `nara-editor`, Dear ImGui, Winit and WGPU; test-only native
dependencies do not establish a shipped headless dependency.

Inspection of the retired Host found another integration omission: the selected Host
did not connect the existing coalesced supervised-Play wake to an event-loop proxy.
The retained 250-millisecond process observation deadline allowed eventual progress,
but was not a substitute for prompt build/output notification. Restore that original
notification path and characterize real proxy delivery/acknowledgement before accepting
the production migration; do not delete the owner notification mechanism as unused code.

Selected Editor run `34143dd1-c4f3-4e52-8dd8-b2770c3990e9` compiles and runs
155 ordinary tests: 153 pass, two fail, with 13 ignored tests outside that run.
The ordinary clean-room supervised Play test passes its real generated-product
READY/Stop/exit trajectory in 168.936 seconds. The two failures are migrated Context
navigation drivers. Cancel passes after the test explicitly enables navigation-cursor
activation (`29d3a458-4ff3-4a3f-b845-4993c320adb9`); Browser still fails to enter Filter.
Its actual navigation initialization and InputText activation are being checked against
the pinned Dear source; that test is not waived.

First native migration run `159690bc-b705-41cb-a003-799fab4eda29` is red for both
new proofs: the input fixture fails trusted project admission, and the lifecycle fixture
times out in Primary before simultaneous view evidence. Their cleanup completes before
failure assertions. Diagnose the fixture/product inputs and exact presentation chain;
neither extending the timeout nor replacing the complete-chain assertions with unit
tests would close these gates.

Targeted Browser run `5e60a17d-44da-41e2-8546-7be589ad43cd` now passes the
real Filter text and PNG selection chain after moving navigation initialization
after NewFrame and checking actual current InputText activation. This is a test-driver
correction; no production navigation behavior or business assertion was changed.
Project manifest run `380d7d5a-b37e-4388-a683-02f74d263758` passes all 40 tests.
The refreshed broad root run `c348aa48-4043-42c2-9e6e-12be06e48d4d` passes all
235 selected tests, with the separate native test and previously verified expensive
prelude consumer excluded from this repeat.

Native fixture diagnosis separates its missing inputs from product defects. The
trusted intent must explicitly select Desktop independently of launch options;
the hidden Winit primary also needs an explicit owned redraw in this bounded test.
After these corrections, `18102c67-6e78-432f-b84b-23e38a965491` passes the complete
native eight-phase lifecycle, independent semantic view IDs, real device replacement,
retirement and proxy wake-coalescing assertions in 2.727 seconds. The input proof
remains red: its rendered Game target is only one pixel high, and actual click does
not request capture. Its layout and input facts are being diagnosed without
weakening capture admission or changing production geometry to accommodate a test.
The input fixture now opens its existing Game panel as a floating instance before
the first DockBuilder layout, preserving its ordinary initial dimensions and actual
resize grip. Run `77bc9992-ff9e-4598-b02a-5820a4c83bd8` advances beyond that
capture failure, then fails an exact-start input admission check. The old/new Game
geometry and release ordering are under diagnosis; this is not an upstream defect
claim or a passing input result.
Diagnostic run `1de37d37-5a85-449c-bc0e-47f9d857f049` confirms the exact owning
failure: the Host resizes Game from geometry 5 (`648x306`) to geometry 6 (`648x386`),
then queues a focus-release envelope carrying geometry 5. The next input flush
correctly rejects that old start. Repair the Host handoff by releasing/flushing the
old route before actual geometry replacement; retain the Runtime's exact-start
admission and the original complete input/Step assertions.
After that Host ordering fix, `a66a650e-4e8e-4672-94ec-e33c3c76fd66` completes
the native workflow and its cleanup without the old-start rejection. Its final
text-release assertion remains red, so this is bounded evidence of the ordering
repair, not full input acceptance. Check actual Inspector field activation and
capture release separately before declaring this proof green.
Run `dc2aedd2-a7f2-4fcc-ada9-d73ed16e2326` shows capture is released, but the
fixture's Inspector field lies below its actual clip. Actual wheel scrolling now
reveals the field before clicking and checks Active ID, InputTextState and routed
text owner directly. Run `f5d6d47a-73f1-48fe-ae67-e70768acbd32` passes those
text/capture checks and completes the workflow/cleanup, but its final Step-motion
assertion remains red: Step is Applied, Paused is retained and its runtime frame
advances, yet the presented tracked sprite center does not increase. Keep the
movement assertion and diagnose key/input/geometry ordering; do not treat Step
status or a fresh frame as equivalent to that complete gameplay result.

Minimal diagnostic run `e71d9038-89a3-4deb-a09b-1a8e402ef344` confirms that the
resized Game is captured, input remains enabled and D reaches the Runtime on
geometry 5 (`648x306`). The pending Step projects `Stepping`; the frontend then
omits its Paused message, increasing the requested image extent to `648x386`.
That resize supersedes the geometry and releases capture. The next Paused frame
restores the message and requests `648x306` again. This is a Nara frontend layout
defect: transient status feedback must not resize a Game render target. It is
separate from the already repaired release-before-resize ordering. A stable
feedback region is under repair; the original presented-motion assertion stays
in place, and this diagnostic result is still red.

Run `4c7e211c-b6d1-4dd0-95e1-8007a344a95a` passes all three Game Context tests,
including equal image mapping and requested extent through Paused, Stepping,
Running and stale feedback. The complete native workflow now retains geometry 3
(`648x354`) and capture through Step, then fails after Stop: control invalidation
queues an old `SurfaceRetired` envelope even though the active Runtime is gone.
The native proof's final motion assertion has not run in this attempt. Repair
that local retired-route cleanup at the Host/input boundary while preserving
release delivery to a still-active Runtime and exact Runtime admission.

The remaining motion failure also reproduces without any Editor or GUI. Run
`c1cb78a8-3b56-4b55-8050-535fac8a0c5b` uses the real InputPlugin, Runtime driver,
Paused control and exact fixed ticks: a newly admitted D press followed directly
by Step reaches simulation as `false`, yielding `[false, false]` instead of
`[true, false]` across the press and focus-release steps. The physical input
timeline previously projected ButtonInput only in ordinary `First`, which the
dedicated exact fixed transaction correctly does not run. The Input owner now
also projects pending entries in fixed `Prepare`, using its existing projection
cursor to avoid replaying already projected edges. No additional ordinary frame,
App-to-input dependency or change to exact-step clock policy is introduced.
Run `9edebcb7-5304-4b93-ae4b-3444d31b124c` passes all 144 Input/App tests with
zero skips in 9.206 seconds, including the new exact-step press/release proof and
the existing paused UI-consumption test. The complete native proof subsequently
passes in `ca507e4a-6543-4e3b-8e77-938432129bb3` (2.995 seconds), together with
all three Game Context tests. Actual D-driven movement, close/reopen, Paused
recapture, Inspector text ownership, independent resize, Step-driven presented
motion, rejected-control release consumed by a still-live Runtime and captured
Stop/retirement all pass with their original image/composite assertions. A
following Step without a D key-up confirms the release affects Runtime-held
input, rather than only clearing local capture. This is the targeted red-to-green
result; temporary diagnostic removal and the final complete Editor suites,
independent U8 review and committed-source external acceptance remain pending.

After deleting the temporary Step/input probes, complete Editor run
`294e4917-3d9e-4fd9-a52d-d7fb15cc160b` passes all 166 selected tests in
170.540 seconds. This includes the ordinary tests and all eleven native tests,
with the preserved authoring, Canvas, image import, Manager, supervised Play,
view/device lifecycle and full Game input/Step assertions. The generated-starter
clean-room Play/Stop/exit test also passes (37.434 seconds). Exactly two ignored
external acceptance tests are explicitly excluded for their separately prepared
projects/committed-source run; they are not counted as completed. This run also
compiles the new external Prefab proof, without claiming it executed. The final
Clippy check passes for `nara-editor` and `nara_input`, all features and targets,
in 1 minute 24 seconds. It reports existing/advisory warnings; no warning-free
claim or broad automatic lint rewrite is made. Cross-owner affected tests remain
a separate gate after the Input scheduling change.

The intermediate native run `12db52de-442e-44bf-be1b-5a416f4aa58a` reaches cleanup
after the retired-route repair but fails a newly added fixture assertion: a
rejected Cancel is reported through its dispatch outcome while the Play view
retains the prior Step result. Correcting that observation channel does not
change the Runtime owner. Run `7ecd5232-93c6-4a5f-85de-80eafc07a4dc` then passes
that release check and reaches the original motion assertion, still red before
the Input-owner repair above. None of these partial native results is acceptance.

The five pipeline simplification passes report no justified behavior-preserving
changes. Protected proof tests remain unchanged by that review. A complete independent
correctness/contract review still follows final native verification.

Reference Game's ordinary `editor` feature passes all nine selected tests with zero
skips in `e2d850d6-2157-4edd-bb8c-cdfb2f66cd44` (53.733 seconds after compilation).
This preserves all four input/GPU-recovery contracts, generic Open without Runtime
publication, ordinary supervised Play Stop/Editor exit with real child cleanup,
custom runtime records and saved UI refresh/restart. The independently locked
consumer no longer relies on a qualification feature or canary executable.

External-acceptance preparation found a concrete AE6 coverage gap: the existing
Collect Game workflow edits Scene/custom data but does not edit its Prefab. Its
ignored test now additionally opens the referenced Player Prefab, edits the
existing custom PlayerConfig speed, exercises Undo/Redo and exact Save/Reopen,
checks complete saved Prefab data and unchanged Scene bytes, and runs strict
headless gameplay. It then restores speed through the same owner actions before
the original radius/Play trajectory. This addition is not yet compiled or run;
the inferred speed-240 result `110.4` remains an expected value, not a measurement.
The original baseline/radius result must still be measured as `108.0 -> 92.0`.

Current-worktree preflight `cf164e1d-1401-45ff-9171-ea404234888e` passes that
complete ignored external workflow in 138.791 seconds. The fresh, separate
Collect Game consumer reports saved Prefab speed changing `player.x` from
`108.0` to `110.4`, restores the source through owner actions, then reports the
saved radius result `108.0 -> 92.0`. Real supervised Play reaches READY, reports
the deliberate compiler failure, retains last-good execution, admits the repaired
successor and cleans up on Stop/exit. Exact observed intervals are 76.140 seconds
for first READY, 9.048 for compiler diagnostics and 16.382 for repaired READY.
These are cached observations, not percentile or cold-build measurements. The
test and child windows run on a fresh private Windows desktop without switching
the user's active desktop or injecting global input. This uses uncommitted source
only as a preflight; final acceptance still requires the exact implementation
commit archived into a separate clean engine directory and a fresh game.

Use the public CLI to Check and Export that same fresh external game from an
unrelated working directory. The separate `authoring_loop` ignored fixture builds
its own starter and assumes `archive/target/debug/nara.exe`; with a shared external
target it cannot silently stand in for that same-project acceptance. Retain its
original contract, and qualify the actual target-path setup if running it. The
committed-source native Scene/Prefab widget proof and independent panel package
remain additional evidence of the same engine commit, with their own data fixtures;
do not report their actions as having occurred in the external Collect Game.

## Independent U8 review and frame-boundary follow-up

Actual `ce-code-review` run `20261017-081030-69c43aff` completes nine reviewer
lenses and an independent validator, with verdict `Ready with fixes` and two
confirmed P2 findings. The complete receipt is
`%TEMP%/compound-engineering-Frankorz/ce-code-review/20261017-081030-69c43aff/review.json`.
This is the current U8 worktree review against `26be74f493fcff04d677de83d16b0ef1286931d8`,
not the historical U7 review or a claim that external acceptance has completed.

Finding #1 identifies repeated `just_pressed`/`just_released` observations across
consecutive exact fixed steps. The canonical Input regression is strengthened to
observe press, an unchanged step, focus-loss release and another unchanged step.
Red run `31a317be-a651-4b4b-a7d2-6a2fb0c3adb2` repeats the edge; the original
held-only assertion could not detect it. Input cleanup now uses one hidden
App-owned `FrameCleanup` schedule after the complete ordinary frame or exact-step
transaction, before exit consumption and tracker rotation. It does not execute
the ordinary First/Last stages during an exact step or retire edges between fixed
ticks of a catch-up frame. Targeted green run
`af590215-601a-4c80-841c-8eef698d8dc2` passes three tests, including after-Finalize
visibility, retained held state, later UI observations, one-time semantic-action
delivery, and ordinary three-tick/Last visibility. Broader App/Input run
`125bee6b-0f67-4a17-8b6f-4264c21d3b95` passes 145 tests with zero skips in
8.563 seconds. An independent bounded correctness follow-up closes finding #1
after checking the actual owner boundary and these logs; it does not claim to
have executed Root's tests. Cross-owner Runtime run
`03f53745-d2e9-49d3-9cfc-70ba5b673bd8` passes all 83 tests with zero skips in
11.490 seconds, including the dedicated exact-step clock, retained debt/remainder,
pause and fault contracts. The four-file fix is committed locally as `2b26f96`.

Finding #2 identifies Paused idle wall time entering the next Running drive when
a Resume redraw precedes `about_to_wait`. The removed ordinary Host previously
kept a Running-specific cadence. Actual native red run
`837bcd43-a8ae-4065-85cb-769141b29a02` reaches the real Resume widget and Host
callbacks: a deliberately aged 60-second Paused baseline becomes a
`60.0108614s` Runtime delta and advances the observed fixed ticks from 1 to 6.
The hidden test window receives local Dear IO only, without a global input/focus
operation. This is a concrete regression reproduction, not a measured frequency
of user failures. The concrete Host now retains a baseline only for continuous
Running intervals and rebases state transitions. Native green run
`f08098e2-0eb9-4557-b380-204977a22661` passes that Resume proof (1.350 seconds),
the complete Game input/Step proof and the multi-view/device lifecycle proof:
three tests in 7.100 seconds. The Resume test observes the actual Runtime frame,
bounds its delta by the newly established Running interval, checks the actual
fixed-system count and conserves prior remainder/debt. An independent bounded
source follow-up closes #2 without claiming to have run these tests. The full
Editor run `b2f68604-6d3a-45f1-ae0d-c1f7c8dcee5c` then passes 167 tests in
312.293 seconds, including all twelve native tests. The two ignored external
trajectories remain explicitly separate. Real supervised Play failure/last-good/
successor cleanup takes 153.560 seconds; clean-room generated-starter READY/Stop/
exit takes 99.819 seconds. These cached command observations include rebuilds and
are not iteration percentiles. Same-game external creation/composition preflight
and final exact committed-source acceptance still follow.

Same-game creation preflight `13886166-de69-4e62-9a58-02f012bfc377` reaches the
Scene create/add/edit/remove/history/Save sequence, then fails its new expected
JSON comparison. The fixture assumed entity-array append order, omitted the
new record's admitted `collect-game` owner, and retained raw integer spellings
inside F64 values. The existing canonical writer sorts the entity table and
normalizes those values while preserving semantic root order. The test must
construct the complete expected source through the existing Scene/Prefab file
codec and explicitly include the component owner. This result is not evidence
of a production composition loss or an upstream ImGui defect. The process aborts
after that assertion while an application is still live; the comparison-failure
path must close it before reporting the mismatch. Final acceptance remains open.

Follow-up preflight `4772e63e-a017-44e8-b8e1-0d979fea3b50` passes the complete
Scene comparison and strict `108.0` result, then finds a second fixture omission:
its prior Reopen result was not acknowledged before the next Prefab Save. The
existing persistence owner explicitly rejects a new request while a Saved/Opened
result awaits acknowledgement. The proof now follows the ordinary acknowledgement
action between documents and verifies the actual Save result before checking dirty
state. This changes no persistence admission rule. Because the earlier assertion
abort left its disposable source restoration incomplete, the next run uses a
new external copy instead of treating that directory as fresh. Fresh preflight
`bc206f2a-9f68-4f87-8e22-e19851dfb91b` is still running at this checkpoint.

That fresh preflight subsequently passes in 251.041 seconds. Scene and Prefab
creation/composition each save/reopen complete expected source and retain the
strict `108.0` outcome. The original subsequent workflow observes saved Prefab
speed `108.0 -> 110.4`, restores it, and observes saved radius `108.0 -> 92.0`.
Actual first READY takes 137.096 seconds, deliberate compiler diagnostics 21.755
seconds and repaired READY 28.366 seconds; last-good/new-generation and physical
Stop/exit assertions pass. This fresh disposable project, private Windows desktop
and shared build cache establish WIP validation only. Final clean archive
acceptance must still name the exact implementation commit.

Affected Runtime UI, gameplay and transform run
`3b8d91eb-a54d-437d-ac94-5c1f374a6b42` passes 134 tests with zero skips in
9.251 seconds after the App frame-boundary change.

Final pre-commit Clippy passes for Editor/App/Input with all features and targets
in 1 minute 38 seconds. Existing/advisory warnings remain; no broad automatic
lint rewrite is applied. The complete workspace all-feature/all-target
locked/offline check then passes in 4 minutes 8 seconds, and `cargo fmt --all --
--check` passes. Independent review findings #1 and #2 have no unresolved source
or targeted-test work; exact committed-source external acceptance remains the
separate completion gate.

The following delivery checklist is complete, with final commands/results in the
acceptance record above. It is retained as the original execution scope.

1. Compile and test the selected default Editor, root feature contracts, independent panel
   and Reference Game consumers after lockfile refresh. Run Cargo serially with Rust 1.95,
   one build job and the existing target directory.
2. Repeat the preserved native authoring, Canvas, import, Manager, supervised Play and
   multi-view recovery tests. Native test windows stay inactive; local Dear IO does not
   establish OS input, IME or monitor scanout.
3. Simplify the final delta, perform an actual independent review and resolve supported
   findings. Commit the complete source migration locally.
4. Extract that exact implementation commit into a fresh external engine directory.
   Create or copy the public collection-game consumer into a separate sibling directory,
   adjust only its engine coordinates, and use the archived tools and tests.
5. Prove ordinary Editor source edits/import/Save/Reopen, real supervised Play failure and
   repair, strict saved-data gameplay, Check and development Export from an unrelated CWD.
   Record the source commit, commands, saved data and actual native/backend scope.

Windows/DX12 is the measured native profile. The previously isolated Vulkan implicit-layer
interaction and pinned Winit combined-shutdown defect remain separate limitations.
Linux/X11 and macOS are not inferred from Windows or compilation. Wayland retains docking
without native detachment. Full native accessibility and a future custom widget toolkit
remain outside this implementation's completion claim.

## Upstream and integration attribution

The separately reproduced Winit combined-shutdown borrow conflict remains an upstream
`dear-imgui-winit` 0.20.0 defect, as recorded in the historical
[U6 evidence](adoption-evidence.md#confirmed-upstream-shutdown-defect).

U8 source comparison identifies another bounded Rust-wrapper defect in the same pinned
0.20.0 package: `WindowFlags::NO_INPUTS` combines only `NO_MOUSE_INPUTS` and
`NO_NAV_INPUTS`. Its bundled C++ `ImGuiWindowFlags_NoInputs` also includes
`NoNavFocus`, and the generated sys binding has that full value (`197120`). The
wrapper's combination is `66048`, missing the `131072` focus bit. The Game notice
overlay explicitly adds `NO_NAV_FOCUS` alongside `NO_INPUTS`; no registry or
reference-repository source is modified. This source mismatch is not attributed
as the cause of the Step motion failure or as a separately reproduced native
keyboard-focus failure. Exact source: pinned
[Rust flags](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-imgui/src/window/mod.rs#L115),
`dear-imgui-sys` 0.20.0 `third-party/cimgui/imgui/imgui.h:1241` and
`src/bindings_pregenerated_windows.rs:671`.

Feedback-driven Game resize, route retirement ordering, omitted view/proxy integration
and exact-step physical-input projection belong to Nara. The WGPU presentation
observability patch fills an interface gap; it is not evidence of an upstream
rendering defect. These categories must remain separate in the adoption decision.
