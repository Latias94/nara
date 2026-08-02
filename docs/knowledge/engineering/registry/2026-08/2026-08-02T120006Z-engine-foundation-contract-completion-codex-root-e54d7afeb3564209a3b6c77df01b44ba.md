---
type: "Work Registration"
title: "Startup scene activation and atomic Retry: SRT-U2 active"
description: "Records the RGS-U4 stop, completes SRT-U1 authority activation, and starts the image revision and invalidation correction gate."
timestamp: 2026-08-02T12:00:06Z
record_id: "e54d7afeb3564209a3b6c77df01b44ba"
tags: ["srt-u1", "srt-u2", "startup-activation", "image-revision", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "d4e166f"
supersedes: "aea0cd34d1954b3bb9b0b1346e5f678b"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-02T120006Z-srt-u1-focused-startup-retry-trial-activation-17ad79c7bdfd417d90ee3d524909b120.md"
---

# Scope

Execute the narrow startup-scene activation and atomic Retry successor plan without accepting broad
ADR 0089 lifecycle or travel semantics.

# Current Claim

SRT-U1 is complete at activation baseline `d4e166f`. SRT-U2 is the sole active implementation unit;
it owns direct image mutation prepare identity, `ImageAsset` construction invariants, and removal of
the unconsumed prepare-invalidation authority before scene activation work resumes.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-02T120006Z-srt-u1-focused-startup-retry-trial-activation-17ad79c7bdfd417d90ee3d524909b120.md

# Handoff

Implement SRT-U2 proof-first. Reuse `AssetSlotRevision`, keep snapshot cache as the sole invalidation
authority, reject invalid RGBA at the `ImageAsset` boundary, run affected Cargo gates serially, and
do not mix startup activation or importer-host abstractions into the correction.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md#srt-u2-correct-image-revision-and-invalidation`
- `crates/nara_asset/src/storage.rs`
- `crates/nara_image/src/prepare.rs`
- `crates/nara_render/src/prepare.rs`
