---
type: "Verification Evidence"
title: "Disposable Scene projection epochs and stable native preview qualification"
description: "Qualify complete-domain Scene epochs while retaining shared authoring owners and renderer producer identity."
timestamp: 2026-10-23T22:58:00Z
record_id: "2478d897135549daacbc8d1fbc90b9d9"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-projection-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "207bbcc38f6797a0bd44de252849a1a3221935ab"
verified_by: "codex-root; five scoped read-only Codex reviews"
---

# Verification

Qualified source: `207bbcc38f6797a0bd44de252849a1a3221935ab`, following the frozen incumbent
collector and entry measurements at `6f558f3`. This is the bounded R5c disposable Scene preview
epoch unit, not completion of R5c, R2, durable property addressing, Prefab redesign or the full
foundation rebaseline. Local date: 2026-10-24, Asia/Shanghai.

# Result

- Identity proves the entire installed domain belongs to the exact current projection: all
  instance/reference claims, registered axes and incarnations, persistent logical records and
  live private markers. Empty Scenes retain ownership; unrelated retired claims, Unloaded/Deleted
  records and adopted-but-unregistered tokens reject. Detached successor tokens are not mistaken
  for predecessor responsibility. No continuing-domain claim ledger is cleared or evicted.
- `SceneProjection` owns materialization and its receipt, not another document/Undo session. It
  reuses Scene's prepared component, asset-binding, hierarchy and exact retirement transaction.
  Fresh tokens belong to a detached domain bound to the same World; complete ownership and the
  captured predecessor are checked again before a lifecycle-free commit. Refusal preserves
  last-good. Prepared candidate disposal uses its detached domain. Ordinary Scene registration
  and same-domain replacement retain their existing semantics.
- The Editor keeps the same authoring Runtime, World, task pools, image store/importer and file
  custody. Accepted source/revision, omissions and producer association change only after Scene
  success. Failed requests cannot authorize a new-revision candidate. Clear retires the complete
  projection epoch, including an empty Scene, without closing shared task/image owners.
- Scene identity epoch, renderer producer and presentation authority are separate lifetimes.
  The renderer producer is reserved at preview-owner installation and retained across same-owner
  edits, source switches and image-only refresh. Native target/texture registration is reused.
  Identical last-good content with invalid presentation authority must be accepted, submitted and
  composited again; cache equality never revives an invalid token.
- ADRs 0058/0026/0047/0111 adopt these contracts. ADR 0115's shared task/image ownership remains
  unchanged. No compatibility layer, global Expired interface or migration guide was added.

# Evidence

| Final scope | Result | Nextest run ID |
|---|---|---|
| Identity, Scene, Tooling, Host and Editor libraries | 860 passed, 27 skipped | `8bd1bce7-d070-4fd3-9a28-5e1faee09b2b` |
| Public persistence/Play, including joined preview workflow | 41 passed, 2 skipped | `9e9464ff-27b2-450d-9588-14dbd4397a83` |
| Independent Scene consumers, four binaries | 15 passed | `8087272b-8ef4-4e3e-bc90-4dcfd9c7e428` |
| Focused real authoring owners and held work | 13 passed, 255 skipped | `140c43ba-2c35-4734-b1f7-c7121985b640` |
| WGPU Editor admission/receipt/resize controls | 14 passed, 49 skipped | `3d07b597-6bc2-4d1f-bd79-55f971267de1` |
| Selected actual Windows/DX12 trajectories | 6 passed, 201 skipped | `6bfb1848-04d5-4177-aa0c-04721ffd95c1` |

Earlier focused owner runs pass Identity 71 (`ffa28cf4-1567-461f-a4a4-46af796862c1`) and Scene 183
(`8c0da6c3-bbbf-420b-98b1-ad2cc7defb03`). These overlap the library cohort and are not added to
its count. The final joined public workflow was added after the 860-case run and then executed
in the final 41-case run; no production source changed during that addition.

The public CPU-toolkit consumer executes colored-sprite creation, four real field edits,
Undo/Redo, structural addition, Save, same-document Reopen, exact preview/picking and Close. It
uses actual Host/presentation APIs, rasterizes projected geometry, retains its physical target
and producer, and rejects prior tokens. Independent consumers also cover nested durable
references and unavailable-record source/history/Save/Reopen workflows; a separate direct
`SceneProjection` consumer preserves repeated Prefab provenance through sixteen bounded epochs.
These are complementary consumers, not one claimed native unknown-Prefab matrix.

The tiny-budget owner holds only three claims while publishing two entities through repeated
scalar edits, Undo/Redo and structural removal/addition. Every historical locator rejects the
new domain. Same-source content may intentionally keep the same content stamp after Clear or a
source return; that does not revive invalid presentation authority. Nonempty omission facts follow
only accepted source revisions, preserve unknown source bytes, and recover after a rejected
candidate. Task/image owner identity and installed image values remain unchanged.

A real Image import is held at the existing post-decode gate across eight preview edits and
Clear/republication. Its task, cancellation state and active budget remain intact; release
produces an actual imported result and a newer published image version. This is not decode-in-
progress scheduling qualification. A real Save is physically held before file publication while
eight Scene epochs advance; release saves only the captured source revision, preserves newer
dirty edits, and completes Reopen. This is not a fault injected during filesystem replacement.

The six native cases include all three critical cadence/input/lifecycle trajectories, retained
Scene/Prefab and Canvas navigation, and trusted Play-source Close continuity. The enhanced Scene
case performs four creation/Undo pairs with exact target identity and texture registration
unchanged, invalidates old tokens and actually presents successors. It also invalidates authority
with an incompatible source revision and requires a new composite token for unchanged last-good
content. That native recovery probe simulates rejected-source observation; the owner tests
separately execute actual materialization rejection/restoration. Hidden native windows and owned
Context input are used; no OS pointer injection or monitor-scanout claim follows.

All builds/tests reuse root `target`, Rust 1.95.0 and one build job. Library features were
`nara/desktop-winit,nara/presentation-2d,nara/presentation-ui,nara/project-workflow-cargo-facts,nara/render-wgpu,nara/runtime-ui,nara/tooling,nara/serde`.
The public command used the same unqualified root features. Libraries, public/native suites and
independent consumers use `--test-threads 1 --no-fail-fast`. The focused owner command uses
`--no-default-features --features presentation-2d,tooling,serde`. The independent command runs
`durable_identity`, `manual_component_codec`, `scene_spawn` and `entity_reference_editing` under
`tests/fixtures/module-consumer/Cargo.toml` with the root target. WGPU admission uses
`--features editor-imgui -E 'test(editor_execution::)'`; native/render runs set `WGPU_BACKEND=dx12`,
and the render control also sets `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.

Affected-library Clippy with `-D clippy::correctness` passes; non-correctness warnings remain,
including 68 Editor warnings. Root and independent-consumer `cargo fmt --all --check`, diff checks,
changed-document local links and baseline/prototype/final artifact hashes pass. Final production
source hashes are recorded in the comparison metadata. Raw terminal logs remain local temporary
`nara-r5c-projection-*.log` / `nara-r5c-preview-lifetime-*.log` files, not committed console dumps.

## Measurements And Rejected Draft

The [comparison](../../../../benchmarks/2026-10-24-preview-projection-lifetime.md) retains three
optimized incumbent processes, three preliminary candidate processes and three corrected final
processes, plus the tiny-capacity saturation witness. Each main comparison has two warmups and
sixteen samples at 64 and 1,024 entities. Source editing is outside the measured intervals;
setup, publication and retirement remain separate. This supports bounded nearest-rank P50/P95,
not P99, allocation bytes or native edit-to-present latency.

The incumbent reaches 19,475 claims after nineteen 1,024-entity publications. The final candidate
retains exactly 1,025 after each publication. Final 1,024-entity P95 is 15.764 / 15.843 / 16.554 ms,
approximately 6-9% above the incumbent. It does not trigger the predeclared greater-than-2x P95
rejection in all three matched runs, but is not a speedup or a complete frame-budget qualification.
Final optimized run IDs are `3ebac4c2-5131-4126-8b55-5533c7c7cecf`,
`66d32eb7-36a0-4d2b-ba03-9bc2763d20e7` and `d63b9207-b74e-4fa6-8a17-9cb758480415`.

The first draft incorrectly rotated renderer producer with each Scene identity epoch. Its CPU
comparison passed the gross threshold but actual native red
`b869f895-bfa7-4b44-b7fc-3f57ede024df` proved unchanged-target/texture reallocation. The corrected
design retains the producer; no cross-producer GPU resource abstraction was added. Preliminary
raw observations remain explicitly named `candidate-before-producer-correction-*`. They are
not relabeled as the final candidate or used to claim native allocation continuity.

The first native probe (`97203450-66ad-47c0-95f4-555ef223a46c`) requested unsupported generic
colored-sprite composition; it was corrected to the supported empty-entity operation before the
verified red. Initial Scene tests corrected two fixture assumptions: hardcoded total World counts
ignored lazily admitted support entities, and an unregistered token cannot use semantic entity
retirement. Its owner despawns that unregistered entity before Clear retries. Exact membership,
stats, locators and bounded population assertions remain. A later owner run
`1d13d284-f893-44f6-94ef-ec52014ccc3e` exposed three stale test assumptions that equal source content
must change its content stamp after every identity epoch. Their assertion/teardown double-panic
abort (`0xc0000409`) is not evidence of production stack corruption. The corrected tests separate
content, domain and producer lifetimes and use an RAII retirement guard; the final 13-case run passes.

## Independent Review

Five scoped read-only Codex lanes covered complete Identity ownership, Scene transactions, initial
Editor integration, producer-lifetime challenge and final producer/presenter correction. The
challenge found the producer/texture coupling and identical-content recovery gap; both were
corrected before final qualification. The final review reports no actionable findings. Two
additional workers implemented disjoint Scene and authoring-owner tests; the integration owner
inspected, corrected and executed them. All seven handles are closed. These are same-family
separately dispatched reviews, not cross-model independence or a rerun of every engine ADR audit.

# Follow-up

The full foundation goal remains active. Advance durable source/property addressing (R5a) before
freezing Behavior and declarative Prefab overlays; R5b, full R2, independent execution scopes and
the remaining delivery/render waves remain open. Same-World epochs close this bounded history
mismatch, not all projection performance: incremental projection and complete native edit-to-
present costs still need their own named workload and admission proof. No perpetual Tombstoned,
continuing-history eviction, checkpoint readmission or Deleted resurrection contract is added.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Epoch design](../../../../architecture/editor-preview-projection-lifetime-design.md)
- [ADR 0058](../../../../architecture/adr/0058-stable-runtime-identity-and-entity-references.md)
- [ADR 0111](../../../../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md)
- [Comparison and raw data](../../../../benchmarks/2026-10-24-preview-projection-lifetime.md)
- [Identity proof](../../../../../crates/nara_identity/src/domain/scene_projection_epoch.rs)
- [Scene projection](../../../../../crates/nara_scene/src/spawn/projection.rs)
- [Editor owner tests](../../../../../src/project_host/runtime/editor/authoring_runtime/preview_content_tests/epoch.rs)
- [Held Save test](../../../../../src/project_host/runtime/editor/persistence/tests/preview_epoch.rs)
- [Joined public toolkit consumer](../../../../../tests/support/fake_preview_toolkit.rs)
- [Independent Prefab consumer](../../../../../tests/fixtures/module-consumer/tests/durable_identity.rs)
- [Native presenter and trajectory](../../../../../editor/src/platform/imgui/scene.rs)
- [Native procedure](../../../../verification/native-editor-qualification.md)
