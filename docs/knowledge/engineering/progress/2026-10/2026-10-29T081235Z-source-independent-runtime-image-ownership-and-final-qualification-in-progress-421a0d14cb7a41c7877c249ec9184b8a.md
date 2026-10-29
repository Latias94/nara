---
type: "Work Progress"
title: "Source-independent Runtime Image ownership and final qualification in progress"
description: "Integrates actual pixel identity, bounded Runtime Image decode and source-owned rejection with independent native Scene consumers; final Host/native qualification and next-owner review remain active."
timestamp: 2026-10-29T08:12:35Z
record_id: "421a0d14cb7a41c7877c249ec9184b8a"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "3ac32a4"
---

# Summary

Continue the complete active foundation Goal, not just this Image checkpoint. Local main and
`refactor/foundation` begin at `3ac32a4`; the current replacement is uncommitted. Preserve the
existing untracked vendor directory. No push/PR or separate build directory is authorized.
The user prioritizes designing foreseeable identity, dependency and lifetime contracts now to
avoid another large replacement; implementation depth can remain staged.

# Details

- ImageAsset now owns complete Product, actual descriptor/color, immutable pixels, a cached
  descriptor-and-pixel hash and optional real import provenance. Prepared snapshots use neutral
  RenderResourceContentHash; superseded descriptor/artifact-hash APIs are removed.
- ImageRuntimeCodec uses bounded v1 JSON metadata plus raw RGBA framing. The independent consumer
  removes producer files, acquires actual primary/auxiliary Images from one captured base and
  uses the existing native resource/Scene transaction. Escaped CPU/prepared owners retain the
  original decoded-output reservation and base; no source database or import lease is invented.
- Huygens found and fixed two P2 source-admission holes: public bytes/file imports bypassed the
  previous provenance guard; fresh TileSet reconciliation could accept an unmatched Image and
  churn publication/revocation. Six new regression tests cover the common admission and repeated
  reconcile paths. The worker froze its files and is closed. Arendt's independent static
  follow-up found no remaining confirmed P1/P2 in that bounded repair scope and is also closed.
- Final Image/Render/TileSet/SpriteRender/UIRender all-feature cohort passes313, with one existing
  default skip, run `e7198c5c-b489-4267-a1a0-a50f3190a79d` (build3m09s/run18.856s).
  Earlier Image/Render164/1 and independent9/0 runs retain their narrower source scope. The
  independent run is `9b09b05a-a463-4f55-aac4-b65b5f912b8e`; later neutral method renaming still
  needs its final re-run. No final root/native/Reference Game result is claimed yet.
- Root and independent workspaces were formatted after writers froze. Offline lock updates only
  add Image/Render's new direct dependencies; no registry package versions changed. Reference
  Game, behavior-language and public-prelude locks were updated as affected consumers.
- Root all-feature library plus ten content/Scene/Play/composition integrations is currently
  compiling in exec session58348, log `%TEMP%/nara-runtime-image-final-root-tests.log`.
  Cargo is strictly serial, Rust1.95.0, jobs1 and the root target.
- Euclid `01a186ae-eee3-7871-8699-7f96ba4189c8` is a read-only next-owner architecture reviewer.
  Review includes intrinsic Image Product requirements for procedural textures/atlases, as well
  as processed TileSet, residency and first-party closed source/processed Host boundaries. No
  verdict on that identity question has been received; do not treat it as a confirmed defect.

# Next Action

Poll the sole Cargo session, collect the architecture review before freezing the checkpoint,
and repair actual findings at their owner. Complete final independent, root, Reference Game,
affected real Windows/DX12, feature and correctness-Clippy checks. Reconcile ADR/guide/plan/ledger
with actual evidence, inspect/stage only intended files, commit and locally fast-forward main.
Keep the complete foundation Goal active. Processed TileSet, first-party Runtime Host/native
residency/input-map delivery, target-build selection, CAS, rendering and Nara-owned Editor UI
remain distinct implementation work.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Image research](../../../../research/2026-10-29-image-content-and-runtime-provenance.md)
- [Runtime Image guide](../../../../guides/runtime-images.md)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
