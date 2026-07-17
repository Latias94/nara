---
type: "Current State"
title: "Current Engineering State"
description: "Derived summary of immutable engineering-memory shards."
tags: ["engineering-memory", "derived"]
source_fingerprint: "8d84ca1fe4de07ab5c22f8fca7afe2c098fbfba74140b40d08fd82ea81d186a7"
---

# Current State

<!-- engineering-wiki-memory: derived -->

This file is derived from immutable shards. Record new facts in shards, then render during integration.

- Source fingerprint: `8d84ca1fe4de07ab5c22f8fca7afe2c098fbfba74140b40d08fd82ea81d186a7`
- Immutable records: 135
- Active lane heads: 1

# Active Registrations

- [Asset render resource seam implementation](registry/asset-render-resource-seam-implementation-asset-render-resource-seam-codex-root.md): `completed` (asset-render-resource-seam-codex-root; producer `codex-root`)
- [Reference-game-driven foundation refactor](registry/2026-07/2026-07-16T163914Z-engine-foundation-contract-completion-codex-root-daa83b6521074f95b4a62fbd838a2425.md): `active` (engine-foundation-contract-completion-codex-root; producer `codex-architecture-review`)

# Recent Evidence

- **Decision**: [Treat ignore-deferred scheduling as an explicit compatibility opt-out](decisions/2026-07/2026-07-17T003947Z-treat-ignore-deferred-scheduling-as-an-explicit-compatibility-opt-out-5cbaef069ee343789b4414771fa516e7.md) - Keep Bevy's typed scheduler surface while excluding explicit ignore-deferred relations from Nara's public semantic-anchor compatibility guarantee.
- **Subagent Finding**: [Bevy lifecycle observer and deferred schedule verification](subagents/2026-07/2026-07-16T164357Z-bevy-lifecycle-observer-and-deferred-schedule-verification-027c48803a9442d8930a3d0f558bafd3.md) - Source-bound correction for lifecycle event names, observer scopes, dynamic hooks, public-anchor deferred policy, and package removal co-ownership.
- **Research Note**: [Bevy and Godot evidence for Nara's remaining early architecture decisions](subagents/2026-07/2026-07-16-bevy-godot-early-architecture-research.md) - Incremental source review of high-migration-cost boundaries that Nara should decide, preserve, defer, or reject.
- **Verification Evidence**: [RGF-U4 pure plugin composition verification](verification/2026-07/2026-07-15T162857Z-rgf-u4-pure-plugin-composition-verification-1796d384bcee48fcbf2c6cb27ed08cdd.md) - Verified pure profile/plugin resolution, stable construction identity, schema-provider closure, sealed App commit, and independent reference-game consumption.
- **Verification Evidence**: [RGF-U22 first-playable evidence protocol verification](verification/2026-07/2026-07-15T081419Z-rgf-u22-first-playable-evidence-protocol-verification-6ae07ce306d14b3bab489129f2758a32.md) - Verified the pre-target decision protocol, independent source attestations, trusted evidence envelope, Git revision admission, and ownership cohort gate.
- **Verification Evidence**: [RGF-U10 bounded PNG ingest verification](verification/2026-07/2026-07-14T174858Z-rgf-u10-bounded-png-ingest-verification-cd08a4898f76409392fbaf093174ea5f.md) - Verified bounded PNG read, decode, publication, last-good reload, review disposition, and focused post-review gates.
- **Session Handoff**: [RGF-U11 completion and U10 entry handoff](sessions/2026-07/2026-07-14T005902Z-rgf-u11-completion-and-u10-entry-handoff-43ed130b90a44e059f52ae2ccdd2ab4a.md) - Safe native surface retirement is committed and verified; RGF-U10 bounded PNG ingest is next.
- **Verification Evidence**: [RGF U11 safe surface retirement verification](verification/2026-07/2026-07-13T215323Z-rgf-u11-safe-surface-retirement-verification-7873dc371c574fa3a0330be2b39ca589.md) - Safe owning wgpu surfaces, owner-scoped Winit retirement, device-loss invalidation, and truthful failure aggregation verified.
- **Verification Evidence**: [RGF-U3 capability and manifest ingest verification](verification/2026-07/2026-07-13T181057Z-rgf-u3-capability-and-manifest-ingest-verification-2aa2885658504654bf7fb5f4c1f55201.md) - RGF-U3 closed its feature surface, manifest authority, CLI privacy, and Server regression evidence on the active refactor branch.
- **Research Note**: [Extension ecosystem research: packages, plugins, and editor contributions](extension-ecosystem-engine-research.md) - Cross-engine evidence for Nara's package and extension contribution boundaries.
- **Session Handoff**: [RGF-U9 governance migration and RGF-U1 entry handoff](sessions/2026-07/2026-07-12T065637Z-rgf-u9-governance-migration-and-rgf-u1-entry-handoff-61e62159c1ee4c74a7b5da57625260df.md) - The reference-game-driven foundation plan is the sole active execution contract; RGF-U1 is the first code unit.
- **Subagent Finding**: [Dioxus hot reload and Subsecond Rust hot-patching boundaries](subagents/2026-07/2026-07-12-dioxus-subsecond-rust-hot-patching-research.md) - Primary-source review of Dioxus RSX and asset reload, Subsecond runtime patching, current limitations, failure behavior, and realistic use with a Rust-first bevy_ecs game runtime.

# Integration Notes

- Registration causality follows `supersedes`; wall-clock timestamps are display and scan hints only.
- Use `render --check` after integrating shards to verify this view and `log.md` are fresh.
