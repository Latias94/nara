# Module And Product-Goal Alignment Review

Status: Completed. Review baseline `727dcbd`; final repaired source `09a18b5` on main.

## Purpose And Scope

Integrate the qualified correction branch into main and push it, then reassess the implemented
engine against STRATEGY.md and the user's earlier 2D/editor production expectations. Examine every
workspace member and root product/Host boundary. Compare responsibilities and actual workflows
with pinned local Bevy and Godot source, not feature counts or unsupported upstream claims.
Reconcile existing docs/research, relevant engineering research, open questions and verification
lineage with current source. Older source references and proposed mechanisms remain dated evidence.

Deliver an English source-backed module matrix, ranked findings, and a dependency-ordered response
plan. Keep a readable standalone HTML outside docs. Distinguish confirmed correctness defects,
structural design debt, deliberate capability limits, and claims needing measurement. Preserve
prior verified corrections and keep historical evidence within its original scope. Do not build
Tile tools or speculative subsystems in this review.

## Execution And Evidence

1. Fetch origin, verify ancestry and preserve concurrent files. Fast-forward main if possible,
   push without rewriting history, and verify the remote main object ID.
2. Establish goal/architecture/consumer evidence and an inventory of all workspace modules.
3. Run seven read-only research lanes in bounded waves, at most three agents concurrently. Trace each
   finding through callers and failure paths; cite current code/tests and pinned mature references.
4. Independently challenge substantive findings, deduplicate root causes, and check unreviewed
   inventory entries. A large file alone, missing mature-engine feature, or absent measurement is
   not a confirmed runtime defect.
5. Publish module coverage, goal alignment, strengths, gaps, priorities and concrete verification
   proposals. Update navigation and record actual review limits; commit and push the review.

Mode: research_spec. Shared checkout; root is the only Git integration and verification owner.
Researchers are read-only, run no Cargo/fmt and preserve other edits. repo-ref is read-only.
No new PR, force push, high-context instruction edits or unrelated vendor staging.

| Lane | Review responsibility | Writable scope |
| --- | --- | --- |
| Root | Goal lineage, inventory, dependency direction, integration, synthesis and external HTML | This plan, new review, navigation and engineering memory; external HTML |
| A | App/runtime, ECS, tasks, core/time, input/gameplay, diagnostics | None |
| B | Reflection/derive, identity, hierarchy/transform, Scene/persistence | None |
| C | Editor/tooling, native UI, panel/action and async ownership | None |
| D | Filesystem, assets/watch, Image, TileMap and project-content lifecycle | None |
| E | Render/WGPU, material, Sprite, window/Winit | None |
| F | Project/recipes, iteration/Play/export, public consumers, scripts and packaging | None |
| G | Runtime UI kernel/widgets/adapter/rendering and text | None |

Reuse researchers for independent challenges across owners after their research waves. A confirmed
defect needs a reachable trigger and consequence, a design debt needs an actual extension cost,
and a capability gap needs a product workflow plus a proposed owner. Resolve significant new
product choices with the user; do not treat research as authorization for every proposed feature.

## Integration Evidence

Fetched origin and confirmed main was an ancestor of the correction branch. Fast-forwarded main
to `727dcbdde5f00bfd63734eb98a420166021f0aee` and pushed normally. `git ls-remote origin
refs/heads/main` returned that exact ID. No source, history or unrelated vendor files were rewritten.
The audit inventory contains 35 workspace packages including the root product and Editor.

The first wave found current managed-runtime guide/ADR prose still teaching the removed raw-App
route and ordinary candidate choreography. Root also owns correcting that guide and the affected
ADR 0003/0084 summaries to the already Accepted ADR 0117 contract. This is documentation repair;
it introduces no new runtime behavior or public API. A separate reader reviews the final prose.

## Confirmed Correctness Repairs During Review

Four independently challenged correctness findings were repaired at their owning boundaries:

- `39a11ab`: new/retargeted TileSets capture an authorized Image publication during later
  Loading/Failed attempts. Cold loading, revocation and invalid identity/slot guards remain intact.
- `d718412`: Winit creation applies configured startup modes; exact exclusive extent is selected
  or rejected explicitly before creation. Actual Windows tests observe mode and physical retirement.
- `8582427`: finite glyph paint baselines are transformed before clipping the raster bitmap;
  outside baselines no longer discard visible ink. Input hit-test bounds remain unchanged.
- `09a18b5`: checked GPU resource stages precede handle use/mapping, preserving deferred pressure,
  validation rejection, valid caches and reservation refunds.

Documentation repairs are `dfd6730` and `3f8b6c5`; the latter also covers the independent Collect
Game workspace in existing CI. Researchers became workers only within disjoint assigned files;
root retained sole Cargo/fmt/Git ownership. Independent final source review found no blocker.

## Delivery And Verification

The [full review](../audits/2026-10-20-module-goal-alignment-review.md) covers all 35 workspace
packages and reconciles existing research. The standalone report is
`D:/Frankorz/Downloads/nara-module-goal-alignment-2026-10-20.html`, outside docs as requested.

The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T044452Z-module-goal-alignment-and-concrete-boundary-repairs-qualification-f7676d2222234fd39b8b565dbcaf996f.md)
records starting-source regressions, passing domain/public-consumer/GPU/Windows native cohorts,
final locked all-feature/all-target workspace and separate GPU feature checks, formatting and
limits. No Cargo ran in parallel. The final documentation commit is normally pushed with the
six repair/documentation commits; its remote object ID is verified in the delivery message.

The next selected work should complete ordinary generic structural authoring and pixel-game
presentation, measure current owner/iteration costs, then prove a persistent two-level game.
Those capability/measurement recommendations are not reported as implemented features. No Tile
authoring tools, new async framework, second job kernel or speculative public abstractions were added.
