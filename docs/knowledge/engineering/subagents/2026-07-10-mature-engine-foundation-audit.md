---
type: "Subagent Finding"
title: "Mature engine foundation audit"
description: "July 2026 Bevy, Godot, WGPU, and nara implementation-gap synthesis."
timestamp: 2026-07-10T02:08:17Z
resource: "nara engine foundation"
tags: ["architecture", "bevy", "godot", "wgpu", "audit"]
status: "active"
subagent_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Finding

Seven read-only architecture reviews compared nara's implemented contracts with the local Bevy, Godot, and wgpu reference trees. The repository has sound high-level ownership boundaries, but multiple Accepted ADRs describe behavior that is still partial, bypassed, or absent in code. Treating ADR acceptance as implementation completion is the central governance defect.

Preserve these boundaries during remediation:

- `bevy_ecs` remains the ECS substrate while `nara_app` keeps the product-facing lifecycle and schedule boundary.
- Gameplay/persistent data stays independent from runtime `Entity`, `AssetId`, winit, wgpu, and editor-toolkit handles.
- `CoreStage::TaskUpdate` remains the main-thread integration point for asynchronous results.
- Asset import, render preparation, editor tooling models, and platform adapters retain separate ownership.
- The root facade remains backend-free by default and `ServerPlugins` remains free of raw physical input and desktop backends.

Highest-risk implementation gaps:

1. Plugin build/finish failure can leave lifecycle state ambiguous, cleanup ownership incomplete, and later schedule execution insufficiently guarded.
2. Fixed time, authoritative command admission, and Bevy tracker cleanup do not yet prove zero/one/many-step semantics or a completed-frame ECS boundary.
3. Task workers need bounded admission, panic isolation, cancellation races, finite shutdown, and deterministic domain-owned result application.
4. Asset path validation is not a containment boundary; resolution/open, stable-ID uniqueness, rename recovery, importer execution, artifact publication, and last-good readiness need separate explicit state machines.
5. Surface lifetime and GPU cache identity need an owned raw-handle lease plus device-domain epoch invalidation.
6. Persistent schemas, migrations, prefab projection provenance, editor receipts/recovery, and Play Mode need failure-atomic candidates and closed lifecycle state machines.
7. Diagnostics, budgets, CI executable-dependency trust, platform support, migration notes, and implementation evidence need machine-checkable contracts.

# Evidence

- The pre-plan baseline completed 335 workspace tests, but the suite did not cover the failure/interruption paths above.
- `crates/nara_app/src/lib.rs` exposed fallible plugin build/finish but cleanup returned `()` and completion was represented by a boolean rather than a terminal failure/cleanup state machine.
- `crates/nara_gameplay/src/lib.rs`, `crates/nara_tasks/src/lib.rs`, and the app schedule showed frame-oriented command/result behavior that required tick-aware retention and deterministic integration contracts.
- Asset and image modules contained direct path/import construction that bypassed the intended source/import registry boundary.
- Existing editor workspace and Play Mode code modeled useful document/provenance data but did not yet own platform persistence receipts, bounded journals, or a scheduled runtime `App` host.
- Existing render crates preserved backend isolation, but surface guards, cache generations, target composition, culling, and dynamic upload allocation remained incomplete relative to ADRs 0053-0054.

Reference-engine evidence:

- `repo-ref/bevy/crates/bevy_ecs/src/world/mod.rs` makes world tracker rotation an explicit outer-frame concern.
- `repo-ref/bevy/crates/bevy_time/src/fixed.rs` advances fixed time per simulation step rather than deducting a batch without per-step clock state.
- `repo-ref/bevy/crates/bevy_asset/src/` separates source identity, dependency processing, load state, and runtime handles.
- `repo-ref/godot/core/object/undo_redo.h` and `repo-ref/godot/editor/` preserve saved history/checkpoint and failure-aware editor workflows.
- `repo-ref/godot/main/main.cpp` demonstrates an explicit engine lifecycle boundary instead of treating subsystem setup as an incidental call sequence.
- `repo-ref/wgpu/examples/standalone/03_hdr_surface` and adjacent surface examples make surface/device capabilities and color-space choices explicit.

# Recommendation

Execute `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md` as an umbrella program with evidence gates rather than one irreversible batch:

- M1 proves lifecycle, fixed time, tasks, diagnostic privacy, and capability filesystem feasibility.
- M2 proves identity, migrations, asset source/import, and immutable artifact publication.
- M3 proves input/hierarchy, editor transactions, persistence/recovery, Play lifecycle, and external trust binding.
- M4 proves device-domain rendering, product journeys, diagnostics bridges, quality policy, and hosted CI structure.

Use fearless pre-1.0 replacement: revise ADRs before their implementation slice, remove obsolete APIs in the same unit, add regression evidence, and append public/persistent breaking changes to the migration guide. If a milestone falsifies a load-bearing decision, revise the plan and ADR before opening dependent work.

# Disposition

Active. The unified implementation-ready plan was produced and document-reviewed on 2026-07-10. Execution is registered on `refactor/engine-foundation-contracts`; milestone decisions and verification evidence belong in sharded engineering-memory entries.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/implementation-status.md` (created by U1)
- `docs/knowledge/engineering/subagents/2026-07-09-codebase-foundation-audit.md`
- `docs/knowledge/engineering/decisions/2026-07-09-cross-cutting-runtime-risk-policies.md`
- `docs/knowledge/engineering/progress/2026-07-09-engine-lifecycle-contracts-implementation.md`
