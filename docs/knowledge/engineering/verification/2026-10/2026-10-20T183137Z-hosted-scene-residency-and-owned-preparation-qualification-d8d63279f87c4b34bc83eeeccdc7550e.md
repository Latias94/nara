---
type: "Verification Evidence"
title: "Hosted Scene residency and owned preparation qualification"
description: "Verification Evidence for Hosted Scene residency and owned preparation qualification."
timestamp: 2026-10-20T18:31:37Z
record_id: "d8d63279f87c4b34bc83eeeccdc7550e"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-persistent-two-level-game-plan.md"
git_branch: "main"
git_commit: "cf1f354"
verified_by: "serial nextest and independent read-only review"
---

# Scope And Result

Starting commit: `cf1f354`. The owning correction adds finite CPU project residency and one
original-Host background Scene request, without a new executor, general scene manager, filesystem
dependency in `nara_scene`, or native rollback promise. Root runs Cargo serially with Rust 1.95.0,
`CARGO_BUILD_JOBS=1`, and the existing root target. Unrelated untracked `vendor/` is preserved.

One `SceneAssetBindings` carries the otherwise empty component batch and one scratch AssetServer
through admitted resource handles and Scene decoding. Root preflights exact Image slot/state
changes, owned TileSet publication/retirement, merged catalogue changes and retained metadata
before the protected Scene transaction. The commit publishes destination allocations even for
shared handles, frees old-only owned publications and preserves foreign records/values. Explicit
TileSet unload frees its source entry; shared or automatically revoked sources retain their atlas
layout barrier. Historical AssetServer identities remain admitted rather than reused.

`ProjectScenes` captures the original candidate, frozen plan, shared loader, full current receipt,
runtime generation and exact request ID. Existing threaded Io workers prepare world-free content;
Inline test executors reject. Prepared content is borrowed and never commits automatically.
Cancellation keeps the single slot until the capture returns physically; terminal acknowledgement
is exact-ID. Safe original loader diagnostics remain available through the same request. Runtime
close revokes interest while existing Tasks retain physical custody.

# Verification

- Asset/Scene/TileSet/Tasks all-feature library cohort: **328/328 pass**, nextest
  `9c92ca5d-4511-43e1-b716-6ab321a1b06f`, tests 20.551 s, compile 41.06 s.
  Local log: `target/project-scene-residency-domains-final-nextest.log`.
- Root all-feature library cohort: **304/304 pass, one pre-existing skip**, nextest
  `0ffbdf65-0b6b-46d6-82c7-77fbb8a384db`, tests 31.606 s, compile 2m04s.
  Local log: `target/project-scene-request-host-nextest.log`. The 14 new tests cover five
  worker/preparation cases, five actual Host residency trajectories and four public Host request
  trajectories. Other existing content, Host, freshness, persistence, shutdown and Retry tests pass.
- Root all-feature library and minimal `runtime-core` library checks pass. Existing subset
  dead-code warnings remain; this is not a warning-free build claim. Logs:
  `target/project-scene-request-root-check.log`, `target/project-scene-request-core-check.log`.
- Public content/startup/Host boundary cohort: **50/50 pass, one pre-existing skip**, nextest
  `8e09b4ad-fd42-4c73-8ab6-0afae4ae7009`, tests 76.840 s, compile 1m52s. Negative compilation
  retains the existing expected diagnostics; no golden updates were needed. Startup's negative
  compilation case takes 66.000 s and is reported slow. Log:
  `target/project-scene-residency-public-nextest.log`.
- Final `cargo +1.95.0 check --workspace --all-features --all-targets` passes in 1m54s, with the
  existing warnings noted above. Log: `target/project-scene-residency-workspace-final-check.log`.
  An earlier attempt reached the concurrently assigned test-module wiring before its new file was
  delivered; the final run follows worker STOP and complete integration.
- `cargo +1.95.0 fmt --all` completed before final validation; formatting and diff checks pass.
- Actual compiled Editor session owner-drive: **1/1 pass**, nextest
  `e7a6c1ae-fb4f-4fdc-9ca4-9e33b2d3d07a`, tests 0.233 s, compile 36.47 s. Ordinary game systems
  request/inspect/commit/ack A -> B -> unchanged A while the preview document stays A. Distinct
  generation high-water is two, returns to one on A, remains one after Stop, then reaches zero only
  after Editor close and actual session drop. This is not native UI/GPU qualification. Local log:
  `target/project-scene-editor-travel-final-nextest.log`. The initial run failed because the test
  compared canonicalized Sprite defaults with raw incomplete JSON. The correction preserves full
  committed-versus-prepared equality and independent path/root/entity/receipt oracles.
- Reference Game content/composition/runtime cohort: **15/15 pass**, nextest
  `87580760-e1cc-4b30-a89a-37d90ed5a69a`, tests 1.631 s, compile 6m11s. Selecting all features also
  builds native product probe binaries; this compile duration is not test execution or a measured
  attribution to Bevy. Log: `target/project-scene-residency-reference-nextest.log`.
- Reference Game wave/Retry/CLI/tracer cohort: **32/32 pass**, nextest
  `f8bd509d-f0cd-4063-9ebd-86297377a4ec`, tests 3.812 s, compile 27.79 s. Consecutive Retry,
  retained-source assets, precise retirement, stable terminal ticks and CLI summaries pass.
  Log: `target/project-scene-residency-reference-retry-nextest.log`.

# Retained Failures And Review Corrections

Earlier domain runs exposed two test-fixture errors, not a discarded production rejection: an
incorrect AssetLink identity, then treating Bevy's allocated entity-index length as a live-entity
count. Final Scene tests compare the exact live entity set, spawned count, old membership and
precise component failure diagnostic. The corrected five bindings tests and three doctests pass;
the final 328-test run includes those unit cases and all ten TileSet publication cases.

Independent read-only review found and closed:

- foreign Loading Image state without a raw value could be overwritten;
- lifecycle observers on nested scoped owners could alter preflighted resources;
- a guarded poll could cancel forever without draining a physically returned worker;
- stale Ready observation and acknowledgement disagreed in the same frame;
- mapping content errors to their category discarded existing actionable safe diagnostics;
- an empty Image plan still borrowed AssetEvents after successful Scene commit.

Final source review found no residual blocker. Actual Host regressions check A -> B -> A -> empty,
shared exact destination pixels, foreign startup publications, atlas/retention refusal, escaped A
blocking a third generation until its last drop, unchanged complete baselines on rejection, empty
replacement without AssetEvents, public cancellation/acknowledgement, and held preparation across
close. The root SystemParam derive needs a local `nara_ecs as bevy_ecs` alias; it adds no second ECS
dependency. A first public test command named nonexistent targets and exited before compilation;
the corrected target inventory is used for the subsequent run.

# Limits And Next Work

The independent two-level consumer has qualified pure bounded checkpoint DTOs, but full gameplay,
file publication, fresh-process restore, exported travel and native desktop acceptance are separate
P3 work. Neither lower CPU residency nor these headless Host tests qualify GPU/native reconstruction.
Additive loading and the full Proposed ADR 0089 remain unaccepted.

Editor authoring, Prefab projection and next-Play preparation legitimately retain their admitted
content generation. Runtime retirement does not imply those independent owners retired. Unchanged
A -> B -> A shares the same A revision; Editor A plus runtime B plus C or changed A2 can reach the
configured two-generation ceiling. Do not delete needed Editor retention or create another loader
to evade that budget. Broader simultaneous Editor content needs its own concrete workflow design.

Hosted CI run `39463001464` concerns **3a9284a**, not these local changes. At 2026-10-21 02:26 +08,
six jobs pass, including Ubuntu Root and compile-only wasm; Windows Root remains in progress.
The first current-Scene commit `cf1f354` has intentionally not been pushed over that running CI.
Deprecation/MSRV and actual Bevy/Nara task dependency costs remain the ordered post-P3 investigation.

# Sources

- [Plan](../../../../plans/2026-10-20-persistent-two-level-game-plan.md)
- [ADR 0117](../../../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md)
- [First ownership qualification](2026-10-20T172653Z-current-scene-ownership-and-decoded-generation-custody-qualification-0bf781c53d9e4cc3950bfb77b98d2b8a.md)
