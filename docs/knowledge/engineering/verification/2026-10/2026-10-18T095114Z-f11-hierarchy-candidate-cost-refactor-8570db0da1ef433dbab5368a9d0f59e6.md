---
type: "Verification Evidence"
title: "F11 hierarchy candidate cost refactor"
description: "Sparse candidate overlay and private validated baseline reduce measured hierarchy transaction cost while preserving raw-dirty and lifecycle contracts."
timestamp: 2026-10-18T09:51:14Z
record_id: "8570db0da1ef433dbab5368a9d0f59e6"
producer_id: "codex-root"
run_id: "f11-hierarchy-candidate-cost-20261018"
related_plan: "docs/plans/2026-10-18-refactor-hierarchy-candidate-cost-plan.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Result

F11 is implemented within its measured scope. The writer no longer copies all roots, children and
 parent entries for every ordinary owner-controlled batch. It resolves against a borrowed World and
 touched-owner copy-on-write overlay. A private validated baseline is reusable only after an owned
 commit; relation hooks, root-resource epochs and completed-order epochs fail closed. Private Scene
 preparation and retirement retain complete validation. Prepared batches still require the existing
 no-flush/no-hierarchy-mutation owner window.

# Evidence

| Check | Result |
| --- | --- |
| Hierarchy unit and adversarial tests | 31 passed; `d941f6` final log. Covers empty-owner tombstones, same-owner anchor matrix, cross-World root mutation, foreign order replacement, raw relation revalidation, lifecycle observers, cycles, deferred commands, retirement and existing atomicity. |
| Scene/Transform consumers | 141 passed; `f11-owning-final.log`. Includes startup empty-world completion after changing initial generation from a false completed state. |
| Workspace build | `cargo check --locked --workspace --all-targets --all-features` passed; existing warnings remain. |
| Trusted timing | Three complete runs: `bfe2b03c-f1af-4bd8-ac3d-26c3178a1e12`, `e6bdfb59-4d75-475f-862c-e834f3f5f735`, `aa1eb6c0-adee-43ea-8499-985c50c4d707`. At 4,096 independent-eight P95/P99: 0.49/0.58, 0.53/0.66, 0.51/0.61 ms. Full correctness oracles pass. |
| Trusted allocation | `20f1da15-9225-449e-82b6-84c8692ef014`; 32/32 windows counter-consistent, adjusted retained delta zero. Batch-eight requests 68,052 bytes/cycle; independent-eight 72,884. This is requested Rust heap evidence, not RSS/GPU/leak qualification. |
| Reference game | `d2a6ab99-69b6-4d84-b932-6bc520dfe27c`; 34 passed. A prior full invocation had one transient desktop probe timeout; focused retry and complete rerun passed. |

# Decisions Worth Preserving

The trusted writer path is distinct from `CompletedHierarchyProjection` and `HierarchyOrder`; public
resource forgery cannot mint a write baseline. Root epochs are globally minted on resource creation and
managed commit, closing the cross-World resource-transfer counterexample. `HierarchyOrder` records the
same epoch and completion rejects a transferred order until the current World rebuilds it.

The sparse overlay must retain empty child-list overrides and `None` placement tombstones. Same-owner
move anchors are resolved against the pre-removal list with an explicit index adjustment. Retirement
operations continue using full traversal and frontier promotion rather than the ordinary overlay.

# Follow-up

F12 remains the next measured Transform consumer gate. A real static-map/active-actor workload that
exceeds its recorded 2 ms P99 threshold in two of three complete processes should trigger a separate
attribution and dirty-subtree investigation. Do not widen this hierarchy optimization into a public
graph, linked lifetime, Tilemap, physics or audio feature.

# Citations

- [F11 implementation plan](../../../../plans/2026-10-18-refactor-hierarchy-candidate-cost-plan.md)
- [Foundation scale report](../../../../benchmarks/2026-10-18-foundation-scale.md)
- [Foundation audit](../../../../audits/2026-10-18-foundation-contract-audit.md)
- [ADR 0100](../../../../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md)
