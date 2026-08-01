---
title: Reference-Game Product Readiness and Delivery Reset - Plan
type: refactor
date: 2026-08-01
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution_state: superseded
superseded_by: docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md
product_contract_source: active-plan-successor
execution: code
origin: docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md
supersedes: docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md
plan_id: reference-game-product-readiness-delivery-reset-2026-08
unit_namespace: RPR
operator_execution_authority: external-control-plane
---

# Reference-Game Product Readiness and Delivery Reset - Plan

## Goal Capsule

- **Objective:** Act on the current first-playable `Redirect`: retire the speculative evidence and release supply chain, preserve the proven candidate path, make ordinary Rust authoring a real product workflow, and collect product evidence before publication design resumes.
- **Authority:** `AGENTS.md`, `STRATEGY.md`, Accepted ADRs, and the implementation ledger remain higher authority. This plan replaces only the remaining delivery order. The exact U8, U9, and U10 evidence stays immutable at its recorded revisions.
- **Terminal contract:** The user-visible pre-publication review and immutable GitHub pre-release outcomes in the original RGF-U20/RGF-U21 Definition of Done remain the goal. Their evidence-ingest, approval-schema, and verifier mechanisms are superseded. A smaller successor may choose new implementation mechanics only after current product evidence returns `Publish`.
- **Execution profile:** Fearless pre-1.0 refactoring is authorized. Delete scripts, workflows, schemas, fixtures, policy interpreters, and documentation that exist only to prove an unavailable delivery state. Do not replace them with a different evidence framework.
- **Stop conditions:** Stop and re-plan if the ordinary author path requires project data to activate Rust code, if one contribution type cannot serve direct and file-backed products without a service registry, or if product evidence cannot be gathered without adding a general telemetry or benchmark framework.
- **Tail ownership:** The active execution driver owns implementation, focused verification, immutable memory records, review follow-up, commits, and delivery. Repository text never grants credentials or bypasses external platform gates.

---

## Product Contract

### Summary

Nara has a proven runtime substrate, hosted CI, and checkout-free Windows/Linux candidates, but the product baseline is `Redirect`: one measured iteration tail fails and ten required desktop, runtime, and ordinary-author metrics are missing. The next useful work is therefore the product path itself, not a larger protocol for approving evidence that does not yet exist.

### Problem Frame

The retired delivery design made the reference game own ingestion, normalization, approval, and release protocols before it proved the normal Rust author workflow. Its specialized Python tools, workflow YAML, schemas, fixtures, and Rust policy interpreters became a second product whose correctness could not establish that a game was playable or pleasant to build. Candidate packaging is different: it already exercises real binaries and a no-checkout consumer on both supported platforms, so it remains a product capability.

### Requirements

#### Authority and Evidence

- R1. Exactly one plan is active. The predecessor remains immutable except for reciprocal supersession metadata, while architecture pointers, the implementation ledger, and engineering-memory registration move atomically to this plan.
- R2. RGD-U8, RGD-U9, and RGD-U10 remain completed at their exact recorded revisions. The U9 `Redirect` remains the current delivery decision until a complete product population bound to one later reviewed revision replaces it; later evidence never rewrites those historical records.
- R3. The delivery-only U11/U12 ingest, normalized approval, custom verifier, and release workflow closure is removed from active code and policy. Immutable verification shards continue to record that the local preparation once existed without presenting it as current capability.

#### Retained Candidate Capability

- R4. The candidate workflow continues to build the real headless and desktop products on Windows and Linux, package a bounded fixed layout, and run both entries from a no-checkout consumer. Invalid-ref dispatches and reused attempts fail visibly; retained policy tests check security and product invariants rather than reimplementing the entire YAML program.

#### Ordinary Rust Authoring

- R5. A gameplay author can compose a normal product through one pure Rust recipe, add an ordinary replayable runtime-only plugin entry without package vocabulary, and add one typed contribution that binds a schema-owning plugin to its schema provider once. Each recipe entry owns a reconstructible factory plus stable typed configuration identity; the recipe performs no I/O or runtime mutation and is inspectable before admission. Raw one-shot plugin values remain available only to direct `App` composition.
- R6. Headless, desktop, editor, and direct-App paths resolve the same contribution identities and schema fingerprint. Normal authors do not handle `PluginDefinition`, raw slot edits, schema-provider parallel lists, candidate promotion, or retirement ledgers; those remain provisional Host-embedding surfaces. Supported typed plugin and contribution configuration stays on the recipe.
- R7. The reference game and one renamed-root, independently locked Cargo consumer use the ordinary path. The consumer starts from arbitrary working-directory and home locations without workspace inheritance, private crates, patches, or repository-private helpers.

#### Product Readiness

- R8. Product evidence comes from real author tasks and product execution: data edit-to-result tail latency, desktop playability, clean-to-desktop time, frame P99, process memory, render-packet cost, module addition, supported typed configuration, and executed public production coverage. The legacy `module.add.*` measures map to adding a runtime-only plugin through the recipe; `slot.configure.*` maps to replacing typed plugin or contribution configuration without exposing raw Host slot edits. Both measure pairs remain required for `Publish`. Static source inspection, probe-only smoke, or private hooks cannot satisfy these measures.
- R9. Evidence collection remains a bounded one-slice fixture. Runtime metrics stay owned by their runtime domains, author-task outcomes stay in focused tests or a compact observation record, and no general benchmark, provenance, approval, or telemetry framework is introduced.
- R10. The resulting current-revision review records `Publish`, `Redirect`, or `Stop`. Only `Publish`, cleared P0/P1 findings, exact current candidates, and an independent ordinary-author journey may activate a separate minimal pre-release successor. That successor must still satisfy the original no-rebuild, immutable tag/Release, protected mutation, exact-byte, and anonymous public-smoke outcomes.

### Acceptance Examples

- AE1. Given this plan becomes active, the prior U8/U9/U10 records remain unchanged, U9 still reads `Redirect`, and current-state points to one new registration rather than manufacturing a U11 approval.
- AE2. Given an off-main or repeated candidate dispatch, at least one job fails and the workflow cannot appear successful through an all-skipped graph. A first-attempt main dispatch retains the existing build and consumer matrix.
- AE3. Given a schema-owning game package, its caller adds one contribution. Direct App, headless, desktop, and editor resolution report the same contribution identity and schema fingerprint without a parallel provider list.
- AE4. Given a replayable runtime-only plugin entry, its caller adds it without constructing a package, definition ID, slot anchor, or provider object; repeated Host construction materializes fresh plugin values with the same configuration identity.
- AE5. Given the reference game and renamed-root consumer, both run through public ordinary-author APIs and the packaged binaries still resolve only bundled project data from arbitrary process locations.
- AE6. Given incomplete or failed product metrics, the result remains `Redirect` or becomes `Stop`, and no release workflow is designed or dispatched. Given a complete `Publish` population, a new short successor owns only the minimal immutable pre-release path.

### Success Criteria

- The delivery-only evidence and release closure is absent from active workflows, tools, schemas, fixtures, tests, and capability documentation. This plan may retain its explicit retirement inventory until completion.
- Candidate packaging and no-checkout consumption retain focused security and product verification with explicit invalid-dispatch failure.
- The first-party reference game demonstrates the same small recipe and contribution surface documented for external Rust authors.
- A renamed-root consumer proves the ordinary author path without workspace or private implementation coupling.
- The missing first-playable product measures have current-revision observations and one honest decision; no release claim exists unless that decision is `Publish`.

### Scope Boundaries

**In scope**

- Delivery-authority correction and removal of the unconsumed U11/U12 supply chain.
- Semantic simplification of candidate policy verification without weakening no-checkout, bounded-package, secret-free, or real-product smoke behavior.
- One pure Rust product recipe, one typed schema-owning contribution, and ordinary headless/desktop run facades.
- Reference-game and renamed-root adoption plus the missing product-readiness observations.

**Deferred to follow-up work**

- The minimal immutable GitHub pre-release successor, admitted only by an R10 `Publish` decision.
- Hierarchy/transform, text, audio, physics, save-game, and broader complete-game slices unless a named product-readiness measure directly requires a bounded correction.
- General project generation or a `cargo nara` command; this plan may keep a checked-in editable launch shell as the tracer.

**Outside this plan**

- A package registry, marketplace, dynamic plugin ABI, mandatory scripting language, Wasm host, universal provider registry, Render Graph, 3D, networking, mobile, console, browser, or Steam publication.
- Rewriting immutable historical verification records or treating candidate smoke as human desktop playability.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Treat the U9 Redirect as the current delivery decision.** (session-settled: user-approved — chosen over continuing the prepared U11/U12 chain: the specialized chain was larger than the game evidence it attempted to approve and could pass without ingesting product evidence.) U8/U9/U10 remain completed history; only delivery work after U10 changes direction.
- KTD2. **Delete the speculative supply chain as one functional closure.** Remove its workflows, Python tools, schemas, fixtures, policy/verifier tests, and active documentation together. Do not retain compatibility stubs or convert them into an archive executable.
- KTD3. **Keep candidate packaging as a product action.** It executes real public binaries in a no-checkout environment on both supported platforms. Its policy oracle becomes a small semantic guard rather than an exact YAML interpreter.
- KTD4. **Use a pure `ProductRecipe` and one typed contribution.** (session-settled: user-approved — chosen over exposing Host lifecycle assembly to ordinary authors: callers should be able to inspect and edit Rust composition without maintaining parallel plugin/schema declarations.) Cargo remains code acquisition authority; `nara.toml` requests product semantics; the recipe maps trusted compiled contributions to those semantics. Recipe entries are replayable typed factories/configurations that lower internally to stable plugin definitions; a raw one-shot `Plugin` value is not a recipe entry.
- KTD5. **Keep advanced embedding explicit but out of the normal path.** `App`, raw plugin definitions, runtime candidates, and retirement control remain available only on provisional advanced surfaces. The ordinary facade owns admission, promotion, drive, and bounded close.
- KTD6. **Measure the product, not the evidence protocol.** Keep the exact U9 raw population and compact verdict oracle as historical evidence, but retire its executable collector and collector-specific test harness from the active tree. Add the smallest observation at the owning product seam for each current metric and remove it when it would otherwise become general infrastructure.
- KTD7. **Design publication only after `Publish`.** The future successor may use GitHub-native artifacts, environments, tags, and Releases, plus existing candidate verification. It must not restore a general normalized-evidence schema, approval language, repository verifier, or multi-stage self-attestation framework.

### System-Wide Impact

- **Gameplay authors:** gain a small editable Rust composition path and do not need Host lifecycle vocabulary.
- **Extension authors:** bind persistent schema and runtime plugin behavior once; runtime-only plugins remain simple.
- **Editor and Hosts:** consume the same resolved recipe without losing their existing strict admission and shutdown contracts.
- **Release operators:** keep proven candidate artifacts but have no active publication workflow until product evidence permits one.
- **Repository maintenance:** loses several thousand lines of self-referential scripts and policy tests while preserving immutable historical evidence.

### Risks and Dependencies

| Risk | Mitigation |
|---|---|
| A recipe becomes a universal package kernel | Admit only compiled Rust plugins and the one proven plugin-plus-schema contribution; no registry, download, dynamic loading, or provider SPI. |
| Simplified candidate tests miss a security regression | Keep one direct semantic assertion per retained invariant and use `actionlint` for YAML validity instead of mirroring every step or building mutation machinery. |
| Reference-game migration hides an engine gap behind local helpers | Require the renamed-root consumer and reject workspace inheritance, patches, private crates, and repository helpers. |
| Metric work grows into another framework | Each observation names the decision it changes, has a fixed bound, and stays at its product owner; otherwise record it missing. |
| `Publish` is inferred from smoke or static coverage | R8 requires real desktop, runtime, author-task, and executed-call populations at one current revision. |

---

## Implementation Units

### U1. Activate Product-Readiness Authority

- **Goal:** Atomically supersede the delivery plan, record the current early `Redirect`, and make one product-readiness lane active without rewriting historical evidence.
- **Requirements:** R1-R3; AE1.
- **Dependencies:** The completed RGD-U8/U9/U10 records.
- **Files:** this plan; predecessor frontmatter; `docs/architecture/README.md`; `docs/architecture/adr/implementation-status.md`; `tests/architecture_docs.rs`; new engineering-memory verification and registration shards plus rendered `current-state.md` and `log.md`.
- **Approach:** Record that the prepared delivery chain is deferred because product evidence is `Redirect`, not because its historical local checks never occurred. Keep original RGF-U20/U21 as the terminal contract and point current execution only here.
- **Execution note:** This is an authority transition. Verify every reciprocal pointer and derived view before any implementation deletion lands.
- **Patterns to follow:** The RGD successor activation and immutable correction records already under `docs/knowledge/engineering/`.
- **Test scenarios:** Exactly one implementation-ready plan is active; predecessor/successor metadata is reciprocal; architecture and ledger each point to the active plan once; the new registration supersedes the prior lane head; U8/U9/U10 evidence paths and claims remain unchanged.
- **Verification:** Architecture governance and engineering-memory validation pass, and a staged-scope review shows only the new authority and derived rollups changed.

### U2. Retire the Speculative Delivery Closure

- **Goal:** Remove the unconsumed ingest/approval/release implementation and shrink retained candidate verification to product and security invariants.
- **Requirements:** R3-R4; AE2.
- **Dependencies:** U1.
- **Files:** `.github/workflows/reference-game-{evidence-ingest,release,candidate}.yml`; `reference-game/tools/`; U11/U12-only schemas under `docs/benchmarks/data/`; `tests/{ci_policy,candidate_measurement_helpers,evidence_envelope,evidence_ingest_policy,measurement_helpers,measurement_policy,release_verification,release_workflow_policy}.rs`; `tests/support/{evidence_ingest_fixture.py,first_playable_evidence.rs}`; `tests/fixtures/release/`; `docs/benchmarks/{reference-game-evidence-review,reference-game-release-preparation}.md`; ADR 0099 and its refinements of ADR 0048/0049/0068; affected manifests, ledger rows, and links.
- **Approach:** Delete the functional closure and remove every non-historical reference. Preserve the exact U9 protocol, committed raw population, compact metric aggregation, and verdict rules needed as historical input for U5; delete its executable collector and collector-specific process/transport tests together with the generalized transfer-envelope, revision/cohort ownership, approval, and provenance machinery that has no remaining consumer. Add explicit invalid-ref rejection to the candidate workflow, keep first-attempt main and rerun behavior, and replace exact-pipeline interpretation with focused assertions for trigger, permissions, bounded jobs, pinned actions, no-checkout consumer, real product smoke, and artifact identity.
- **Execution note:** Add the invalid-ref policy expectation first and observe the current all-skipped behavior before editing the workflow. Treat pure closure deletion as a no-test removal; retained candidate behavior owns the regression proof.
- **Patterns to follow:** `tests/artifact_package_policy.rs`, `reference-game/tools/package.py`, and `reference-game/tools/smoke_artifact.py`.
- **Test scenarios:** A first-attempt main dispatch is admitted; a repeated attempt and an off-main dispatch fail visibly; write permissions, secrets/OIDC, mutable actions, consumer checkout, missing package verification, missing real headless/desktop smoke, or an unbounded artifact fail policy; deleted tools and schemas have no active consumers.
- **Verification:** Focused CI and artifact-package tests plus `actionlint` pass, deleted targets are absent from test discovery, and repository search finds only this plan's retirement inventory, immutable historical citations, or superseded plans.

### U3. Add the Ordinary Rust Product Recipe

- **Goal:** Provide one inspectable recipe and typed schema contribution plus small headless and desktop run facades that hide managed-runtime lifecycle plumbing.
- **Requirements:** R5-R6; AE3-AE4.
- **Dependencies:** U2.
- **Files:** `src/product.rs`; `src/project_host/composition.rs`; `src/project_host/runtime/`; `src/lib.rs`; `docs/architecture/open-questions.md`; affected recipe/contribution ADR and ledger rows; focused root product/composition/runtime tests and public-prelude fixtures; affected API documentation and migration notes.
- **Approach:** Resolve the recipe completely before App mutation. A runtime-only entry owns a reconstructible typed factory/configuration and lowers internally to a stable plugin definition; a schema-owning contribution binds its replayable plugin definitions and providers once. Existing advanced edited-group and Host intent mechanisms may implement the facade internally but do not remain requirements for normal callers. OQ-045 stays open during this implementation trial rather than being treated as pre-proven architecture.
- **Execution note:** Start with public compile and integration tests that express the target author code shape. Introduce and verify the new surface while temporarily retaining existing production callers; U4 removes obsolete ordinary-author helpers only after every real consumer migrates.
- **Patterns to follow:** Existing failure-atomic plugin planning, immutable `SchemaValidationInput`, `HeadlessRun`, and `DesktopRun` lifecycle ownership.
- **Test scenarios:** Runtime-only addition resolves once and materializes a fresh value for every runtime construction; schema-owning contribution registers once; typed plugin or contribution configuration replaces its prior value without exposing raw slot edits and retains stable identity across reconstruction; raw one-shot plugins are rejected from recipes but remain valid for direct `App`; duplicate identity and conflicting schema fail before mutation; direct/headless/desktop/editor resolutions share identity and schema fingerprint; facade startup failure and `CloseIncomplete` remain truthful; advanced embedding still compiles only through its explicit surface.
- **Verification:** Focused product, plugin composition, project runtime, public prelude, and module-consumer tests pass under supported feature profiles.

### U4. Dogfood the Ordinary Author Path

- **Goal:** Move the reference game and one renamed-root consumer to the public recipe and facades with no parallel schema or lifecycle assembly.
- **Requirements:** R7; AE5.
- **Dependencies:** U3.
- **Files:** `reference-game/src/`; `reference-game/tests/`; `reference-game/README.md`; `tests/fixtures/runtime-runner/renamed-root/`; `tests/runtime_runner_contract.rs`; `docs/architecture/open-questions.md`; affected recipe/contribution ADR and ledger rows; candidate packaging inputs and affected public-surface tests. `module-consumer/` remains a separate direct-domain regression fixture.
- **Approach:** Keep the launch shell checked in and editable. The reference game owns gameplay contributions only; Nara owns project loading, admission, drive, and close. The renamed-root consumer must prove the same path outside workspace inheritance before U4 deletes the old ordinary-author helpers. After direct, file-backed, renamed-root, and reference-game tracers pass, disposition OQ-045 and update its owning ADR/ledger authority from the evidence rather than from the pre-trial API shape.
- **Execution note:** After U3, an executor who did not implement the recipe first completes the renamed-root plugin-add, typed-contribution, typed-configuration, and run journey from public documentation alone. Migrate the reference game only after that journey passes. Characterize current direct/file-backed outcomes first, move one entry at a time, and keep candidate package bytes semantically equivalent unless the public product layout intentionally changes.
- **Patterns to follow:** The existing renamed-root runtime fixture, U10 no-checkout consumer, and reference-game headless/desktop product entries.
- **Test scenarios:** Source headless and desktop entries use the same recipe; editor resolution matches; renamed-root build and smoke use only public dependencies; arbitrary cwd/home does not alter project discovery; packaged headless/desktop smoke remains stable; no normal path mentions definitions, provider lists, candidate promotion, or retirement ledgers.
- **Verification:** Reference-game default/desktop suites, renamed-root consumer, module consumer, and local package/smoke checks pass with source-level public-surface assertions.

### U5. Re-evaluate Product Readiness

- **Goal:** Collect the missing real product observations at one reviewed revision and issue an honest `Publish`, `Redirect`, or `Stop` decision.
- **Requirements:** R8-R10; AE6.
- **Dependencies:** U4 and fresh Windows/Linux candidate evidence for the same executable revision.
- **Files:** focused product-owned metrics/tests; `docs/benchmarks/reference-game-first-playable-baseline.md` only as historical input; a new immutable product-readiness evidence record and registration; no reusable evidence schema. A `Publish` result additionally owns a new minimal successor plan, this plan's reciprocal supersession metadata, architecture and ledger pointers, and rendered engineering-memory views.
- **Approach:** Re-observe current-revision automatic metrics at their owning build, runtime, data-edit, and structural-edit seams, including data edit-to-result tail latency, without restoring the retired U9 collector or a replacement framework. Product-owned observations fill the ten formerly missing metrics without editing the historical U9 population. Reuse existing runtime counters and bounded OS process observations where they already own the fact. Record bounded manual desktop evidence separately from automated smoke. Measure ordinary module, contribution, and typed-configuration tasks through the renamed-root consumer and executed public coverage through the real product paths. A `Publish` result performs the same reciprocal plan, architecture, ledger, registration, and derived-view transition established by U1; `Redirect` and `Stop` create decision evidence only.
- **Execution note:** Start from the ten explicitly missing U9 metrics and the failed data-tail metric. A measure with no honest population stays missing and therefore prevents `Publish`.
- **Patterns to follow:** The compact U9 baseline, U10 exact candidate identities, and immutable engineering-memory evidence format.
- **Test scenarios:** Complete current populations produce deterministic threshold outcomes; missing, stale, mixed-revision, probe-only, static-only, private-hook, or unbounded observations cannot produce `Publish`; P0/P1 review findings block `Publish`; `Redirect` keeps publication absent; `Publish` activates one separate short plan that cites the original RGF-U20/U21 terminal contract.
- **Verification:** The reviewed current source, exact candidates, author journey, product observations, and decision are mutually revision-bound; engineering memory is fresh; no publication workflow exists unless a `Publish` successor has been activated.

---

## Verification Contract

### Focused Gates

| Unit | Required verification |
|---|---|
| U1 | `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1`; engineering-memory `validate` and `render --check`; exact pointer and staged-scope audit. |
| U2 | Root `ci_policy` and `artifact_package_policy` tests; candidate workflow `actionlint`; deleted-consumer search; direct assertions for retained candidate invariants. |
| U3 | Root product capability, plugin composition, project runtime, public-prelude, and runtime lifecycle tests under relevant feature profiles. |
| U4 | Locked reference-game default/desktop suites, renamed-root and module-consumer checks, and local no-checkout candidate smoke. |
| U5 | Focused product observation tests, fresh same-revision hosted candidates, independent product/correctness review, and immutable decision evidence. |

### Regression Gates

- Serialize Cargo work with `CARGO_BUILD_JOBS=1`, reuse the shared `target`, prefer focused `cargo nextest run`, and expand only when a shared public or runtime contract changes.
- Run `cargo fmt --all -- --check`, affected locked check/test matrices, strict Clippy for changed targets with documented pre-existing allowances, and `git diff --check`.
- Keep `winit`, `wgpu`, `egui`, and `notify` imports inside their owning Adapter crates.
- Preserve exact U8/U9/U10 evidence files; create corrective or successor records instead of rewriting immutable shards.

### Review Gates

- U2 receives correctness and test-simplicity review so deletion does not weaken retained candidate guarantees or leave another YAML interpreter.
- U3/U4 receive API-contract, correctness, and maintainability review focused on the ordinary author surface and removal of duplicate composition authority.
- U5 receives independent product and correctness review; reviewers are not told to force `Publish`.

---

## Definition of Done

- U1 leaves exactly one active plan and registration, reciprocal supersession, truthful current-state, and an immutable record of the current `Redirect` without changing U8/U9/U10 history.
- U2 removes the delivery-only ingest/approval/release closure and leaves a smaller candidate workflow whose invalid dispatches fail visibly and whose real no-checkout product guarantees remain tested.
- U3 gives ordinary Rust authors one pure recipe, direct runtime-only plugins, one typed schema contribution, and small run facades without exposing Host lifecycle assembly.
- U4 proves that surface through the reference game and an independently locked renamed-root consumer across source, headless, desktop, editor resolution, and packaged execution.
- U5 records every required product metric as complete or honestly missing at one current revision and produces `Publish`, `Redirect`, or `Stop` without a reusable evidence framework.
- A `Redirect` or `Stop` leaves no publication implementation active. A `Publish` activates a separate minimal successor that preserves the original RGF-U20/U21 terminal outcomes without restoring the deleted framework.
- Every changed public contract has aligned English API documentation, migration guidance, examples, and public compile coverage.
- Every completed unit has focused verification, a precise Conventional Commit, immutable memory evidence, and no unresolved P0/P1 finding.
- No compatibility shim, duplicate composition authority, workflow interpreter, false approval/publication claim, abandoned helper, generated scratch file, or unrelated staged change remains.
- Work outside Scope Boundaries remains absent from production APIs.
