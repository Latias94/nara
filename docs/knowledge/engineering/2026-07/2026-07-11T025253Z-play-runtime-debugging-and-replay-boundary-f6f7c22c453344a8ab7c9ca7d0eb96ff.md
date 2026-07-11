---
type: "Research Findings"
title: "Play runtime debugging and replay boundary"
description: "Research Findings for Play runtime debugging and replay boundary."
timestamp: 2026-07-11T02:52:53Z
record_id: "f6f7c22c453344a8ab7c9ca7d0eb96ff"
producer_id: "codex-root"
run_id: "2026-07-11-play-runtime-debug-adr-0071"
---

# Summary

Research across nara, Bevy, Godot, 7 Billion Humans, and the 2025 Jai keynote supports landing ADR
0071 now. nara should provide exact complete fixed-tick control, stable-identity bounded observation,
honest correlation/causality labels, and optional domain-owned execution cursors. Historical
navigation is completed-tick checkpoint restore plus forward replay. System-level stepping,
persistent replay artifact details, and native Rust hot replacement remain separately triggered
decisions.

# Details

## Verified external behavior

- 7 Billion Humans visibly synchronizes stop/pause/step-style controls, speed, worker instruction
  positions, and world state. Its first-party materials do not establish source breakpoints, reverse
  execution, or time travel.
- The Jai keynote demonstrates an approximately 2.3-second full build of an approximately
  300,000-line game on the presented machine and later identifies it as a full clean rebuild rather
  than an incremental build. The video also demonstrates programmable compile-time tooling. It does
  not demonstrate runtime machine-code replacement or automatic state migration.
- Bevy 0.19 stepping is a schedule cursor plus precomputed skip set. Skipped systems are treated as
  completed, independent multithreaded systems can cross an intuitive breakpoint, and its frame
  model cannot guarantee one complete nara fixed tick.
- Godot separates scene pause/suspend, next rendered frame, VM source-line stepping, remote scene
  observation, script/resource reload, and native-extension reload. VM line stepping works because
  GDScript owns explicit line opcodes; GDExtension reload requires recreation hooks and retains
  significant type/state restrictions.

## nara findings at `723f80f`

- `ScenePlaySession` owns a bare `World`; pause is a boolean and Stop drops the session without an
  app/service shutdown state machine.
- `WorldSnapshot` stores only allocator-local `Entity` values and cannot distinguish matching entity
  bit patterns from different worlds.
- `GameplayCommandSet::Capture` is the correct immutable command recording seam, but the completed
  checkpoint safe point occurs only after the entire fixed schedule returns and acknowledgement is
  complete. End-of-frame tracker clearing is a separate boundary.
- U8 remains a mandatory identity spike. Current scene instance allocation is spawner-local,
  `SceneEntityMap::insert` can overwrite collisions, saturating instance allocation can reuse the
  maximum value, and magic instance-to-`SceneEntityId` export can collide with authored IDs.
- U8 must keep scene-local ID, scene instance, persistent runtime ID, stable asset ID, and runtime
  `Entity` as distinct axes. Runtime-only/internal entities need an explicit omitted/count-only or
  world-scoped non-persistent observation policy; tooling must not assign persistent IDs merely to
  observe them.

## Frozen boundary

- `nara_app`: pause/resume/time scale, exact one-complete-fixed-tick step, completed-tick safe point,
  and bounded app/service close.
- U8 identity owner: one world identity domain with allocation, bidirectional lookup, remap,
  unload, and bounded observable tombstones. Exact module/type/bit/wire choices remain spike output.
- `nara_tooling`: host orchestration plus bounded snapshot/diff/timeline models; no raw `Entity`,
  Bevy `NodeId`, native handle, arbitrary world dump, or high-frequency diagnostic-bus trace.
- `nara_reflect`: schema/value codecs only. Inspect eligibility does not itself authorize remote,
  logged, or persistent disclosure; a host policy still allowlists/redacts observation fields.
- Interpreter-like domains: optional stable subject/program-generation/instruction cursor, bounded
  held-data projection, source map, and diagnostic link. Ordinary Rust ECS code has no inferred
  source line.
- Future replay domain: explicit deny-by-default coverage, original command keys, named RNG stream
  state, canonical semantic checksums, bounded checkpoint/log segments, service recovery classes,
  fresh-App restore, and forward replay.

Service recovery classes are `DeterministicRecompute`, `RecordedOutcome`, `RebuildDerived`,
`PresentationOnly`, or `Unsupported`; a missing classification fails closed. The first replay slice
should reject checkpoints while authoritative background results are outstanding.

# Next Action

1. Run the U8/M2 two-world identity spike and select the deep identity owner without precommitting
   UUID width or serialized shape.
2. Complete U9 schema/capability/envelope work before enabling arbitrary component capture.
3. Complete U16 with a real isolated `App`, exact fixed-tick stepping, completed-tick observation,
   and bounded shutdown.
4. Revisit OQ-019 only when a real workflow needs mid-tick system stepping. Revisit OQ-018 after
   U8/U9/U16 and representative replay size/latency measurements. Revisit OQ-020 only after measured
   rebuild-and-restart pressure.

# Citations

- `docs/architecture/adr/0071-play-runtime-debug-control-and-observation.md`
- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md` U8 and U16
- `crates/nara_tooling/src/play.rs#ScenePlaySession`
- `crates/nara_tooling/src/snapshot.rs#WorldSnapshot`
- `crates/nara_gameplay/src/lib.rs#GameplayCommandSet`
- `crates/nara_app/src/lib.rs#run_once`
- `repo-ref/bevy/crates/bevy_ecs/src/schedule/stepping.rs`
- `repo-ref/godot/scene/debugger/scene_debugger.cpp`
- `repo-ref/godot/modules/gdscript/gdscript_vm.cpp`
- https://tomorrowcorporation.com/7billionhumans
- https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/792100/ss_a9863d36d08c6022ba9efcf77ebeba1bf66a0fb6.1920x1080.jpg
- https://www.youtube.com/watch?v=IdpD5QIVOKQ&t=647s
- https://www.youtube.com/watch?v=IdpD5QIVOKQ&t=1060s
