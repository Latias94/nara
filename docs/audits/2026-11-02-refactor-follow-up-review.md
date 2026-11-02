# Refactor Follow-up Review

Date: 2026-11-02, Asia/Shanghai. Status: review evidence and recommendations, not an
implementation change or a new architecture contract.

Base: `75e1284ed28bc5a96686609f7dae8805d1924788`.
Reviewed HEAD: `a2640a10973b6d1724176a506237bc458aa5f3bc`.
Comparison: `git diff 75e1284ed28bc5a96686609f7dae8805d1924788...a2640a10973b6d1724176a506237bc458aa5f3bc`.
The range contains 63 commits and 1,267 changed files, including documentation, lockfiles and
fixtures. This was a risk-directed review of the major replacement boundaries, not a claim
that every changed line received equal scrutiny.

The checkout was fast-forwarded to `origin/main`. The prior session's research/index edits
were compared with the incoming copies and backed up before resolving their overlap. The
unrelated untracked engineering registration was preserved. No implementation fix or Git
commit is part of this review.

## Assessment

The refactor addresses several of the October 30 study's highest-cost boundaries. CP1 authored
upgrades, CP3 reference validity/demand, CP4 encoded backing, CP5 local builds, explicit Cargo
artifact selection, bounded Input ownership, and the principal render composition/output
owners now have implementations and scoped qualification records. Repeating the historical
absence claims would misdirect development.

Four P2 findings remain after review. F1 and F2 are freshly reproduced by focused probes;
F3 and F4 are source-confirmed, with an independent challenge of F3. They concern round-trip
authoring, cancellation/retry, same-window interaction authority and failed-build diagnostics.
They do not justify replacing the new architecture. Repair the owning boundaries and extend
their existing consumer tests.

Complete CP2 extraction, editable Nara-drawn UI, joint motion/execution and Rust rebuild/resume
remain explicitly unfinished. This review does not classify those declared gaps as defects.

## Correctness Findings

### F1 — P2: Complete Prefab conversion drops a required contextual default

**Trigger:** A supported component field has a source-dependent entity-reference default,
such as `SceneLocal("root")`. Its Prefab source correctly supplies the corresponding
`contextual_defaults` entry. Convert that complete Prefab instance into local Scene records,
then serialize and open the resulting Scene under the same schema basis.

**Evidence:** The staged compiler clears contextual declarations in
[`output.rs:267`](../../crates/nara_scene/src/prefab/overlay/compile/staged/output.rs#L267).
That is appropriate for flat Native output, but the same records feed source conversion.
[`localize.rs:417`](../../crates/nara_scene/src/authoring/prefab_conversion/localize.rs#L417)
remaps the effective value without restoring the contextual declaration. The specialized
conversion classifier can accept this prepared source. Ordinary source admission calls
[`validate_contextual_defaults`](../../crates/nara_scene/src/document.rs#L253), which requires
the declaration even when an explicit value masks the default. Thus the conversion can
produce authored data that fails fresh admission with
`scene.authoring-contextual-default-invalid`.

**Impact:** A successful conversion preview is not proof that the produced Scene can survive
Save/Open. This concerns the already supported complete conversion, not the unfinished CP2
extraction workflow. The existing Persistent-default test is a useful control, but Persistent
references do not require a lexical source declaration.

**Fix boundary:** Preserve or reconstruct the correctly remapped source contextual contract
at localization. Keep the Native boundary free of authored declarations; globally retaining
them in flat Native records would weaken a different invariant. Cover prepare/commit, fresh
serialized admission, Undo/Redo and masked defaults with the existing conversion helpers.

**Reproduced:** `review_contextual_default_conversion_can_reopen`, added temporarily beside
`materialized_persistent_default_keeps_its_namespace_and_identity` in
[`localize/tests.rs`](../../crates/nara_scene/src/authoring/prefab_conversion/localize/tests.rs).
The new test reaches successful conversion preparation, then fails in fresh serialized-source
admission with `scene.authoring-contextual-default-invalid`. The existing Persistent-default
control passes. Nextest reports 1 passed, 1 failed, 578 filtered out; exit code 100.

### F2 — P2: Cancelling after recovery publication prevents an identical upgrade retry

**Trigger:** An authored Schema upgrade publishes its recovery package, then is refused before
the manifest becomes Pending, for example by the cumulative work ceiling. Cancel after the
actual task return, start a new operation with sufficient limits, and submit the same proposal.

**Evidence:** The package path in
[`source_upgrade/publication.rs:532`](../../src/project_host/authoring/project/source_upgrade/publication.rs#L532)
contains only the deterministic recovery digest. The original recovery file remains after
cancellation. [`source_upgrade_storage.rs:204`](../../src/project_host/authoring/source_upgrade_storage.rs#L204)
rejects an existing destination with `RecoveryAlreadyExists`. The identical proposal therefore
chooses the same path and cannot proceed. Fresh Pending recovery does not solve the case:
the cancelled manifest is still Ready.

**Impact:** Cancellation preserves the original project, but the documented retry escape
route is blocked. The
[upgrade plan](../plans/2026-11-01-authored-schema-upgrade-plan.md#L390) explicitly permits
cancelling and starting a new operation with a new explicit ceiling. No source corruption or
incorrect adoption is alleged.

**Fix boundary:** Give each publication attempt a distinct owned destination, or add a
carefully validated observation/reuse path for an exact matching recovery package. Continue
rejecting unrelated existing bytes and preserve recovery custody. Do not solve this by blind
overwrite or by asking the user to delete recovery data.

**Reproduced:** Extend
[`pending_prepublication_work_refusal_keeps_ready_and_can_cancel_after_actual_return`](../../src/project_host/runtime/editor/authored_upgrade/tests/proposal/publication.rs#L260)
with a second `prepared`/confirm/wait cycle and require `Adopted`. The second operation stops
in `RequiresReconcile`, with `outcome: None`, `writes_fenced: true` and publication failure
`Storage(RecoveryAlreadyExists)`. The existing complete successful-upgrade control passes.
Nextest reports 1 passed, 1 failed, 1,294 filtered out; exit code 100.

### F3 — P2: Window composition can leave the replaced Runtime frame pickable

**Trigger:** The same `WgpuRenderHost` presents a Runtime frame in window W, then uses public
`present_window` to replace W's pixels, for example with a clear or a different composition.
Keep the producer, window resolution and Canvas unchanged, and query the old Runtime's
`WorldPointer` before another Runtime frame is presented.

**Evidence:** Successful
[`present_window`](../../crates/nara_render_wgpu/src/host.rs#L3006) updates statistics and the
presentation generation without invalidating the previous Runtime presentation for W.
The Runtime-to-Runtime path explicitly does this in
[`commit_runtime_world_presentation`](../../crates/nara_render_wgpu/src/host/runtime.rs#L489).
[`WorldPointer`](../../crates/nara_render/src/world_pointer.rs#L36) checks the retained live
guard, Runtime generation and geometry; it cannot detect replaced pixels by itself.

**Counterchecks:** An independent reviewer challenged the call sequence. The retained window
authority and existing Surface allow this mixed use; there is no Runtime-free-window
restriction. Native queue presentation does not revoke Nara's guard. Producer retirement,
device/window loss and Editor-specific receipt handling are separate paths and do not cover
this successful public call.

**Impact and limit:** Old scene coordinates can remain actionable after those scene pixels
are no longer displayed. This is restricted to mixing Runtime direct presentation and window
composition on one Host. It is not evidence that ordinary desktop presentation is generally
broken. Confidence is based on static control flow and an independent challenge; no fresh GPU
reproduction was run.

**Fix boundary:** Centralize the same-window successful-replacement invalidation in the Host
and invoke it from both successful presentation paths. Preserve other windows and retained
immutable pixel outputs. Test Runtime A -> composition in W, while a separate window retains
its current authority; also test failed presentation leaves the selected previous state intact.
The contract is already selected by
[ADR0094](../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#L199);
this correction does not depend on accepting the broader proposed ADR0121.

### F4 — P2: Failed project-tool bootstrap discards actionable compiler diagnostics

**Trigger:** `nara check`, `nara schema`, `nara export` or startup repair builds a headless
project tool containing a Rust compilation error.

**Evidence:** [`bootstrap.rs:247`](../../src/project_tooling/bootstrap.rs#L247) captures Cargo
stdout using `--message-format=json`, but returns on nonzero status before decoding that stream.
The diagnostic printing at line 258 therefore never sees normal compiler failures. The CLI
prints the generic returned error, which tells the user to fix diagnostics that this path has
discarded. Inherited Cargo stderr may show its progress/failure summary, but does not restore
the captured compiler-message records.

**Regression evidence:** The baseline decoded/emitted messages before checking failure. The
new general [`build_executable`](../../src/project_tooling/cargo_build.rs#L614) correctly
retains failed-build diagnostics, but bootstrap duplicates the build handling and bypasses
that protection. Successful warnings remain visible; bad executables are still refused.

**Fix boundary:** Decode and expose bounded diagnostics regardless of process success, while
retaining the actual process/protocol error and cleanup custody. Merely moving the status
check after `finish()?` is insufficient: `finish` can fail before the printing loop. Add a
bootstrap consumer with a distinctive `compile_error!` and require the message to reach the
user. This finding is source-confirmed; the real CLI failure was not reproduced in this run.

## Standards

No documented hard ownership/dependency-standard violation was substantiated by this review
lane. The finite render-program and stock-policy extensions are backed by named consumers
and accepted scope; their size alone is not evidence of speculative generality.

**S1 — P3, maintainability heuristic:** The upgrade Host's
[`selected_schema_retained_bytes`](../../src/project_host/authoring/source_upgrade_ready.rs#L1155)
hardcodes three catalog copies and reconstructs private Scene classification-map storage.
That couples Host admission accounting to the layout of
[`AuthoringClassificationBasis`](../../crates/nara_scene/src/authoring_decode.rs#L1581).
Expose the representation-specific cost/preparation bound from the Scene owner, leaving
aggregate ceilings and overlap policy in Host. This is a possible ownership smell, not a
proven incorrect budget calculation or a required new framework.

Standards total: zero substantiated hard violations; one P3 heuristic.

## Spec

The owning foundation/child plans and Accepted ADRs are the specification. No originating
issue is needed to infer the user's intended comparison: this is a follow-up from the prior
review baseline to the pulled HEAD.

**D1 — P3, current-status drift:** The parent
[foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md) still calls
inline-reference replacement (line 551), typed policy work (line 784) and shared stock
composition (line 880) next work, although their child plans and ledger record qualified
successors. Reconcile those current-status sentences and keep full CP2, durable materials
and broader View/time open. Preserve historical verification rather than rewriting its scope.

No additional scope expansion or missing promised complete workflow was established by this
status-review lane. The unfinished units below are explicit in the plans. The four behavioral
findings above were independently evaluated by the domain review lanes and are not additional
missing-feature assertions.

Spec-status total: one P3 documentation inconsistency; no newly alleged unfinished-feature bug.

## Disposition Of The October 30 Study

"Qualified" below means implemented owners plus an existing scoped qualification record.
It does not mean this review reran that record or certified the whole parent milestone.

| Earlier item | Disposition at the reviewed HEAD | What remains |
| --- | --- | --- |
| CP1: authored evolution | Reviewed proposal, checkpoint policy, publication and fresh-process recovery exist. F2 exposes a missed retry transition. | Arbitrary retained-history translation is not promised; CP2 is separate. |
| CP2: cross-source occurrence continuity | Partial. Inline lexical values, contextual defaults, final demand and source-only copying are implemented. F1 concerns the existing complete-conversion path. | Independent closed compiled storage/activation, root/placement changes, dependent occurrence remap and complete extraction. |
| CP3: validity vs current demand | Qualified. Required/Immediate and Reference/Deferred have distinct roles; Soft cannot satisfy required payload references. | General streaming/prefetch and measured large-content performance. |
| CP4: encoded backing vs decoded residency | Qualified. Pack-backed reads authenticate selected members without retaining every encoded member. Captured mode intentionally retains stronger availability. | Broader streaming/cache policy; no general RSS improvement is claimed here. |
| CP5: durable local builds | Qualified. Tracked observations, target-sensitive cooking, verified reuse and complete-group publication exist. | Remote cache/general scheduling and hermetic executable builds. |
| Render F1 | The central extension limitation is substantially addressed by typed programs/policy and shared stock stages. | Durable material/shader generations, broader View/time and cost attribution. |
| Render F2/F3 | Immutable outputs, display occurrences, a selected single-interactive-occurrence policy, offscreen Host use and producer retirement are implemented. F3 identifies one replacement gap. | Broader platforms and full proposed ADR0121 scope. Multiple interactive mirrors are not the selected promise. |
| Render F4/F5 and Editor E1 | Partial. Nara-drawn clipped composition exists; semantic document/session operations already have separate ownership. | Editable text, IME/caret/selection, accessibility, retained drafts, docking and custom frontend parity. |
| Runtime RE1/RE2/RE3/RE5 | Deliberately open: automatic phase plan, explicit executor policy, same-tick solver jobs and motion/discontinuity authority. | A real joint solver/animation consumer; existing custom schedules and task scopes are not missing. |
| Runtime RE4/RE6 | Retained tradeoff/conditional work. Spatial maintenance cost has not been shown to violate a frame budget; TRS remains the current representation. | Sparse/dense measurement and an actual affine/KeepWorld consumer before expanding spatial semantics. |
| Editor E2/E4 | Deliberately open: standalone commands and two independently available tool owners. | Closed-panel menu/shortcut use, coexistence, independent disable/update. |
| Editor E3/E5 | Explicit Cargo capture advances iteration; rebuild/resume and resident-code unload remain separate. | Game-owned checkpoint compatibility and a real native unload consumer. No speculative stable ABI is required now. |

Additional delivered units include processed physical Input, finite endpoint claims and
terminal credits, total UI completion ownership, registry Builder/Snapshot separation and
bounded Windows headless cooked-Player delivery. The last item is not all-platform desktop
export qualification.

Read the current [implementation ledger](../architecture/adr/implementation-status.md),
[Prefab extraction plan](../plans/2026-11-02-prefab-extraction-plan.md),
[render composition plan](../plans/2026-10-31-render-composition-and-host-execution-plan.md),
[Input endpoint plan](../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md),
[authored upgrade plan](../plans/2026-11-01-authored-schema-upgrade-plan.md) and
[cooked Player plan](../plans/2026-10-31-cooked-player-delivery-plan.md) for their exact boundaries
and original qualification links. The [October 30 study](../research/2026-10-30-engine-direction/README.md)
and its [receiving disposition](../research/2026-10-30-engine-direction-reconciliation.md)
retain their original snapshots; this report supplies a later review, not a silent historical rewrite.

## Verification And Coverage Limits

All Cargo work in this review was serial in the shared checkout with `CARGO_BUILD_JOBS=1`,
using Windows/MSVC stable Rust 1.99.1. This is not an MSRV qualification.

| Fresh check | Result |
| --- | --- |
| `cargo nextest run --locked -p nara_input -p nara_ui -p nara_ui_render --all-features --lib --test-threads=1 --no-fail-fast` | 236 passed, 0 failed, 0 skipped. |
| Focused Scene conversion probe plus the existing Persistent-default control | 1 passed, 1 failed, 578 skipped. The new probe reproduces F1 during fresh admission. |
| Initial root test build with `--no-default-features --features runtime-2d,serde,tooling` | Build failed before test execution: 12 E0432/E0433 errors from existing test fixtures importing feature-gated `project_tooling::host_fs`. Exit 101; this does not reproduce F2. |
| Focused authored-upgrade cancellation/retry probe and successful-upgrade control, adding `project-tooling` | 1 passed, 1 failed, 1,294 skipped. F2 is reproduced as `Storage(RecoveryAlreadyExists)`. |
| Fresh native/GPU and full Editor/product suite | Not run. Existing records remain historical scoped evidence. |
| Cargo-bootstrap failing-project reproduction | Not run; source/control-flow regression only. |

The temporary probe additions are review evidence, not implementation fixes. Their patches
and logs are retained in `target/review-notes/2026-11-02/` and the review delivery bundle;
the additions were removed from active Rust files after execution. The bundle identifies the
exact HEAD against which to apply them. Neither failed regression expectation is represented
as a passing product check.

The initial root test-build failure is a separate test-configuration gap: the ordinary
`runtime-2d,serde,tooling` library-test target imports a module whose feature it does not enable.
The review probe only adds assertions in an existing test and does not introduce those imports.
Fix the test-support ownership or its feature gates when maintaining that suite; do not infer a
production-library compile failure from this `cfg(test)` failure. The second run explicitly
enables `project-tooling` so it can reach the behavioral probe.

**Automatic coverage gap:** New independent `render-output-consumer`,
`render-program-consumer`, `render-policy-consumer`, `render-composition-consumer` and
`stock-composition-consumer` workspaces have no named CI execution in the inspected workflow.
The Editor dev-dependency on stock composition compiles its library, not its integration
tests. Existing one-time qualifications are useful, but will not automatically detect a later
regression. Add a small explicit CPU contract lane and selected native/GPU consumers to the
existing CI matrix; avoid a Cartesian feature matrix or a custom compiler-like verifier.
This is a coverage gap, not a demonstrated runtime defect or a substantiated hard ADR breach.

## Recommended Continuation

Use these concrete transition checks when extending the current owners:

| Boundary | Acceptance property | Selected pressure case |
| --- | --- | --- |
| Compiled values -> authored source | A successful source-producing operation yields data that ordinary admission can reopen under the same basis. Native-flat validity is a separate property. | Contextual default masked by an explicit value; nested occurrence remapping; Undo/Redo followed by Save/Open. |
| Cancelled operation -> new attempt | The returned terminal state identifies whether the same unchanged proposal can be submitted again, and who owns retained recovery artifacts. | Recovery package visible, Pending not published, cancel after actual return, retry with adequate limits. |
| Presented pixels -> interaction | Replacing a physical window revokes the previous frame's interactive authority regardless of which presentation entry was used. | Runtime -> clear/composition in one window, another window unchanged, retained image output still usable. |
| Child build -> caller diagnostics | Normal child failure preserves bounded actionable error information through every adapter. | Compile failure in bootstrap, ordinary explicit build as control, no executable launch. |
| Input/UI close -> terminal completion | Closing one logical consumer does not destroy the original receiver's outstanding terminal obligations. | Saturated ordinary lane, endpoint replacement, late success/rejection, owner close. Existing fresh tests provide evidence here. |

These are tests for the existing owners, not a request for a shared transaction framework.

1. Repair F1/F2 first, since both affect user-authored work. Promote the focused repros into
   complete round-trip/retry tests. Repair F3/F4 in their existing owners; none needs another
   architecture layer. Reconcile D1 while touching the relevant plan status.
2. Finish the current CP2 unit before expanding durable editing semantics. Use one nested
   extraction consumer with old/new occurrence mapping, dependent references, defaults,
   dormant claims, publication failure, recovery, Undo and Save/Reopen. Treat the independent
   compiled representation and production activation switch as explicit remaining deliverables.
3. Use the existing Host and immutable outputs for one editable Nara-drawn Inspector. Require
   multilingual input, IME candidate placement, numeric drafts, focus transfer, Undo/Redo and
   DPI/window changes. Resolve widget/draft/platform boundaries from this consumer rather
   than building a second generic UI framework in advance.
4. Select the motion/execution protocol with a small real solver and animation consumer before
   those features spread. Specify authoritative pose writes, discontinuities, same-tick joins,
   pause/step and executor feature-unification behavior. Do not replace Tasks with a universal
   scheduler merely to fill the diagram.
5. Complete one material/effect lifecycle and one Rust edit/rebuild/resume path in the reference
   game. Measure P50/P95 edit-to-result, frame P99 and retained memory on named workloads.
   Keep desktop/all-platform delivery claims separate from the qualified headless package.

The useful next investment is complete transitions across these owners: admission -> execution
-> publication -> cancellation/retry -> persistence/reopen. More isolated type boundaries or
more design documents alone will not expose the failures found in this review.
