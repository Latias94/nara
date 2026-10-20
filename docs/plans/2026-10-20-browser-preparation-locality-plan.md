# Browser Preparation Locality

Status: Complete within the finite Browser preparation scope. Selected L2 unit from the
[locality follow-up](2026-10-20-measured-work-locality-plan.md).
Starting source: `b1f9dfec1c51214a46937ac5adf879cdd785d150`.
Implementation: `7f78c5923297862d0d4d4ff63ac3ed478360b96e`.

## Intent

Move measured filesystem enumeration out of the Editor owner call using existing finite task
custody. Preserve exact source-index admission, diagnostics, budgets, last-good snapshots and
physical Close/Drop cleanup. Then prepare Browser Open through an explicit pending intent while
retaining document/revision/source/index authority and dirty-close outcomes. Public consumers and
native maintenance must observe truthful progress and acknowledge exact terminal results.

## Lane Map

- Mode: serial implementation with parallel source research and independent review.
- Checkout: existing main; preserve concurrent work and untracked vendor. repo-ref is read-only.
- Root: Browser owner/controller, integration, this plan and evidence. Sole Cargo/fmt/Git owner;
  one build job, shared target, no concurrent Cargo commands.
- A: read-only source-index/task-custody research. Return ownership seams, failure cases and tests;
  do not edit until assigned a disjoint implementation path.
- B: read-only Tile extraction/visibility research for L3, including pinned Bevy/Godot anchors;
  no implementation during this L2 unit.
- C: read-only Rust/Play attribution research for L4, including existing collector and public
  first-presentation seams; no Cargo, measurements or implementation during this L2 unit.
- Forbidden: AGENTS.md, unrelated changes, historical datasets and repo-ref writes.
- Stop/reconcile affected work for overlapping file ownership or a contradicted accepted contract.
  New async traits, job kernels, Tile tools and speculative public storage layers are excluded.

## Work And Acceptance

- [x] Reconcile source-index attempt ownership and existing task/Host retirement contracts.
- [x] Move bounded scan preparation to finite owned work. Prove coalescing, last-good state,
  stale attempt/project rejection, cancellation/retry and held-worker Close/Drop custody.
- [x] Integrate explicit pending Browser Open preparation with current document/revision,
  source/schema/index identity and dirty Save/Discard/Cancel authority. Reconcile owning ADRs.
- [x] Verify ordinary public/native consumers, exact result acknowledgement and failure recovery.
- [x] Repeat public scan/Open workloads with the explicit result contract, distinguishing owner occupancy from
  total completion. Independently review, document actual limits, update external HTML and commit.

## Outcome

The [report](../benchmarks/2026-10-20-browser-preparation.md) binds three passing release processes,
immutable raw data and exact protocol differences. At 4,096 files, maximum scan owner-drive P50
is 0.135-0.201 ms; total completion remains approximately 0.8 seconds. Scene Open dispatch reports
Preparing in approximately 0.017 ms; its owner-drive P50 is 5.474-5.717 ms and total completion
68-72 ms. This removes long owner calls without claiming less total filesystem/decoding work.

328 aggregate root/Tooling tests, final 26 focused tests, 172 Editor/public recovery tests,
17 release public persistence tests, two actual Windows/DX12 trajectories and the locked workspace
check pass within the [qualification](../knowledge/engineering/verification/2026-10/2026-10-20T095710Z-browser-preparation-locality-qualification-554a0c2edbfb455f84d4b32304446a2e.md)
scope. Counts overlap. The native qualification exposed and then verified a corrected final-frame
TaskPools access after physical close. Exact result IDs, retained Hosts and physical task return
remain independent of UI selection or logical cancellation. ADRs 0047/0118 and the panel guide
describe the deliberate public pending/result contract change.

Final delivery identity is recorded in the external HTML and delivery response. L3/L4 selected
research briefs now live in their linked plans; this unit does not implement them.

L3 and L4 research may proceed without implementation or Cargo overlap. Their selected changes
and verification remain separate units. Remaining scalar encoding/clone costs are still open.
