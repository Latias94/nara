---
type: "Verification Evidence"
title: "U4 immutable spatial cuts and exact hierarchy publication"
description: "Phase readers replace transferable globals; real foreign hierarchy order, same-tick edits and measured costs are qualified."
timestamp: 2026-10-22T19:44:51Z
record_id: "4023754775ba445a887c9056715584e0"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "cfde813"
---

# Verification

Windows 11 / Rust 1.99.0. Cargo runs serially with one build job and the root target. Source is
the coordinated U4 working tree on the recorded parent. Native Editor and full desktop Reference
qualification remain U11; ordinary headless/owner tests do not establish those claims.

# Result

GlobalTransform2d is an ordinary copyable affine value, not an ECS Component. Private simulation
and presentation payloads own captured local/parent/global/order data. Historical, Fixed and
Presented read-only SystemParams and shared-World readers validate World/runtime, role and time.
Resolve uses the fixed semantic helper before Finalize; Gameplay Capture reads the same cut directly.
Later writes/spawn/retire do not mutate an already published cut. A failed publication is unavailable.

Source-value and membership scans observe safe edits sharing one change tick. Contiguous observation
rows avoid World/hash lookup per unchanged source; changed query/archetype order conservatively
rebuilds. Candidate construction reuses the exact validated Hierarchy order instead of retaining
another adjacency graph. Non-spatial direct parents end inheritance. Interpolation's starting
sample reads actual local/parent values after Prepare; history carries World/runtime provenance.

Hierarchy's private completion, concrete order and generation state share one World/sequence
publication. Equal generation, roots and root epoch from another World cannot authorize reuse.
Render checks every live renderable's captured membership as well as its global before publishing.

# Evidence

- Final Hierarchy/Transform/Gameplay/Render/SpriteRender all-feature owner and integration cohort:
  `cargo nextest run --locked -p nara_hierarchy -p nara_transform -p nara_gameplay -p nara_render
  -p nara_sprite_render --all-features --test-threads=1 --no-fail-fast`: 191/191 passes, five ignored
  measurement tests skipped, run `8edcd9d4-e1b0-4455-93ae-85a90cd229c6`.
- Includes actual safe transfer of roots and a different foreign sibling order with equal generation
  and root epoch; prior same-World order rejection; same-change-tick edits and archetype moves;
  non-spatial domains; missing/wrong role/time/World/generation; consume then retire/spawn; paused,
  zero/multiple fixed steps and First/Prepare edits; foreign history; failed publication and unplaced
  Sprite/Tilemap rejection. All three reader params satisfy ReadOnlySystemParam.
- Final slot-identity follow-up plus Asset owners: 329/329 passes in
  `3bb6abef-0e57-4bf0-b7dd-160cf4d191b6`. Simulation and Presentation share one checked
  publication allocator independent from movable slot/history storage. Actual two-slot restore
  and History-owner rebuild/restore regressions pass. The initial deprecated fetch_update spelling
  was replaced by the repository's existing try_update operation; no new deprecation warning remains.
- Reference first-wave/authoring/runtime-core 32/32 and root Check/content/Workspace Play 43/43
  pass in the earlier integration records. Final full consumer reruns are U11.
- [Cost report](../../../../benchmarks/2026-10-23-low-level-spatial-cuts.md) retains separate before,
  initial and final samples. At 4,096, final static/leaf/root/interpolation/four-step P50 is
  39.7/177.4/178.0/344.0/230.0 us versus before 18.5/159.3/178.7/419.6/181.8 us. The initial
  avoidable scan/adjacency overhead was investigated and removed. Remaining complete source scan
  cost is explicit; static and one-leaf measured windows allocate zero. Sparse warmed live
  requested storage adds 475,600 bytes, not per-frame allocation. No global speed claim follows.
- Final-source timing `0bfd8da8-f8d3-4ba9-b0bf-307bfdd30d3b` and allocation
  `f55a5b01-2d10-458f-bdbe-7760f6b1c1f5` both pass after the checked allocator. At 4,096,
  static/leaf/root/interpolation/four-step P50 is 38.3/174.3/173.1/343.5/225.5 us. Steady
  static/leaf allocations remain zero; sparse retained requested storage is 474,576 bytes above
  original. Separate process summaries and all populations remain in the report; static P95
  59.9 us is an observed tail, not hidden by pooling. No material added allocator cost is measured.

# Follow-up

Final independent rereview found and closed a slot-restore collision in the slot-local spatial
publication sequence. The shared checked allocator plus real restore regressions now prevent old
payloads or mismatched presentation associations from being authorized. Final rereview reports no
remaining blocker. Final-source cost confirmation and all five Windows/DX12 native Editor
trajectories pass; Reference headless87 and desktop render6 also pass in U11. Remaining root/lint
closure is recorded in U11 rather than widening the platform claim here.
No post-affine, KeepWorld, linked lifetime, physics or 3D scope is added. Broader ADRs remain partial.
The ordinary owner cohort has an existing Reflect dead-code warning; it is not warning-free.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Snapshot owner/readers](../../../../../crates/nara_transform/src/snapshot.rs)
- [Source and candidate publication](../../../../../crates/nara_transform/src/propagation.rs)
- [Spatial regressions](../../../../../crates/nara_transform/src/snapshot_tests.rs)
- [Hierarchy regressions](../../../../../crates/nara_hierarchy/src/tests.rs)
- [ADR 0100](../../../../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md)
