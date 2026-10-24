---
type: "Subagent Finding"
title: "Runtime and Canvas complete-key lowering findings"
description: "Read-only next-unit evidence separates complete logical key coordinates, presentation fencing and bounded representation costs."
timestamp: 2026-10-24T15:49:55Z
record_id: "e010e14b557f42aea70f859c48c9f47f"
status: "research-only"
producer_id: "codex"
run_id: "session-01a1629b-ui-key-followup"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a16e7a-ee70-7ca2-bedb-9af2e2174244"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
---

# Finding

Runtime UI omits containing source and declared occurrence, and lowers the diagnostic AssetRef
request rather than canonical Prefab source. Canvas has a raw-Prefab local-only projection with
no semantic context in its compilation/cache basis. These are the next coordinated adoption
unit, not another reason to redo the already-owned Scene source/occurrence mechanism.

The main owner independently confirmed that `UiKey::scoped` admits `2 + root + local` bytes while
`encoded_len` reports `1 + root + local`. This cost inconsistency must close with the replacement.

# Evidence

The read-only lane inspected Runtime projection, provenance, Canvas authoring/compile caches,
presented-session routing, the 256-byte hard key ceiling and 32-step projection ceiling. It ran
no Cargo and wrote no files; it is closed. Its two terminal responses were collected because the
first response was truncated before its initial ownership/cost sections.

Scene already supplies required containing source, namespace/slot, source-local entity and
ordered canonical Prefab placement. Generated expanded locators, instance numbers, request
spelling and provenance's derived Eq are not substitutes. Current Runtime prefers Scene source
over UiRuntimeKey; preserve that precedence. Canvas document/revision/gesture/image-generation
joins remain exact presentation/picking facts, independent of logical key equality.

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` creates fresh SceneEntityReferences for a
root apply in `resolved_scene.rs`; a resolved patch can serve multiple targets. Pinned Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` separates resource UID/path, per-instantiation nodes
and owner-chain local placement. Neither supplies a ready-made Nara durable occurrence service.

# Recommendation

Compare exact bounded opaque context with shared immutable context/scope-relative placement
before ADR 0102 refreezes. Kernel must not import Scene/ECS types. Keep full semantic bytes and
discriminants for equality; no truncation, Display-based identity or hash-only equality.

An illustrative flat encoding uses a complete document atom of 21 bytes for StableAsset and
23 plus UTF-8 text length for Named/ProjectLocation. If complete context is repeated in root and
local, a short stable-source two-level Prefab example costs 208 bytes; a three-level example costs
262 and exceeds the present hard ceiling. These are proposed-encoding arithmetic, not a measured
production representation or a selected support limit. Main integration retains shared-context
alternatives rather than silently accepting shallow practical depth or increasing a limit.

Qualification needs independent multi-Scene/same-local-ID and same-source/two-occurrence
interaction, canonical alias continuity, repeated/diamond placement, source-switch versus
compatible rebuild, Runtime/Canvas equal-coordinate parity, exact cost boundaries and selected
native Canvas/source/presentation trajectories. Retained logical input requests follow their
existing presented-history contract; never equate an equal logical key with a live old entity
incarnation or promise immediate invalidation of all historical requests after compatible rebuild.

# Disposition

Research only. The source/occurrence design records these next-unit findings. No UI key, budget,
presented-history or neutral-conformance contract changed here; no native/UI qualification is
claimed. Ordinary Scene preparation qualification remains a separate current unit. The complete
foundation goal stays active.

# Citations

- [Owning design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Runtime lowering](../../../../../crates/nara_ui/src/runtime/projection.rs)
- [Kernel identity and cost](../../../../../crates/nara_ui_kernel/src/identity.rs)
- [Canvas](../../../../../crates/nara_ui/src/canvas.rs)
- [ADR 0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
