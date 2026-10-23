---
type: "Verification Evidence"
title: "Multi-source Prefab refresh obligation qualification"
description: "Complete source-keyed refresh, canonical Prefab observations, budgeted loader admission and bounded recovery/native evidence."
timestamp: 2026-10-23T19:27:21Z
record_id: "47f6266123094e7bb2ea046f42131843"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-multi-refresh-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "883bc196b75ccd1246d872f62ee3eed0dfce81dc"
verified_by: "codex-root; CE full 20261024-r5c-multi-refresh-5d7ef10a; follow-up 01a16a32-1349-7cd0-9c2a-44aa9584cb93"
---

# Verification

Qualified implementation: `883bc196b75ccd1246d872f62ee3eed0dfce81dc`, on top of `16b21ba`.
This is R5c's multi-source refresh prerequisite, not additive Open, exact Play source continuity
through retirement, the complete R5c replacement or the foundation rebaseline. The full goal
remains active. Documentation closure does not rerun or broaden the evidence below.

# Result

- One concrete source-keyed obligation map replaces singleton pending/failed requests. Save or
  Reopen supersedes only its own source; an already-current source cannot erase another obligation.
  Close retains outstanding obligations. Existing freshness state owns phase; no parallel phase
  authority, generic dependency graph or source registry is introduced.
- Browser and anchored persistence derive the same Prefab-domain key from their retained lane's
  project-relative path and admitted configured root. Sources outside that root do not acquire
  invented asset-domain observations. Nested configured roots have independent consumer coverage.
- One successor must verify every expected digest and closure membership before publishing
  Current. Failure preserves every Host obligation and the last-good snapshot. Retry and automatic
  resumption after Runtime retirement use the same complete set, even after its representative
  document closes. Public pending/stale fields are a representative, not necessarily the offender.
- The Loader borrows expectations, reserves existing WorkBytes before cloning/inserting each
  entry, and retains that ticket through the attempt. Failed admission releases already charged
  work. Existing generation, lineage and physical Host custody remain unchanged.

# Evidence

## Executed Qualification

Windows, Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1`, serial Cargo and one nextest test thread.
Native tests use actual Windows/DX12 with `WGPU_BACKEND=dx12`. Independent consumers reuse the
root target. Production code did not change after the full review; later changes add tests only.

| Scope | Actual result | Nextest run |
|---|---|---|
| Final root, Reflect, Scene, Tooling and Editor libraries | 850 passed, 25 ignored | `4eaacec8-1163-455b-a7f6-e11764f280ae` |
| Content boundary, delivery and budget integrations | 39 passed | `8fd01165-0851-4183-bed2-319bec15fbbf` |
| Final independent Editor consumers, including nested roots | 31 passed, 2 ignored | `94fc2808-8646-4186-9f44-ecdef7a01b15` |
| Selected actual native trajectories | 8 passed, 198 skipped | `8b1a2410-4ca6-48b4-9f23-4ac509dda95c` |
| Four-combination recovery regression, also in final libraries | 1 passed | `2aabb75e-4715-41ad-9c22-52ce15364cad` |

```powershell
cargo +1.95.0 nextest run -p nara -p nara_reflect -p nara_scene -p nara_tooling -p nara-editor --lib --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run -p nara --features 'desktop-winit,presentation-2d,presentation-ui,project-workflow-cargo-facts,render-wgpu,runtime-ui,tooling,serde' --test project_content_limits --test project_content_boundary --test project_content_delivery --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --tests --locked --test-threads 1 --no-fail-fast
cargo +1.95.0 clippy -p nara -p nara_tooling -p nara-editor --lib --locked -- -D clippy::correctness
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 fmt --manifest-path tests/fixtures/module-consumer/Cargo.toml --all -- --check
```

Clippy correctness and both formatting checks pass. Clippy remains warning-bearing (approximately
root 211, Tooling 18 and Editor 68 non-correctness warnings); no automatic fixes were applied.
`git diff --check` and 103 local links in the four affected architecture documents passed before
source commit. Final logs are local temporary files `nara-r5c-multi-refresh-{libraries-final,
content,consumers-nested-final,native,recovery-final,clippy}.log`, not repository artifacts.

The eight actual hidden-window native cases are the seven retained source/preview cases from
the preceding qualification plus `native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`.
They do not qualify monitor scanout, accessibility, Linux/macOS or the complete release matrix.
The independent round trip uses ordinary Browser/prepared edit/persistence/Close/anchor/query
interfaces and a fresh document ID, checking both bounded query pages across Save/Reopen and
default/nested roots. It does not qualify public same-kind multi-open.

## Real Reds And Fixture Corrections

- `69209ed3-1cbd-4efb-afea-291138e0bb2a` reproduces the production defect: save P, fail refresh,
  close P, then save Q incorrectly publishes Current. `9e4609a4-2a6e-4589-a25b-151f574a3233`
  reproduces the canonical-key defect: Browser Save, Close and anchored Open reject the new source
  with `project.editor.prefab-validation-failed`.
- An initial compile missed two private measurement-test constructor migrations; they now use
  the empty expectation map. The independent consumer initially expected two entities in a
  deliberately one-record page; it now consumes the continuation and checks both pages.
- Recovery run `b4249c0a-c17b-4610-9b0a-c2f5586881e4` failed because synchronous raw Stop reached
  Empty without the fixture driving the ordinary Editor frame that schedules automatic refresh.
  One normal drive fixes the fixture; no production recovery fence was changed. These fixture
  issues are not additional production architecture findings.

## Independent Review And Disposition

Design lane `01a169f8-5233-7550-adae-cf74971b47dc` identified the unbudgeted expectation copy and
verified reservation-before-copy. Its nonzero-charge cleanup suggestion is covered by the lazy
512-byte test (first expectation admitted, second rejected, all charges released). The one-byte
case also rejects before consuming later expectations. Source review, not allocator instrumentation,
establishes clone ordering; no generic allocator-observation framework was added.

Five simplification lenses completed: reuse `01a16a11-9f82-7f93-b039-0bd3b010a9a4`, quality
`01a16a11-f789-7f32-9320-d5f6b4a40a9b`, efficiency `01a16a12-532c-7402-b502-012354cb9a98`, tests
`01a16a12-becd-7b62-b8b8-d93a7c9c939d`, comments `01a16a13-2c19-7761-a444-799e6b3e96f0`.
A P3 general-retention-helper suggestion was not adopted: direct checked WorkBytes arithmetic is
clearer than a helper requiring error-domain remapping. Public View comments retain real contracts.

The actual CE full workflow completed as `20261024-r5c-multi-refresh-5d7ef10a`: eight personas,
merge leaf, independent validation and report leaf. Its immutable local `review.json` records
`status: complete`, `verdict: Ready with fixes`, one confirmed P2 finding (#1: full-set recovery
coverage), and no confirmed implementation defect or P0/P1. Artifacts remain under the temporary
`compound-engineering-nara/ce-code-review/20261024-r5c-multi-refresh-5d7ef10a` directory. The receipt
predates the follow-up test and is not rewritten into a retroactive green verdict.

Finding #1 is addressed by the four combinations InvalidContent/Retry versus retirement/automatic
resumption, each with/without Q drift, closing representative P and performing no Save/Reopen
requeue. Drift retains last-good and reports SavedSourceChanged; no drift reaches a changed
Current revision with one active generation. Independent follow-up
`01a16a32-1349-7cd0-9c2a-44aa9584cb93` confirms the coverage and nested-root fresh-document oracle
statically; execution evidence belongs to the parent runs above. All review handles are closed.
No external-model or cross-model independence is claimed.

# Follow-up

1. Implement additive Open and delete predecessor capture/admission, AwaitingClose, replacement
   retirement hooks and obsolete UI branches. Preserve source/index/Schema admission and explicit
   Close's dirty decisions, failed Save, cancellation and physical cleanup.
2. Retain trusted Play's exact source through cancellation and physical retirement before claiming
   unrelated Close remains source-local there. Keep global Scene Reopen-versus-Play exclusion.
3. Qualify independent public two-Scene/two-Prefab workflows and affected native adapters. Direct
   outside-root/sibling-prefix qualification remains absent; current source review is not its
   executed consumer evidence. Broader R5c/R2 and all remaining foundation waves stay incomplete.

The pre-existing untracked vendor Cargo.lock and imgui.ini remain untouched/uncommitted. No push,
PR, release, migration guide, second registry or Runtime-per-document was introduced.

# Citations

- [Owning design](../../../../architecture/editor-document-persistence-custody-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Prior source/preview evidence](2026-10-23T180456Z-common-source-admission-and-retained-scene-preview-qualification-45ffac37614a4edaa792f08d650f292e.md)
- [Refresh owner](../../../../../src/project_host/runtime/editor/content_refresh.rs)
- [Loader](../../../../../src/project_content.rs)
- [Recovery tests](../../../../../src/project_host/runtime/editor/persistence/tests/content_refresh.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
