---
type: "Work Progress"
title: "Compiled Scene production storage and Native Runtime integration in progress"
description: "Inline prerequisite is committed and local main advanced; compiled-only producers consumers and compact immutable storage compile while full qualification and coherent naming remain open."
timestamp: 2026-11-02T04:09:20Z
record_id: "f8d1c94059364a21ac62e8b27226845a"
producer_id: "codex-foundation-main"
run_id: "compiled-scene-production-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "a2640a10973b6d1724176a506237bc458aa5f3bc"
supersedes: "6f60b71dc5c6458bbd6c9c63f1df239e"
---

# Summary

The complete foundation Goal remains active. The qualified inline-reference/contextual-default
prerequisite is committed as `a2640a1`; local main and refactor/foundation both point there.
Preceding corrections are `94cb092` and `840325b`. Nothing was pushed. The current compiled
production replacement and consumer migration remain uncommitted; preserve all their changes
and the original untracked vendor directory. The user explicitly authorized handling all current
changes. No compatibility reader, alias or migration guide is needed.

Windows/pwsh, Rust1.95.0, serial Cargo, jobs1 and the root target remain mandatory. Independent
workspaces use that same target. F has roughly342GiB free; no artifact cleanup is needed. An
unrelated Cargo process belongs to the baozi checkout; do not interfere with it.

# Details

Compiled storage is now connected to actual producers and consumers. Its private components carry
actual selected owner/version/value; immutable roots/entities/children use Box slices with no
hidden spare Vec capacity. Admission precedes possible root/child compaction and replacement
containers. Values and identity strings move; no raw-source clone bridge or public constructor
manufactures execution authority. Private FlatSceneClaims contains only the flat fields. The
Runtime wire independently accepts owner/version/value and rejects source fields at every level.

Strict PrefabExpansionReport currently returns CompiledSceneDocument. Authoring observations retain
ExpandedSceneDocument; the original report alone can filter and seal Native preview. Selected
conversion remains a fragment. ExpandedSceneCandidate, its plain constructors and the observation
binding bridge have been deleted. Ordinary native-authority sync goes through the same compiler
with an empty resolver. Runtime candidates hold compiled data, exact definitions witness and
limits; cook uses actual flat claims without supplying defaults or rebuilding source documents.

Native spawn/prepare/replacement/projection/product/claims accept CompiledSceneCandidate. The
receiver checks the complete ordinary batch before codecs, with exact owner absence, current
versions and explicit values. Runtime reuses pure admission only after its exact witness and World
guard. Existing asset, lifecycle, retirement and guarded commit owners remain. SceneDataComponents
now stores CompiledSceneComponent; direct setters use the private admitted-data constructor after
their existing admission. Export deliberately produces fresh source records from actual carrier
values; it is not a reverse bridge into activation.

Parent owns compiled.rs/claims/tests, validation.rs, schema_authority.rs, candidate.rs, Scene
authoring.rs/document.rs/lib.rs, Root lib exports and remaining integration. Shared pure validation
uses actual flat component facts and one common hierarchy kernel for source, claims and compiled
data. Schema/default/Native-leaf/asset/local-closure callbacks preserve nodes, bytes and allocation
axes. Reference-work documentation now describes the complete effective-reference proof, including
empty discovery, defaults and shape, rather than asset output alone. No allowance was raised.
Explicit aggregate cost traversal admits scratch and propagates the original caller error; its
unadmitted extra depth stack and the sealer's duplicate aggregate-value scan were removed.
The admitted Native walker proves depth and flatness. New exact/one-short callback controls cover
independent work nodes and bytes; WideSchema accounts actual cost/raw walker storage.

Root production uses real compiled custody and getters, retaining source documents separately.
The unused root_effective/effective_document cache and its clone were removed. Dependency authoring
observations remain ExpandedSceneDocument plus original outer source facts. Native preview keeps
its original compiler allowance, permits only cap reduction, and returns final actual work after
sealing so the Host reservation can settle truthfully. Root startup/retention/content/cook and
Native projection adapters compile. Parent also migrated Runtime dependency/prepared/deep queries
and Scene request views to CompiledSceneDocument, without source reconstitution.

Actual results so far, not complete qualification:

- Scene all-feature and no-default library checks passed. Root+Editor all-feature library check
  passed, as did Tooling/UI libraries.
- First compiled/Runtime/receiver/preview cohort46/49, run
  a39c6c1a-4218-42b0-a37d-bfcad9c5571d. All three failures assumed World::new has zero entities;
  this Bevy version has four metadata entities. Corrected tests compare the entire original ID
  set, preserving zero-codec/migration and absent-domain assertions. Receiver4/4 replay passed,
  e410e84a-9cc3-4f9f-9b6b-c36e6f04f4fc.
- Focused compiled/Runtime/receiver/preview49/49 passed,
  5e443346-82f5-47ea-9a54-f1f80552ba7d. This precedes the final full callback-axis forwarding.
- Full Scene library606/608, run78643f1f-aba4-4947-bb07-8124a62820c0. Two tests need the actual
  owning-stage expectation: Visibility invalid string is Schema-valid and should fail Native
  projection while preserving last-good; a dangling reference now fails compilation rather than
  the old direct Native diagnostic. Gibbs owns both corrections.
- Tooling/UI libraries and public conversion/source-identity260/260 passed,
  851db8ac-f214-4123-baf6-e5192df57b22. One unused Arc import was removed afterward.
- Root public test compile stopped at an old plain candidate in plugin_composition. Root library
  compile then showed five test errors: capture/tests.rs:329 needs compiled getters; Scene requests
  tests need an explicit SceneDocument import no longer inherited from a production import.
  Avicenna owns these, not product regressions. No Root test run passed yet.

Independent Kepler review closed the callback and temporary-allocation findings after parent
corrections; no remaining assigned P1/P2 in its shared-core scope. It was read-only, ran no Cargo
and is closed. Harvey is independently reviewing the Native/Runtime/retention integration. These
are scoped reviews, not a review receipt for all current modified files.

Active agents and disjoint ownership at capture:

- Avicenna01a19a47-0a82-7ef3-987f-d401f6f1910c: Root production is frozen, nine files. Now writes
  only Root src test modules/blocks and root tests/support, excluding all fixtures, crates, Editor
  and Reference Game. Prioritize Root library-test readiness, then public tests. Parent sent the
  five actual compiler errors. No Cargo/fmt/commit.
- Gibbs01a19a47-516d-7ef0-81e6-b80ed1130b67: Native production is frozen. Now owns export.rs,
  src/tests.rs and tests/ subtree, Scene public integration tests, and overlay/regression_tests.rs.
  The two full-Scene failures above are assigned to it. Parent alone fixed the new receiver test
  baseline and validation's prepared-record getter. No Cargo/fmt/commit.
- Harvey01a19a5c-a9d9-7db3-baa7-a95089d3f39e:35 external consumer files are source-ready/frozen
  across examples, Reference Game, module consumers and behavior-language consumers. No budget
  constants/formulas changed. Now read-only reviewing other workers' actual Native/Runtime and
  Root retention code, not its own migration. No Cargo/fmt/commit.

# Next Action

Poll Cargo session73121 before any next Cargo. It runs the final current Scene all-feature library
suite, log TEMP/nara-compiled-scene-integrated-lib-tests.log, run
9480d671-6f74-4e9a-8765-0050f96f3e1d,608 selected. It covers the final callback/scratch corrections
and Gibbs's library changes as sampled at build time. Do not assume the result from earlier runs.

Then collect actual remaining results and owner freezes. Run Root library/public qualification,
Scene public/minimal, independent consumers, Reference Game and affected native trajectories.
Use exact formulas for new storage/work costs rather than loosening caps. Perform one coherent
public rename after every writer freezes: compile/SceneCompilation for full compilation,
compile_scene on mixed bindings; preserve actual Prefab source/declaration names and selected
fragments. Remove displaced expand_prefabs names without aliases. Do not format globally while
writers are active. Qualify the final interface, then precise commit and local ff main.

The complete CP2 root/placement representation, extraction correspondence, publication/recovery
and the broader foundation Goal remain open. No production qualification is claimed for this
unfinished replacement. Original inline prerequisite evidence remains independently valid.

# Citations

- [Complete CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Qualified inline prerequisite](../../verification/2026-11/2026-11-02T031431Z-inline-authored-references-and-contextual-default-production-qualification-6f60b71dc5c6458bbd6c9c63f1df239e.md)
- [Source/compiled contract](../../../../architecture/adr/0006-scene-and-prefab-data-model.md)
- [Native qualification](../../../../verification/native-editor-qualification.md)
