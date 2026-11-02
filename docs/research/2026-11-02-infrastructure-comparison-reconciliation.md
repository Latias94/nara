# Infrastructure Comparison: Receiving Disposition

Date: 2026-11-02. Status: research disposition and bounded plan amendments, not implementation
qualification or acceptance of a new ADR.

The supplied [seven-document study](2026-11-02-infrastructure-comparison/README.md) compares Nara
at `a2640a1` with fixed Bevy, Fyrox and Godot snapshots. Its documents were copied and SHA256
checked before import; only five relative links to the preceding review were repaired. Retain
their original snapshot claims. Current contracts and results belong to the Accepted ADRs,
[foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md) and its ledger.

Three independent read-only lanes compared Scene/assets, runtime/rendering, and Editor/delivery
with current Nara source and plans. They found no additional confirmed product defect. Their
useful increment is one concrete dependent-resource refresh unit and sharper joint acceptance
within existing units. Keep the current compiled integration moving; this comparison is not
another prerequisite audit or a replacement roadmap.

## Snapshot Reconciliation

The unchecked source-backed materialization input discussed by the study has been replaced in
the current working tree. [Compiled storage](../../crates/nara_scene/src/compiled.rs) has private
fields, borrowed accessors and no public source constructor. Native reception checks a complete
batch before codecs and does not migrate values or fill receiving defaults. The
[owner/public cohort](../knowledge/engineering/verification/2026-11/2026-11-02T053411Z-compiled-scene-owners-and-reviewed-transition-corrections-pass-current-public-cohort-e8f7d20e3e2b49d5b086ce2100f1b9b2.md)
passes 1,629 tests. The current independent module workspace also passes 281 tests, with four
existing skips. The all-feature Reference Game workspace passes 179 tests without skips. Remaining
compiled integration gates are still in progress; these results do not complete CP2 extraction.

The preceding review's F2, F3 and F4 corrections are committed locally as `080852c`, `d6dccbc`
and `349921f`. F1 conversion and the related live-export correction belong to the current
compiled unit. Conversion preserves the original hidden lexical defaults; live export establishes
a new flattened source context from lowered effective values. Those contracts are distinct in
[ADR0006](../architecture/adr/0006-scene-and-prefab-data-model.md) and
[ADR0038](../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md).

The five independent render CPU workspaces and two Windows DX12 cases are now enrolled in the
working [CI workflow](../../.github/workflows/ci.yml). The runtime-2d/serde/tooling test-support
configuration has a local 28-test passing result in the
[integration record](../knowledge/engineering/progress/2026-11/2026-11-02T055844Z-compiled-integration-closes-root-public-feature-and-editor-native-controls-d5ddf64af6434b3d881fe5f949260a5b.md).
Remote CI has not run. Enrollment, local execution and remote qualification remain separate facts.

## Plan Disposition

| Study recommendation | Receiving decision | Owning implementation and acceptance |
| --- | --- | --- |
| C1: source/compiled separation and complete extraction | Retain the selected direction and finish the current prerequisite. Do not restart the closed-storage design. | R5 and the [CP2 plan](../plans/2026-11-02-prefab-extraction-plan.md): root binding, source placements, complete occurrence correspondence, affected dirty/closed sources, publication and recovery remain open. |
| C2: pose and execution | Retain R4c/R10's real solver/animation and executor comparison. Add the shared temporal interleaving below. | Keep narrow command Ack separate from whole-tick success under ADR0057 until an explicit successor is selected. No universal scheduler or replacement ECS. |
| C3: View/material meaning | Preserve qualified typed programs, policies, stock composition and output custody. Extend their existing temporal consumer. | R9 owns captured time, effect-specific history compatibility and durable material generations; binding ABI compatibility alone does not preserve accumulated pixels. |
| C4: custom Editor interaction | Preserve R1/R11's Nara-drawn Inspector, closed-panel command and two-owner coexistence consumers. Add explicit draft-history transitions. | Buffer Undo/Redo, document Undo, Escape and focus loss must preserve explicit Apply and conflict recovery. Reuse the current semantic owner; do not embed another engine's UI runtime. |
| C5: rebuild/resume | Refine the existing R12 consumer with an exact restored-state oracle. | A new executable, startup READY and an authored restart anchor do not prove restoration of live position, velocity, input or tick. |
| C6: asset change propagation | Add a bounded R6 dependent-resource refresh unit, independently schedulable from CP2. | Publish generation-bound dependency subscriptions at successful live adoption, including absent raw inputs and warm reuse; maintain failed-attempt retry subscriptions separately. |
| C7: cost, diagnosis and coverage | Keep existing measurement and CI work; add one bounded observational correlation consumer. | R12 joins existing owner facts in a standard trace viewer and measures overhead/loss without changing execution or cleanup. |

## New Concrete Pressure Cases

### Adopted Resource Generation And Dependency Subscriptions

The current [Asset dependency graph](../../crates/nara_asset/src/state.rs) uses stable source IDs;
its `record_recipe` and `add_source_dependency` engine/consumer callers occur in tests. The
[build observation model](../../src/asset_build/model.rs) already records present and absent raw
paths, and [verified reuse](../../src/asset_build/execute.rs) can return without re-executing an
importer. Finite build qualification therefore does not establish automatic live refresh.

Select one concrete project Host to replace the current generation's complete dependency set
when that generation is actually adopted. Artifact-store publication cannot acquire World or
live-content publication authority; [ADR0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
keeps those boundaries separate. Auxiliary paths require scope and observations, not invented
asset UUIDs. Keep first-failure retry subscriptions separate from last-good dependencies, and
restore subscriptions on warm reuse. Cancellation and late completions cannot replace current
relationships. Escaped immutable content retains its captured generation.

Use the existing independent [TextBundleDomain](../../tests/fixtures/module-consumer/asset-domain-consumer/src/bundle.rs)
and its optional `build-suffix.txt`. Through public Host APIs and existing change notifications,
prove create/change/delete, first-failure recovery, A-to-B dependency replacement, warm reuse and
superseded work. Observe actual typed output updates without manual graph mutation or explicit
rebuild, stable Product identity, last-good behavior and zero unrelated importer/decoder calls.
Reconcile ADRs0007/0037/0103 before implementation. General build scheduling remains separate.

### Discontinuity Before A Retained Capture's First Preparation

Existing [history admission](../../crates/nara_render_wgpu/src/policy.rs) and the
[prepare-then-reset control](../../tests/fixtures/render-policy-consumer/tests/history.rs) are
useful but do not settle a capture admitted before a cut and first prepared afterward. Extend
the motion and temporal-policy consumers together: capture A, apply a teleport/correction or
camera cut, capture B, then interleave preparation and execution of A/B. Select whether A refuses
or renders an isolated result; it cannot accidentally advance B's current history. Preserve a
second View, retained outputs, prepared-candidate refusal and original submission custody.
Bind shader time to the original spatial sample, clock domain and interpolation state. Cover
zero/multiple fixed ticks, Pause/exact Step and delayed offscreen execution. Material/shader edits
need effect-specific history compatibility in addition to their existing binding and pipeline rules.
This is a future contract/acceptance decision, not a reproduced history defect.

### Restored Situation And Observational Diagnosis

The current [Two Level checkpoint](../../samples/two-level-game/src/save/dto.rs) intentionally
reconstructs several transient values. R12 must name its own game DTO, promised restored state,
capture-consistency cut and build/Schema/content compatibility before extending
[ADR0116](../architecture/adr/0116-standalone-supervised-editor-play.md)'s one-shot READY protocol.
Use ADR0076's completed-frame capture boundary where applicable. Exercise stale generation
responses and incompatible restoration without promoting an unqualified candidate.

Reuse the [diagnostic runtime](../../crates/nara_diagnostic/src/runtime.rs) and
[GPU observations](../../crates/nara_render_wgpu/src/telemetry.rs) for a Host-composed capture of
work, waits and outcomes. Explain slow import, rejected edit and delayed GPU completion while
measuring enabled/disabled overhead, retention and dropped observations. App-frame,
render-transaction-frame and device epoch are distinct coordinates. App-frame-to-completion
latency is not per-pass GPU time. Respect ADRs0048/0068; collector failure cannot alter operations.

## What This Does Not Select

Keep separate source-only, build, processed CPU and native preparation facets. Do not make every
small asset extension implement the entire delivery stack. Warm-cook decode, catalog-sized
scratch, spatial scans, allocation reuse and cross-acquisition sharing need named measurements
before optimization. Record bytes read/hashed, decode count, allocations, owner occupancy,
cancellation tail and actual release; no cross-engine speed or reliability ranking follows here.

The study does not justify a new ECS, RHI, global transaction/graph kernel, trusted-input bypass,
automatic importer discovery, universal dynamic ABI, general profiler UI or embedded foreign
Editor runtime. CP2, motion, materials, dependent refresh and rebuild/resume remain separate
delivery units; pure runtime work and the editable custom Inspector need not wait for every
unrelated unit to finish.

## Reference Limits

| Reference | Supplied study pin | Available local HEAD | Receiving limitation |
| --- | --- | --- | --- |
| Bevy | `ae6152152d6fb8308ad6038314b48ef295e80fbc` | `85503941e488152711a4695bbe248e9ed9f453fc` | Study commit is absent locally; its exact source claims were not requalified. |
| Godot | `95c0fa5fe8a019adc29d074c16b64469b04571f7` | `81ab961f91066a63bcbf3ec340962bb1a1ada558` | Study commit is absent locally; its exact source claims were not requalified. |
| Fyrox | `b5b151b603bba0549ac03db676568726f17e6807` | `f008e4c2cdfdcda7b37bf8534d0df8de155748f6` | Commit exists but some blobs are absent; only selected available mechanisms were checked. |

One read of a missing Fyrox blob triggered promisor fetching and failed TLS; it supplied no new
evidence. Further object reads must disable lazy fetching when a local-only comparison is
intended. No reference checkout was updated. Plan changes above rest on current Nara contracts
and concrete consumers, not an assertion that the references are latest, faster or bug-free.
