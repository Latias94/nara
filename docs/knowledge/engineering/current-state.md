---
type: "Current State"
title: "Current Engineering State"
description: "Derived summary of immutable engineering-memory shards."
tags: ["engineering-memory", "derived"]
source_fingerprint: "2afdb26aced37cc99a6e2b0a665e957b82129b89f70c4ca0bb5f732724d68d84"
---

# Current State

<!-- engineering-wiki-memory: derived -->

This file is derived from immutable shards. Record new facts in shards, then render during integration.

- Source fingerprint: `2afdb26aced37cc99a6e2b0a665e957b82129b89f70c4ca0bb5f732724d68d84`
- Immutable records: 158
- Active lane heads: 1

# Active Registrations

- [Asset render resource seam implementation](registry/asset-render-resource-seam-implementation-asset-render-resource-seam-codex-root.md): `completed` (asset-render-resource-seam-codex-root; producer `codex-root`)
- [Reference-game-driven foundation refactor: RGF-U24](registry/2026-07/2026-07-18T174549Z-engine-foundation-contract-completion-codex-root-422d9a1354ee41d8bbea896b84f83443.md): `active` (engine-foundation-contract-completion-codex-root; producer `codex-root`)

# Recent Evidence

- **Verification Evidence**: [RGF-U26 manual ownership counterfactual verification](verification/2026-07/2026-07-18T174414Z-rgf-u26-manual-ownership-counterfactual-verification-9730fc03912d4eaa874e5384771b0fe8.md) - Commit a2d695d freezes the minimal pre-Host success and three-failure ownership baseline without a policy engine.
- **Session Handoff**: [SRP-level renderer target and ADR admission handoff](sessions/2026-07/2026-07-18T160416Z-srp-level-renderer-target-and-adr-admission-handoff-58aeee5efc21436fb6ed62fb6b3757d5.md) - Captures the non-authoritative conclusion that SRP-level renderer replaceability is a product-goal candidate while graph and public extension mechanisms remain evidence-gated.
- **Verification Evidence**: [RGF-U29 explicit persistent composition verification](verification/2026-07/2026-07-18T102650Z-rgf-u29-explicit-persistent-composition-verification-57a04a45133744909a7c43441c99c9e0.md) - Commit e95cd4b closes frozen registry binding and guarded target-World persistent apply without changing runtime-only ECS behavior.
- **Verification Evidence**: [RGF-U12 authorized startup content verification](verification/2026-07/2026-07-18T035846Z-rgf-u12-authorized-startup-content-verification-ac05708ff3764164865e4cae207bf5a3.md) - Commit f341255 closes bounded authorized scene, prefab, and image startup-content publication as an immutable budget-leased snapshot.
- **Engineering Research**: [C# Gameplay Authoring Surface: Parameterless Behaviour over an ECS Kernel](csharp-gameplay-authoring-surface-research.md) - Primary-source comparison of Unity, Godot, C#, and Nara constraints for hiding frame and data-access mechanics while keeping gameplay object and dependency sources explicit.
- **Engineering Research**: [Godot C# Integration Implications for Nara](godot-csharp-integration-research.md) - Primary-source review of Godot's .NET product stack and the boundaries it suggests for a future, optional Nara C# gameplay Adapter.
- **Subagent Finding**: [LogLog Rust gamedev critique against an optional C# gameplay Adapter](subagents/2026-07/2026-07-17-loglog-rust-gamedev-csharp-gameplay-research.md) - Primary-source review of which Leaving Rust gamedev pain points an optional first-party C#/CoreCLR gameplay path could relieve, which remain engine-product problems, and which CLR/interop risks it introduces.
- **Verification Evidence**: [RGF-U28 public schedule compatibility verification](verification/2026-07/2026-07-17T100726Z-rgf-u28-public-schedule-compatibility-verification-55f3d96f00e544dba50a1802e35c3190.md) - Commit c24b38a closes the four public schedule anchors and seal-time deferred compatibility contract.
- **Verification Evidence**: [RGF-U5 managed runtime correction verification](verification/2026-07/2026-07-17T051052Z-rgf-u5-managed-runtime-correction-verification-5a58cbaf030041fe8ead8cfdf4119c51.md) - Supersedes the first U5 verification after raw managed World access exposed a safe Bevy change-detection bypass; ff2e02a structurally seals the scope and re-verifies the six correction regressions.
- **Verification Evidence**: [RGF-U5 managed runtime verification](verification/2026-07/2026-07-17T014655Z-rgf-u5-managed-runtime-verification-5b37e0cb30ca4d24bb1b30fc98dc7e47.md) - Verified sealed-App admission, sticky fault propagation, exact stepping, bounded close ownership, Winit retirement ordering, and independent reference-game consumption.
- **Subagent Finding**: [RGF-U5 runtime ownership closure review](subagents/2026-07/2026-07-17-rgf-u5-runtime-closure-review.md) - Independent closure review of the corrected managed-runtime ownership, fault, driver, and finite-close contracts.
- **Decision**: [Treat ignore-deferred scheduling as an explicit compatibility opt-out](decisions/2026-07/2026-07-17T003947Z-treat-ignore-deferred-scheduling-as-an-explicit-compatibility-opt-out-5cbaef069ee343789b4414771fa516e7.md) - Keep Bevy's typed scheduler surface while excluding explicit ignore-deferred relations from Nara's public semantic-anchor compatibility guarantee.

# Integration Notes

- Registration causality follows `supersedes`; wall-clock timestamps are display and scan hints only.
- Use `render --check` after integrating shards to verify this view and `log.md` are fresh.
