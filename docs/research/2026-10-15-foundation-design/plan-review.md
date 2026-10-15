# Foundation plan review

Review date: 2026-10-15. Document: `docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md`.

Reviewed draft SHA-256: `42b9f7e2b374789f5f82316f6493abf77861ed4e43bccb95a94a4af49b26bbc6`.

The ce-doc-review workflow ran in non-interactive mode. Three Codex subagents performed separate persona passes; they had also researched this repository, so this is not claimed as independent cross-model corroboration. No external model CLI was used. Compound Pack resolution returned no roots, warnings, or errors. The user authorized producing and improving the implementation plan; corrections below preserve the established Product Contract and user-settled KD1–KD4. No source implementation, lane activation, build, or test occurred.

## Coverage and Original Results

| Persona | Result | Original findings | Remaining decisions / FYIs / residuals |
|---|---|---|---|
| Coherence | Completed | 0 | 0 / 0 / 0 |
| Feasibility | Completed; targeted correction rechecked | 1 P1 at confidence 75 | 0 / 0 / 0 |
| Scope guardian | Completed; targeted corrections rechecked | 2 P2 at confidence 75 | 0 / 0 / 0 |
| Product lens | Completed | 0 | 0 / 0 / 0 |
| Design lens | Completed | 0 | 0 / 0 / 0 |
| Adversarial document | Completed | 0 | 0 / 0 / 0 |
| Security lens | Completed | 0 | 0 / 0 / 0 |

All three original findings were `error`/`omission` with `gated_auto` remedies. None was dropped, merged, confidence-promoted, or left as an advisory. The English descriptions below preserve their meaning; evidence quotes are from the reviewed draft. Empty results contain no residual risks or deferred questions.

## Resolved Findings

### RV1 — Project strict tools lack a bootstrap and process-local closure owner

**Original:** Feasibility, P1, omission, confidence 75, gated_auto. Draft U6 requires reuse of the project protocol without specifying the process that can own the strict export closure.

**Draft evidence:** “Route strict project Check/Export through the compiled project's selected schema”; “Reuse the existing project invocation/protocol seam, with one bounded explicit operation and structured result”.

**Source verification:** `src/bin/nara/check.rs:42` and `src/bin/nara/export.rs:86-89` use an empty ProductRecipe. Export strictly checks content before building the project. `src/bin/nara/export/cargo.rs:78-100` requires an existing StartupContentClosureReceipt before building. `src/project_content/closure.rs:69-84` retains a process-local directory capability and explicitly forbids serializing it as evidence. Existing product observations carry digests and fingerprints, not that capability.

**Consequence:** A fresh project with custom persisted components cannot pass the first strict check. Passing an observation summary back to the generic CLI cannot transfer authority to prepare/publish the content closure. The draft's one-definition decision requires a bootstrap and a single process owning those operations.

**Disposition:** Apply within planning authority. Choose an explicit mode on the existing project headless executable. The generic CLI discovers/builds it without a schema-dependent strict precheck; the project mode branches before gameplay startup, freezes the same definition, and owns strict Check and Export operations. Export keeps check, build preparation/agreement, and publication in that process. The CLI sends bounded explicit requests and receives diagnostics/results only. Missing mode/build failure is an explicit error; there is no empty-definition fallback. This refines KTD2 and U6 and adds focused bootstrap failure cases.

### RV2 — Supervised Play is sequenced behind unrelated renderer work

**Original:** Scope guardian, P2, error, confidence 75, gated_auto.

**Draft evidence:** U11 depends on “U6, U10”; U12 depends on “U4, U5, U11”.

**Source verification:** `src/project_host/runtime/editor/actions.rs:719-733` dispatches Play commands through `request_play`, independently of multi-view or document transaction implementation. Existing frame identity can continue protecting geometry while document transactions change.

**Consequence:** The executor would have to finish material and multi-view work before correcting close retry, quiet observation redraw, and cancellable publication. That is avoidable work ordering and delays the explicitly carried SPL obligations.

**Disposition:** Apply within planning authority. U11 depends on U6; it retains current geometry checks until U10 migrates their token. U12 depends on U4/U5. U13 still integrates all paths. Replace the draft's strict phase narrative with a dependency-guided sequence so prose does not restore the artificial dependency.

### RV3 — A palette example accidentally requires shader hot reload

**Original:** Scope guardian, P2, error, confidence 75, gated_auto.

**Draft evidence:** U9 includes “failed shader replacement retains the accepted material” despite choosing a narrow trusted Rust material contribution for one example.

**Source verification:** `crates/nara_render_wgpu/src/quad.rs:179` creates its shader from embedded WGSL. No production shader-reload path was found. Runtime shader replacement would introduce a new workflow beyond the selected material seam.

**Consequence:** A small material contribution would unnecessarily require candidate pipeline replacement and last-good recovery mechanics.

**Disposition:** Apply within planning authority. Register trusted Rust shader contributions at startup; validate parameters/textures/capabilities and shared ordering. Shader-code changes take effect on restart. Defer runtime shader replacement until a real material-editing/hot-reload workflow requires it; synchronize the research recommendation so it does not reintroduce the removed obligation.

## Application Record

Applied three corrections: RV1 changes KTD2/U6 and bootstrap failure verification; RV2 changes the U11/U12 dependency table, sequencing, and geometry-token migration notes; RV3 changes U9, its deferral trigger, and the material research recommendation. The original feasibility and scope reviewers then read the changed sections and confirmed their issues were resolved without a new direct contradiction. Product requirements and user-settled decisions are unchanged.

Final plan SHA-256: `d00a56cd14758ccc46301b9a52c9c4c5ef4b15307c7644d32bc49798acfdf4da`.

Confidence check: after the targeted content-lifecycle follow-up and these corrections, no plan section reached the two-trigger deepening threshold. KTDs, units, and risks each carry the normal critical-section point, but have source grounding, a chosen alternative, dependencies, and observable failure examples. This is a bounded confidence assessment, not proof that implementation will need no design judgment.

Final result: `fixes_applied = 3`, `proposed_fixes_count = 0`, `decisions_count = 0`, `fyi_count = 0`. No material review finding remains unresolved. U1's planned lane/ADR handoff remains necessary before implementation; this review does not perform it. Document structure, IDs, fences, literal local references, and the final diff were checked without Cargo. No claim of new runtime, native platform, or GPU test evidence is made.

This record preserves original counts and evidence separately from the final handoff counts. No user-reserved product choice is reopened.
