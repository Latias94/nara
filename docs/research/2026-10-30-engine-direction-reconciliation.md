# Engine Direction Research: Current-HEAD Disposition

Date: 2026-10-30. Receiving baseline: `b939da269c693349c5aeb099831731e70e013c95`.
Status: source-backed research disposition and refinement of the existing foundation plan.
This document does not adopt every recommendation, qualify production replacements or create
another implementation roadmap.

## Input And Provenance

The user supplied `nara-engine-research-2026-10-30.zip`, packaged at
`2026-10-30T18:31:54+08:00`, with SHA-256
`99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`.
Its ten session research documents are preserved byte-for-byte under
[engine direction research](2026-10-30-engine-direction/README.md); their sizes and SHA-256
digests matched the archive manifest. The eighty supporting document snapshots were not copied
over the receiving checkout. The archive contains no Rust implementation or fresh product tests.

The research examined Nara `75e1284ed28bc5a96686609f7dae8805d1924788`. In particular, its
processed-Input gap is superseded by `ffa5795` and the
[processed startup/process qualification](../knowledge/engineering/verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md).
The current Input endpoint/lifetime work is a separate unfinished unit. Neither this review nor
the archive reran the predecessor's native or broad product evidence.

Reference revisions also differ. The archive inspected Bevy `ae6152152d6fb8308ad6038314b48ef295e80fbc`,
Godot `7be59b463a4798834d72d6d0a2942ddd01d920ee` and Fyrox
`5495d012b32145674501af2e67d2fd06a06f04dd`. The receiving reference checkouts remain Bevy
`85503941e488152711a4695bbe248e9ed9f453fc`, Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` and Fyrox
`f008e4c2cdfdcda7b37bf8534d0df8de155748f6`. No reference checkout was updated. An archived
source link is evidence for its stated snapshot, not a current local line or release claim.
Industry announcements in the archive are background; no plan change below depends on assuming
their release or performance claims were independently reverified in this receiving session.

## Verdict And Boundaries To Preserve

The research supports the selected Rust-first integrated product and Nara-drawn Editor. It does
not establish a reason to replace Bevy ECS, make a live World authoring truth, restore operation
patches as durable Prefab overlays, or invent universal task, migration, plugin or rendering kernels.
The important additions concern cross-owner semantics and complete consumers, rather than more
crate boundaries. Keep the existing [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
as the execution owner.

Current source confirms the content distinctions in the report: property claims require their
Schema version in [overlay interpretation](../../crates/nara_scene/src/prefab/overlay/properties.rs);
native value migration and data/authoring classification differ in
[Schema authority](../../crates/nara_scene/src/schema_authority.rs); actual Scene references require
present `Required` catalogue edges in [Runtime admission](../../src/runtime_content.rs); and
[Scene preparation](../../src/runtime_content/prepared.rs) traverses that closure. The same base
currently retains all encoded members, including when a decoded acquisition escapes. These are
real semantics, not evidence of infinite recursion, duplicate base copies or an unbounded leak.

## Finding Dispositions

The lane IDs below belong to the imported studies. "Refine" means adjust the named existing
plan; it does not mark the replacement implemented or silently amend an Accepted ADR.

| Finding | Current disposition | Owning decision and observable acceptance |
| --- | --- | --- |
| Content CP1: Schema/intent evolution | Planned; refine R5 | A reviewed upgrade transforms direct values, versioned claims/selectors, dormant overlays and dirty dependants. Select an explicit history checkpoint or prove retained-history bases; unknown semantics preserve or refuse. Additive owner adoption is already qualified and is not this workflow. |
| Content CP2: cross-source occurrence continuity | Planned; refine R5 | Extract a nested subtree with an explicit old/new occurrence map and dependent reference/claim rewrites. Preserve exact source/hop identity; do not guess from names or terminal entity IDs. Complete selected-instance conversion stays closed in its qualified scope. |
| Content CP3: valid target versus load demand | New explicit freeze boundary in R6/R7 | Three linked Scenes, a shared atlas and heavy later-level resources pass strict Check/Export without preparing all later resources for A. Define Schema/reference/catalogue demand deliberately; existing `Soft` permits missing members and is not the answer. |
| Content CP4: authority versus backing/residency | Planned bounded successor; refine R7 | Compare eager capture with one retained immutable local pack. Prove per-member demand/eviction, cancellation, overlapping generations and an escaped Image against the original backing. Preserve each backend's explicit availability guarantee. This does not solve CP3. |
| Content CP5: durable build recipe | Planned R6b freeze gate | Track the actual versioned importer/cooker, settings, observed dependencies and target facts. Keep Product, recipe, output digest and group manifest separate. Use a real target-sensitive cook and interrupted compound publication; no existing wrong cache hit is asserted. |
| Runtime RE1: phase composition | Planned; bring the R10 motion oracle into R4c selection | Compare public sets, finite sealed phases and Bevy App with the same intent/solve/pose/publication workflow. Keep one driver; select narrow command Ack versus stronger whole-tick finalization before interface freeze, with separate Ack/tick-success/effect observations. The success boundary is not yet selected. |
| Runtime RE2: executor selection | New explicit decision in R4c | Built-in schedules currently inherit Bevy's feature/platform default. Select profiles at composition/seal, verify feature-unified serial and explicit parallel controls, and reject unavailable requests before Startup. Record admitted profiles; a comparison or worker-pool count does not qualify parallel support. |
| Runtime RE3: jobs versus Tasks | Planned; refine R4b/R4c | Borrowed same-tick solver jobs join before pose commit. Owned cross-frame Tasks retain their existing cancellation/disposal custody. A completed scope is not a worker join or native release. No new universal job executor follows. |
| Runtime RE4: spatial publication cost | Existing Accepted tradeoff; measured follow-up | Preserve immutable read cuts. After the motion oracle, reuse sparse/dense benchmarks to compare private maintenance against complete publication. No current frame-budget failure was measured by this study. |
| Runtime RE5: pose writes/discontinuities | Planned capability; refine R10 | Select intent, dynamic solver output and correction owners together; test pause, zero/multiple ticks, parent frames, teleport/reset and interpolation. Ordinary non-physics local editing remains usable. |
| Runtime RE6: affine import/KeepWorld | Existing candidate design; conditional gate | Reuse the affine representation comparison before freezing a non-TRS importer, KeepWorld tool or affected persistent animation targets. Include edit/animate/history/Save/Reopen and inverse/error policy. Ordinary TRS/KeepLocal motion need not wait. |
| Render F1/F3: composition and Host custody | Planned R8/R9; preserve selection | Execute the chosen finite resource graph, external texture/buffer Feature, alternate policy, window-free boot and producer-local retirement. Do not restart the linear-plan versus graph decision from a weak two-pass demo. |
| Render F2: output/display/UI completion | New cross-contract refinement in R8/R11 and Proposed 0121 | Admission names an exact interactive completing attempt; passive mirrors/repaints cannot settle the original permit. Active-to-passive revokes route/capture/IME/actionable accessibility and rejects old actions without gameplay leakage. Hidden/failed mirrors have occurrence scope. Keep exact pixels independently retained across producer rates and retirement. |
| Render F4/F5: self-drawn Editor/text | Planned R11; sharpen the first consumer | Editable multilingual and numeric Inspector plus clipped live Scene image. Separate document/property, widget incarnation and saved panel identity; qualify caret/selection/IME/platform semantics, not labels alone. |
| Editor E1: frontend separation | Implemented semantics; planned frontend replacement | Reuse current exact document operations and drafts as behavioral evidence. Port one complete Inspector before deleting its predecessor; extract shared private draft logic only when consumers justify it. |
| Editor E2: closed-panel command | New concrete R1 successor | An external Snap command remains available through menu/shortcut with its panel closed. Both invoke one semantic operation with exact preview/commit/results/Undo. A panel factory is not command lifetime. |
| Editor E3: Rust rebuild/resume | Planned product gap; name the R12 successor | Join game-owned checkpoint capture/compatibility to supervised build and immutable executable activation. Body and structural edits return to a meaningful correct playable frame; capture/build/restore failure remains repairable. |
| Editor E4: two tools available | Deliberate current limit; refine R1b | Replace singleton contribution routing for two installed, pre-admitted owners. Updating/disabling A preserves B's route and valid review while A's stale results reject. Execution may remain serialized. Installation graph membership alone is insufficient. |
| Editor E5: code retirement | Deferred until a resident executable consumer | Activation fencing is not physical code unload. Views, candidates, callbacks, history and destructors retain their original code needs. Reuse R3c/R4a custody evidence with a real Editor object-lifetime consumer before claiming unload. Do not add tokens to every static API now. |
| Feedback F1/F2 | Existing measurement/CI work; refine unit evidence | Use correct-result endpoints and current machine/workload baselines; keep prompt focused owner tests and complete relevant qualification. No compiler-like verification script or repeated broad run for documentation closure. |
| Feedback F3; supplemental S5 | Capability/qualification boundary | Actual filesystem, Save/rename/Export/Play/close, failure cleanup and custom-UI IME/accessibility evidence per claimed platform. Missing hardware leaves that platform cell unqualified. Select numeric/sample budgets before comparing implementations. |
| Feedback F4; supplemental S1 | Planned cross-domain/generated-output refinement | One authored collision definition generates body/collider/debug state with source/occurrence/property/revision provenance. Edit/remove/duplicate it; current membership matches current intent and old outputs retire under their owners. No universal baker is selected. |
| Feedback F5 | Navigation correction applied | OQ-010 now follows the already selected custom-drawn Editor direction and first editable slice, rather than asking whether to adopt an existing toolkit. |
| Supplemental S2 | Refine R5 recovery oracle | Interrupt an upgrade before/between file replacements and after publication before acknowledgment, then reopen in a fresh process. Publish bounded reviewed recovery inputs first; identify mixed facts, fence external edits and make retry idempotent. Do not promise cross-file atomic rename or global Undo. |
| Supplemental S3 | Already planned color/mapping; sharpen R9/R11 oracle | Compare direct/offscreen sprite, glyph and UI edges with numeric linear-premultiplied readback, declared tolerances, fractional DPI/letterbox/clipping and the same paint/pick/accessibility mapping. Screenshots alone do not prove it. |
| Supplemental S4 | Additional evidence in R12 | A non-implementing author uses a fresh external project and public guides to add a persistent property/tool and complete edit/Play/Undo/Save/Reopen/Export. Record assistance and engine repairs; zero private engine changes and a reproducible artifact are the pass condition. Automated fixtures remain useful separate evidence. |

## Development Order

Finish the in-progress bounded Input endpoint/terminal owner before widening its consumers. Actual
regressions already demonstrate growth and stuck physical/semantic holds; that correction has a
stronger immediate failure signal than an unexecuted future capability. The research does not
justify abandoning it or repeating the completed processed Input unit.

Next settle valid-reference/demand semantics before target/package delivery freezes the newly
introduced Scene reference model. The eager-backing comparison is separate and can remain private
until its representative content consumer is ready. Carry Schema/claim evolution and cross-source
extraction as explicit R5 workflows before promising durable upgrade/source-refactor support.

Bring the joint motion oracle into phase-plan selection, and bring editable text plus output/UI
completion into the first render/custom-Editor design. These are design dependencies, not a reason
to serialize all R4/R8/R11 research or freeze unrelated algorithms. Rendering remains the selected
finite graph with one native execution owner. Static Rust contributions and finite child tools
remain valid without a universal ABI or all-language Host.

The first visual consumer is an editable multilingual/numeric Inspector plus clipped live Scene
image and the ordinary external Snap command. Corresponding predecessor flows retire only after
their behavior is matched. Tool coexistence and Rust capture/rebuild/restore keep separate complete
outcomes; neither is concealed inside full toolkit parity or native hot unload.

## Limits And Follow-Up

The receiving review inspected current owners and used independent read-only reviews for
runtime/motion, render/UI and Editor/extensions. It did not run new native rendering, solver,
editable-text, project-upgrade, external-author or platform trials. The archive's source studies
identified risks and deliberate gaps, not a reproduced new violation of every existing contract.

Future production changes reconcile their affected Accepted ADRs first: reference/demand and
backing owners under R6/R7; Schema/address/history under R5; execution/finalization under
0003/0008/0039/0057/0097/0100; UI completion under 0041/0102/0108/0112 and Proposed 0121;
command/tool lifetime under 0118; rebuild/resume under 0093/0116. The implementation ledger must
retain the distinction between selected contracts, implemented replacements and actual evidence.
