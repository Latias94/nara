---
type: "Verification Evidence"
title: "Required Scene sources and active occurrence ownership qualification"
description: "Required source-bound materialization and Scene-owned active logical occurrence replace identity-free spawn, qualified through owning, public, independent and selected native consumers."
timestamp: 2026-10-24T11:49:01Z
record_id: "e249da9a1f404677a72e52457ac990c4"
producer_id: "codex"
run_id: "session-01a1629b-scene-occurrence"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_base: "c03ac37"
status: "qualified-scoped"
---

# Verification

# Result

Pure Prefab expansion now returns a move-only `ExpandedSceneDocument`. Its source owner consumes
`bind_source` to obtain a required-source `ExpandedSceneCandidate`; plain construction likewise
requires the source. The sole ordinary spawn function requires that candidate and an explicitly
declared occurrence namespace/slot. Source-less spawn/Prefab convenience variants and the public
stateless `SceneSpawner` surface are removed, rather than kept as compatibility wrappers.

Scene owns a private World-bound active association keyed only by namespace/slot, with source and
exact domain/activation as values. Empty instances and instances emptied through entity retirement
retain occupancy. Exact whole-instance end, replacement and disposable projection clear publish
or release only that activation's association in the same exclusive transaction. Source switches
cannot bypass occupancy, and a stale epoch receipt cannot match a successor's reused instance
number. Limits bound active entries and modeled retained bytes; there is no eviction, permanent
slot history or second Entity resolver. Runtime source components share the containing context.

Final registration now re-prepares and validates its exact installed/detached domain and allocator
baseline before publication. The previous immediate proof could not safely survive intervening
registrations. Ordinary public no-entity preparation remains a subsequent seam; the existing
advanced retained token still requires explicit World cleanup.

The product declares fixed current-Scene and preview occurrences as its consumer policy. Editor
documents retain their own admitted source, not the selected startup Scene's identity. Native
document admission reads exact stored paths and policy-required sidecars; Open, preview, trusted
Play and Reopen carry those source associations. The scoped final execution and review below
qualify this owning replacement, not the remaining ordinary preparation or UI lowering.

# Evidence

- Initial six public occurrence scenarios cover occupied-slot refusal before target allocation,
  independent sources/Worlds, empty/individually emptied instances, exact end/reuse, budgets,
  projection domain fencing and exact replacement source switches.
- The first complete Scene run (`832e1dd9-c60f-4a61-b6e7-293fece660e6`) had 308 passed, 2 failed,
  1 skipped. Existing tests exposed premature baseline flush before overlay refusal and changed
  event-global observer diagnostic precedence. The owning correction moves occurrence admission
  after existing product/persistent baseline validation while keeping it before target allocation.
- The two regressions plus all six occurrence cases then passed: 8 tests, run
  `f5655db9-e5fc-474f-a3d8-33fbef7aa278`. Final full cohorts are recorded below.
- The subsequent Scene/UI/tooling cohort passed 462 tests, 1 skipped, run
  `7ffa34b3-48d7-4e20-b04d-05d1e7e00195`, using
  `cargo nextest run -p nara_scene -p nara_ui -p nara_tooling --all-features --locked -j 1
  --no-fail-fast --failure-output immediate --status-level fail --final-status-level fail`.
  It includes a private deferred-registration counterexample for intervening allocator/domain
  publication as well as the six public occurrence tests. Product changes were still pending then.
- After adding the limit-shrink/non-eviction case and preserving expansion diagnostics through
  the shared authoring sync adapter, Scene/UI/tooling passes 463 tests, 1 skipped, run
  `a90cfe0c-4c97-48c4-b1f4-eac9bd8513c6`. The eight selected occurrence/registration cases pass
  independently in runs `ddb65758-d0f7-433a-b9b8-9fa50c2ac044` and
  `1638d15c-fdb8-4d5a-9d73-b901ad959111`.
- All seven Scene compile-fail doctests pass. `cargo check -p nara_scene --no-default-features
  --all-targets --locked --message-format short` passes with existing non-blocking warnings.
- The first root/public cohort had 558 passed, 2 failed, 6 skipped, run
  `e7be6b9b-54d5-47c5-8a36-05689992716f`. Both failures were trybuild recompilations observing
  transient repair-lane signatures before their callers were updated, not intended compile-fail
  diagnostics. The cohort must be rerun after that lane stops writing; no final root claim is
  derived from this run.
- The frozen repair-source run (`977ba643-3669-4f32-aaa7-6dc5c041ff91`) had 563 passed, 3 failed,
  6 skipped. One new fixture incorrectly requested a Browser index from the Trusted Host, which
  has no such public path; the test now uses Authoring Browser Open and Trusted's actual owner
  gate plus anchored Prefab Open. The process aborted after the test's `Option::unwrap` panic;
  the Windows exit message alone does not establish a production buffer-overflow defect. Two unrelated compile-fail expectations
  differed only in rustc 1.99 diagnostics. CI and the native procedure pin Rust 1.95.0; the final
  root/public run uses that toolchain rather than rewriting unrelated snapshots for local 1.99.
- `cargo check --workspace --all-features --all-targets --locked --message-format short` passed
  after API migration fixes. Further behavioral changes require their corresponding checks.
- The first pinned Rust 1.95.0 root/public run (`76ce316b-7159-4591-9622-3dd7ea103ef5`)
  had 565 passed, 1 failed, 6 skipped. Both compile-fail suites passed unchanged on the CI
  toolchain. The remaining new fixture called anchored Trusted Prefab Open without selecting
  its anchor; it now performs the actual workspace selection first. No product safety gate was
  relaxed to make the fixture pass.
- Handle review produced one additional reachable budget defect: Prefab native admission retains
  the original Prefab file and directory in addition to the three schema handles. Policy source
  and metadata probes therefore reach nine native handles on Linux and eight on Windows, not
  seven. Location policy also needs eight on Linux. The path-reference fixture deliberately
  avoids catalogue scanning, whose separate high-water accounting previously masked this path.
  The new test failed before the fix (`2e10c3eb-65d8-4d7b-8f2c-236946973128`, 0 passed, 1 failed).
  The owner now admits the simultaneous peak before opening, separately from unique-file
  accounting. Shared directory `Arc`s do not count as additional native handles. The four
  selected handle/metadata/retention/Trusted-Open regressions then passed on Windows/Rust 1.95.0
  (`8b24b48c-5556-4107-a93d-383a17f76cce`). Linux figures are source-derived, not native Linux
  execution evidence.
- The repaired product/public cohort passed all 567 tests, 6 skipped, on Rust 1.95.0
  (`81805759-b6af-4e6e-bac7-fc4313ce2242`). The subsequent private-storage/proof simplifications
  below required fresh affected core and product runs; this result is not their final evidence.
- Those final-source Rust 1.95.0 reruns pass: Scene/UI/tooling 463 tests, 1 skipped
  (`3b9173a3-234b-4b4a-99f3-c3010c6e9893`), and root/product/public 567 tests, 6 skipped
  (`760d62f9-5bef-46a6-821f-ca59169b0062`). Root selection is `--lib` plus
  `project_content_boot`, `project_content_limits`, `project_content_delivery`,
  `project_scene_content`, `project_workflow_creation`, `project_workflow_cli`,
  `scene_authoring_session`, `scene_inspector`, `scene_component_composition`,
  `scene_sprite_serialization`, `stable_runtime_identity`, `plugin_composition` and
  `scene_product_public_surface`, with `-p nara --all-features --locked -j 1`. Ignored allocation,
  filesystem and projection measurements and three nested-Cargo CLI trajectories plus the linked
  export trajectory are not rerun or implicitly qualified by these cohorts.
- The independently resolved module consumer passes 35 tests, 1 skipped
  (`afab631f-263d-4d0f-bbd9-4635a59eca56`), using `cargo +1.95.0 nextest run --manifest-path
  tests/fixtures/module-consumer/Cargo.toml --target-dir target -p nara-scene-module-consumer`
  with `scene_spawn`, `nested_schema`, `durable_identity`, `prefab_sources`, `source_context_trial`,
  `domain_reconciliation` and `image_reconciliation`. The skipped test is the explicit owner-trial
  timing experiment. That historical experiment still has a disposable private trial claim table;
  its intent preparation is not the forthcoming ordinary Scene preparation API. Actual Scene
  occupancy/retirement guarantees are established by the public occurrence tests and the migrated
  real module workflow, not by relabelling prototype guards as engine proof.
- The actual reference-game consumers pass 31 tests, none skipped
  (`1576b45f-e211-4769-83e9-db68599fd5a7`): `cargo +1.95.0 nextest run --manifest-path
  reference-game/Cargo.toml --target-dir target --test first_wave --test authoring --locked -j 1`
  with the same failure-output settings. This is the selected headless authoring/startup workflow,
  not the entire reference-game suite or standalone desktop delivery.
- Six selected actual Windows/DX12 Editor trajectories pass
  (`1c6ae84f-1007-45ad-8cc5-e3d941234bc4`, 6 executed/passed; 202 unselected):
  `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`,
  `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`,
  `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`,
  `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`,
  `native_ui_canvas_priority_survives_closing_retained_scene`, and
  `native_unrelated_prefab_close_preserves_presented_play_source_until_source_close`.
  Command: `cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E`
  with the OR of those exact `test(...)` names and `--test-threads=1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`.
  `WGPU_BACKEND=dx12` and `CARGO_BUILD_JOBS=1` were explicit. Hidden native windows submit/present
  through production paths without pointer/keyboard injection. This does not establish monitor
  scanout, OS IME/accessibility, the entire native group, or Linux/macOS qualification.
- Tested source is `c03ac37` plus this unit's owned dirty changes. The 71 changed/new Rust paths
  have an LF-normalized source manifest SHA-256 of
  `86294f071176ce5e1f15b07f1b0dafd9b960c2ae5991fa19796909b8488821d1`.
  The digest hashes the sorted path-to-SHA-256 map as compact, sorted-key JSON; it excludes
  documentation/derived rollups. No source mutation is intended between this freeze and commit.
- All Cargo work is serial, `CARGO_BUILD_JOBS=1`, with the shared `target`.
- Final Rust 1.95.0 workspace all-feature/all-target `cargo check --locked -j 1` and Scene's
  no-default-feature/all-target check pass. All seven Scene compile-fail doctests pass again on
  the frozen final source. Clippy passes for `nara_scene`, `nara`, `nara-editor`, `nara_ui` and
  `nara_tooling` with `--all-features --all-targets --locked -j 1 -- -D clippy::correctness`.
  This is not warning-free qualification: non-blocking unused, large-result and argument-count
  warnings remain, and no blanket auto-fix was applied. Rust 1.95.0 format checks pass for the
  root, module-consumer and reference-game workspaces. No code changes follow these checks.

## Review And Integration

Independent core review `01a16db1-b877-7bc2-8d2e-08e7da9df12b` found no supported new defect in
the four named owning files. It inspected lifecycle/budget admission, deferred Register, exact
replacement, empty instance end and projection epoch scope, and executed no tests. It did not
review the separate Editor/native-source implementation or qualify UI lowering.

Implementation lanes `01a16da6-1d47-7313-9bf4-fdbfd4e25824`,
`01a16da6-6e2b-7633-927a-12e144e9f979` and
`01a16da6-eb12-7b72-86b6-548da73e5aff` migrated disjoint candidate/product/consumer scopes.
The main owner owns all build/test results; worker completion is not acceptance. Those handles
and the core reviewer are closed. Product review and qualification are recorded below.

Independent product review `01a16dbb-d22e-73a1-8409-751fc9bfa2d7` identified five concrete gaps:
new Open did not compare stable IDs against loaded but unopened Prefabs/assets; Reopen did not
capture exact preparation-time metadata identity/digest; metadata input budget rejection lost its
aggregate/source classification; retained-source budget admission happened after allocation; and
document kind incorrectly selected one global root for metadata-relative paths. These findings
were assigned to repair lane `01a16dc3-bd1f-7883-bb0d-dea41dd7730b` and fixed before final
qualification. They are not accepted residual risks. The review executed no tests.

The repair lane is complete and closed, with no worker-executed Cargo results. Its six focused
regressions exercise non-kind-root native documents, typed metadata limits, pre-open retained/work
admission, loaded-but-unopened stable identity conflicts in both Hosts, and Scene/Prefab Reopen
after same-key byte/object replacements. Native metadata receipts remain local to a preparation
completion and are revalidated through their captured admission coordinate; no new global registry
or persistent receipt lane was added. The final main-owned results are recorded above.

Final read-only product follow-up `01a16dd4-a275-7813-9b7d-157f0140ed15` found no supported new
defect in the five repairs, but its initial statement that the Trusted conflict test exercised
Browser Open was contradicted by execution. The reviewer corrected that coverage after the fixture
repair: Authoring uses actual Browser Open; Trusted uses its retained catalogue/private owner
gate for conflicts and actual anchored Open for the positive same-source path. Same-key metadata
mutation tests execute the Authoring Host; shared Trusted publication gates have static coverage
only. These limits remain explicit and do not qualify a nonexistent Trusted Browser workflow.

Read-only handle review `01a16de3-2b2b-7882-b782-dd3deacc089d` traced actual native file and cursor
ownership rather than Rust wrapper counts, identified the additional handle-peak defect, and is
closed. The owning repair and its red/green regression are recorded above.
Its read-only follow-up `01a16dee-7131-71d1-b496-89c1f2f83ae6` confirmed the pre-open placement,
unique-file separation and bounded nested traversal accounting without a supported new finding;
it is closed. Neither review directly measured native handles.

The bounded pipeline simplification pass used five read-only lanes: reuse
`01a16def-ff7e-70a1-a3d3-fd944b211c95`, quality `01a16df0-2f71-7532-8a1d-b21f0a08b8c5`,
efficiency `01a16df0-5910-7762-8e28-76494375af9f`, test value
`01a16df0-b46c-7982-a336-f4637401e042` and comments `01a16df0-f33d-7962-a517-b5f74c182d39`.
All are closed and executed no Cargo. Reuse removes the duplicated public Register dispatch.
Quality retains only the initially validated instance ID, rather than a complete old registration
proof that final validation replaces. Efficiency uses a direct occurrence lookup on successful
replacement, preserving exact error classification on failure. It also identified the new plain
sync adapter's full document copy: the private Scene session now shares an `Arc<SceneDocument>`
and mutates through copy-on-write, including its existing prepared-edit publication path. This
changes no public document, history or materialization semantics and adds no cache or skip-sync
policy. These are source-grounded allocation/work removals, not measured latency claims.
Three redundant internal comments are removed; publication-cut and admission-root caller
constraints remain because this bounded change cannot encode their full ownership preconditions
in one test assertion. No protected proof test was deleted, weakened or rewritten; static test
review found no supported false-confidence defect. The final affected execution above passes.

Code review: skipped (ce-code-review unavailable)

The actual portable invocation `01a16de1-23f5-77e1-a7f5-470dc543102a` terminated with `status: failed`
because its client could not discover the required native child-agent/terminal-collection tools.
It is closed and is not a completed portable receipt. The scoped native independent reviews above
and an explicit final main-owned diff scan remain separate evidence, not a substituted receipt.

# Follow-up

The owning checkpoint is qualified by the final-source cohorts above, not merely a workspace
build or the eight-test fix check. The main owner's final diff scan checks exact retirement,
deferred proof use, metadata revalidation, retained source custody and migrated signatures;
no supported blocking defect remains in this bounded unit. Document links and memory rollups
are checked after the final body update. A subsequent progress shard binds this tested source
manifest to its precise local implementation commit.

After this owning checkpoint, expose ordinary preparation at unbound values, migrate Runtime and
Canvas complete-key lowering together and refreeze ADR 0102. Current UI collision characterization
remains an incumbent expectation; these required Scene inputs alone do not fix UI equality.
Imported compound products and the other foundation waves remain open under the full goal.

# Citations

- [Candidate and document separation](../../../../../crates/nara_scene/src/candidate.rs)
- [Occurrence owner](../../../../../crates/nara_scene/src/occurrence.rs)
- [Public lifecycle tests](../../../../../crates/nara_scene/tests/occurrence_lifetime.rs)
- [Source and occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
