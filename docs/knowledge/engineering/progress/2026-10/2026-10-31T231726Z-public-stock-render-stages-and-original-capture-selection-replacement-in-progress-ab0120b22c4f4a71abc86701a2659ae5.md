---
type: "Work Progress"
title: "Public stock render stages and original capture selection replacement in progress"
description: "Shared world/Canvas/UI lowering, policy-independent collection and recipe-selected Scene/Canvas adapters implemented; independent external effect workflows remain in integration."
timestamp: 2026-10-31T23:17:26Z
record_id: "ab0120b22c4f4a71abc86701a2659ae5"
producer_id: "codex-foundation-main"
run_id: "stock-composition-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "3d7eb7b"
---

# Summary

Continue the complete active foundation Goal on `refactor/foundation`, based on `3d7eb7b`.
The native-composition predecessor is committed and locally fast-forwarded to main. This new
public-stock-composition replacement is uncommitted and unqualified as a complete workflow.
Full0121, editable Inspector, material, motion/execution and Schema/Prefab evolution remain open.
Only `vendor/` was initially untracked; preserve it untouched, keep `repo-ref` read-only and run
all Cargo serially with jobs1/root target. No push/PR or compatibility layer.

# Details

- Accepted0094 now selects shared typed stock construction; owning0017/0032/0105/0106/0111/0117
  are reconciled before replacement. This is selection, not qualification. The current design
  and guide name original stock collection and explicit world/Canvas/UI wiring.
- Parent core: `WgpuStockStages` retains the original declarations and binds `WgpuStockBindings`.
  `begin` returns one `WgpuStockProgramBuilder`; `world`, optional `resolve_canvas`, normal
  `graph()` operations and `finish` use the same lowerers as the rewritten default policy.
  WorldColor and SceneColor mark actual pixel domains. No-Canvas directly finishes world color.
  Original stage identity and target interpretation reject mismatches. Additional final output
  merging uses borrowed exact-size iterators, not a temporary output vector. An effect-disabled
  original/final version deduplicates once, while repeated caller entries still reject.
- `WgpuStockCaptureAdapter` owns an original installation plus a typed immutable capture mapper.
  It erases a whole capture only, handles callback failure and holds no UI sidecar/native owner.
  Accepted captures retain their definition after adapter retirement. Old public
  `WgpuStockPolicyBindings` was removed without an alias.
- Raman implements policy-independent collection and public `WgpuStockFrameReader` handoff.
  `WgpuStockCapturePlugin` is now explicit; stock contributors no longer depend on concrete default
  policy. Logical Close returns an unhanded original draft. Parent migrated direct renderer test
  setup. Parent also removed obsolete partial `frame.collect_ui` and IncompleteUiTransaction;
  private frame tests now use atomic `collect_policy_with_ui`.
- Hooke implements `ProductRecipe::select_wgpu_policy(configuration, plugin_factory,
  adapter_factory)`, existing configured definition/fingerprint selection and common capture slot.
  Scene/Canvas create original adapters from that recipe, content refresh preserves it, and
  picking retains original SpriteBatches independently of default-policy downcasts. Production
  is source-ready; focused source tests continue in that worker's scope.
- Planck completed six new files in `stock-composition-consumer/effects/{blur,grade}`. Both use
  public sampled-read/attachment-write Features for actual RGBA8/BGRA8 linear/sRGB premultiplied
  ports. Blur radius uses source texels and two passes through private Rgba16Float scratch; the
  numeric oracle must include that half-float rounding. No Cargo/fmt yet for those packages.
- Independent static review accepts same-marker effects and Canvas/noCanvas dispatch. It found
  a P1: failed UI reader admission left a draft that permanently refused successors. Raman is
  correcting the no-sidecar failure path and adding first-refusal/recovered-success coverage.
  Do not discard an actual original sidecar or weaken the contract to hide failed recovery.

Actual early evidence (not final source qualification): renderer all-feature lib check7.90s;
initial focused stock/policy-plugin/frame cohort35/35, run
`8fc5675a-fd8a-4fd2-bd4e-e17def4c8393`,4.501s; Root+Editor all-feature lib check17.99s.
The focused run may predate final partial-path removal/new P1 regression. No latest full owner,
independent effects/Runtime/Scene View, native, minimal, Clippy or formatting result exists yet.
No Cargo is live after terminal exec7628; recheck before the next invocation.

# Next Action

1. Freeze Raman production/tests and run full focused/ordinary renderer qualification; inspect
   actual P1 recovery result. Hooke adds focused recipe/Scene/Canvas source tests, no Cargo.
2. Meitner owns the new independent workspace except effects, using the same new stages, reader,
   recipe factories and actual Sprite/UI content. Complete real Runtime/window, actual Editor
   Scene View, offscreen replay, world-vs-target effects, crisp HUD, exact original branches and
   final-UI-binding rejection. No synthetic/private SpriteBatches or duplicate lowering shortcut.
3. Align the independent lock to root shared versions, reuse root target; freeze writers before
   fmt. Run owner/public/root/Editor/independent, affected actual native and static checks serially.
   Independently review actual final code, not only accepted design, before commit/local ff main.

Agents: read-only Russell `01a19433-c20c-7fa3-97f8-8764dab9ea2d` completed; effects Planck
`01a19433-dd45-7bd2-88df-0640cbf2ad7b` completed. Active source lanes: Raman
`01a19435-9f56-7cf3-a708-ba7736710f4e` owns plugin.rs/plugin tests; Hooke
`01a19435-c27b-7a91-b05d-6f94649072e5` owns root/product/editor production and focused tests;
Meitner `01a19438-abb6-7c52-9a38-9d49d47af7eb` owns new consumer workspace except effects/lock.
Parent owns stages/adapter/program/frame/lib and verification. No worker Cargo/fmt/commit.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Selected stock boundary](../../../../architecture/render-program-production-design.md#next-production-unit-public-stock-composition)
- [Accepted refinement](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#shared-stock-program-construction)
- [Native predecessor](../../verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md)
